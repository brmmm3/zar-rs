use super::pipeline::{TreeEntry, archive_prefix, scan_tree};
use crate::ArchivePath;
use flate2::read::MultiGzDecoder;
use sevenz_rust2::{ArchiveReader, Password};
use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use zip::ZipArchive;

pub(super) struct ScannedInputs {
    pub entries: Vec<TreeEntry>,
    _temporary_dirs: Vec<TempDir>,
}

#[derive(Clone, Copy)]
enum ImportArchiveKind {
    Zip,
    SevenZip,
    Tar,
    TarGz,
}

/// Returns whether a path has a supported archive format for importing into ZAR.
pub fn is_supported_import_archive(path: &Path) -> bool {
    import_archive_kind(path).is_some()
}

pub(super) fn scan_input_paths(
    paths: &[PathBuf],
    cwd: &Path,
    temporary_parent: &Path,
) -> Result<ScannedInputs, Box<dyn std::error::Error>> {
    let mut entries = Vec::new();
    let mut temporary_dirs = Vec::new();
    let mut temporary_roots = Vec::new();

    for input in paths {
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
                ImportArchiveKind::Zip => extract_zip(&source, &temporary_root)?,
                ImportArchiveKind::SevenZip => extract_7z(&source, &temporary_root)?,
                ImportArchiveKind::Tar => extract_tar(File::open(&source)?, &temporary_root)?,
                ImportArchiveKind::TarGz => {
                    extract_tar(MultiGzDecoder::new(File::open(&source)?), &temporary_root)?
                }
            }
            entries.extend(scan_tree(&temporary_root, "")?);
            temporary_roots.push(temporary_root);
            temporary_dirs.push(temporary_dir);
        } else {
            let prefix = archive_prefix(input, cwd)?;
            entries.extend(scan_tree(&source, &prefix)?.into_iter().filter(|entry| {
                !temporary_roots
                    .iter()
                    .any(|root: &PathBuf| entry.source_path.starts_with(root))
            }));
        }
    }

    Ok(ScannedInputs {
        entries,
        _temporary_dirs: temporary_dirs,
    })
}

fn import_archive_kind(path: &Path) -> Option<ImportArchiveKind> {
    let filename = path.file_name()?.to_str()?;
    let filename = filename.to_ascii_lowercase();
    if filename.ends_with(".tar.gz") || filename.ends_with(".tgz") {
        return Some(ImportArchiveKind::TarGz);
    }

    let extension = path.extension()?.to_str()?;
    if extension.eq_ignore_ascii_case("zip") {
        Some(ImportArchiveKind::Zip)
    } else if extension.eq_ignore_ascii_case("7z") {
        Some(ImportArchiveKind::SevenZip)
    } else if extension.eq_ignore_ascii_case("tar") {
        Some(ImportArchiveKind::Tar)
    } else {
        None
    }
}

fn extract_zip(source: &Path, destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut archive = ZipArchive::new(File::open(source)?)?;
    for index in 0..archive.len() {
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
        let copied = io::copy(&mut entry, &mut output)?;
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

fn extract_7z(source: &Path, destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut archive = ArchiveReader::new(File::open(source)?, Password::empty())?;
    archive.for_each_entries(|entry, reader| {
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
        let copied = io::copy(reader, &mut output)?;
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

fn extract_tar<R: Read>(source: R, destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut archive = tar::Archive::new(source);
    let mut symbolic_links = Vec::new();
    let mut hard_links = Vec::new();
    for result in archive.entries()? {
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
        let copied = io::copy(&mut entry, &mut output)?;
        if copied != entry.size() {
            return Err(format!(
                "TAR entry {entry_name:?} contained {copied} bytes, expected {}",
                entry.size()
            )
            .into());
        }
    }

    while !hard_links.is_empty() {
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
