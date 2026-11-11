use super::common::*;
use crate::{
    BLOCK_FLAG_ENCRYPTED, BlockFrame, CommitValidationLimits, ContainerHeader, ContentAead,
    FileEntry, FileGroupKind, HEADER_OFFSET_SIZE, LogicalHeader, MetadataType, TocChange,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

#[derive(Debug, Clone, PartialEq)]
pub struct HeaderCompressionStats {
    pub compressed_size: u64,
    pub uncompressed_size: Option<u64>,
    pub ratio_percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommitInfo {
    pub id: [u8; 4],
    pub size: u64,
    pub header_size: u32,
    pub data_size: u64,
    pub uncompressed_size: Option<u64>,
    pub ratio_percent: Option<f64>,
    pub header_compression: Option<HeaderCompressionStats>,
    pub header_changes: Option<HeaderChangeStats>,
    pub created_unix_millis: Option<i64>,
    pub user: Option<String>,
    pub details: Option<CommitFileStats>,
    pub header_encrypted: bool,
    pub data_encrypted: bool,
    pub encryption_algorithm: Option<ContentAead>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockInfo {
    pub index: u32,
    pub compressed_size: u64,
    pub ratio_percent: Option<f64>,
    pub file_count: u64,
    pub file_groups: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommitInfoWithBlocks {
    pub commit: CommitInfo,
    pub blocks: Vec<BlockInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileGroupStats {
    pub files: u64,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub has_encrypted_blocks: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitFileStats {
    pub groups: BTreeMap<&'static str, FileGroupStats>,
    pub header_changes: HeaderChangeStats,
    pub deduplicated_files: u64,
    pub deduplicated_uncompressed_size: u64,
    pub symlinks: u64,
    pub named_pipes: u64,
    pub devices: u64,
    pub entry_metadata: EntryMetadataStats,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeaderChangeStats {
    pub files_added_or_updated: u64,
    pub directories_added_or_updated: u64,
    pub symlinks_added_or_updated: u64,
    pub named_pipes_added_or_updated: u64,
    pub devices_added_or_updated: u64,
    pub files_removed: u64,
    pub directories_removed: u64,
}

/// Number of entries in a commit header that carry each kind of saved metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntryMetadataStats {
    pub owner: u64,
    pub group: u64,
    pub permissions: u64,
    pub modification_times: u64,
    pub creation_times: u64,
    pub access_times: u64,
}

pub fn info_archive(
    path: &str,
    verbose: bool,
) -> Result<Vec<CommitInfo>, Box<dyn std::error::Error>> {
    Ok(
        read_archive_info(path, verbose, false, &PasswordSet::default())?
            .into_iter()
            .map(|details| details.commit)
            .collect(),
    )
}

/// Like [`info_archive`], with per-block compression details when `verbose` is true.
pub fn info_archive_with_blocks(
    path: &str,
    verbose: bool,
) -> Result<Vec<CommitInfoWithBlocks>, Box<dyn std::error::Error>> {
    info_archive_with_blocks_and_passwords(path, verbose, &PasswordSet::default())
}

/// Like [`info_archive_with_blocks`], with encrypted commit headers unlocked by `passwords`.
pub fn info_archive_with_blocks_and_passwords(
    path: &str,
    verbose: bool,
    passwords: &PasswordSet,
) -> Result<Vec<CommitInfoWithBlocks>, Box<dyn std::error::Error>> {
    read_archive_info(path, verbose, true, passwords)
}

fn read_archive_info(
    path: &str,
    verbose: bool,
    include_block_details: bool,
    passwords: &PasswordSet,
) -> Result<Vec<CommitInfoWithBlocks>, Box<dyn std::error::Error>> {
    let mut file = File::open(path)?;
    let container = ContainerHeader::read_from(&mut file)?;
    let limits = CommitValidationLimits::default();
    let mut commits = Vec::<CommitInfoWithBlocks>::new();
    let mut header_offset = container.first_header_offset;
    while header_offset != 0 {
        let (commit_header, data_start, next_header_offset) =
            read_commit_at_offset(&mut file, header_offset, limits)?;
        let mut details = None;
        let mut header_compression = None;
        let mut blocks = Vec::new();
        let commit_size = header_offset
            .checked_add(u64::from(commit_header.header_frame_size))
            .and_then(|size| size.checked_add(HEADER_OFFSET_SIZE))
            .and_then(|size| size.checked_sub(data_start))
            .and_then(|size| size.checked_sub(HEADER_OFFSET_SIZE))
            .ok_or("commit size overflow")?;
        file.seek(SeekFrom::Start(header_offset))?;
        let (validated, _) = read_header_with_passwords(&mut file, limits, passwords)?;
        let uncompressed_size = validated
            .logical_header
            .as_ref()
            .map(uncompressed_commit_size)
            .transpose()?;
        let header_encrypted = validated.encryption.is_header_encrypted();
        let header_changes = validated
            .logical_header
            .as_ref()
            .map(summarize_header_changes);
        let created_unix_millis = validated
            .logical_header
            .as_ref()
            .and_then(|header| header.metadata.get(COMMIT_CREATED_KEY))
            .and_then(|value| match value {
                MetadataType::I64(value) => Some(*value),
                _ => None,
            });
        let user = validated
            .logical_header
            .as_ref()
            .and_then(|header| header.metadata.get(COMMIT_USER_KEY))
            .and_then(|value| match value {
                MetadataType::String(value) => Some(value.clone()),
                _ => None,
            });
        let data_encrypted = validated.encryption.is_data_encrypted();
        let content_algorithm = encryption_algorithm(&validated.encryption);
        let ratio_percent = uncompressed_size.map(|size| {
            if size == 0 {
                0.0
            } else {
                commit_size as f64 * 100.0 / size as f64
            }
        });
        if verbose {
            header_compression = Some(HeaderCompressionStats {
                compressed_size: validated.header_compressed_size,
                uncompressed_size: validated.header_uncompressed_size,
                ratio_percent: validated.header_uncompressed_size.map(|size| {
                    if size == 0 {
                        0.0
                    } else {
                        validated.header_compressed_size as f64 * 100.0 / size as f64
                    }
                }),
            });
        }
        if verbose {
            let logical_header = validated.logical_header.as_ref();
            if include_block_details {
                blocks = scan_commit_blocks(
                    &mut file,
                    data_start,
                    commit_header.data_section_size,
                    logical_header,
                    limits,
                )?;
            }
            if let Some(header) = logical_header {
                details = Some(if has_stored_group_sizes(header) {
                    summarize_commit_header_only(header, None)?
                } else {
                    let block_sizes = if include_block_details {
                        blocks
                            .iter()
                            .map(|block| (block.index, block.compressed_size))
                            .collect::<HashMap<_, _>>()
                    } else {
                        scan_block_sizes(
                            &mut file,
                            data_start,
                            commit_header.data_section_size,
                            limits,
                        )?
                    };
                    summarize_commit_header_only(header, Some(&block_sizes))?
                });
            }
        }
        commits.push(CommitInfoWithBlocks {
            commit: CommitInfo {
                id: commit_header.commit_id,
                size: commit_size,
                header_size: commit_header.header_frame_size,
                data_size: commit_header.data_section_size,
                uncompressed_size,
                ratio_percent,
                header_compression,
                header_changes,
                created_unix_millis,
                user,
                details,
                header_encrypted,
                data_encrypted,
                encryption_algorithm: content_algorithm,
            },
            blocks,
        });
        header_offset = next_header_offset;
    }
    Ok(commits)
}

fn uncompressed_commit_size(header: &LogicalHeader) -> Result<u64, Box<dyn std::error::Error>> {
    header.changes.iter().try_fold(0_u64, |total, change| {
        let file_size = match change {
            TocChange::PutFile { entry, .. } => entry.size,
            _ => 0,
        };
        total
            .checked_add(file_size)
            .ok_or_else(|| "uncompressed commit size overflow".into())
    })
}

const GROUP_SIZE_KEY_PREFIX: &str = "zar.group.compressed_size.";

fn has_stored_group_sizes(header: &LogicalHeader) -> bool {
    header
        .metadata
        .keys()
        .any(|key| key.starts_with(GROUP_SIZE_KEY_PREFIX))
}

fn stored_group_size(header: &LogicalHeader, group: &str) -> Option<u64> {
    match header
        .metadata
        .get(&format!("{GROUP_SIZE_KEY_PREFIX}{group}"))
    {
        Some(MetadataType::I64(size)) => u64::try_from(*size).ok(),
        _ => None,
    }
}

/// Builds the header metadata holding the compressed size of every file group,
/// so that `info -v` does not have to scan the data section.
pub fn group_size_metadata(
    header: &LogicalHeader,
    block_sizes: &HashMap<u32, u64>,
) -> Result<HashMap<String, MetadataType>, Box<dyn std::error::Error>> {
    let stats = summarize_commit_header_only(header, Some(block_sizes))?;
    stats
        .groups
        .iter()
        .map(|(name, group)| {
            let size = i64::try_from(group.compressed_size)
                .map_err(|_| Box::<dyn std::error::Error>::from("group size overflow"))?;
            Ok((
                format!("{GROUP_SIZE_KEY_PREFIX}{name}"),
                MetadataType::I64(size),
            ))
        })
        .collect()
}

fn scan_commit_blocks(
    file: &mut File,
    data_start: u64,
    data_section_size: u64,
    header: Option<&LogicalHeader>,
    limits: CommitValidationLimits,
) -> Result<Vec<BlockInfo>, Box<dyn std::error::Error>> {
    let mut groups_by_block = HashMap::<u32, BTreeSet<&'static str>>::new();
    let mut files_by_block = HashMap::<u32, u64>::new();
    if let Some(header) = header {
        for change in &header.changes {
            if let TocChange::PutFile { entry, .. } = change {
                let group = file_group_name(header, entry);
                for &index in &entry.block_indices {
                    groups_by_block.entry(index).or_default().insert(group);
                    let count = files_by_block.entry(index).or_default();
                    *count = count.checked_add(1).ok_or("block file count overflow")?;
                }
            }
        }
    }

    let mut blocks = Vec::new();
    let mut remaining = data_section_size;
    file.seek(SeekFrom::Start(data_start))?;
    while remaining > 0 {
        if blocks.len() as u64 >= limits.max_blocks {
            return Err(crate::FramingError::InvalidSize.into());
        }
        let (frame, consumed) = {
            let mut limited_data = file.take(remaining);
            let frame = BlockFrame::read_from(&mut limited_data, limits.max_frame_payload_size)?;
            let consumed = remaining - limited_data.limit();
            if consumed == 0 || frame.encoded_size()? as u64 != consumed {
                return Err(crate::FramingError::InvalidSize.into());
            }
            (frame, consumed)
        };
        remaining -= consumed;
        let uncompressed_size = if frame.flags & BLOCK_FLAG_ENCRYPTED == 0 {
            Some(
                u64::try_from(
                    frame
                        .uncompressed_at_most(limits.max_uncompressed_block_size)?
                        .len(),
                )
                .map_err(|_| "block uncompressed size overflow")?,
            )
        } else {
            None
        };
        let compressed_size =
            u64::try_from(frame.payload.len()).map_err(|_| "block compressed size overflow")?;
        let ratio_percent = uncompressed_size.map(|size| {
            if size == 0 {
                0.0
            } else {
                compressed_size as f64 * 100.0 / size as f64
            }
        });
        blocks.push(BlockInfo {
            index: frame.block_index,
            compressed_size,
            ratio_percent,
            file_count: files_by_block
                .remove(&frame.block_index)
                .unwrap_or_default(),
            file_groups: groups_by_block
                .remove(&frame.block_index)
                .unwrap_or_default()
                .into_iter()
                .collect(),
        });
    }
    Ok(blocks)
}

fn scan_block_sizes(
    file: &mut File,
    data_start: u64,
    data_section_size: u64,
    limits: CommitValidationLimits,
) -> Result<HashMap<u32, u64>, Box<dyn std::error::Error>> {
    let mut block_sizes = HashMap::new();
    let mut remaining = data_section_size;
    file.seek(SeekFrom::Start(data_start))?;
    while remaining > 0 {
        if block_sizes.len() as u64 >= limits.max_blocks {
            return Err(crate::FramingError::InvalidSize.into());
        }
        let metadata = BlockFrame::read_metadata_from(file, limits.max_frame_payload_size)?;
        if metadata.encoded_size == 0 || metadata.encoded_size > remaining {
            return Err(crate::FramingError::InvalidSize.into());
        }
        if block_sizes
            .insert(metadata.block_index, u64::from(metadata.payload_size))
            .is_some()
        {
            return Err(crate::FramingError::InvalidSize.into());
        }
        remaining -= metadata.encoded_size;
    }
    Ok(block_sizes)
}

/// Summarizes a commit from its header. Compressed group sizes come from
/// `block_sizes` when given, otherwise from the sizes stored in the header.
pub fn summarize_commit_header_only(
    header: &LogicalHeader,
    block_sizes: Option<&HashMap<u32, u64>>,
) -> Result<CommitFileStats, Box<dyn std::error::Error>> {
    let mut stats = CommitFileStats::default();
    let mut seen_files = std::collections::HashSet::new();
    let mut counted_blocks = std::collections::HashSet::new();
    stats.header_changes = summarize_header_changes(header);
    for entry in &header.entry_metadata {
        let counts = &mut stats.entry_metadata;
        counts.owner += u64::from(entry.owner.is_some());
        counts.group += u64::from(entry.group.is_some());
        counts.permissions += u64::from(entry.permissions.is_some());
        counts.modification_times += u64::from(entry.modified_unix_millis.is_some());
        counts.creation_times += u64::from(entry.created_unix_millis.is_some());
        counts.access_times += u64::from(entry.accessed_unix_millis.is_some());
    }
    for change in &header.changes {
        let entry = match change {
            TocChange::PutFile { entry, .. } => entry,
            TocChange::PutSymlink { .. } => {
                stats.symlinks += 1;
                continue;
            }
            TocChange::PutNamedPipe { .. } => {
                stats.named_pipes += 1;
                continue;
            }
            TocChange::PutDevice { .. } => {
                stats.devices += 1;
                continue;
            }
            _ => continue,
        };
        let name = file_group_name(header, entry);
        let group = stats.groups.entry(name).or_default();
        group.files += 1;
        let is_duplicate = entry.size > 0
            && !seen_files.insert((
                entry.size,
                entry.block_indices.as_slice(),
                entry.offset_in_block,
            ));
        if is_duplicate {
            stats.deduplicated_files += 1;
            stats.deduplicated_uncompressed_size = stats
                .deduplicated_uncompressed_size
                .checked_add(entry.size)
                .ok_or("deduplicated size overflow")?;
        } else {
            group.uncompressed_size = group
                .uncompressed_size
                .checked_add(entry.size)
                .ok_or("uncompressed group size overflow")?;
            if let Some(block_sizes) = block_sizes {
                for index in &entry.block_indices {
                    if counted_blocks.insert((name, *index)) {
                        group.compressed_size = group
                            .compressed_size
                            .checked_add(*block_sizes.get(index).ok_or("missing block frame size")?)
                            .ok_or("compressed group size overflow")?;
                    }
                }
            }
        }
    }
    if block_sizes.is_none() {
        for (name, group) in &mut stats.groups {
            group.compressed_size = stored_group_size(header, name).unwrap_or(0);
        }
    }
    Ok(stats)
}

fn summarize_header_changes(header: &LogicalHeader) -> HeaderChangeStats {
    let mut stats = HeaderChangeStats::default();
    for change in &header.changes {
        record_header_change(&mut stats, change);
    }
    stats
}

fn record_header_change(stats: &mut HeaderChangeStats, change: &TocChange) {
    match change {
        TocChange::PutFile { .. } => stats.files_added_or_updated += 1,
        TocChange::PutDirectory { .. } => stats.directories_added_or_updated += 1,
        TocChange::PutSymlink { .. } => stats.symlinks_added_or_updated += 1,
        TocChange::PutNamedPipe { .. } => stats.named_pipes_added_or_updated += 1,
        TocChange::PutDevice { .. } => stats.devices_added_or_updated += 1,
        TocChange::RemoveFile { .. } => stats.files_removed += 1,
        TocChange::RemoveDirectory { .. } => stats.directories_removed += 1,
    }
}

pub fn file_group_name(header: &LogicalHeader, entry: &FileEntry) -> &'static str {
    match header.block_size {
        Some(block_size) => match entry.group_kind(block_size) {
            FileGroupKind::EmptyFiles => "empty files",
            FileGroupKind::TinyFiles => "tiny files",
            FileGroupKind::SmallFiles => "small files",
            FileGroupKind::BigFiles => "big files",
        },
        None => match entry.size {
            0 => "empty files",
            1..=8 => "tiny files",
            _ => "files (block size unknown)",
        },
    }
}

pub fn summarize_commit_files(
    header: &LogicalHeader,
    blocks: &HashMap<u32, (u64, Option<u64>)>,
) -> Result<CommitFileStats, Box<dyn std::error::Error>> {
    let mut stats = CommitFileStats::default();
    let mut seen_files = std::collections::HashSet::new();
    let mut counted_blocks = std::collections::HashSet::new();
    stats.header_changes = summarize_header_changes(header);
    for change in &header.changes {
        let entry = match change {
            TocChange::PutFile { entry, .. } => entry,
            TocChange::PutSymlink { .. } => {
                stats.symlinks += 1;
                continue;
            }
            TocChange::PutNamedPipe { .. } => {
                stats.named_pipes += 1;
                continue;
            }
            TocChange::PutDevice { .. } => {
                stats.devices += 1;
                continue;
            }
            _ => continue,
        };
        let group_name = match header.block_size {
            Some(block_size) => match entry.group_kind(block_size) {
                FileGroupKind::EmptyFiles => "empty files",
                FileGroupKind::TinyFiles => "tiny files",
                FileGroupKind::SmallFiles => "small files",
                FileGroupKind::BigFiles => "big files",
            },
            None => match entry.size {
                0 => "empty files",
                1..=8 => "tiny files",
                _ => "files (block size unknown)",
            },
        };
        let group = stats.groups.entry(group_name).or_default();
        group.files += 1;
        let is_duplicate = entry.size > 0
            && !seen_files.insert((
                entry.size,
                entry.block_indices.clone(),
                entry.offset_in_block,
            ));
        if is_duplicate {
            stats.deduplicated_files += 1;
            stats.deduplicated_uncompressed_size = stats
                .deduplicated_uncompressed_size
                .checked_add(entry.size)
                .ok_or("deduplicated size overflow")?;
        } else {
            group.uncompressed_size = group
                .uncompressed_size
                .checked_add(entry.size)
                .ok_or("uncompressed group size overflow")?;
            for index in &entry.block_indices {
                let (compressed_size, uncompressed_size) = blocks
                    .get(index)
                    .ok_or_else(|| format!("file entry references unavailable block {index}"))?;
                if counted_blocks.insert((group_name, *index)) {
                    group.compressed_size = group
                        .compressed_size
                        .checked_add(*compressed_size)
                        .ok_or("compressed group size overflow")?;
                    if uncompressed_size.is_none() {
                        group.has_encrypted_blocks = true;
                    }
                }
            }
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::{EncodingOptions, create_archive_with_password};

    #[test]
    fn archive_info_can_unlock_encrypted_commit_headers() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("secret.txt");
        let archive = directory.path().join("encrypted.zar");
        std::fs::write(&input, b"secret contents").unwrap();
        create_archive_with_password(
            archive.to_str().unwrap(),
            &[input],
            false,
            EncodingOptions::default(),
            Some(b"correct password"),
        )
        .unwrap();

        let locked = info_archive_with_blocks(archive.to_str().unwrap(), true).unwrap();
        assert!(locked[0].commit.header_encrypted);
        assert!(locked[0].commit.details.is_none());

        let passwords = PasswordSet::new(Some(b"correct password".to_vec()));
        let unlocked =
            info_archive_with_blocks_and_passwords(archive.to_str().unwrap(), true, &passwords)
                .unwrap();
        assert!(unlocked[0].commit.header_encrypted);
        assert!(unlocked[0].commit.data_encrypted);
        assert!(unlocked[0].commit.details.is_some());

        let wrong_password = PasswordSet::new(Some(b"incorrect password".to_vec()));
        assert!(
            info_archive_with_blocks_and_passwords(
                archive.to_str().unwrap(),
                true,
                &wrong_password
            )
            .is_err()
        );
    }
}
