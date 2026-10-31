use super::common::*;
use super::list::load_toc;
use super::pipeline::archive_prefix;
use crate::{
    ArchivePath, CommitHeader, CommitValidationLimits, HEADER_OFFSET_SIZE, HeaderFrame,
    LogicalHeader, TocChange, TocEntry, UNENCRYPTED_DESCRIPTOR, ValidatedCommit,
};
use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub fn remove_archive(archive: &str, paths: &[PathBuf]) -> Result<(), Box<dyn std::error::Error>> {
    remove_archive_with_progress(archive, paths, None)
}

pub fn remove_archive_with_progress(
    archive: &str,
    paths: &[PathBuf],
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
    ensure_archive_unencrypted(archive_path)?;
    let mut toc = load_toc(&mut File::open(archive_path)?)?;
    let cwd = std::env::current_dir()?;
    let mut changes = Vec::new();
    let mut seen_paths = std::collections::BTreeSet::new();
    for input in paths {
        let archive_name = archive_prefix(input, &cwd)?;
        let path = ArchivePath::new(archive_name)?;
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

    let id = TEMP_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let unique_id = unique_temp_file_id()?;
    let stem = archive_path
        .file_name()
        .ok_or("archive path has no filename")?
        .to_string_lossy();
    let _staged_commit = parent.join(format!(".{stem}.zarc-remove-{unique_id}-{id}.tmp"));
    let staged_archive = parent.join(format!(".{stem}.zarc-remove-full-{unique_id}-{id}.tmp"));
    let mut cleanup = TemporaryFiles(Vec::new());
    let header_bytes = LogicalHeader::new(changes).encode()?;
    let limits = CommitValidationLimits::default();
    let new_commit_id = next_commit_id(archive_path)?;
    let data_section_size = 0u64;
    let temp_ch = CommitHeader::new(0, new_commit_id, data_section_size, 0)?;
    let staged_header =
        HeaderFrame::from_uncompressed(temp_ch, &header_bytes, UNENCRYPTED_DESCRIPTOR.to_vec(), 3)?;
    let header_size = u32::try_from(staged_header.encoded_size()?)?;
    let commit_header = CommitHeader::new(0, new_commit_id, data_section_size, header_size)?;
    let header = HeaderFrame::from_uncompressed(
        commit_header.clone(),
        &header_bytes,
        UNENCRYPTED_DESCRIPTOR.to_vec(),
        3,
    )?;

    let mut existing = File::open(archive_path)?;
    let first_header_offset = read_first_header_offset(&mut existing)?;
    let prev_last = if first_header_offset != 0 {
        Some(find_last_commit(&mut existing, limits)?)
    } else {
        None
    };
    drop(existing);

    let mut combined = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged_archive)?;
    cleanup.0.push(staged_archive.clone());
    io::copy(&mut File::open(archive_path)?, &mut combined)?;
    combined.write_all(&0u64.to_le_bytes())?;
    let new_header_offset = combined.stream_position()?;
    header.write_to(&mut combined)?;
    combined.write_all(&new_header_offset.to_le_bytes())?;
    combined.flush()?;

    if let Some((prev_header_offset, prev_commit_header)) = prev_last {
        let next_offset_pos = prev_header_offset
            .checked_sub(prev_commit_header.data_section_size)
            .and_then(|pos| pos.checked_sub(HEADER_OFFSET_SIZE))
            .ok_or("previous commit offset underflow")?;
        combined.seek(SeekFrom::Start(next_offset_pos))?;
        combined.write_all(&new_header_offset.to_le_bytes())?;
    } else {
        combined.seek(SeekFrom::Start(4))?;
        combined.write_all(&new_header_offset.to_le_bytes())?;
    }
    combined.flush()?;
    drop(combined);

    let mut validation = File::open(&staged_archive)?;
    let first_header_offset = read_first_header_offset(&mut validation)?;
    let mut header_offset = first_header_offset;
    while header_offset != 0 {
        let (ch, _, next_header_offset) =
            read_commit_at_offset(&mut validation, header_offset, limits)?;
        ValidatedCommit::read_body(&mut validation, ch, limits)?;
        header_offset = next_header_offset;
    }
    let _ = load_toc(&mut File::open(&staged_archive)?)?;
    drop(validation);
    std::fs::rename(&staged_archive, archive_path)?;
    cleanup.0.retain(|path| path != &staged_archive);
    drop(cleanup);
    Ok(())
}
