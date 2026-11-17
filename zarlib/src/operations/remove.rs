use super::common::*;
use super::pipeline::archive_prefix;
use crate::{
    ArchivePath, CommitHeader, CommitValidationLimits, ContainerHeader, ContentAead,
    EncryptionDescriptor, EncryptionScope, EntryMetadata, HEADER_OFFSET_SIZE, HeaderFrame,
    LogicalHeader, Toc, TocChange, TocEntry, ValidatedCommit, generate_dek, generate_nonce,
    wrap_dek_with_password,
};
use std::collections::{BTreeMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

/// Renames an archive entry by appending a commit without rewriting its data.
pub fn rename_archive_entry(
    archive: &str,
    from: &ArchivePath,
    to: &ArchivePath,
) -> Result<(), Box<dyn std::error::Error>> {
    rename_archive_entry_with_encryption(archive, from, to, None, &PasswordSet::default())
}

/// Renames an entry using passwords for the existing history and optional commit encryption.
pub fn rename_archive_entry_with_encryption(
    archive: &str,
    from: &ArchivePath,
    to: &ArchivePath,
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
    passwords: &PasswordSet,
) -> Result<(), Box<dyn std::error::Error>> {
    rename_entries_in_archives(
        None,
        archive,
        &[(from.clone(), to.clone())],
        encryption,
        passwords,
    )
}

/// Moves or renames multiple entries in one archive commit.
pub fn rename_archive_entries_with_encryption(
    archive: &str,
    paths: &[(ArchivePath, ArchivePath)],
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
    passwords: &PasswordSet,
) -> Result<(), Box<dyn std::error::Error>> {
    rename_entries_in_archives(None, archive, paths, encryption, passwords)
}

/// Renames an entry in an overlay against the combined base and overlay state.
pub fn rename_overlay_entry_with_encryption(
    base: &str,
    overlay: &str,
    from: &ArchivePath,
    to: &ArchivePath,
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
    passwords: &PasswordSet,
) -> Result<(), Box<dyn std::error::Error>> {
    rename_overlay_entries_with_encryption(
        base,
        overlay,
        &[(from.clone(), to.clone())],
        encryption,
        passwords,
    )
}

/// Moves or renames multiple entries in one overlay commit.
pub fn rename_overlay_entries_with_encryption(
    base: &str,
    overlay: &str,
    paths: &[(ArchivePath, ArchivePath)],
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
    passwords: &PasswordSet,
) -> Result<(), Box<dyn std::error::Error>> {
    rename_entries_in_archives(Some(base), overlay, paths, encryption, passwords)
}

fn rename_entries_in_archives(
    base: Option<&str>,
    archive: &str,
    paths: &[(ArchivePath, ArchivePath)],
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
    passwords: &PasswordSet,
) -> Result<(), Box<dyn std::error::Error>> {
    if paths.is_empty() {
        return Err("no archive entries to move".into());
    }
    let mut toc = Toc::default();
    let mut metadata = BTreeMap::new();
    for path in base.into_iter().chain(std::iter::once(archive)) {
        read_rename_state(path, passwords, &mut toc, &mut metadata)?;
    }
    let mut changes = Vec::new();
    let mut moved = Vec::new();
    let mut originals = HashSet::new();
    let mut destinations = HashSet::new();
    for (from, to) in paths {
        if from == to
            || to
                .as_str()
                .strip_prefix(from.as_str())
                .is_some_and(|suffix| suffix.starts_with('/'))
        {
            return Err("rename destination must be different and outside the source".into());
        }
        let source = toc.get(from);
        let implicit_directory = source.is_none()
            && toc.iter().any(|(path, _)| {
                path.as_str()
                    .strip_prefix(from.as_str())
                    .is_some_and(|suffix| suffix.starts_with('/'))
            });
        if source.is_none() && !implicit_directory {
            return Err(format!("path not found in archive: {from}").into());
        }
        let directory = implicit_directory || matches!(source, Some(TocEntry::Directory));
        if implicit_directory {
            changes.push(TocChange::PutDirectory { path: from.clone() });
        }
        changes.push(if directory {
            TocChange::RemoveDirectory { path: from.clone() }
        } else {
            TocChange::RemoveFile { path: from.clone() }
        });
        if implicit_directory {
            if toc.get(to).is_some() || !destinations.insert(to.clone()) {
                return Err(format!("rename destination already exists: {to}").into());
            }
            moved.push((from.clone(), to.clone(), TocEntry::Directory));
        }
        for (path, entry) in toc.iter() {
            let suffix = if path == from {
                ""
            } else if directory {
                match path.as_str().strip_prefix(from.as_str()) {
                    Some(suffix) if suffix.starts_with('/') => suffix,
                    _ => continue,
                }
            } else {
                continue;
            };
            let destination = ArchivePath::new(format!("{}{suffix}", to.as_str()))?;
            if toc.get(&destination).is_some() || !destinations.insert(destination.clone()) {
                return Err(format!("rename destination already exists: {destination}").into());
            }
            if !originals.insert(path.clone()) {
                return Err(format!("overlapping source paths: {path}").into());
            }
            moved.push((path.clone(), destination, entry.clone()));
        }
    }
    let mut moved_metadata = Vec::new();
    for (original, destination, entry) in moved {
        let change = match entry {
            TocEntry::File(mut entry) => {
                if entry.size > 0 {
                    entry.source_commit = Some(
                        toc.file_origin(&original)
                            .ok_or("file data origin is unavailable")?,
                    );
                }
                TocChange::PutFile {
                    path: destination.clone(),
                    entry,
                }
            }
            TocEntry::Directory => TocChange::PutDirectory {
                path: destination.clone(),
            },
            TocEntry::Symlink { target } => TocChange::PutSymlink {
                path: destination.clone(),
                target,
            },
            TocEntry::NamedPipe => TocChange::PutNamedPipe {
                path: destination.clone(),
            },
            TocEntry::Device { kind, major, minor } => TocChange::PutDevice {
                path: destination.clone(),
                kind,
                major,
                minor,
            },
        };
        if let Some(mut item) = metadata.remove(&original) {
            item.path = destination;
            moved_metadata.push(item);
        }
        changes.push(change);
    }
    let archive_path = Path::new(archive);
    let reusable_tail = find_reusable_empty_tail(archive_path, passwords)?;
    let commit_id = match reusable_tail {
        Some(tail) => {
            toc.forget_empty_commit(tail.commit_id)?;
            tail.commit_id
        }
        None => next_commit_id(archive_path)?,
    };
    toc.apply_archive_commit(commit_id, &changes)?;
    let header = LogicalHeader::new(changes)
        .with_entry_metadata(moved_metadata)
        .with_metadata(commit_metadata(None)?)
        .encode()?;
    append_header_only_commit_with_id_and_encryption(
        archive_path,
        &header,
        commit_id,
        true,
        encryption,
        passwords,
    )
}

fn read_rename_state(
    archive: &str,
    passwords: &PasswordSet,
    toc: &mut Toc,
    metadata: &mut BTreeMap<ArchivePath, EntryMetadata>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::open(archive)?;
    let limits = CommitValidationLimits::default();
    let mut offset = read_first_header_offset(&mut file)?;
    let mut offsets = HashSet::new();
    while offset != 0 {
        if !offsets.insert(offset) {
            return Err("archive commit chain contains a cycle".into());
        }
        let (commit, _, next) = read_commit_at_offset(&mut file, offset, limits)?;
        file.seek(SeekFrom::Start(offset))?;
        let (validated, key) = read_header_with_passwords(&mut file, limits, passwords)?;
        if validated.encryption.is_data_encrypted() && key.is_none() {
            return Err("renaming entries requires passwords for all encrypted commits".into());
        }
        let header = validated
            .logical_header
            .ok_or("renaming entries requires passwords for all encrypted commits")?;
        toc.apply_archive_commit(commit.commit_id, &header.changes)?;
        for change in &header.changes {
            let path = match change {
                TocChange::PutFile { path, .. }
                | TocChange::PutDirectory { path }
                | TocChange::PutSymlink { path, .. }
                | TocChange::PutNamedPipe { path }
                | TocChange::PutDevice { path, .. }
                | TocChange::RemoveFile { path }
                | TocChange::RemoveDirectory { path } => path,
            };
            metadata.remove(path);
        }
        metadata.retain(|path, _| toc.get(path).is_some());
        for item in header.entry_metadata {
            metadata.insert(item.path.clone(), item);
        }
        offset = next;
    }
    Ok(())
}

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
    remove_archive_entries_with_initial_toc(archive, paths, progress, encryption, passwords, None)
}

/// Adds removals to an overlay, checking paths against the base and overlay together.
pub fn remove_overlay_entries_with_progress_and_encryption(
    base: &str,
    overlay: &str,
    paths: &[ArchivePath],
    progress: Option<&dyn Progress>,
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
    passwords: &PasswordSet,
) -> Result<(), Box<dyn std::error::Error>> {
    let (base_toc, _) = load_toc_for_removal(Path::new(base), passwords, Toc::default())?;
    remove_archive_entries_with_initial_toc(
        overlay,
        paths,
        progress,
        encryption,
        passwords,
        Some(base_toc),
    )
}

fn remove_archive_entries_with_initial_toc(
    archive: &str,
    paths: &[ArchivePath],
    progress: Option<&dyn Progress>,
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
    passwords: &PasswordSet,
    initial_toc: Option<Toc>,
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
    let (mut toc, next_commit_id) =
        load_toc_for_removal(archive_path, passwords, initial_toc.unwrap_or_default())?;
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
    mut toc: Toc,
) -> Result<(Toc, [u8; 4]), Box<dyn std::error::Error>> {
    let mut file = File::open(archive_path)?;
    let limits = CommitValidationLimits::default();
    let container = ContainerHeader::read_from(&mut file)?;
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
