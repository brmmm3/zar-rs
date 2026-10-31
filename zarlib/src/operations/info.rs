use super::common::*;
use crate::{
    BlockFrame, CommitValidationLimits, ContainerHeader, ContentAead, FileEntry, FileGroupKind,
    HEADER_OFFSET_SIZE, LogicalHeader, MetadataType, TocChange, ValidatedHeader,
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
                details = Some(if has_stored_group_sizes(header) {
                    summarize_commit_header_only(header, None)?
                } else {
                    // Archives written before group sizes were stored in the header.
                    let block_sizes = scan_block_sizes(
                        &mut file,
                        data_start,
                        commit_header.data_section_size,
                        limits,
                    )?;
                    summarize_commit_header_only(header, Some(&block_sizes))?
                });
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
        let is_duplicate =
            entry.size > 0 && !seen_files.insert((entry.size, entry.block_indices.as_slice()));
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
                    group.compressed_size = group
                        .compressed_size
                        .checked_add(*block_sizes.get(index).ok_or("missing block frame size")?)
                        .ok_or("compressed group size overflow")?;
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
