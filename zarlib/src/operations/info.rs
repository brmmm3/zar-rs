use super::common::*;
use crate::{
    BlockFrame, CommitValidationLimits, ContainerHeader, ContentAead, FileEntry, FileGroupKind,
    HEADER_OFFSET_SIZE, LogicalHeader, TocChange, ValidatedHeader,
};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{Seek, SeekFrom};

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
    pub details: Option<CommitFileStats>,
    pub header_encrypted: bool,
    pub data_encrypted: bool,
    pub encryption_algorithm: Option<ContentAead>,
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
    pub deduplicated_files: u64,
    pub deduplicated_uncompressed_size: u64,
    pub symlinks: u64,
    pub named_pipes: u64,
    pub devices: u64,
}

pub fn info_archive(
    path: &str,
    verbose: bool,
) -> Result<Vec<CommitInfo>, Box<dyn std::error::Error>> {
    let mut file = File::open(path)?;
    let container = ContainerHeader::read_from(&mut file)?;
    let limits = CommitValidationLimits::default();
    let mut commits = Vec::<CommitInfo>::new();
    let mut header_offset = container.first_header_offset;
    while header_offset != 0 {
        let (commit_header, data_start, next_header_offset) =
            read_commit_at_offset(&mut file, header_offset, limits)?;
        let mut details = None;
        let mut header_compression = None;
        let commit_size = header_offset
            .checked_add(u64::from(commit_header.header_frame_size))
            .and_then(|size| size.checked_add(HEADER_OFFSET_SIZE))
            .and_then(|size| size.checked_sub(data_start))
            .and_then(|size| size.checked_sub(HEADER_OFFSET_SIZE))
            .ok_or("commit size overflow")?;
        file.seek(SeekFrom::Start(header_offset))?;
        let validated = ValidatedHeader::read_header(&mut file, limits)?;
        let uncompressed_size = validated
            .logical_header
            .as_ref()
            .map(uncompressed_commit_size)
            .transpose()?;
        let header_encrypted = validated.logical_header.is_none();
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
            if let Some(header) = validated.logical_header.as_ref() {
                let mut block_sizes = HashMap::new();
                let mut remaining = commit_header.data_section_size;
                let mut blocks_seen = 0_u64;
                file.seek(SeekFrom::Start(data_start))?;
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
                    if block_sizes
                        .insert(metadata.block_index, u64::from(metadata.payload_size))
                        .is_some()
                    {
                        return Err(crate::FramingError::InvalidSize.into());
                    }
                    remaining -= consumed;
                    blocks_seen += 1;
                }
                details = Some(summarize_commit_header_only(header, &block_sizes)?);
            }
        }
        commits.push(CommitInfo {
            id: commit_header.commit_id,
            size: commit_size,
            header_size: commit_header.header_frame_size,
            data_size: commit_header.data_section_size,
            uncompressed_size,
            ratio_percent,
            header_compression,
            details,
            header_encrypted,
            data_encrypted,
            encryption_algorithm: content_algorithm,
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

pub fn summarize_commit_header_only(
    header: &LogicalHeader,
    block_frame_sizes: &HashMap<u32, u64>,
) -> Result<CommitFileStats, Box<dyn std::error::Error>> {
    let mut stats = CommitFileStats::default();
    let mut seen_files = std::collections::HashSet::new();
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
        let refs = entry.block_indices.clone();
        let is_duplicate = entry.size > 0 && !seen_files.insert((entry.size, refs.clone()));
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
            for index in refs {
                group.compressed_size = group
                    .compressed_size
                    .checked_add(
                        *block_frame_sizes
                            .get(&index)
                            .ok_or("missing block frame size")?,
                    )
                    .ok_or("compressed group size overflow")?;
            }
        }
    }
    Ok(stats)
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
        let is_duplicate =
            entry.size > 0 && !seen_files.insert((entry.size, entry.block_indices.clone()));
        if is_duplicate {
            stats.deduplicated_files += 1;
            stats.deduplicated_uncompressed_size = stats
                .deduplicated_uncompressed_size
                .checked_add(entry.size)
                .ok_or("deduplicated size overflow")?;
        } else {
            for index in &entry.block_indices {
                let (compressed_size, uncompressed_size) = blocks
                    .get(index)
                    .ok_or_else(|| format!("file entry references unavailable block {index}"))?;
                group.compressed_size = group
                    .compressed_size
                    .checked_add(*compressed_size)
                    .ok_or("compressed group size overflow")?;
                match uncompressed_size {
                    Some(size) => {
                        group.uncompressed_size = group
                            .uncompressed_size
                            .checked_add(*size)
                            .ok_or("uncompressed group size overflow")?;
                    }
                    None => group.has_encrypted_blocks = true,
                }
            }
        }
    }
    Ok(stats)
}
