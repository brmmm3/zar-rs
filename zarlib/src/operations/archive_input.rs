use super::pipeline::{TreeEntry, archive_prefix, scan_tree};
use crate::ArchivePath;
use sevenz_rust2::{ArchiveReader, Password};
use std::fs::{File, OpenOptions};
use std::io;
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
    let extension = path.extension()?.to_str()?;
    if extension.eq_ignore_ascii_case("zip") {
        Some(ImportArchiveKind::Zip)
    } else if extension.eq_ignore_ascii_case("7z") {
        Some(ImportArchiveKind::SevenZip)
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
