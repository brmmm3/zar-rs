use super::common::*;
use super::pipeline::archive_prefix;
use crate::{
    ArchivePath, CommitHeader, CommitValidationLimits, ContainerHeader, ContentAead,
    EncryptionDescriptor, EncryptionScope, HEADER_OFFSET_SIZE, HeaderFrame, LogicalHeader, Toc,
    TocChange, TocEntry, ValidatedCommit, generate_dek, generate_nonce, wrap_dek_with_password,
};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

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
    remove_archive_entries_with_progress_and_encryption(
        archive,
        paths,
        progress,
        None,
        &PasswordSet::default(),
    )
}

/// Removes entries and optionally encrypts the resulting header-only commit.
/// Passwords for existing encrypted commits are required to build the complete
/// archive state before applying removals.
pub fn remove_archive_entries_with_progress_and_encryption(
    archive: &str,
    paths: &[ArchivePath],
    progress: Option<&dyn Progress>,
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
    passwords: &PasswordSet,
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
    let (mut toc, next_commit_id) = load_toc_for_removal(archive_path, passwords)?;
    let reusable_tail = find_reusable_empty_tail(archive_path, passwords)?;
    let new_commit_id = reusable_tail.map_or(next_commit_id, |tail| tail.commit_id);
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
    append_header_only_commit_with_id_and_encryption(
        archive_path,
        &header_bytes,
        new_commit_id,
        true,
        encryption,
        passwords,
    )
}

/// Appends a commit that carries only a logical header (no data section).
pub(crate) fn append_header_only_commit(
    archive_path: &Path,
    header_bytes: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let new_commit_id = next_commit_id(archive_path)?;
    append_header_only_commit_with_id(archive_path, header_bytes, new_commit_id, false)
}

fn append_header_only_commit_with_id(
    archive_path: &Path,
    header_bytes: &[u8],
    new_commit_id: [u8; 4],
    reuse_empty_tail: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    append_header_only_commit_with_id_and_encryption(
        archive_path,
        header_bytes,
        new_commit_id,
        reuse_empty_tail,
        None,
        &PasswordSet::default(),
    )
}

fn append_header_only_commit_with_id_and_encryption(
    archive_path: &Path,
    header_bytes: &[u8],
    new_commit_id: [u8; 4],
    reuse_empty_tail: bool,
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
    passwords: &PasswordSet,
) -> Result<(), Box<dyn std::error::Error>> {
    let limits = CommitValidationLimits::default();
    let data_section_size = 0u64;
    let (dek, descriptor_bytes) = if let Some((password, algorithm, scope)) = encryption {
        let dek = Zeroizing::new(generate_dek()?);
        let descriptor = EncryptionDescriptor::Password {
            scope,
            algorithm,
            nonce: generate_nonce()?,
            key_wrap: wrap_dek_with_password(password, &dek, algorithm, 65_536, 3, 1)?,
        };
        (Some(dek), descriptor.encode()?)
    } else {
        (None, crate::UNENCRYPTED_DESCRIPTOR.to_vec())
    };
    let temp_ch = CommitHeader::new(0, new_commit_id, data_section_size, 0)?;
    let staged_header = if encryption.is_some_and(|(_, _, scope)| {
        matches!(
            scope,
            EncryptionScope::Header | EncryptionScope::HeaderAndData
        )
    }) {
        HeaderFrame::from_uncompressed_encrypted(
            temp_ch,
            header_bytes,
            descriptor_bytes.clone(),
            3,
            dek.as_deref()
                .ok_or("encrypted header is missing its key")?,
        )?
    } else {
        HeaderFrame::from_uncompressed(temp_ch, header_bytes, descriptor_bytes.clone(), 3)?
    };
    let header_size = u32::try_from(staged_header.encoded_size()?)?;
    let commit_header = CommitHeader::new(0, new_commit_id, data_section_size, header_size)?;
    let header = if encryption.is_some_and(|(_, _, scope)| {
        matches!(
            scope,
            EncryptionScope::Header | EncryptionScope::HeaderAndData
        )
    }) {
        HeaderFrame::from_uncompressed_encrypted(
            commit_header,
            header_bytes,
            descriptor_bytes,
            3,
            dek.as_deref()
                .ok_or("encrypted header is missing its key")?,
        )?
    } else {
        HeaderFrame::from_uncompressed(commit_header, header_bytes, descriptor_bytes, 3)?
    };

    if reuse_empty_tail && let Some(tail) = find_reusable_empty_tail(archive_path, passwords)? {
        if tail.commit_id != new_commit_id {
            return Err("last archive commit changed while preparing header commit".into());
        }
        return replace_empty_tail_with_header(archive_path, tail, &header, limits);
    }

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

fn replace_empty_tail_with_header(
    archive_path: &Path,
    tail: ReusableEmptyTail,
    header: &HeaderFrame,
    limits: CommitValidationLimits,
) -> Result<(), Box<dyn std::error::Error>> {
    let parent = archive_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let id = TEMP_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let unique_id = unique_temp_file_id()?;
    let stem = archive_path
        .file_name()
        .ok_or("archive path has no filename")?
        .to_string_lossy();
    let staged_archive = parent.join(format!(".{stem}.zarc-reuse-{unique_id}-{id}.tmp"));
    let mut cleanup = TemporaryFiles(vec![staged_archive.clone()]);
    let mut input = File::open(archive_path)?;
    let mut staged = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged_archive)?;
    let mut limited_input = Read::by_ref(&mut input).take(tail.link_position);
    std::io::copy(&mut limited_input, &mut staged)?;
    staged.write_all(&0u64.to_le_bytes())?;
    let header_offset = staged.stream_position()?;
    header.write_to(&mut staged)?;
    staged.write_all(&header_offset.to_le_bytes())?;
    staged.flush()?;
    drop(staged);

    if tail.is_root {
        let mut staged = OpenOptions::new().write(true).open(&staged_archive)?;
        staged.seek(SeekFrom::Start(4))?;
        staged.write_all(&header_offset.to_le_bytes())?;
        staged.flush()?;
    }

    let mut validation = File::open(&staged_archive)?;
    let mut header_offset = read_first_header_offset(&mut validation)?;
    while header_offset != 0 {
        let (commit_header, _, next_header_offset) =
            read_commit_at_offset(&mut validation, header_offset, limits)?;
        ValidatedCommit::read_body(&mut validation, commit_header, limits)?;
        header_offset = next_header_offset;
    }
    drop(validation);
    drop(input);
    std::fs::rename(&staged_archive, archive_path)?;
    cleanup.0.clear();
    Ok(())
}

fn load_toc_for_removal(
    archive_path: &Path,
    passwords: &PasswordSet,
) -> Result<(Toc, [u8; 4]), Box<dyn std::error::Error>> {
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
        let (validated, dek) = read_header_with_passwords(&mut file, limits, passwords)?;
        if validated.logical_header.is_none()
            || (validated.encryption.is_data_encrypted() && dek.is_none())
        {
            return Err("removing entries requires passwords for all encrypted commits".into());
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
