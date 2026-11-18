use super::common::{Progress, ProgressReporter, copy_interruptibly};
use super::pipeline::{TreeEntry, archive_prefix, scan_tree};
use crate::ArchivePath;
use flate2::read::MultiGzDecoder;
use sevenz_rust2::{ArchiveReader, Password};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use zip::ZipArchive;

pub(super) struct ScannedInputs {
    pub entries: Vec<TreeEntry>,
    _temporary_dirs: Vec<TempDir>,
}

#[derive(Clone, Copy)]
pub(super) enum ImportArchiveKind {
    Zip,
    SevenZip,
    Rar,
    Iso,
    Tar,
    TarGz,
    Gzip,
}

/// Returns whether a path has a supported archive format for importing into ZAR.
pub fn is_supported_import_archive(path: &Path) -> bool {
    import_archive_kind(path).is_some()
}

pub(super) fn scan_input_paths(
    paths: &[PathBuf],
    cwd: &Path,
    temporary_parent: &Path,
    progress: Option<&dyn Progress>,
) -> Result<ScannedInputs, Box<dyn std::error::Error>> {
    scan_input_paths_in_folder(paths, cwd, temporary_parent, progress, None)
}

pub(super) fn scan_input_paths_in_folder(
    paths: &[PathBuf],
    cwd: &Path,
    temporary_parent: &Path,
    progress: Option<&dyn Progress>,
    folder: Option<&ArchivePath>,
) -> Result<ScannedInputs, Box<dyn std::error::Error>> {
    let mut entries = Vec::new();
    let mut temporary_dirs = Vec::new();
    let mut temporary_roots = Vec::new();
    let progress = ProgressReporter::new(progress);

    for input in paths {
        progress.check_cancelled()?;
        let source = if input.is_absolute() {
            input.clone()
        } else {
            cwd.join(input)
        };
        if let Some(kind) = import_archive_kind(input).filter(|_| source.is_file()) {
            let temporary_dir = tempfile::Builder::new()
                .prefix(".zarlib-archive-import-")
                .tempdir_in(temporary_parent)?;
            let temporary_root = temporary_dir.path().canonicalize()?;
            match kind {
                ImportArchiveKind::Zip => extract_zip(&source, &temporary_root, &progress)?,
                ImportArchiveKind::SevenZip => extract_7z(&source, &temporary_root, &progress)?,
                ImportArchiveKind::Rar => extract_rar(&source, &temporary_root, &progress)?,
                ImportArchiveKind::Iso => extract_iso(&source, &temporary_root, &progress)?,
                ImportArchiveKind::Tar => {
                    extract_tar(File::open(&source)?, &temporary_root, &progress)?
                }
                ImportArchiveKind::TarGz => extract_tar(
                    MultiGzDecoder::new(File::open(&source)?),
                    &temporary_root,
                    &progress,
                )?,
                ImportArchiveKind::Gzip => {
                    extract_gzip_file(&source, &temporary_root, input, &progress)?
                }
            }
            let scanned_entries = scan_tree(&temporary_root, "")?;
            progress.check_cancelled()?;
            entries.extend(scanned_entries);
            temporary_roots.push(temporary_root);
            temporary_dirs.push(temporary_dir);
        } else {
            let prefix = if folder.is_some() {
                source
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or("input path has no UTF-8 file name")?
                    .to_owned()
            } else {
                archive_prefix(input, cwd)?
            };
            let scanned_entries = scan_tree(&source, &prefix)?;
            progress.check_cancelled()?;
            entries.extend(scanned_entries.into_iter().filter(|entry| {
                !temporary_roots
                    .iter()
                    .any(|root: &PathBuf| entry.source_path.starts_with(root))
            }));
        }
    }

    if let Some(folder) = folder {
        for entry in &mut entries {
            entry.archive_path = format!("{}/{}", folder.as_str(), entry.archive_path);
        }
    }
    Ok(ScannedInputs {
        entries,
        _temporary_dirs: temporary_dirs,
    })
}

pub(super) fn import_archive_kind(path: &Path) -> Option<ImportArchiveKind> {
    let filename = path.file_name()?.to_str()?;
    let filename = filename.to_ascii_lowercase();
    if filename.ends_with(".tar.gz") || filename.ends_with(".tgz") {
        return Some(ImportArchiveKind::TarGz);
    }
    if filename.ends_with(".gz") {
        return Some(ImportArchiveKind::Gzip);
    }

    let extension = path.extension()?.to_str()?;
    if extension.eq_ignore_ascii_case("zip") {
        Some(ImportArchiveKind::Zip)
    } else if extension.eq_ignore_ascii_case("7z") {
        Some(ImportArchiveKind::SevenZip)
    } else if extension.eq_ignore_ascii_case("rar") {
        Some(ImportArchiveKind::Rar)
    } else if extension.eq_ignore_ascii_case("iso") {
        Some(ImportArchiveKind::Iso)
    } else if extension.eq_ignore_ascii_case("tar") {
        Some(ImportArchiveKind::Tar)
    } else {
        None
    }
}

struct InterruptibleIsoReader<'a, 'b> {
    file: File,
    progress: &'a ProgressReporter<'b>,
}

impl Read for InterruptibleIsoReader<'_, '_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.progress.is_cancelled() {
            return Err(io::Error::other("operation cancelled"));
        }
        self.file.read(buf)
    }
}

impl Seek for InterruptibleIsoReader<'_, '_> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.progress.check_cancelled()?;
        self.file.seek(position)
    }
}

pub(super) fn extract_iso(
    source: &Path,
    destination: &Path,
    progress: &ProgressReporter<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    progress.check_cancelled()?;
    let reader = InterruptibleIsoReader {
        file: File::open(source)?,
        progress,
    };
    let options = libmkiso::IsoReadOptions {
        namespace: libmkiso::IsoNamespace::PreferRockRidge,
        ..Default::default()
    };
    let iso = libmkiso::IsoReader::open_with_options(reader, options)?;
    let (mut reader, index) = iso.into_parts();
    for (entry, extents) in index.entries.iter().zip(&index.extents) {
        progress.check_cancelled()?;
        if entry.link_target.is_some() {
            return Err(format!("ISO symbolic links are not supported: {:?}", entry.name).into());
        }
        if let Some(metadata) = &entry.unix {
            let file_type = metadata.mode & 0o170000;
            let expected = if entry.directory { 0o040000 } else { 0o100000 };
            if file_type != 0 && file_type != expected {
                return Err(
                    format!("ISO special entries are not supported: {:?}", entry.name).into(),
                );
            }
        }
        let archive_path = ArchivePath::new(entry.name.clone())
            .map_err(|error| format!("invalid ISO entry path {:?}: {error}", entry.name))?;
        let output_path = destination.join(archive_path.as_str());
        if entry.directory {
            std::fs::create_dir_all(&output_path)?;
            continue;
        }
        let parent = output_path
            .parent()
            .ok_or("ISO entry has no parent directory")?;
        std::fs::create_dir_all(parent)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output_path)?;
        let mut total = 0_u64;
        for extent in extents {
            reader.seek(SeekFrom::Start(extent.offset))?;
            let copied =
                copy_interruptibly(&mut (&mut reader).take(extent.size), &mut output, progress)?;
            if copied != extent.size {
                return Err(format!("truncated ISO entry: {:?}", entry.name).into());
            }
            total = total.checked_add(copied).ok_or("ISO entry size overflow")?;
        }
        if total != entry.size {
            return Err(format!("ISO entry size mismatch: {:?}", entry.name).into());
        }
    }
    Ok(())
}

pub(super) fn extract_rar(
    source: &Path,
    destination: &Path,
    progress: &ProgressReporter<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    progress.check_cancelled()?;
    let cancellation = rars::ReadCancellation::new();
    #[cfg(not(target_os = "wasi"))]
    let result = std::thread::scope(|scope| {
        let (finished, signal) = std::sync::mpsc::channel::<()>();
        let watcher = if progress.0.is_some() {
            let watcher_cancellation = cancellation.clone();
            Some(scope.spawn(move || {
                loop {
                    if progress.is_cancelled() {
                        watcher_cancellation.cancel();
                        break;
                    }
                    match signal.recv_timeout(std::time::Duration::from_millis(20)) {
                        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
            }))
        } else {
            None
        };
        let result = extract_rar_inner(source, destination, progress, &cancellation);
        drop(finished);
        if let Some(watcher) = watcher {
            watcher.join().expect("RAR cancellation watcher panicked");
        }
        result
    });
    #[cfg(target_os = "wasi")]
    let result = extract_rar_inner(source, destination, progress, &cancellation);
    result?;
    progress.check_cancelled()?;
    Ok(())
}

fn extract_rar_inner(
    source: &Path,
    destination: &Path,
    progress: &ProgressReporter<'_>,
    cancellation: &rars::ReadCancellation,
) -> Result<(), Box<dyn std::error::Error>> {
    let options = rars::ArchiveReadOptions::new().with_cancellation(cancellation);
    let archive = rars::ArchiveReader::read_reader_with_options(File::open(source)?, options)?;
    let is_volume = match &archive {
        rars::Archive::Rar13(archive) => archive.main.is_volume(),
        rars::Archive::Rar15To40(archive) => archive.main.is_volume(),
        rars::Archive::Rar50Plus(archive) => archive.main.is_volume(),
        _ => return Err("unsupported RAR archive format".into()),
    };
    if is_volume {
        return Err("multi-volume RAR archives are not supported".into());
    }
    for member in archive.members() {
        progress.check_cancelled()?;
        let meta = &member.meta;
        if meta.is_encrypted {
            return Err("encrypted RAR entries are not supported".into());
        }
        if meta.is_redirection || meta.is_split_before || meta.is_split_after {
            return Err("RAR links and split entries are not supported".into());
        }
        if matches!(meta.attr_source(), rars::AttrSource::Unix) {
            let file_type = meta.file_attr & 0o170000;
            if file_type != 0 && file_type != 0o100000 && file_type != 0o040000 {
                return Err("RAR links and special entries are not supported".into());
            }
        }
        if matches!(meta.attr_source(), rars::AttrSource::Dos) && meta.file_attr & 0x400 != 0 {
            return Err("RAR links and reparse points are not supported".into());
        }
        let name = member.decoded_name(None)?;
        rar_entry_path(&name, meta.is_directory)?;
    }
    archive.extract_with_links(
        options,
        |entry| {
            progress.check_cancelled()?;
            let name = rars::filename::decoded_name(&entry.name, entry.name_is_unicode, None)?;
            let path = rar_entry_path(&name, entry.is_directory)?;
            let output_path = destination.join(path.as_str());
            if entry.is_directory {
                std::fs::create_dir_all(&output_path)?;
                return Ok(Box::new(io::sink()));
            }
            let parent = output_path
                .parent()
                .ok_or_else(|| io::Error::other("RAR entry has no parent directory"))?;
            std::fs::create_dir_all(parent)?;
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output_path)?;
            Ok(Box::new(file))
        },
        |_entry, _link| {
            Err(io::Error::new(io::ErrorKind::InvalidData, "RAR links are not supported").into())
        },
    )?;
    progress.check_cancelled()?;
    Ok(())
}

fn rar_entry_path(name: &[u8], is_directory: bool) -> Result<ArchivePath, io::Error> {
    let text = std::str::from_utf8(name)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
        .replace('\\', "/");
    let path = if is_directory {
        text.trim_end_matches('/')
    } else {
        &text
    };
    ArchivePath::new(path.to_owned()).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid RAR entry path {text:?}: {error}"),
        )
    })
}

pub(super) fn extract_gzip_file(
    source: &Path,
    destination: &Path,
    input: &Path,
    progress: &ProgressReporter<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let source_name = input
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("gzip input has no valid filename")?;
    let output_name = source_name
        .get(..source_name.len() - 3)
        .filter(|name| !name.is_empty())
        .ok_or("gzip input must have a filename before .gz")?;
    let archive_path = ArchivePath::new(output_name.to_owned())
        .map_err(|error| format!("invalid gzip output path {output_name:?}: {error}"))?;
    let output_path = destination.join(archive_path.as_str());
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut decoder = MultiGzDecoder::new(File::open(source)?);
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output_path)?;
    copy_interruptibly(&mut decoder, &mut output, progress)?;
    Ok(())
}

pub(super) fn extract_zip(
    source: &Path,
    destination: &Path,
    progress: &ProgressReporter<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut archive = ZipArchive::new(File::open(source)?)?;
    for index in 0..archive.len() {
        progress.check_cancelled()?;
        let mut entry = archive.by_index(index)?;
        let entry_name = entry.name()?.into_owned();
        let path = if entry.is_dir() {
            entry_name.strip_suffix('/').unwrap_or(&entry_name)
        } else {
            &entry_name
        };
        let archive_path = ArchivePath::new(path.to_owned())
            .map_err(|error| format!("invalid ZIP entry path {entry_name:?}: {error}"))?;
        let destination_path = destination.join(archive_path.as_str());

        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(format!("ZIP symbolic links are not supported: {entry_name:?}").into());
        }

        if entry.is_dir() {
            std::fs::create_dir_all(&destination_path)?;
            continue;
        }

        let parent = destination_path
            .parent()
            .ok_or("ZIP entry has no parent directory")?;
        std::fs::create_dir_all(parent)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination_path)
            .map_err(|error| {
                format!(
                    "cannot create ZIP entry {entry_name:?} without overwriting another entry: {error}"
                )
            })?;
        let copied = copy_interruptibly(&mut entry, &mut output, progress)?;
        if copied != entry.size() {
            return Err(format!(
                "ZIP entry {entry_name:?} contained {copied} bytes, expected {}",
                entry.size()
            )
            .into());
        }
    }
    Ok(())
}

pub(super) fn extract_7z(
    source: &Path,
    destination: &Path,
    progress: &ProgressReporter<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut archive = ArchiveReader::new(File::open(source)?, Password::empty())?;
    archive.for_each_entries(|entry, reader| {
        progress.check_cancelled()?;
        if entry.is_anti_item() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("7z anti-items are not supported: {:?}", entry.name),
            )
            .into());
        }

        if entry.has_windows_attributes
            && (entry.windows_attributes() & 0x400 != 0
                || entry.windows_attributes() & 0o170000 == 0o120000)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("7z symbolic links are not supported: {:?}", entry.name),
            )
            .into());
        }

        let entry_name = &entry.name;
        let path = if entry.is_directory() {
            entry_name.strip_suffix('/').unwrap_or(entry_name)
        } else {
            entry_name
        };
        let archive_path = ArchivePath::new(path.to_owned()).map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("invalid 7z entry path {entry_name:?}: {error}"),
            )
        })?;
        let destination_path = destination.join(archive_path.as_str());

        if entry.is_directory() {
            std::fs::create_dir_all(&destination_path)?;
            return Ok(true);
        }

        let parent = destination_path
            .parent()
            .ok_or_else(|| std::io::Error::other("7z entry has no parent directory"))?;
        std::fs::create_dir_all(parent)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination_path)?;
        let copied = copy_interruptibly(reader, &mut output, progress)?;
        if copied != entry.size() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "7z entry {entry_name:?} contained {copied} bytes, expected {}",
                    entry.size()
                ),
            )
            .into());
        }
        Ok(true)
    })?;
    Ok(())
}

pub(super) fn extract_tar<R: Read>(
    source: R,
    destination: &Path,
    progress: &ProgressReporter<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut archive = tar::Archive::new(source);
    let mut symbolic_links = Vec::new();
    let mut hard_links = Vec::new();
    for result in archive.entries()? {
        progress.check_cancelled()?;
        let mut entry = result?;
        let entry_type = entry.header().entry_type();
        if entry_type.is_pax_global_extensions() {
            continue;
        }

        let entry_name = entry
            .path()?
            .to_str()
            .ok_or("TAR entry path is not valid UTF-8")?
            .to_owned();
        let is_directory = entry_type.is_dir();
        let is_file = entry_type.is_file() || entry_type.is_contiguous();
        let archive_path = tar_archive_path(&entry_name, is_directory)?;

        if !is_directory && !is_file && !entry_type.is_symlink() && !entry_type.is_hard_link() {
            return Err(format!("unsupported TAR entry type: {entry_name:?}").into());
        }

        let Some(archive_path) = archive_path else {
            continue;
        };
        let destination_path = destination.join(archive_path.as_str());

        if entry_type.is_symlink() {
            let target = entry
                .link_name()?
                .ok_or("TAR symbolic link has no target")?;
            let target = target
                .to_str()
                .ok_or("TAR symbolic-link target is not valid UTF-8")?
                .to_owned();
            if target.is_empty() || target.as_bytes().contains(&0) {
                return Err("TAR symbolic-link target is empty or contains NUL".into());
            }
            symbolic_links.push((destination_path, target, entry_name));
            continue;
        }
        if entry_type.is_hard_link() {
            let target = entry.link_name()?.ok_or("TAR hard link has no target")?;
            let target = target
                .to_str()
                .ok_or("TAR hard-link target is not valid UTF-8")?;
            if target.as_bytes().contains(&0) {
                return Err("TAR hard-link target contains NUL".into());
            }
            let target_path =
                tar_archive_path(target, false)?.ok_or("TAR hard link has an empty target")?;
            hard_links.push((
                destination_path,
                destination.join(target_path.as_str()),
                entry_name,
            ));
            continue;
        }

        if is_directory {
            std::fs::create_dir_all(&destination_path)?;
            continue;
        }

        let parent = destination_path
            .parent()
            .ok_or("TAR entry has no parent directory")?;
        std::fs::create_dir_all(parent)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination_path)
            .map_err(|error| {
                format!(
                    "cannot create TAR entry {entry_name:?} without overwriting another entry: {error}"
                )
            })?;
        let copied = copy_interruptibly(&mut entry, &mut output, progress)?;
        if copied != entry.size() {
            return Err(format!(
                "TAR entry {entry_name:?} contained {copied} bytes, expected {}",
                entry.size()
            )
            .into());
        }
    }

    while !hard_links.is_empty() {
        progress.check_cancelled()?;
        let mut unresolved = Vec::new();
        let mut linked_any = false;
        for (link_path, target_path, entry_name) in hard_links {
            match std::fs::symlink_metadata(&target_path) {
                Ok(metadata) if metadata.file_type().is_file() => {
                    ensure_tar_parent_directories(destination, &link_path)?;
                    std::fs::hard_link(&target_path, &link_path).map_err(|error| {
                        format!(
                            "cannot create TAR hard link {entry_name:?} without overwriting another entry: {error}"
                        )
                    })?;
                    linked_any = true;
                }
                Ok(_) => {
                    return Err(format!(
                        "TAR hard-link target is not a regular file: {}",
                        target_path.display()
                    )
                    .into());
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    unresolved.push((link_path, target_path, entry_name));
                }
                Err(error) => return Err(error.into()),
            }
        }
        if !unresolved.is_empty() && !linked_any {
            let (_, target_path, entry_name) = &unresolved[0];
            return Err(format!(
                "TAR hard link {entry_name:?} has no regular-file target: {}",
                target_path.display()
            )
            .into());
        }
        hard_links = unresolved;
    }

    symbolic_links.sort_by_key(|(link_path, _, _)| link_path.components().count());
    for (link_path, target, entry_name) in symbolic_links {
        progress.check_cancelled()?;
        ensure_tar_parent_directories(destination, &link_path)?;
        create_tar_symlink(&target, &link_path)
            .map_err(|error| format!("cannot create TAR symbolic link {entry_name:?}: {error}"))?;
    }

    Ok(())
}

fn ensure_tar_parent_directories(
    destination: &Path,
    entry_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let parent = entry_path
        .parent()
        .ok_or("TAR entry has no parent directory")?;
    let relative_parent = parent.strip_prefix(destination)?;
    let mut current = destination.to_path_buf();
    for component in relative_parent.components() {
        let std::path::Component::Normal(name) = component else {
            return Err("invalid TAR entry parent path".into());
        };
        current.push(name);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(
                    format!("TAR entry parent is not a directory: {}", current.display()).into(),
                );
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                std::fs::create_dir(&current)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn create_tar_symlink(target: &str, link_path: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link_path)
}

#[cfg(windows)]
fn create_tar_symlink(target: &str, link_path: &Path) -> io::Result<()> {
    use std::os::windows::fs::{symlink_dir, symlink_file};

    let target_path = Path::new(target);
    let resolved_target = if target_path.is_absolute() {
        target_path.to_path_buf()
    } else {
        link_path
            .parent()
            .ok_or_else(|| io::Error::other("TAR symbolic link has no parent directory"))?
            .join(target_path)
    };
    if std::fs::metadata(resolved_target).is_ok_and(|metadata| metadata.is_dir()) {
        symlink_dir(target, link_path)
    } else {
        symlink_file(target, link_path)
    }
}

#[cfg(not(any(unix, windows)))]
fn create_tar_symlink(_target: &str, _link_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "symbolic links are not supported on this platform",
    ))
}

fn tar_archive_path(
    entry_name: &str,
    is_directory: bool,
) -> Result<Option<ArchivePath>, Box<dyn std::error::Error>> {
    if entry_name.is_empty() {
        return Err("invalid TAR entry path: empty path".into());
    }

    let path = if is_directory {
        entry_name.strip_suffix('/').unwrap_or(entry_name)
    } else {
        entry_name
    };
    let mut components = Vec::new();
    for component in path.split('/') {
        match component {
            "." => {}
            ".." => {
                return Err(
                    format!("invalid TAR entry path {entry_name:?}: parent component").into(),
                );
            }
            "" => {
                return Err(
                    format!("invalid TAR entry path {entry_name:?}: empty component").into(),
                );
            }
            component => components.push(component),
        }
    }

    if components.is_empty() && is_directory && path == "." {
        return Ok(None);
    }

    let normalized_path = components.join("/");
    let archive_path = ArchivePath::new(normalized_path)
        .map_err(|error| format!("invalid TAR entry path {entry_name:?}: {error}"))?;
    Ok(Some(archive_path))
}

#[cfg(test)]
mod import_tests {
    use super::*;

    #[test]
    fn relative_gzip_import_does_not_duplicate_temporary_files_when_scanning_parent() {
        use std::io::Write;

        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let mut encoder = flate2::write::GzEncoder::new(
            File::create(root.join("payload.txt.gz")).unwrap(),
            flate2::Compression::default(),
        );
        encoder.write_all(b"gzip contents").unwrap();
        encoder.finish().unwrap();
        std::fs::write(root.join("plain.txt"), b"plain").unwrap();
        let scanned = scan_input_paths(
            &[PathBuf::from("payload.txt.gz"), PathBuf::from(".")],
            &root,
            &root,
            None,
        )
        .unwrap();
        let imported: Vec<_> = scanned
            .entries
            .iter()
            .filter(|entry| entry.archive_path == "payload.txt")
            .collect();
        assert_eq!(imported.len(), 1);
        assert_eq!(
            std::fs::read(&imported[0].source_path).unwrap(),
            b"gzip contents"
        );
        assert!(
            scanned
                .entries
                .iter()
                .any(|entry| entry.archive_path == "plain.txt")
        );
        assert!(
            !scanned
                .entries
                .iter()
                .any(|entry| entry.archive_path.contains(".zarlib-archive-import-"))
        );
    }

    #[test]
    fn seven_zip_import_handles_directories_and_rejects_reparse_points() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("input.7z");
        let output = directory.path().join("output");
        std::fs::create_dir(&output).unwrap();
        let mut writer = sevenz_rust2::ArchiveWriter::new(File::create(&source).unwrap()).unwrap();
        writer
            .push_archive_entry(
                sevenz_rust2::ArchiveEntry::new_directory("empty"),
                None::<&[u8]>,
            )
            .unwrap();
        writer.finish().unwrap();
        extract_7z(&source, &output, &ProgressReporter(None)).unwrap();
        assert!(output.join("empty").is_dir());

        let mut writer = sevenz_rust2::ArchiveWriter::new(File::create(&source).unwrap()).unwrap();
        let mut entry = sevenz_rust2::ArchiveEntry::new_file("link");
        entry.has_windows_attributes = true;
        entry.windows_attributes = 0x400;
        writer
            .push_archive_entry(entry, Some(&b"target"[..]))
            .unwrap();
        writer.finish().unwrap();
        let error = extract_7z(&source, &output, &ProgressReporter(None)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("symbolic links are not supported")
        );
        assert!(!output.join("link").exists());

        let mut writer = sevenz_rust2::ArchiveWriter::new(File::create(&source).unwrap()).unwrap();
        let mut entry = sevenz_rust2::ArchiveEntry::new_file("deleted");
        entry.has_stream = false;
        entry.is_anti_item = true;
        writer.push_archive_entry(entry, None::<&[u8]>).unwrap();
        writer.finish().unwrap();
        let error = extract_7z(&source, &output, &ProgressReporter(None)).unwrap_err();
        assert!(error.to_string().contains("anti-items are not supported"));
        assert!(!output.join("deleted").exists());
    }

    #[test]
    fn zip_import_rejects_symbolic_links_and_duplicate_entries() {
        use std::io::Write;

        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("input.zip");
        let output = directory.path().join("output");
        std::fs::create_dir(&output).unwrap();
        let mut writer = zip::ZipWriter::new(File::create(&source).unwrap());
        writer
            .add_symlink("link", "target", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.finish().unwrap();
        let error = extract_zip(&source, &output, &ProgressReporter(None)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("symbolic links are not supported")
        );
        assert!(!output.join("link").exists());

        let mut writer = zip::ZipWriter::new(File::create(&source).unwrap());
        writer
            .start_file("file", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"archive contents").unwrap();
        writer.finish().unwrap();
        std::fs::write(output.join("file"), b"existing contents").unwrap();
        let error = extract_zip(&source, &output, &ProgressReporter(None)).unwrap_err();
        assert!(error.to_string().contains("without overwriting"));
        assert_eq!(
            std::fs::read(output.join("file")).unwrap(),
            b"existing contents"
        );
    }

    #[test]
    fn tar_paths_normalize_current_directories_and_reject_invalid_components() {
        assert!(tar_archive_path("./", true).unwrap().is_none());
        assert_eq!(
            tar_archive_path("./nested/./file", false)
                .unwrap()
                .unwrap()
                .as_str(),
            "nested/file"
        );
        assert_eq!(
            tar_archive_path("nested/", true).unwrap().unwrap().as_str(),
            "nested"
        );
        for invalid in [
            "",
            "../outside",
            "nested/../outside",
            "nested//file",
            "/absolute",
        ] {
            assert!(tar_archive_path(invalid, false).is_err(), "{invalid:?}");
        }
    }

    #[test]
    fn tar_import_rejects_unsupported_special_entries() {
        let directory = tempfile::tempdir().unwrap();
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Fifo);
        header.set_size(0);
        header.set_mode(0o644);
        builder.append_data(&mut header, "pipe", &[][..]).unwrap();
        let archive = builder.into_inner().unwrap();
        let error =
            extract_tar(&archive[..], directory.path(), &ProgressReporter(None)).unwrap_err();
        assert!(error.to_string().contains("unsupported TAR entry type"));
        assert!(!directory.path().join("pipe").exists());
    }

    #[test]
    fn tar_and_zip_import_directories_and_refuse_overwrite() {
        use std::io::Write;

        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("output");
        std::fs::create_dir(&output).unwrap();
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Directory);
        header.set_size(0);
        header.set_mode(0o755);
        builder
            .append_data(&mut header, "nested/", &[][..])
            .unwrap();
        let mut header = tar::Header::new_gnu();
        header.set_size(4);
        header.set_mode(0o644);
        builder
            .append_data(&mut header, "nested/file", &b"data"[..])
            .unwrap();
        let tar = builder.into_inner().unwrap();
        extract_tar(&tar[..], &output, &ProgressReporter(None)).unwrap();
        assert_eq!(std::fs::read(output.join("nested/file")).unwrap(), b"data");
        let error = extract_tar(&tar[..], &output, &ProgressReporter(None)).unwrap_err();
        assert!(error.to_string().contains("without overwriting"));

        let zip = directory.path().join("input.zip");
        let mut writer = zip::ZipWriter::new(File::create(&zip).unwrap());
        writer
            .add_directory("empty/", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer
            .start_file("empty/file", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"zip data").unwrap();
        writer.finish().unwrap();
        extract_zip(&zip, &output, &ProgressReporter(None)).unwrap();
        assert_eq!(
            std::fs::read(output.join("empty/file")).unwrap(),
            b"zip data"
        );
    }

    fn archive_with_links(links: &[(&str, &str)], files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, target) in links {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Link);
            header.set_size(0);
            header.set_mode(0o644);
            header.set_link_name(target).unwrap();
            builder.append_data(&mut header, path, &[][..]).unwrap();
        }
        for (path, contents) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            builder.append_data(&mut header, path, *contents).unwrap();
        }
        builder.into_inner().unwrap()
    }

    #[test]
    fn resolves_forward_hard_links_and_rejects_cycles_and_invalid_parents() {
        let directory = tempfile::tempdir().unwrap();
        let progress = ProgressReporter(None);
        let forward = archive_with_links(
            &[("nested/final", "middle"), ("middle", "target")],
            &[("target", b"linked data")],
        );
        let output = directory.path().join("forward");
        std::fs::create_dir(&output).unwrap();
        extract_tar(&forward[..], &output, &progress).unwrap();
        assert_eq!(
            std::fs::read(output.join("nested/final")).unwrap(),
            b"linked data"
        );
        assert_eq!(
            std::fs::read(output.join("middle")).unwrap(),
            b"linked data"
        );

        let cycle = archive_with_links(&[("first", "second"), ("second", "first")], &[]);
        let output = directory.path().join("cycle");
        std::fs::create_dir(&output).unwrap();
        let error = extract_tar(&cycle[..], &output, &progress).unwrap_err();
        assert!(error.to_string().contains("no regular-file target"));

        let invalid_parent = archive_with_links(
            &[("parent/link", "target")],
            &[("parent", b"not a directory"), ("target", b"data")],
        );
        let output = directory.path().join("invalid-parent");
        std::fs::create_dir(&output).unwrap();
        let error = extract_tar(&invalid_parent[..], &output, &progress).unwrap_err();
        assert!(error.to_string().contains("parent is not a directory"));

        let directory_target = archive_with_links(&[("link", "target")], &[]);
        let output = directory.path().join("directory-target");
        std::fs::create_dir_all(output.join("target")).unwrap();
        let error = extract_tar(&directory_target[..], &output, &progress).unwrap_err();
        assert!(error.to_string().contains("target is not a regular file"));
        assert!(output.join("target").is_dir());
        assert!(!output.join("link").exists());

        let existing_link = archive_with_links(&[("link", "target")], &[("target", b"data")]);
        let output = directory.path().join("existing-link");
        std::fs::create_dir(&output).unwrap();
        std::fs::write(output.join("link"), b"existing").unwrap();
        let error = extract_tar(&existing_link[..], &output, &progress).unwrap_err();
        assert!(error.to_string().contains("without overwriting"));
        assert_eq!(std::fs::read(output.join("link")).unwrap(), b"existing");
    }
}

#[cfg(test)]
mod rar_path_tests {
    use super::*;

    struct ImportProgress {
        cancelled: bool,
    }

    impl Progress for ImportProgress {
        fn message(&self, _: &str) {}
        fn advance(&self, _: u64) {}
        fn is_cancelled(&self) -> bool {
            self.cancelled
        }
    }

    #[test]
    fn rar_import_with_progress_honors_cancellation() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("input.rar");
        let output = directory.path().join("output");
        std::fs::create_dir(&output).unwrap();
        let mut builder = rars::Builder::new(rars::ArchiveVersion::Rar50);
        builder
            .add_bytes(
                b"nested/file".to_vec(),
                b"RAR contents".to_vec(),
                None,
                None,
            )
            .unwrap();
        std::fs::write(&source, builder.to_bytes().unwrap()).unwrap();
        let progress = ImportProgress { cancelled: false };
        extract_rar(&source, &output, &ProgressReporter(Some(&progress))).unwrap();
        assert_eq!(
            std::fs::read(output.join("nested/file")).unwrap(),
            b"RAR contents"
        );

        let cancelled = ImportProgress { cancelled: true };
        let error = extract_rar(&source, &output, &ProgressReporter(Some(&cancelled))).unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        assert_eq!(
            std::fs::read(output.join("nested/file")).unwrap(),
            b"RAR contents"
        );
    }

    #[test]
    fn validates_rar_member_paths() {
        assert_eq!(
            rar_entry_path(br"nested\file.txt", false).unwrap().as_str(),
            "nested/file.txt"
        );
        assert_eq!(rar_entry_path(b"nested/", true).unwrap().as_str(), "nested");
        for path in [
            b"../outside".as_slice(),
            b"/absolute",
            br"C:\outside",
            br"nested\..\outside",
            b"bad\xff",
        ] {
            assert!(rar_entry_path(path, false).is_err(), "{path:?}");
        }
    }
}

#[cfg(test)]
mod iso_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn rejects_iso_with_truncated_file_payload() {
        let directory = tempfile::tempdir().unwrap();
        let iso_path = directory.path().join("truncated.iso");
        let output = directory.path().join("output");
        std::fs::create_dir(&output).unwrap();
        let mut image = Vec::new();
        flate2::read::GzDecoder::new(&include_bytes!("../../tests/fixtures/iso-large.iso.gz")[..])
            .read_to_end(&mut image)
            .unwrap();
        let iso = libmkiso::IsoReader::open_with_options(
            io::Cursor::new(&image),
            libmkiso::IsoReadOptions {
                namespace: libmkiso::IsoNamespace::PreferRockRidge,
                ..Default::default()
            },
        )
        .unwrap();
        let (_, index) = iso.into_parts();
        let extent = index
            .entries
            .iter()
            .zip(&index.extents)
            .find(|(entry, _)| entry.name == "large.bin")
            .unwrap()
            .1
            .first()
            .unwrap();
        let truncated_length = usize::try_from(extent.offset + extent.size - 1).unwrap();
        std::fs::write(&iso_path, &image[..truncated_length]).unwrap();
        let error = extract_iso(&iso_path, &output, &ProgressReporter(None)).unwrap_err();
        assert!(
            error.to_string().contains("truncated ISO volume"),
            "{error}"
        );
        assert!(!output.join("large.bin").exists());
    }

    struct CancellingProgress(AtomicUsize);

    impl Progress for CancellingProgress {
        fn message(&self, _: &str) {}
        fn advance(&self, _: u64) {}
        fn is_cancelled(&self) -> bool {
            self.0.fetch_add(1, Ordering::Relaxed) >= 5
        }
    }

    #[test]
    fn cancels_iso_index_reads() {
        let directory = tempfile::tempdir().unwrap();
        let iso = directory.path().join("input.iso");
        let mut fixture = flate2::read::GzDecoder::new(
            &include_bytes!("../../tests/fixtures/iso-primary.iso.gz")[..],
        );
        io::copy(&mut fixture, &mut File::create(&iso).unwrap()).unwrap();
        let callback = CancellingProgress(AtomicUsize::new(0));
        let progress = ProgressReporter::new(Some(&callback));
        let error = extract_iso(&iso, directory.path(), &progress).unwrap_err();
        assert!(error.to_string().contains("cancelled"), "{error}");
        assert!(callback.0.load(Ordering::Relaxed) > 5);
    }

    struct CancelAfterFirstChunk(PathBuf);

    impl Progress for CancelAfterFirstChunk {
        fn message(&self, _: &str) {}
        fn advance(&self, _: u64) {}
        fn is_cancelled(&self) -> bool {
            std::fs::metadata(&self.0).is_ok_and(|metadata| metadata.len() > 0)
        }
    }

    #[test]
    fn cancels_iso_payload_copy() {
        let directory = tempfile::tempdir().unwrap();
        let iso = directory.path().join("input.iso");
        let mut fixture = flate2::read::GzDecoder::new(
            &include_bytes!("../../tests/fixtures/iso-large.iso.gz")[..],
        );
        io::copy(&mut fixture, &mut File::create(&iso).unwrap()).unwrap();
        let output = directory.path().join("output");
        std::fs::create_dir(&output).unwrap();
        let target = output.join("large.bin");
        let callback = CancelAfterFirstChunk(target.clone());
        let progress = ProgressReporter::new(Some(&callback));
        let error = extract_iso(&iso, &output, &progress).unwrap_err();
        assert!(error.to_string().contains("cancelled"), "{error}");
        let written = std::fs::metadata(target).unwrap().len();
        assert!(written > 0 && written < 300 * 1024);
    }
}
