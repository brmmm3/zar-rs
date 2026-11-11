use super::common::*;
use crate::{
    ArchivePath, BLOCK_FLAG_ENCRYPTED, BLOCK_FLAG_UNCOMPRESSED, BlockFrame, CommitValidationLimits,
    ContainerHeader, EntryMetadata, Toc, TocChange, ValidatedCommit,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArchiveEntryDetails {
    /// Estimated using the arithmetic mean of the file's block compression ratios.
    pub compressed_size: Option<u64>,
    /// Compressed-to-uncompressed ratio, before rounding the estimated byte count.
    pub compression_ratio: Option<f64>,
    pub owner: Option<u32>,
    pub group: Option<u32>,
    pub permissions: Option<u32>,
    pub metadata_platform: Option<String>,
    pub modified_unix_millis: Option<i64>,
}

pub fn list_archive_with_details(
    path: &str,
    to: Option<&str>,
    password: Option<&[u8]>,
) -> Result<(Toc, BTreeMap<ArchivePath, ArchiveEntryDetails>), Box<dyn std::error::Error>> {
    let passwords = PasswordSet::new(password.map(<[u8]>::to_vec));
    list_archive_with_details_and_passwords(path, to, &passwords)
}

pub fn list_archive_with_details_and_passwords(
    path: &str,
    to: Option<&str>,
    passwords: &PasswordSet,
) -> Result<(Toc, BTreeMap<ArchivePath, ArchiveEntryDetails>), Box<dyn std::error::Error>> {
    let mut file = File::open(path)?;
    let container = ContainerHeader::read_from(&mut file)?;
    let limits = CommitValidationLimits::default();
    let mut toc = Toc::default();
    let mut details = BTreeMap::new();
    let mut header_offset = container.first_header_offset;
    let requested_id = to.map(parse_commit_id).transpose()?;
    let mut matching_commits = 0_u32;
    let mut stopped_at_to = false;

    while header_offset != 0 {
        let (commit_header, data_start, next_header_offset) =
            read_commit_at_offset(&mut file, header_offset, limits)?;
        if requested_id == Some(commit_header.commit_id) {
            matching_commits += 1;
            if matching_commits > 1 {
                return Err("--to commit ID is ambiguous".into());
            }
        }
        if stopped_at_to {
            header_offset = next_header_offset;
            continue;
        }
        file.seek(SeekFrom::Start(header_offset))?;
        let (validated, key) = read_header_with_passwords(&mut file, limits, passwords)?;
        let header = validated.logical_header;
        if header.is_none() && !validated.encryption.is_header_encrypted() {
            return Err("commit header is missing its logical header".into());
        }

        file.seek(SeekFrom::Start(data_start))?;
        let mut block_sizes = HashMap::new();
        let mut remaining = validated.commit_header.data_section_size;
        let mut blocks_seen = 0_u64;
        while remaining > 0 {
            if blocks_seen >= limits.max_blocks {
                return Err(crate::FramingError::InvalidSize.into());
            }
            let before = file.stream_position()?;
            let metadata =
                BlockFrame::read_metadata_from(&mut file, limits.max_frame_payload_size)?;
            let consumed = file
                .stream_position()?
                .checked_sub(before)
                .ok_or("block offset underflow")?;
            if consumed == 0 || consumed > remaining || consumed != metadata.encoded_size {
                return Err(crate::FramingError::InvalidSize.into());
            }
            let uncompressed_size = if metadata.flags & BLOCK_FLAG_ENCRYPTED != 0 {
                if let Some(key) = &key {
                    let algorithm = encryption_algorithm(&validated.encryption)
                        .ok_or("missing encryption algorithm")?;
                    file.seek(SeekFrom::Start(before))?;
                    let frame = BlockFrame::read_from(&mut file, limits.max_frame_payload_size)?;
                    Some(
                        frame
                            .uncompressed_encrypted(
                                &validated.commit_header,
                                key,
                                algorithm,
                                limits.max_uncompressed_block_size,
                            )?
                            .len() as u64,
                    )
                } else {
                    None
                }
            } else if metadata.flags & BLOCK_FLAG_UNCOMPRESSED != 0 {
                if metadata.payload_size as usize > limits.max_uncompressed_block_size {
                    return Err(crate::FramingError::InvalidSize.into());
                }
                Some(u64::from(metadata.payload_size))
            } else {
                let end = file.stream_position()?;
                file.seek(SeekFrom::Start(end - u64::from(metadata.payload_size)))?;
                let mut prefix = [0_u8; 18];
                let length = usize::try_from(metadata.payload_size)?.min(prefix.len());
                file.read_exact(&mut prefix[..length])?;
                let size = match zstd::zstd_safe::get_frame_content_size(&prefix[..length]) {
                    Ok(Some(size)) => size,
                    _ => {
                        file.seek(SeekFrom::Start(before))?;
                        let frame =
                            BlockFrame::read_from(&mut file, limits.max_frame_payload_size)?;
                        frame
                            .uncompressed_at_most(limits.max_uncompressed_block_size)?
                            .len() as u64
                    }
                };
                file.seek(SeekFrom::Start(end))?;
                if size > limits.max_uncompressed_block_size as u64 {
                    return Err(crate::FramingError::InvalidSize.into());
                }
                Some(size)
            };
            if block_sizes
                .insert(
                    metadata.block_index,
                    (u64::from(metadata.payload_size), uncompressed_size),
                )
                .is_some()
            {
                return Err(crate::FramingError::InvalidSize.into());
            }
            remaining -= consumed;
            blocks_seen += 1;
        }

        let block_indices = block_sizes.keys().copied().collect::<HashSet<_>>();
        if let Some(header) = header {
            toc.apply_commit_with_available_blocks(&header.changes, &block_indices)?;
            apply_entry_details(
                &mut details,
                &header.changes,
                &header.entry_metadata,
                &block_sizes,
            )?;
        }
        if requested_id == Some(validated.commit_header.commit_id) {
            stopped_at_to = true;
        }
        header_offset = next_header_offset;
    }

    if requested_id.is_some() && matching_commits == 0 {
        return Err("--to commit ID was not found".into());
    }

    Ok((toc, details))
}

fn apply_entry_details(
    details: &mut BTreeMap<ArchivePath, ArchiveEntryDetails>,
    changes: &[TocChange],
    metadata: &[EntryMetadata],
    block_sizes: &HashMap<u32, (u64, Option<u64>)>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Later records win, matching the previous reverse linear search.
    let metadata: HashMap<&ArchivePath, &EntryMetadata> =
        metadata.iter().map(|entry| (&entry.path, entry)).collect();
    for change in changes {
        match change {
            TocChange::PutFile { path, entry } => {
                let mut ratios = Vec::with_capacity(entry.block_indices.len());
                for index in &entry.block_indices {
                    let (compressed, uncompressed) =
                        block_sizes.get(index).ok_or("missing block frame size")?;
                    match uncompressed {
                        Some(0) => return Err("file references an empty block".into()),
                        Some(size) => ratios.push(*compressed as f64 / *size as f64),
                        None => break,
                    }
                }
                let ratio = if entry.size == 0 {
                    None
                } else if ratios.len() == entry.block_indices.len() && !ratios.is_empty() {
                    Some(ratios.iter().sum::<f64>() / ratios.len() as f64)
                } else {
                    None
                };
                let compressed_size = if entry.size == 0 {
                    Some(0)
                } else if let Some(ratio) = ratio {
                    let estimate = (ratio * entry.size as f64).round();
                    if !estimate.is_finite() || estimate >= u64::MAX as f64 {
                        return Err("file compressed size overflow".into());
                    }
                    Some(estimate as u64)
                } else {
                    None
                };
                details.insert(
                    path.clone(),
                    entry_details(path, &metadata, compressed_size, ratio),
                );
            }
            TocChange::PutDirectory { path }
            | TocChange::PutSymlink { path, .. }
            | TocChange::PutNamedPipe { path }
            | TocChange::PutDevice { path, .. } => {
                details.insert(path.clone(), entry_details(path, &metadata, None, None));
            }
            TocChange::RemoveFile { path } => {
                details.remove(path);
            }
            TocChange::RemoveDirectory { path } => {
                details.retain(|entry_path, _| {
                    entry_path != path
                        && !entry_path
                            .as_str()
                            .strip_prefix(path.as_str())
                            .is_some_and(|suffix| suffix.starts_with('/'))
                });
            }
        }
    }
    Ok(())
}

fn entry_details(
    path: &ArchivePath,
    metadata: &HashMap<&ArchivePath, &EntryMetadata>,
    compressed_size: Option<u64>,
    compression_ratio: Option<f64>,
) -> ArchiveEntryDetails {
    let entry_metadata = metadata.get(path).copied();
    ArchiveEntryDetails {
        compressed_size,
        compression_ratio,
        owner: entry_metadata.and_then(|entry| entry.owner),
        group: entry_metadata.and_then(|entry| entry.group),
        permissions: entry_metadata.and_then(|entry| entry.permissions),
        metadata_platform: entry_metadata.map(|entry| entry.platform.clone()),
        modified_unix_millis: entry_metadata.and_then(|entry| entry.modified_unix_millis),
    }
}

pub fn load_toc(file: &mut File) -> Result<Toc, Box<dyn std::error::Error>> {
    let container = ContainerHeader::read_from(file)?;
    let limits = CommitValidationLimits::default();
    let mut toc = Toc::default();
    let mut header_offset = container.first_header_offset;
    while header_offset != 0 {
        let (commit_header, _data_start, next_header_offset) =
            read_commit_at_offset(file, header_offset, limits)?;
        let validated = ValidatedCommit::read_body(file, commit_header, limits)?;
        if let Some(header) = validated.logical_header {
            toc.apply_commit_with_available_blocks(&header.changes, &validated.block_indices)?;
        } else {
            return Err("cannot build archive TOC from a header-encrypted commit".into());
        }
        header_offset = next_header_offset;
    }
    Ok(toc)
}

pub fn list_archive(
    path: &str,
    to: Option<&str>,
    password: Option<&[u8]>,
) -> Result<Toc, Box<dyn std::error::Error>> {
    let passwords = PasswordSet::new(password.map(<[u8]>::to_vec));
    list_archive_with_passwords(path, to, &passwords)
}

pub fn list_archive_with_passwords(
    path: &str,
    to: Option<&str>,
    passwords: &PasswordSet,
) -> Result<Toc, Box<dyn std::error::Error>> {
    load_toc_through_with_passwords(path, to, passwords)
}

pub fn load_toc_through(
    archive: &str,
    to: Option<&str>,
    password: Option<&[u8]>,
) -> Result<Toc, Box<dyn std::error::Error>> {
    let passwords = PasswordSet::new(password.map(<[u8]>::to_vec));
    load_toc_through_with_passwords(archive, to, &passwords)
}

/// Builds the visible archive state. Commits with encrypted headers are skipped
/// when their passwords are unavailable; later removals of entries from those
/// commits are treated as removals of unknown paths.
pub fn load_toc_through_with_passwords(
    archive: &str,
    to: Option<&str>,
    passwords: &PasswordSet,
) -> Result<Toc, Box<dyn std::error::Error>> {
    load_toc_through_with_passwords_and_progress(archive, to, passwords, None)
}

pub fn load_toc_through_with_progress(
    archive: &str,
    to: Option<&str>,
    password: Option<&[u8]>,
    progress: Option<&ProgressReporter<'_>>,
) -> Result<Toc, Box<dyn std::error::Error>> {
    let passwords = PasswordSet::new(password.map(<[u8]>::to_vec));
    load_toc_through_with_passwords_and_progress(archive, to, &passwords, progress)
}

pub fn load_toc_through_with_passwords_and_progress(
    archive: &str,
    to: Option<&str>,
    passwords: &PasswordSet,
    progress: Option<&ProgressReporter<'_>>,
) -> Result<Toc, Box<dyn std::error::Error>> {
    let mut file = File::open(archive)?;
    let container = ContainerHeader::read_from(&mut file)?;
    let requested_id = to.map(parse_commit_id).transpose()?;
    let mut toc = Toc::default();
    let mut matches = 0_u32;
    let mut stopped_at_to = false;
    let limits = CommitValidationLimits::default();

    let mut header_offset = container.first_header_offset;
    while header_offset != 0 {
        if let Some(progress) = progress {
            progress.check_cancelled()?;
        }
        let (commit_header, _data_start, next_header_offset) =
            read_commit_at_offset(&mut file, header_offset, limits)?;
        if requested_id == Some(commit_header.commit_id) {
            matches += 1;
            if matches > 1 {
                return Err("--to commit ID is ambiguous".into());
            }
        }
        if stopped_at_to {
            header_offset = next_header_offset;
            continue;
        }
        let body_start = file.stream_position()?;
        file.seek(SeekFrom::Start(header_offset))?;
        let (validated, _) = read_header_with_passwords(&mut file, limits, passwords)?;
        if let Some(progress) = progress {
            let header_end = file.stream_position()?;
            progress.inc(header_end.saturating_sub(body_start));
            progress.set_message(format!(
                "Reading header for commit {:08x}",
                u32::from_le_bytes(validated.commit_header.commit_id)
            ));
        }
        if let Some(header) = validated.logical_header.as_ref() {
            toc.apply_commit(&header.changes)?;
        } else if !validated.encryption.is_header_encrypted() {
            return Err("commit header is missing its logical header".into());
        }
        if requested_id == Some(validated.commit_header.commit_id) {
            stopped_at_to = true;
        }
        header_offset = next_header_offset;
    }

    if requested_id.is_some() && matches == 0 {
        return Err("--to commit ID was not found".into());
    }
    Ok(toc)
}
