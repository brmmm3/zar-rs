use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::Parser;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use zarlib::ArchivePath;
use zarlib::operations::{
    EncodingOptions, ExistingPolicy, PasswordSet, StreamInput, create_archive_from_readers,
    create_extraction_file, extract_single_file_to_writer,
};

mod streaming_protocol;
use streaming_protocol as protocol;

#[derive(Debug, Parser)]
#[command(about = "Request files from a directory of ZAR archives")]
pub(crate) struct Args {
    address: String,
    #[arg(
        short = 'z',
        long,
        required_unless_present = "list",
        conflicts_with = "list"
    )]
    zar: Option<String>,
    #[arg(short, long)]
    list: bool,
    #[arg(short, long, default_value = ".", conflicts_with = "list")]
    output: PathBuf,
    #[arg(short, long, conflicts_with = "list")]
    select: Vec<String>,
    #[arg(short, long, conflicts_with = "list")]
    force: bool,
    #[arg(long)]
    no_progress: bool,
}

fn read_status(socket: &mut impl Read) -> Result<(), Box<dyn Error>> {
    let mut status = [0];
    socket.read_exact(&mut status)?;
    match status[0] {
        0 => Ok(()),
        1 => Err(format!("server: {}", protocol::read_text(socket)?).into()),
        _ => Err("invalid server response status".into()),
    }
}

pub(crate) fn request_archives(
    socket: &mut (impl Read + Write),
) -> Result<Vec<String>, Box<dyn Error>> {
    socket.write_all(protocol::MAGIC_STREAMING)?;
    protocol::write_text(socket, "")?;
    protocol::write_u32(socket, 0)?;
    socket.flush()?;
    read_status(socket)?;
    let count = protocol::read_u32(socket)? as usize;
    if count > protocol::MAX_FILES {
        return Err("invalid number of archives in server response".into());
    }
    (0..count)
        .map(|_| protocol::read_text(socket).map_err(Into::into))
        .collect()
}

pub(crate) fn request_files(
    socket: &mut (impl Read + Write),
    archive: &str,
    output: &Path,
    patterns: &[String],
    force: bool,
    progress: Option<&ProgressBar>,
) -> Result<(), Box<dyn Error>> {
    if !(1..=protocol::MAX_PATTERNS).contains(&patterns.len()) {
        return Err("provide between 1 and 64 patterns".into());
    }
    if archive.len() > protocol::MAX_TEXT
        || patterns
            .iter()
            .any(|pattern| pattern.len() > protocol::MAX_TEXT)
    {
        return Err("archive name or pattern is too long".into());
    }
    socket.write_all(protocol::MAGIC_STREAMING)?;
    protocol::write_text(socket, archive)?;
    protocol::write_u32(socket, patterns.len() as u32)?;
    for pattern in patterns {
        protocol::write_text(socket, pattern)?;
    }
    socket.flush()?;

    read_status(socket)?;
    let count = protocol::read_u32(socket)? as usize;
    if !(1..=protocol::MAX_FILES).contains(&count) {
        return Err("invalid number of files in server response".into());
    }
    let mut total_bytes = [0; 8];
    socket.read_exact(&mut total_bytes)?;
    let total_bytes = u64::from_be_bytes(total_bytes);
    if let Some(bar) = progress {
        bar.set_length(total_bytes);
    }
    std::fs::create_dir_all(output)?;
    #[cfg(not(unix))]
    if std::fs::symlink_metadata(output)?.file_type().is_symlink() {
        return Err("output directory must not be a symlink".into());
    }
    let mut received = std::collections::BTreeSet::new();
    let mut declared_bytes = 0_u64;
    for _ in 0..count {
        read_status(socket)?;
        let path = ArchivePath::new(protocol::read_text(socket)?)?;
        if !received.insert(path.clone()) {
            return Err(format!("duplicate output path: {path}").into());
        }
        let mut size = [0; 8];
        socket.read_exact(&mut size)?;
        let size = u64::from_be_bytes(size);
        declared_bytes = declared_bytes
            .checked_add(size)
            .ok_or("transfer size overflow")?;
        if declared_bytes > total_bytes {
            return Err("file sizes exceed the server's declared total".into());
        }
        let target = output.join(path.as_str());
        #[cfg(not(unix))]
        {
            if path.as_str().contains(':') {
                return Err("output path contains a colon".into());
            }
            let mut parent = output.to_path_buf();
            for component in Path::new(path.as_str())
                .parent()
                .ok_or("missing output parent")?
                .components()
            {
                parent.push(component);
                match std::fs::create_dir(&parent) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.into()),
                }
                let kind = std::fs::symlink_metadata(&parent)?.file_type();
                if kind.is_symlink() || !kind.is_dir() {
                    return Err("output parent is not a regular directory".into());
                }
            }
            if force {
                match std::fs::symlink_metadata(&target) {
                    Ok(metadata) if !metadata.file_type().is_file() => {
                        return Err("output target is not a regular file".into());
                    }
                    Ok(_) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        let policy = if force {
            ExistingPolicy::Overwrite
        } else {
            ExistingPolicy::Error
        };
        let mut file = create_extraction_file(output, &target, policy)?;
        let mut input = socket.take(size);
        let mut buffer = [0; 64 * 1024];
        while input.limit() != 0 {
            let read = input.read(&mut buffer)?;
            if read == 0 {
                return Err(format!("incomplete transfer for {path}").into());
            }
            file.write_all(&buffer[..read])?;
            if let Some(bar) = progress {
                bar.inc(read as u64);
            }
        }
        file.flush()?;
    }
    if declared_bytes != total_bytes {
        return Err("file sizes do not match the server's declared total".into());
    }
    if let Some(bar) = progress {
        bar.finish();
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let raw: Vec<OsString> = env::args_os().collect();
    if raw.len() == 4
        && raw[1..]
            .iter()
            .all(|arg| !arg.to_string_lossy().starts_with('-'))
    {
        return legacy(&raw[1..]);
    }
    let args = Args::parse_from(raw);
    let mut socket = TcpStream::connect(&args.address)?;
    if args.list {
        let stdout = io::stdout();
        let mut output = stdout.lock();
        for name in request_archives(&mut socket)? {
            writeln!(output, "{name}")?;
        }
        output.flush()?;
        return Ok(());
    }
    let patterns = if args.select.is_empty() {
        vec!["*".to_owned()]
    } else {
        args.select
    };
    let progress = (!args.no_progress).then(make_progress);
    let result = request_files(
        &mut socket,
        args.zar.as_deref().ok_or("archive name is required")?,
        &args.output,
        &patterns,
        args.force,
        progress.as_ref(),
    );
    if result.is_err()
        && let Some(bar) = progress
    {
        bar.abandon();
    }
    result
}

fn make_progress() -> ProgressBar {
    let bar = ProgressBar::with_draw_target(Some(0), ProgressDrawTarget::stderr_with_hz(5));
    bar.set_style(
        ProgressStyle::with_template(
            "{prefix:<22} [{elapsed_precise}] [{bar:40.cyan}] {binary_bytes}/{binary_total_bytes} ({binary_bytes_per_sec}, {eta})",
        )
        .expect("static progress template is valid")
        .progress_chars("##>-"),
    );
    bar.set_prefix("Receiving");
    bar.enable_steady_tick(Duration::from_millis(200));
    bar
}

fn legacy(args: &[OsString]) -> Result<(), Box<dyn Error>> {
    let address = &args[0];
    let archive_path = &args[1];
    let entry = &args[2];
    let address = address.to_str().ok_or("address must be valid UTF-8")?;
    let entry = entry.to_str().ok_or("archive entry must be valid UTF-8")?;

    let mut socket = TcpStream::connect(address)?;
    let mut size = [0_u8; 8];
    socket.read_exact(&mut size)?;
    let size = u64::from_be_bytes(size);
    let mut input = socket.take(size);
    let mut archive = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(archive_path)?;
    create_archive_from_readers(
        &mut archive,
        &mut [StreamInput {
            path: entry,
            reader: &mut input,
        }],
        EncodingOptions::default(),
        None,
    )?;
    if input.limit() != 0 {
        return Err("server closed the connection before sending the complete file".into());
    }
    archive.flush()?;

    let stdout = io::stdout();
    let mut output = stdout.lock();
    extract_single_file_to_writer(
        &mut archive,
        entry,
        &mut output,
        &PasswordSet::default(),
        None,
    )?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_selection_flags_and_defaults() {
        let args = Args::try_parse_from([
            "streaming_client",
            "127.0.0.1:8081",
            "-z",
            "linux.zar",
            "-o",
            "output",
            "-s",
            "*.txt",
            "--select",
            "*.md",
            "-f",
            "--no-progress",
        ])
        .unwrap();
        assert_eq!(args.address, "127.0.0.1:8081");
        assert_eq!(args.zar.as_deref(), Some("linux.zar"));
        assert_eq!(args.output, Path::new("output"));
        assert_eq!(args.select, ["*.txt", "*.md"]);
        assert!(args.force && args.no_progress);
        let defaults =
            Args::try_parse_from(["streaming_client", "localhost:8081", "--zar", "x.zar"]).unwrap();
        assert_eq!(defaults.output, Path::new("."));
        assert!(defaults.select.is_empty() && !defaults.force && !defaults.no_progress);
    }

    #[test]
    fn list_and_selection_options_are_validated() {
        assert!(
            Args::try_parse_from(["streaming_client", "localhost:8081", "--list"])
                .unwrap()
                .list
        );
        for args in [
            vec!["streaming_client", "localhost:8081"],
            vec![
                "streaming_client",
                "localhost:8081",
                "--list",
                "--zar",
                "a.zar",
            ],
            vec!["streaming_client", "localhost:8081", "--list", "-s", "*"],
            vec![
                "streaming_client",
                "localhost:8081",
                "--zar",
                "a.zar",
                "--select",
            ],
            vec!["streaming_client", "--list"],
        ] {
            assert!(Args::try_parse_from(args).is_err());
        }
    }
}
