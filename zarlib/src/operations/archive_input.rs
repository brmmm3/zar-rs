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
    if filename.to_ascii_lowercase().ends_with(".tar.gz") {
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
    for result in archive.entries()? {
        let mut entry = result?;
        let entry_type = entry.header().entry_type();
        let entry_name = entry
            .path()?
            .to_str()
            .ok_or("TAR entry path is not valid UTF-8")?
            .to_owned();
        let is_directory = entry_type.is_dir();
        let is_file = entry_type.is_file() || entry_type.is_contiguous();
        let archive_path = tar_archive_path(&entry_name, is_directory)?;

        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(format!("TAR links are not supported: {entry_name:?}").into());
        }
        if !is_directory && !is_file {
            return Err(format!("unsupported TAR entry type: {entry_name:?}").into());
        }

        let Some(archive_path) = archive_path else {
            continue;
        };
        let destination_path = destination.join(archive_path.as_str());

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
    Ok(())
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
