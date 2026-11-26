use super::common::*;
use super::remove_commits::CrossCommitReference;
#[cfg(not(target_os = "wasi"))]
use super::worker_priority::{worker_pool, worker_priority};
use crate::{
    BLOCK_FLAG_ENCRYPTED, BLOCK_FLAG_UNCOMPRESSED, BlockFrame, CommitHeader,
    CommitValidationLimits, ContainerHeader, ContentAead, FileEntry, FileGroupKind,
    HEADER_OFFSET_SIZE, LogicalHeader, MetadataType, TocChange,
};
#[cfg(not(target_os = "wasi"))]
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufReader, Seek, SeekFrom};
use xxhash_rust::xxh3::Xxh3Builder;

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
    pub cross_commit_references: Option<Vec<CrossCommitReference>>,
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
    info_archive_with_passwords(path, verbose, &PasswordSet::default())
}

pub fn info_archive_with_passwords(
    path: &str,
    verbose: bool,
    passwords: &PasswordSet,
) -> Result<Vec<CommitInfo>, Box<dyn std::error::Error>> {
    Ok(read_archive_info(path, verbose, false, passwords)?
        .into_iter()
        .map(|details| details.commit)
        .collect())
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
    let mut raw_commits = Vec::new();
    let mut header_offset = container.first_header_offset;
    while header_offset != 0 {
        let (commit_header, data_start, next_header_offset) =
            read_commit_at_offset(&mut file, header_offset, limits)?;
        let commit_size = header_offset
            .checked_add(u64::from(commit_header.header_frame_size))
            .and_then(|size| size.checked_add(HEADER_OFFSET_SIZE))
            .and_then(|size| size.checked_sub(data_start))
            .and_then(|size| size.checked_sub(HEADER_OFFSET_SIZE))
            .ok_or("commit size overflow")?;
        file.seek(SeekFrom::Start(header_offset))?;
        let (validated, _) = read_header_with_passwords(&mut file, limits, passwords)?;
        raw_commits.push((commit_header, data_start, commit_size, validated));
        header_offset = next_header_offset;
    }

    #[cfg(not(target_os = "wasi"))]
    let mmap = unsafe { memmap2::Mmap::map(&file).ok() };
    #[cfg(target_os = "wasi")]
    let mmap: Option<Vec<u8>> = None;
    drop(file);

    #[cfg(not(target_os = "wasi"))]
    let workers = available_compression_workers();
    #[cfg(target_os = "wasi")]
    let workers = 1;

    #[cfg(not(target_os = "wasi"))]
    let pool = worker_pool(workers, "zarc-info", worker_priority())?;

    let mmap_ref = mmap.as_deref();
    let analyze_commit = |(commit_header, data_start, commit_size, validated): (
        CommitHeader,
        u64,
        u64,
        crate::ValidatedHeader,
    )|
     -> Result<CommitInfoWithBlocks, Box<dyn std::error::Error + Send + Sync>> {
        let uncompressed_size = validated
            .logical_header
            .as_ref()
            .map(|h| {
                uncompressed_commit_size(h)
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })
            })
            .transpose()?;
        let header_encrypted = validated.encryption.is_header_encrypted();
        let header_changes = validated
            .logical_header
            .as_ref()
            .map(summarize_header_changes);
        let cross_commit_references = validated.logical_header.as_ref().map(|header| {
            header
                .changes
                .iter()
                .filter_map(|change| match change {
                    TocChange::PutFile { path, entry } => {
                        entry
                            .source_commit
                            .map(|source_commit| CrossCommitReference {
                                referencing_commit: commit_header.commit_id,
                                source_commit,
                                path: path.clone(),
                            })
                    }
                    _ => None,
                })
                .collect()
        });
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
        let mut header_compression = None;
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
        let mut details = None;
        let mut blocks = Vec::new();
        if verbose {
            let logical_header = validated.logical_header.as_ref();
            if let Some(header) = logical_header {
                if include_block_details {
                    blocks = if let Some(entries) = &header.block_index {
                        build_blocks_from_header(header, entries, data_encrypted)?
                    } else {
                        scan_commit_blocks(
                            ScanDataParams {
                                path,
                                data_start,
                                data_section_size: commit_header.data_section_size,
                                limits,
                                mmap: mmap_ref,
                            },
                            Some(header),
                            workers,
                            #[cfg(not(target_os = "wasi"))]
                            Some(&pool),
                        )?
                    };
                }
                details = Some(if has_stored_group_sizes(header) {
                    summarize_commit_header_only(header, None)
                        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?
                } else {
                    let block_sizes = if include_block_details {
                        blocks
                            .iter()
                            .map(|block| (block.index, block.compressed_size))
                            .collect::<HashMap<_, _>>()
                    } else if let Some(entries) = &header.block_index {
                        let overhead = if data_encrypted { 32 } else { 20 };
                        entries
                            .iter()
                            .map(|e| (e.index, e.encoded_size.saturating_sub(overhead)))
                            .collect::<HashMap<_, _>>()
                    } else {
                        let mut file = File::open(path)?;
                        scan_block_sizes(
                            &mut file,
                            data_start,
                            commit_header.data_section_size,
                            limits,
                        )
                        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?
                    };
                    summarize_commit_header_only(header, Some(&block_sizes))
                        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.to_string().into() })?
                });
            } else if include_block_details {
                blocks = scan_commit_blocks(
                    ScanDataParams {
                        path,
                        data_start,
                        data_section_size: commit_header.data_section_size,
                        limits,
                        mmap: mmap_ref,
                    },
                    None,
                    workers,
                    #[cfg(not(target_os = "wasi"))]
                    Some(&pool),
                )?;
            }
        }
        Ok(CommitInfoWithBlocks {
            commit: CommitInfo {
                id: commit_header.commit_id,
                size: commit_size,
                header_size: commit_header.header_frame_size,
                data_size: commit_header.data_section_size,
                uncompressed_size,
                ratio_percent,
                header_compression,
                header_changes,
                cross_commit_references,
                created_unix_millis,
                user,
                details,
                header_encrypted,
                data_encrypted,
                encryption_algorithm: content_algorithm,
            },
            blocks,
        })
    };

    #[cfg(not(target_os = "wasi"))]
    if raw_commits.len() > 1 && workers > 1 {
        let results: Result<Vec<_>, _> = pool.install(|| {
            raw_commits.into_par_iter().map(analyze_commit).collect()
        });
        return results.map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() });
    }

    raw_commits
        .into_iter()
        .map(analyze_commit)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })
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

fn build_blocks_from_header(
    header: &LogicalHeader,
    entries: &[crate::HeaderBlockIndexEntry],
    data_encrypted: bool,
) -> Result<Vec<BlockInfo>, Box<dyn std::error::Error + Send + Sync>> {
    let max_block_index = entries.iter().map(|e| e.index).max().unwrap_or(0) as usize;
    let use_flat_tables = max_block_index <= 4_000_000;
    let mut flat_files = if use_flat_tables {
        vec![0_u32; max_block_index + 1]
    } else {
        Vec::new()
    };
    let mut flat_groups: Vec<Vec<&'static str>> = if use_flat_tables {
        vec![Vec::new(); max_block_index + 1]
    } else {
        Vec::new()
    };
    let mut flat_uncompressed = if use_flat_tables {
        vec![0_u64; max_block_index + 1]
    } else {
        Vec::new()
    };
    let mut hash_files = HashMap::<u32, u64>::new();
    let mut hash_groups = HashMap::<u32, Vec<&'static str>>::new();
    let mut hash_uncompressed = HashMap::<u32, u64>::new();

    let block_size = header
        .block_size
        .map(u64::from)
        .unwrap_or(CREATE_BLOCK_SIZE as u64);

    for change in &header.changes {
        if let TocChange::PutFile { entry, .. } = change
            && entry.source_commit.is_none()
        {
            let group = file_group_name(header, entry);
            for &index in &entry.block_indices {
                let idx = index as usize;
                if use_flat_tables && idx <= max_block_index {
                    flat_files[idx] = flat_files[idx].saturating_add(1);
                    let g = &mut flat_groups[idx];
                    if !g.contains(&group) {
                        g.push(group);
                    }
                } else {
                    let count = hash_files.entry(index).or_default();
                    *count = count.checked_add(1).ok_or("block file count overflow")?;
                    let g = hash_groups.entry(index).or_default();
                    if !g.contains(&group) {
                        g.push(group);
                    }
                }
            }

            let n_blocks = entry.block_indices.len();
            if n_blocks == 1 {
                let b = entry.block_indices[0];
                let uncompressed = if let Some(offset) = entry.offset_in_block {
                    u64::from(offset).saturating_add(entry.size)
                } else {
                    entry.size
                };
                if use_flat_tables && (b as usize) <= max_block_index {
                    flat_uncompressed[b as usize] = flat_uncompressed[b as usize].max(uncompressed);
                } else {
                    let cur = hash_uncompressed.entry(b).or_default();
                    *cur = (*cur).max(uncompressed);
                }
            } else if n_blocks > 1 {
                for (i, &b) in entry.block_indices.iter().enumerate() {
                    let uncompressed = if i + 1 == n_blocks {
                        entry.size.saturating_sub((i as u64).saturating_mul(block_size))
                    } else {
                        block_size
                    };
                    if use_flat_tables && (b as usize) <= max_block_index {
                        flat_uncompressed[b as usize] = uncompressed;
                    } else {
                        hash_uncompressed.insert(b, uncompressed);
                    }
                }
            }
        }
    }

    let overhead = if data_encrypted { 32 } else { 20 };
    let mut blocks = Vec::with_capacity(entries.len());
    for entry in entries {
        let compressed_size = entry.encoded_size.saturating_sub(overhead);
        let ratio_percent = if data_encrypted {
            None
        } else {
            let uncompressed = if use_flat_tables && (entry.index as usize) <= max_block_index {
                flat_uncompressed[entry.index as usize]
            } else {
                hash_uncompressed.get(&entry.index).copied().unwrap_or(0)
            };
            if uncompressed == 0 {
                Some(0.0)
            } else {
                Some(compressed_size as f64 * 100.0 / uncompressed as f64)
            }
        };

        let idx = entry.index as usize;
        let (file_count, file_groups) = if use_flat_tables && idx <= max_block_index {
            (flat_files[idx] as u64, std::mem::take(&mut flat_groups[idx]))
        } else {
            (
                hash_files.remove(&entry.index).unwrap_or_default(),
                hash_groups.remove(&entry.index).unwrap_or_default(),
            )
        };

        blocks.push(BlockInfo {
            index: entry.index,
            compressed_size,
            ratio_percent,
            file_count,
            file_groups,
        });
    }

    Ok(blocks)
}

struct ScanDataParams<'a> {
    path: &'a str,
    data_start: u64,
    data_section_size: u64,
    limits: CommitValidationLimits,
    mmap: Option<&'a [u8]>,
}

fn scan_commit_blocks(
    params: ScanDataParams<'_>,
    header: Option<&LogicalHeader>,
    workers: usize,
    #[cfg(not(target_os = "wasi"))] pool: Option<&rayon::ThreadPool>,
) -> Result<Vec<BlockInfo>, Box<dyn std::error::Error + Send + Sync>> {
    let ScanDataParams {
        path,
        data_start,
        data_section_size,
        limits,
        mmap,
    } = params;
    let max_block_index = header
        .and_then(|h| h.block_index.as_ref())
        .and_then(|entries| entries.iter().map(|e| e.index).max())
        .unwrap_or(0) as usize;

    let use_flat_tables = max_block_index <= 4_000_000;
    let mut flat_files = if use_flat_tables {
        vec![0_u32; max_block_index + 1]
    } else {
        Vec::new()
    };
    let mut flat_groups: Vec<Vec<&'static str>> = if use_flat_tables {
        vec![Vec::new(); max_block_index + 1]
    } else {
        Vec::new()
    };
    let mut hash_files = HashMap::<u32, u64>::new();
    let mut hash_groups = HashMap::<u32, Vec<&'static str>>::new();

    if let Some(header) = header {
        for change in &header.changes {
            if let TocChange::PutFile { entry, .. } = change
                && entry.source_commit.is_none()
            {
                let group = file_group_name(header, entry);
                for &index in &entry.block_indices {
                    let idx = index as usize;
                    if use_flat_tables && idx <= max_block_index {
                        flat_files[idx] = flat_files[idx].saturating_add(1);
                        let g = &mut flat_groups[idx];
                        if !g.contains(&group) {
                            g.push(group);
                        }
                    } else {
                        let count = hash_files.entry(index).or_default();
                        *count = count.checked_add(1).ok_or("block file count overflow")?;
                        let g = hash_groups.entry(index).or_default();
                        if !g.contains(&group) {
                            g.push(group);
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "wasi"))]
    if let Some(header) = header
        && let Some(entries) = &header.block_index
        && let Some(mmap_bytes) = mmap
    {
        type ScannedBlock = (u32, u64, Option<f64>);
        type ScanResult = Result<Vec<Vec<ScannedBlock>>, Box<dyn std::error::Error + Send + Sync>>;
        let chunk_size = entries.len().div_ceil(workers).max(32);
        let process_chunk = |chunk: &[crate::HeaderBlockIndexEntry]| -> Result<Vec<ScannedBlock>, Box<dyn std::error::Error + Send + Sync>> {
            let mut items = Vec::with_capacity(chunk.len());
            for entry in chunk {
                let start = (data_start + entry.offset) as usize;
                let end = (start + (entry.encoded_size as usize).min(50)).min(mmap_bytes.len());
                let slice = mmap_bytes.get(start..end).ok_or(crate::FramingError::InvalidSize)?;
                let (metadata, prefix, prefix_len) =
                    BlockFrame::parse_metadata_and_prefix(slice, limits.max_frame_payload_size)?;
                let uncompressed_size = if metadata.flags & BLOCK_FLAG_ENCRYPTED != 0 {
                    None
                } else if metadata.flags & BLOCK_FLAG_UNCOMPRESSED != 0 {
                    if metadata.payload_size as usize > limits.max_uncompressed_block_size {
                        return Err(crate::FramingError::InvalidSize.into());
                    }
                    Some(u64::from(metadata.payload_size))
                } else {
                    let size = match zstd::zstd_safe::get_frame_content_size(&prefix[..prefix_len]) {
                        Ok(Some(size)) => size,
                        _ => {
                            let frame = BlockFrame::read_from(
                                &mut &mmap_bytes[start..(start + entry.encoded_size as usize).min(mmap_bytes.len())],
                                limits.max_frame_payload_size,
                            )?;
                            frame
                                .uncompressed_at_most(limits.max_uncompressed_block_size)?
                                .len() as u64
                        }
                    };
                    if size > limits.max_uncompressed_block_size as u64 {
                        return Err(crate::FramingError::InvalidSize.into());
                    }
                    Some(size)
                };
                let compressed_size = u64::from(metadata.payload_size);
                let ratio_percent = uncompressed_size.map(|size| {
                    if size == 0 {
                        0.0
                    } else {
                        compressed_size as f64 * 100.0 / size as f64
                    }
                });
                items.push((metadata.block_index, compressed_size, ratio_percent));
            }
            Ok(items)
        };

        let chunk_results: ScanResult = if workers > 1 && let Some(pool) = pool {
            pool.install(|| {
                entries
                    .par_chunks(chunk_size)
                    .map(process_chunk)
                    .collect()
            })
        } else {
            entries.chunks(chunk_size).map(process_chunk).collect()
        };

        let chunk_results = chunk_results?;
        let mut blocks = Vec::with_capacity(entries.len());
        for chunk in chunk_results {
            for (block_index, compressed_size, ratio_percent) in chunk {
                let idx = block_index as usize;
                let (file_count, file_groups) = if use_flat_tables && idx <= max_block_index {
                    (flat_files[idx] as u64, std::mem::take(&mut flat_groups[idx]))
                } else {
                    (hash_files.remove(&block_index).unwrap_or_default(), hash_groups.remove(&block_index).unwrap_or_default())
                };
                blocks.push(BlockInfo {
                    index: block_index,
                    compressed_size,
                    ratio_percent,
                    file_count,
                    file_groups,
                });
            }
        }
        return Ok(blocks);
    }

    let mut file = File::open(path)?;
    let mut blocks = Vec::new();
    let mut remaining = data_section_size;
    file.seek(SeekFrom::Start(data_start))?;
    let mut reader = BufReader::with_capacity(128 * 1024, file);
    while remaining > 0 {
        if blocks.len() as u64 >= limits.max_blocks {
            return Err(crate::FramingError::InvalidSize.into());
        }
        let before = reader.stream_position()?;
        let (metadata, prefix, prefix_len) =
            BlockFrame::read_metadata_and_prefix_from(&mut reader, limits.max_frame_payload_size)?;
        let consumed = reader
            .stream_position()?
            .checked_sub(before)
            .ok_or("block offset underflow")?;
        if consumed == 0 || consumed > remaining || consumed != metadata.encoded_size {
            return Err(crate::FramingError::InvalidSize.into());
        }
        remaining -= consumed;
        let uncompressed_size = if metadata.flags & BLOCK_FLAG_ENCRYPTED != 0 {
            None
        } else if metadata.flags & BLOCK_FLAG_UNCOMPRESSED != 0 {
            if metadata.payload_size as usize > limits.max_uncompressed_block_size {
                return Err(crate::FramingError::InvalidSize.into());
            }
            Some(u64::from(metadata.payload_size))
        } else {
            let size = match zstd::zstd_safe::get_frame_content_size(&prefix[..prefix_len]) {
                Ok(Some(size)) => size,
                _ => {
                    let cur_pos = reader.stream_position()?;
                    reader.seek(SeekFrom::Start(before))?;
                    let frame =
                        BlockFrame::read_from(&mut reader, limits.max_frame_payload_size)?;
                    let len = frame
                        .uncompressed_at_most(limits.max_uncompressed_block_size)?
                        .len() as u64;
                    reader.seek(SeekFrom::Start(cur_pos))?;
                    len
                }
            };
            if size > limits.max_uncompressed_block_size as u64 {
                return Err(crate::FramingError::InvalidSize.into());
            }
            Some(size)
        };
        let compressed_size = u64::from(metadata.payload_size);
        let ratio_percent = uncompressed_size.map(|size| {
            if size == 0 {
                0.0
            } else {
                compressed_size as f64 * 100.0 / size as f64
            }
        });
        let idx = metadata.block_index as usize;
        let (file_count, file_groups) = if use_flat_tables && idx <= max_block_index {
            (flat_files[idx] as u64, std::mem::take(&mut flat_groups[idx]))
        } else {
            (hash_files.remove(&metadata.block_index).unwrap_or_default(), hash_groups.remove(&metadata.block_index).unwrap_or_default())
        };
        blocks.push(BlockInfo {
            index: metadata.block_index,
            compressed_size,
            ratio_percent,
            file_count,
            file_groups,
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
    let mut reader = BufReader::with_capacity(128 * 1024, file);
    while remaining > 0 {
        if block_sizes.len() as u64 >= limits.max_blocks {
            return Err(crate::FramingError::InvalidSize.into());
        }
        let metadata = BlockFrame::read_metadata_from(&mut reader, limits.max_frame_payload_size)?;
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
    let mut seen_files = std::collections::HashSet::with_capacity_and_hasher(
        header.changes.len(),
        Xxh3Builder::new(),
    );
    let mut counted_blocks = std::collections::HashSet::with_capacity_and_hasher(
        block_sizes.map_or(0, |b| b.len()),
        Xxh3Builder::new(),
    );
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

    let flat_block_sizes = block_sizes.and_then(|sizes| {
        let max_idx = sizes.keys().copied().max().unwrap_or(0) as usize;
        if max_idx <= sizes.len().saturating_mul(4).max(1024) && max_idx <= 4_000_000 {
            let mut flat = vec![0_u64; max_idx + 1];
            for (&idx, &size) in sizes {
                flat[idx as usize] = size;
            }
            Some(flat)
        } else {
            None
        }
    });

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
            && (entry.source_commit.is_some()
                || !seen_files.insert((
                    entry.size,
                    entry.block_indices.as_slice(),
                    entry.offset_in_block,
                )));
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
            if let Some(flat) = &flat_block_sizes {
                for index in &entry.block_indices {
                    if counted_blocks.insert((name, *index)) {
                        let size = flat
                            .get(*index as usize)
                            .copied()
                            .filter(|&s| s != 0)
                            .ok_or("missing block frame size")?;
                        group.compressed_size = group
                            .compressed_size
                            .checked_add(size)
                            .ok_or("compressed group size overflow")?;
                    }
                }
            } else if let Some(block_sizes) = block_sizes {
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
            && (entry.source_commit.is_some()
                || !seen_files.insert((
                    entry.size,
                    entry.block_indices.clone(),
                    entry.offset_in_block,
                )));
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
    fn block_size_scan_rejects_duplicate_indices_and_limits() {
        let mut file = tempfile::tempfile().unwrap();
        let first = BlockFrame::from_stored(1, b"alpha").unwrap();
        let second = BlockFrame::from_stored(2, b"beta").unwrap();
        first.write_to(&mut file).unwrap();
        second.write_to(&mut file).unwrap();
        let size = file.metadata().unwrap().len();
        let limits = CommitValidationLimits::default();

        let sizes = scan_block_sizes(&mut file, 0, size, limits).unwrap();
        assert_eq!(sizes, HashMap::from([(1, 5), (2, 4)]));
        assert!(scan_block_sizes(&mut file, 0, size - 1, limits).is_err());
        let mut restricted = limits;
        restricted.max_blocks = 1;
        assert!(scan_block_sizes(&mut file, 0, size, restricted).is_err());

        let mut duplicate_file = tempfile::tempfile().unwrap();
        first.write_to(&mut duplicate_file).unwrap();
        first.write_to(&mut duplicate_file).unwrap();
        let duplicate_size = duplicate_file.metadata().unwrap().len();
        assert!(scan_block_sizes(&mut duplicate_file, 0, duplicate_size, limits).is_err());
    }

    #[test]
    fn summarize_files_counts_groups_deduplication_and_encrypted_blocks() {
        use crate::{ArchivePath, DeviceKind};

        let path = |name| ArchivePath::new(name).unwrap();
        let tiny = FileEntry::new(4, vec![1]).unwrap();
        let header = LogicalHeader::with_block_size(
            vec![
                TocChange::PutFile {
                    path: path("empty"),
                    entry: FileEntry::new(0, vec![]).unwrap(),
                },
                TocChange::PutFile {
                    path: path("tiny"),
                    entry: tiny.clone(),
                },
                TocChange::PutFile {
                    path: path("duplicate"),
                    entry: tiny,
                },
                TocChange::PutFile {
                    path: path("small"),
                    entry: FileEntry::new(12, vec![2]).unwrap(),
                },
                TocChange::PutFile {
                    path: path("big"),
                    entry: FileEntry::new(32, vec![3]).unwrap(),
                },
                TocChange::PutSymlink {
                    path: path("link"),
                    target: "tiny".into(),
                },
                TocChange::PutNamedPipe { path: path("pipe") },
                TocChange::PutDevice {
                    path: path("device"),
                    kind: DeviceKind::Character,
                    major: 1,
                    minor: 2,
                },
                TocChange::RemoveFile {
                    path: path("removed"),
                },
            ],
            32,
        );
        let blocks = HashMap::from([(1, (10, Some(4))), (2, (20, None)), (3, (30, Some(32)))]);
        let stats = summarize_commit_files(&header, &blocks).unwrap();
        assert_eq!(stats.groups["empty files"].files, 1);
        assert_eq!(stats.groups["tiny files"].files, 2);
        assert_eq!(stats.groups["tiny files"].uncompressed_size, 4);
        assert_eq!(stats.groups["tiny files"].compressed_size, 10);
        assert!(stats.groups["small files"].has_encrypted_blocks);
        assert_eq!(stats.groups["big files"].compressed_size, 30);
        assert_eq!(stats.deduplicated_files, 1);
        assert_eq!(stats.deduplicated_uncompressed_size, 4);
        assert_eq!(
            (stats.symlinks, stats.named_pipes, stats.devices),
            (1, 1, 1)
        );
        assert_eq!(stats.header_changes.files_removed, 1);
        let error = summarize_commit_files(&header, &HashMap::new()).unwrap_err();
        assert!(error.to_string().contains("unavailable block 1"));

        let mut unknown_block_size = header;
        unknown_block_size.block_size = None;
        let stats = summarize_commit_files(&unknown_block_size, &blocks).unwrap();
        assert_eq!(stats.groups["files (block size unknown)"].files, 2);
    }

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
