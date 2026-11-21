use std::error::Error;
use std::fs::File;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpListener};
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::Parser;
use zarlib::operations::{FilterSet, PasswordSet, PreparedExtraction};
use zarlib::{ArchivePath, TocEntry};

mod streaming_protocol;
use streaming_protocol as protocol;

const LEGACY_MAGIC: &[u8; 8] = b"ZARREQ01";
const MAGIC_WITH_TOTAL: &[u8; 8] = b"ZARREQ02";
const MAX_IN_MEMORY_FILE: u64 = 8 * 1024 * 1024;

struct SelectedFiles {
    directory: tempfile::TempDir,
    files: Vec<(ArchivePath, u64)>,
    total_bytes: u64,
    prepared: PreparedExtraction,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Protocol {
    Legacy,
    WithTotal,
    Streaming,
}

enum PreparedFile {
    Memory(Vec<u8>),
    File(tempfile::NamedTempFile),
}

enum Response {
    Files(Box<SelectedFiles>),
    Archives(Vec<String>),
}

#[derive(Debug, Parser)]
#[command(about = "Serve a file or a directory of ZAR archives")]
struct Args {
    address: String,
    file_or_directory: PathBuf,
    #[arg(short, long, help = "Suppress client request logging")]
    quiet: bool,
}

fn validate_archive_name(name: &str) -> Result<ArchivePath, Box<dyn Error>> {
    let archive_name = ArchivePath::new(name)?;
    #[cfg(windows)]
    if name.contains(':') {
        return Err("archive name must not contain a colon".into());
    }
    if name.contains('/')
        || !name
            .rsplit_once('.')
            .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("zar"))
    {
        return Err("archive name must be a single .zar filename".into());
    }
    Ok(archive_name)
}

fn list_archives(root: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let mut archives = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_file()
            || !entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("zar"))
        {
            continue;
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "archive filename is not valid UTF-8")?;
        validate_archive_name(&name)?;
        if name.len() > protocol::MAX_TEXT {
            return Err("archive filename is too long".into());
        }
        archives.push(name);
        if archives.len() > protocol::MAX_FILES {
            return Err("too many archives".into());
        }
    }
    archives.sort();
    Ok(archives)
}

fn prepare_file(
    prepared: &PreparedExtraction,
    directory: &Path,
    path: &ArchivePath,
    size: u64,
) -> Result<PreparedFile, Box<dyn Error>> {
    if size <= MAX_IN_MEMORY_FILE {
        return Ok(PreparedFile::Memory(prepared.read_file_range(
            path.as_str(),
            0,
            size,
        )?));
    }
    let file = tempfile::NamedTempFile::new_in(directory)?;
    let written = prepared.extract_single_file(path.as_str(), file.path())?;
    if written != size {
        return Err(format!("extracted size mismatch for {path}").into());
    }
    Ok(PreparedFile::File(file))
}

fn prepare_request(
    socket: &mut impl Read,
    root: &Path,
    mut log: Option<&mut dyn Write>,
) -> Result<(Response, Protocol), Box<dyn Error>> {
    let mut magic = [0; 8];
    socket.read_exact(&mut magic)?;
    let version = match &magic {
        LEGACY_MAGIC => Protocol::Legacy,
        MAGIC_WITH_TOTAL => Protocol::WithTotal,
        protocol::MAGIC_STREAMING => Protocol::Streaming,
        _ => return Err("invalid request protocol".into()),
    };
    let name = protocol::read_text(socket)?;
    let count = protocol::read_u32(socket)? as usize;
    if name.is_empty() && count == 0 {
        if let Some(writer) = log.as_deref_mut() {
            writeln!(writer, "Request: list archives")?;
            writer.flush()?;
        }
        return Ok((Response::Archives(list_archives(root)?), version));
    }
    if !(1..=protocol::MAX_PATTERNS).contains(&count) {
        return Err("invalid number of patterns".into());
    }
    let patterns = (0..count)
        .map(|_| protocol::read_text(socket))
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(writer) = log {
        writeln!(writer, "Request: archive={name:?} patterns={patterns:?}")?;
        writer.flush()?;
    }
    let archive_name = validate_archive_name(&name)?;
    let mut matching_entry = None;
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_name() == std::ffi::OsStr::new(archive_name.as_str()) {
            matching_entry = Some(entry);
            break;
        }
    }
    let entry = matching_entry.ok_or_else(|| format!("archive not found: {name}"))?;
    if !entry.file_type()?.is_file() {
        return Err("archive must be a regular file, not a symlink".into());
    }
    let path = entry.path().canonicalize()?;
    if path.parent() != Some(root) {
        return Err("archive is outside the configured directory".into());
    }
    let filters = FilterSet::from_patterns(&patterns, &[], &[], &[], &[] as &[&Path])?;
    let prepared = PreparedExtraction::new_complete(
        path.to_str().ok_or("archive path is not valid UTF-8")?,
        None,
        &PasswordSet::default(),
    )?;
    let files: Vec<(ArchivePath, u64)> = prepared
        .toc()
        .iter()
        .filter_map(|(path, entry)| {
            if let TocEntry::File(file) = entry
                && filters.includes(path.as_str(), false)
            {
                Some((path.clone(), file.size))
            } else {
                None
            }
        })
        .collect();
    if files.is_empty() {
        return Err("no regular files match the requested patterns".into());
    }
    if files.len() > protocol::MAX_FILES {
        return Err("too many matching files".into());
    }
    if files
        .iter()
        .any(|(path, _)| path.as_str().len() > protocol::MAX_TEXT)
    {
        return Err("matching file path is too long".into());
    }
    let total_bytes = files.iter().try_fold(0_u64, |total, (_, size)| {
        total.checked_add(*size).ok_or("total file size overflow")
    })?;
    let directory = tempfile::tempdir()?;
    if version != Protocol::Streaming {
        for (index, (path, size)) in files.iter().enumerate() {
            let target = directory.path().join(index.to_string());
            match prepare_file(&prepared, directory.path(), path, *size)? {
                PreparedFile::Memory(contents) => std::fs::write(target, contents)?,
                PreparedFile::File(file) => {
                    file.persist(target)?;
                }
            }
        }
    }
    Ok((
        Response::Files(Box::new(SelectedFiles {
            directory,
            files,
            total_bytes,
            prepared,
        })),
        version,
    ))
}

fn write_error(socket: &mut impl Write, error: &dyn Error) -> io::Result<()> {
    socket.write_all(&[1])?;
    let message = error.to_string();
    let mut end = message.len().min(protocol::MAX_TEXT);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    protocol::write_text(socket, &message[..end])?;
    socket.flush()
}

pub(crate) fn serve_directory_request(
    socket: &mut (impl Read + Write),
    root: &Path,
    log: Option<&mut dyn Write>,
) -> Result<(), Box<dyn Error>> {
    let (response, version) = match prepare_request(socket, root, log) {
        Ok(response) => response,
        Err(error) => {
            write_error(socket, error.as_ref())?;
            return Err(error);
        }
    };
    socket.write_all(&[0])?;
    let selected = match response {
        Response::Archives(archives) => {
            protocol::write_u32(socket, archives.len() as u32)?;
            for archive in archives {
                protocol::write_text(socket, &archive)?;
            }
            socket.flush()?;
            return Ok(());
        }
        Response::Files(selected) => selected,
    };
    protocol::write_u32(socket, selected.files.len() as u32)?;
    if version != Protocol::Legacy {
        socket.write_all(&selected.total_bytes.to_be_bytes())?;
    }
    for (index, (path, size)) in selected.files.into_iter().enumerate() {
        if version == Protocol::Streaming {
            let file =
                match prepare_file(&selected.prepared, selected.directory.path(), &path, size) {
                    Ok(file) => file,
                    Err(error) => {
                        write_error(socket, error.as_ref())?;
                        return Err(error);
                    }
                };
            socket.write_all(&[0])?;
            protocol::write_text(socket, path.as_str())?;
            socket.write_all(&size.to_be_bytes())?;
            match file {
                PreparedFile::Memory(contents) => socket.write_all(&contents)?,
                PreparedFile::File(mut file) => {
                    if io::copy(&mut file, socket)? != size {
                        return Err(format!("staged file was truncated: {path}").into());
                    }
                }
            }
            continue;
        }
        protocol::write_text(socket, path.as_str())?;
        socket.write_all(&size.to_be_bytes())?;
        let file = File::open(selected.directory.path().join(index.to_string()))?;
        if io::copy(&mut file.take(size), socket)? != size {
            return Err(format!("staged file was truncated: {path}").into());
        }
    }
    socket.flush()?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let path = args.file_or_directory;
    let metadata = std::fs::metadata(&path)?;
    if !metadata.is_dir() && !metadata.is_file() {
        return Err("input must be a regular file or directory".into());
    }
    let listener = TcpListener::bind(&args.address)?;
    eprintln!("Listening on {}", listener.local_addr()?);
    if metadata.is_dir() {
        let root = path.canonicalize()?;
        for connection in listener.incoming() {
            let mut socket = connection?;
            socket.set_read_timeout(Some(Duration::from_secs(60)))?;
            socket.set_write_timeout(Some(Duration::from_secs(60)))?;
            let peer = socket.peer_addr()?;
            let stderr = io::stderr();
            let mut log = stderr.lock();
            if !args.quiet {
                writeln!(log, "Client {peer} connected")?;
            }
            if let Err(error) = serve_directory_request(
                &mut socket,
                &root,
                if args.quiet { None } else { Some(&mut log) },
            ) {
                writeln!(log, "Request from {peer} failed: {error}")?;
            }
        }
        return Ok(());
    }
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err("input must be a regular file".into());
    }
    let size = metadata.len();
    let (mut socket, peer) = listener.accept()?;
    if !args.quiet {
        eprintln!("Sending {size} bytes to {peer}");
    }
    socket.write_all(&size.to_be_bytes())?;
    let sent = io::copy(&mut file.take(size), &mut socket)?;
    if sent != size {
        return Err("input was truncated during transfer".into());
    }
    socket.flush()?;
    socket.shutdown(Shutdown::Write)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_option_is_optional() {
        let default = Args::try_parse_from(["streaming_server", "localhost:8080", "."]).unwrap();
        assert!(!default.quiet);
        for flag in ["-q", "--quiet"] {
            let args =
                Args::try_parse_from(["streaming_server", flag, "localhost:8080", "."]).unwrap();
            assert!(args.quiet);
        }
    }
}
