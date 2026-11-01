use super::common::*;
use crate::{
    BlockFrame, COMMIT_HEADER_FIXED_SIZE, CONTAINER_HEADER_SIZE, CommitHeader,
    CommitValidationLimits, ContainerHeader, HEADER_OFFSET_SIZE, HeaderFrame, Toc, TocChange,
    ValidatedCommit,
};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek};
use xxhash_rust::xxh64::Xxh64;

pub fn check_archive_with_recovery(
    path: &str,
    recover: bool,
    password: Option<&[u8]>,
) -> Result<CheckReport, Box<dyn std::error::Error>> {
    check_archive_with_recovery_progress(path, recover, password, None)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    pub commits: u64,
    pub checked_blocks: u64,
    pub encrypted_blocks: u64,
    pub recoveries: u64,
    pub logical_state_readable: bool,
    pub recovery_messages: Vec<String>,
}

pub fn check_archive_with_recovery_progress(
    path: &str,
    recover: bool,
    password: Option<&[u8]>,
    progress: Option<&dyn Progress>,
) -> Result<CheckReport, Box<dyn std::error::Error>> {
    let passwords = PasswordSet::new(password.map(<[u8]>::to_vec));
    check_archive_with_recovery_and_passwords_progress(path, recover, &passwords, progress)
}

pub fn check_archive_with_recovery_and_passwords(
    path: &str,
    recover: bool,
    passwords: &PasswordSet,
) -> Result<CheckReport, Box<dyn std::error::Error>> {
    check_archive_with_recovery_and_passwords_progress(path, recover, passwords, None)
}

pub fn check_archive_with_recovery_and_passwords_progress(
    path: &str,
    recover: bool,
    passwords: &PasswordSet,
    progress: Option<&dyn Progress>,
) -> Result<CheckReport, Box<dyn std::error::Error>> {
    let progress = ProgressReporter::new(progress);
    let mut file = File::open(path)?;
    let container = ContainerHeader::read_from(&mut file)?;
    if let Some(callback) = progress.0 {
        callback.set_total(file.metadata()?.len().saturating_sub(CONTAINER_HEADER_SIZE));
    }
    let limits = CommitValidationLimits::default();
    let mut toc = Toc::default();
    let mut logical_state_readable = true;
    let mut commits = 0_u64;
    let mut checked_blocks = 0_u64;
    let mut encrypted_blocks = 0_u64;
    let mut recoveries = 0_u64;
    let mut recovery_messages = Vec::new();

    let mut header_offset = container.first_header_offset;
    while header_offset != 0 {
        let commit_start = header_offset;
        let (commit_header, data_start, next_header_offset) =
            match read_commit_at_offset(&mut file, header_offset, limits) {
                Ok(entry) => entry,
                Err(error) if recover => {
                    let Some((offset, ch, ds, next)) =
                        find_next_valid_commit(&mut file, commit_start.saturating_add(1), limits)?
                    else {
                        return Err(error);
                    };
                    recovery_messages.push(format!(
                        "warning: recovered commit framing at offset {offset}"
                    ));
                    recoveries += 1;
                    toc = Toc::default();
                    logical_state_readable = true;
                    (ch, ds, next)
                }
                Err(error) => return Err(error),
            };
        let (validated, dek) =
            match read_commit_with_passwords(&mut file, commit_header.clone(), limits, passwords) {
                Ok(commit) => commit,
                Err(error) if recover => {
                    let Some((offset, ch, _ds, _next)) =
                        find_next_valid_commit(&mut file, data_start.saturating_add(1), limits)?
                    else {
                        return Err(error);
                    };
                    recovery_messages.push(format!(
                        "warning: recovered commit framing at offset {offset}"
                    ));
                    recoveries += 1;
                    toc = Toc::default();
                    logical_state_readable = true;
                    read_commit_with_passwords(&mut file, ch.clone(), limits, passwords)?
                }
                Err(error) => return Err(error),
            };
        let commit_end = file.stream_position()?;
        progress.inc(commit_end.saturating_sub(commit_start));
        progress.set_message(format!(
            "Checking commit {:08x}",
            u32::from_le_bytes(validated.commit_header.commit_id)
        ));
        file.seek(io::SeekFrom::Start(data_start))?;
        let mut data_reader = (&mut file).take(validated.commit_header.data_section_size);
        let mut block_sizes = HashMap::with_capacity(validated.block_count as usize);
        let mut block_offsets = HashMap::with_capacity(validated.block_count as usize);
        let algorithm = encryption_algorithm(&validated.encryption);
        progress.set_message(format!(
            "Checking commit {:08x}",
            u32::from_le_bytes(validated.commit_header.commit_id)
        ));
        while data_reader.limit() != 0 {
            let block_offset = data_start
                .checked_add(validated.commit_header.data_section_size - data_reader.limit())
                .ok_or("block offset overflow")?;
            let block = BlockFrame::read_from(&mut data_reader, limits.max_frame_payload_size)?;
            let size = if block.flags & crate::BLOCK_FLAG_ENCRYPTED != 0 {
                match (dek.as_ref(), algorithm) {
                    (Some(dek), Some(algorithm)) => Some(
                        block
                            .uncompressed_encrypted(
                                &validated.commit_header,
                                dek,
                                algorithm,
                                limits.max_uncompressed_block_size,
                            )?
                            .len(),
                    ),
                    _ => None,
                }
            } else {
                Some(
                    block
                        .uncompressed_at_most(limits.max_uncompressed_block_size)?
                        .len(),
                )
            };
            block_offsets.insert(block.block_index, block_offset);
            if let Some(size) = size {
                checked_blocks += 1;
                progress.inc(size as u64);
                block_sizes.insert(block.block_index, Some(size));
            } else {
                encrypted_blocks += 1;
                progress.inc(1);
                block_sizes.insert(block.block_index, None);
            }
        }

        if let Some(header) = &validated.logical_header {
            if logical_state_readable {
                for change in &header.changes {
                    if let TocChange::PutFile { path, entry } = change {
                        let mut decoded_size = 0_u64;
                        let mut all_blocks_readable = true;
                        let mut checksum = Xxh64::new(0);
                        for index in &entry.block_indices {
                            match block_sizes.get(index) {
                                Some(Some(size)) => {
                                    decoded_size = decoded_size
                                        .checked_add(*size as u64)
                                        .ok_or("decoded file size overflow")?;
                                    if entry.checksum.is_some() {
                                        let offset = block_offsets.get(index).ok_or_else(|| {
                                            format!("missing data offset for block {index}")
                                        })?;
                                        file.seek(io::SeekFrom::Start(*offset))?;
                                        let block = BlockFrame::read_from(
                                            &mut file,
                                            limits.max_frame_payload_size,
                                        )?;
                                        let contents =
                                            if block.flags & crate::BLOCK_FLAG_ENCRYPTED != 0 {
                                                match (dek.as_ref(), algorithm) {
                                                    (Some(dek), Some(algorithm)) => block
                                                        .uncompressed_encrypted(
                                                            &validated.commit_header,
                                                            dek,
                                                            algorithm,
                                                            limits.max_uncompressed_block_size,
                                                        )?,
                                                    _ => {
                                                        all_blocks_readable = false;
                                                        continue;
                                                    }
                                                }
                                            } else {
                                                block.uncompressed_at_most(
                                                    limits.max_uncompressed_block_size,
                                                )?
                                            };
                                        checksum.update(&contents);
                                    }
                                }
                                Some(None) => all_blocks_readable = false,
                                None => {
                                    return Err(format!(
                                        "TOC references unavailable block {index} for {path}"
                                    )
                                    .into());
                                }
                            }
                        }
                        if all_blocks_readable && decoded_size != entry.size {
                            return Err(format!(
                                "decoded size mismatch for {path}: expected {} bytes, got {decoded_size}",
                                entry.size
                            )
                            .into());
                        }
                        if all_blocks_readable
                            && entry
                                .checksum
                                .is_some_and(|expected| expected != checksum.digest())
                        {
                            return Err(format!("file checksum mismatch for {path}").into());
                        }
                    }
                }
                toc.apply_commit_with_available_blocks(&header.changes, &validated.block_indices)?;
            }
        } else {
            logical_state_readable = false;
        }

        file.seek(io::SeekFrom::Start(commit_end))?;
        commits += 1;
        header_offset = next_header_offset;
    }

    Ok(CheckReport {
        commits,
        checked_blocks,
        encrypted_blocks,
        recoveries,
        logical_state_readable,
        recovery_messages,
    })
}

fn find_next_valid_commit(
    file: &mut File,
    start: u64,
    limits: CommitValidationLimits,
) -> io::Result<Option<(u64, CommitHeader, u64, u64)>> {
    let file_len = file.metadata()?.len();
    let min_size = COMMIT_HEADER_FIXED_SIZE as u64 + HEADER_OFFSET_SIZE;
    if start.saturating_add(min_size) > file_len {
        return Ok(None);
    }
    let mut offset = start;
    while offset.saturating_add(min_size) <= file_len {
        file.seek(io::SeekFrom::Start(offset))?;
        let header_frame = match HeaderFrame::read_from(file, limits.max_frame_payload_size) {
            Ok(frame) => frame,
            Err(_) => {
                offset += 1;
                continue;
            }
        };
        let commit_header = header_frame.commit_header.clone();
        if commit_header.validate().is_err() {
            offset += 1;
            continue;
        }
        let data_start = match offset.checked_sub(commit_header.data_section_size) {
            Some(ds) => ds,
            None => {
                offset += 1;
                continue;
            }
        };
        let next_offset_pos = match data_start.checked_sub(HEADER_OFFSET_SIZE) {
            Some(pos) => pos,
            None => {
                offset += 1;
                continue;
            }
        };
        let commit_end = offset
            .checked_add(u64::from(commit_header.header_frame_size))
            .and_then(|size| size.checked_add(HEADER_OFFSET_SIZE));
        if !commit_end.is_some_and(|end| end <= file_len) {
            offset += 1;
            continue;
        }
        file.seek(io::SeekFrom::Start(data_start))?;
        if ValidatedCommit::read_body(file, commit_header.clone(), limits).is_err() {
            offset += 1;
            continue;
        }
        file.seek(io::SeekFrom::Start(next_offset_pos))?;
        let mut next_bytes = [0u8; 8];
        file.read_exact(&mut next_bytes)?;
        let next_header_offset = u64::from_le_bytes(next_bytes);
        if next_header_offset != 0 && next_header_offset >= file_len {
            offset += 1;
            continue;
        }
        file.seek(io::SeekFrom::Start(data_start))?;
        return Ok(Some((
            offset,
            commit_header,
            data_start,
            next_header_offset,
        )));
    }
    Ok(None)
}
