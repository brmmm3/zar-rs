use super::common::*;
use super::pipeline::archive_prefix;
use crate::{
    ArchivePath, CommitHeader, CommitValidationLimits, ContainerHeader, HEADER_OFFSET_SIZE,
    HeaderFrame, LogicalHeader, Toc, TocChange, TocEntry, UNENCRYPTED_DESCRIPTOR, ValidatedHeader,
};
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub fn remove_archive(archive: &str, paths: &[PathBuf]) -> Result<(), Box<dyn std::error::Error>> {
    remove_archive_with_progress(archive, paths, None)
}

pub fn remove_archive_with_progress(
    archive: &str,
    paths: &[PathBuf],
    progress: Option<&dyn Progress>,
) -> Result<(), Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir()?;
    let mut archive_paths = Vec::with_capacity(paths.len());
    for input in paths {
        let archive_name = archive_prefix(input, &cwd)?;
        archive_paths.push(ArchivePath::new(archive_name)?);
    }
    remove_archive_entries_with_progress(archive, &archive_paths, progress)
}

pub fn remove_archive_entries_with_progress(
    archive: &str,
    paths: &[ArchivePath],
    progress: Option<&dyn Progress>,
) -> Result<(), Box<dyn std::error::Error>> {
    let progress = ProgressReporter::new(progress);
    if let Some(callback) = progress.0 {
        callback.set_total(paths.len() as u64);
    }
    let archive_path = Path::new(archive);
    let parent = archive_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err(format!("archive parent is not a directory: {}", parent.display()).into());
    }
    let (mut toc, new_commit_id) = load_toc_for_removal(archive_path)?;
    let mut changes = Vec::new();
    let mut seen_paths = std::collections::BTreeSet::new();
    for path in paths {
        let path = path.clone();
        push_unique_path(path.clone(), &mut seen_paths)?;
        progress.set_message(format!("Removing {}", path.as_str()));
        match toc.get(&path) {
            Some(TocEntry::File(_)) => {
                changes.push(TocChange::RemoveFile { path: path.clone() });
            }
            Some(TocEntry::Directory) => {
                changes.push(TocChange::RemoveDirectory { path: path.clone() });
            }
            Some(TocEntry::Symlink { .. } | TocEntry::NamedPipe | TocEntry::Device { .. }) => {
                changes.push(TocChange::RemoveFile { path: path.clone() });
            }
            None => return Err(format!("path not found in archive: {path}").into()),
        }
        toc.apply_commit(std::slice::from_ref(
            changes.last().expect("change was just added"),
        ))?;
        progress.inc(1);
    }

    let header_bytes = LogicalHeader::new(changes)
        .with_metadata(commit_metadata(None)?)
        .encode()?;
    append_header_only_commit_with_id(archive_path, &header_bytes, new_commit_id)
}

/// Appends a commit that carries only a logical header (no data section).
pub(crate) fn append_header_only_commit(
    archive_path: &Path,
    header_bytes: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let new_commit_id = next_commit_id(archive_path)?;
    append_header_only_commit_with_id(archive_path, header_bytes, new_commit_id)
}

fn append_header_only_commit_with_id(
    archive_path: &Path,
    header_bytes: &[u8],
    new_commit_id: [u8; 4],
) -> Result<(), Box<dyn std::error::Error>> {
    let limits = CommitValidationLimits::default();
    let data_section_size = 0u64;
    let temp_ch = CommitHeader::new(0, new_commit_id, data_section_size, 0)?;
    let staged_header =
        HeaderFrame::from_uncompressed(temp_ch, header_bytes, UNENCRYPTED_DESCRIPTOR.to_vec(), 3)?;
    let header_size = u32::try_from(staged_header.encoded_size()?)?;
    let commit_header = CommitHeader::new(0, new_commit_id, data_section_size, header_size)?;
    let header = HeaderFrame::from_uncompressed(
        commit_header.clone(),
        header_bytes,
        UNENCRYPTED_DESCRIPTOR.to_vec(),
        3,
    )?;

    let mut archive = OpenOptions::new()
        .read(true)
        .write(true)
        .open(archive_path)?;
    let original_len = archive.metadata()?.len();
    let first_header_offset = read_first_header_offset(&mut archive)?;
    let link_position = if first_header_offset != 0 {
        let (prev_header_offset, prev_commit_header) = find_last_commit(&mut archive, limits)?;
        let (last_header, data_start, next_header_offset) =
            read_commit_at_offset(&mut archive, prev_header_offset, limits)?;
        if next_header_offset != 0 {
            return Err("last archive commit already links to another commit".into());
        }
        if last_header.commit_id != prev_commit_header.commit_id {
            return Err("last archive commit changed while preparing header commit".into());
        }
        data_start
            .checked_sub(HEADER_OFFSET_SIZE)
            .ok_or("previous commit offset underflow")?
    } else {
        4
    };
    let mut old_link = [0u8; 8];
    archive.seek(SeekFrom::Start(link_position))?;
    std::io::Read::read_exact(&mut archive, &mut old_link)?;
    if u64::from_le_bytes(old_link) != 0 {
        return Err("archive commit chain has an unexpected next-commit link".into());
    }

    let append_result = (|| -> Result<(), Box<dyn std::error::Error>> {
        archive.seek(SeekFrom::Start(original_len))?;
        archive.write_all(&0u64.to_le_bytes())?;
        let new_header_offset = archive.stream_position()?;
        header.write_to(&mut archive)?;
        archive.write_all(&new_header_offset.to_le_bytes())?;
        archive.flush()?;
        archive.sync_all()?;

        archive.seek(SeekFrom::Start(link_position))?;
        archive.write_all(&new_header_offset.to_le_bytes())?;
        archive.flush()?;
        archive.sync_all()?;
        Ok(())
    })();

    if let Err(error) = append_result {
        let rollback = (|| -> std::io::Result<()> {
            archive.seek(SeekFrom::Start(link_position))?;
            archive.write_all(&old_link)?;
            archive.set_len(original_len)?;
            archive.sync_all()
        })();
        return match rollback {
            Ok(()) => Err(error),
            Err(rollback_error) => Err(format!(
                "{error}; failed to restore archive after append error: {rollback_error}"
            )
            .into()),
        };
    }
    Ok(())
}

fn load_toc_for_removal(archive_path: &Path) -> Result<(Toc, [u8; 4]), Box<dyn std::error::Error>> {
    let mut file = File::open(archive_path)?;
    let limits = CommitValidationLimits::default();
    let container = ContainerHeader::read_from(&mut file)?;
    let mut toc = Toc::default();
    let mut highest_commit_id = 0_u32;
    let mut header_offset = container.first_header_offset;

    while header_offset != 0 {
        let (commit_header, _, next_header_offset) =
            read_commit_at_offset(&mut file, header_offset, limits)?;
        file.seek(SeekFrom::Start(header_offset))?;
        let validated = ValidatedHeader::read_header(&mut file, limits)?;
        if validated.encryption.is_header_encrypted() || validated.encryption.is_data_encrypted() {
            return Err(
                "mutations on archives containing encrypted commits are unsupported".into(),
            );
        }
        let header = validated
            .logical_header
            .ok_or("cannot build archive TOC from a header-encrypted commit")?;
        toc.apply_commit(&header.changes)?;
        highest_commit_id = highest_commit_id.max(u32::from_le_bytes(commit_header.commit_id));
        header_offset = next_header_offset;
    }

    Ok((
        toc,
        highest_commit_id
            .checked_add(1)
            .ok_or("commit ID counter exhausted")?
            .to_le_bytes(),
    ))
}
