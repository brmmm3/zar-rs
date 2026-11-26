use super::common::*;
use super::info::group_size_metadata;
use super::list::load_toc_through_with_passwords;
use super::pipeline::{BlockWorkItem, EncryptionContext, SmallFilePacker, encode_block};
use crate::{
    ArchivePath, BlockFrame, CommitHeader, CommitValidationLimits, ContentAead,
    EncryptionDescriptor, EncryptionScope, FileEntry, HEADER_OFFSET_SIZE, HeaderFrame,
    LogicalHeader, Toc, TocChange, TocEntry, UNENCRYPTED_DESCRIPTOR, generate_dek, generate_nonce,
    wrap_dek_with_password,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use xxhash_rust::xxh3::Xxh3;
use zeroize::Zeroizing;

struct JoinCommit {
    header: CommitHeader,
    data_start: u64,
    commit_end: u64,
    logical_header: Option<LogicalHeader>,
    encryption: EncryptionDescriptor,
    block_indices: HashSet<u32>,
    block_count: u64,
    key: Option<Zeroizing<[u8; crate::DEK_SIZE]>>,
}

fn write_joined_block(
    data: &mut Vec<u8>,
    block_sizes: &mut HashMap<u32, u64>,
    block_index: &mut Vec<crate::HeaderBlockIndexEntry>,
    block: BlockWorkItem,
    compression_level: i32,
    encryption: Option<&EncryptionContext>,
) -> Result<(), Box<dyn std::error::Error>> {
    let frame = encode_block(
        block.block_index,
        &block.data,
        compression_level,
        block.skip_compression,
        encryption,
    )?;
    block_sizes.insert(
        block.block_index,
        u64::try_from(frame.payload.len()).map_err(|_| "joined block size overflow")?,
    );
    write_joined_frame(data, block_index, &frame)?;
    Ok(())
}

fn write_joined_frame(
    data: &mut Vec<u8>,
    block_index: &mut Vec<crate::HeaderBlockIndexEntry>,
    frame: &BlockFrame,
) -> Result<(), Box<dyn std::error::Error>> {
    let offset = u64::try_from(data.len())?;
    let encoded_size = u64::try_from(frame.encoded_size()?)?;
    frame.write_to(data)?;
    block_index.push(crate::HeaderBlockIndexEntry {
        index: frame.block_index,
        offset,
        encoded_size,
    });
    Ok(())
}

/// Creates an empty overlay with a root commit ID following the base archive's IDs.
pub fn create_overlay_archive(
    base: &Path,
    destination: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut output = tempfile::NamedTempFile::new_in(parent)?;
    let id = next_commit_id(base)?;
    let logical = LogicalHeader::new(Vec::new()).encode()?;
    let provisional = CommitHeader::new(0, id, 0, 0)?;
    let frame =
        HeaderFrame::from_uncompressed(provisional, &logical, UNENCRYPTED_DESCRIPTOR.to_vec(), 3)?;
    let header = CommitHeader::new(0, id, 0, u32::try_from(frame.encoded_size()?)?)?;
    let frame =
        HeaderFrame::from_uncompressed(header, &logical, UNENCRYPTED_DESCRIPTOR.to_vec(), 3)?;
    let header_offset = crate::CONTAINER_HEADER_SIZE + HEADER_OFFSET_SIZE;
    crate::ContainerHeader::new(header_offset).write_to(&mut output)?;
    output.write_all(&0u64.to_le_bytes())?;
    frame.write_to(&mut output)?;
    output.write_all(&header_offset.to_le_bytes())?;
    output.as_file_mut().sync_all()?;
    output
        .persist_noclobber(destination)
        .map_err(|error| error.error)?;
    Ok(())
}

/// Writes a new archive containing the commits of `base` followed by `overlay`.
///
/// The destination must not exist. Commit IDs must be unique across both inputs.
pub fn combine_archives(
    base: &Path,
    overlay: &Path,
    destination: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    combine_archives_with_progress(base, overlay, destination, None)
}

/// Writes a combined archive, checking for cancellation while copying its data.
pub fn combine_archives_with_progress(
    base: &Path,
    overlay: &Path,
    destination: &Path,
    progress: Option<&dyn Progress>,
) -> Result<(), Box<dyn std::error::Error>> {
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut output = tempfile::NamedTempFile::new_in(parent)?;
    let mut base_file = File::open(base)?;
    let mut overlay_file = File::open(overlay)?;
    let limits = CommitValidationLimits::default();
    let mut ids = std::collections::HashSet::new();
    let mut offsets = std::collections::HashSet::new();
    let mut offset = read_first_header_offset(&mut base_file)?;
    let mut link_position = 4;
    while offset != 0 {
        if !offsets.insert(offset) {
            return Err("base archive commit chain contains a cycle".into());
        }
        let (header, data_start, next) = read_commit_at_offset(&mut base_file, offset, limits)?;
        if !ids.insert(header.commit_id) {
            return Err("base archive contains duplicate commit IDs".into());
        }
        link_position = data_start
            .checked_sub(HEADER_OFFSET_SIZE)
            .ok_or("invalid commit link")?;
        offset = next;
    }
    base_file.seek(SeekFrom::Start(0))?;
    copy_archive_data(&mut base_file, &mut output, progress)?;

    offsets.clear();
    offset = read_first_header_offset(&mut overlay_file)?;
    while offset != 0 {
        if !offsets.insert(offset) {
            return Err("overlay archive commit chain contains a cycle".into());
        }
        let (header, data_start, next) = read_commit_at_offset(&mut overlay_file, offset, limits)?;
        if !ids.insert(header.commit_id) {
            return Err("archive commit IDs collide".into());
        }
        let body_size = header
            .data_section_size
            .checked_add(u64::from(header.header_frame_size))
            .ok_or("commit size overflow")?;
        output.seek(SeekFrom::End(0))?;
        let new_link_position = output.stream_position()?;
        output.write_all(&0u64.to_le_bytes())?;
        let new_header_offset = output
            .stream_position()?
            .checked_add(header.data_section_size)
            .ok_or("header offset overflow")?;
        overlay_file.seek(SeekFrom::Start(data_start))?;
        if copy_archive_data(
            &mut (&mut overlay_file).take(body_size),
            &mut output,
            progress,
        )? != body_size
        {
            return Err("overlay commit is truncated".into());
        }
        output.write_all(&new_header_offset.to_le_bytes())?;
        output.seek(SeekFrom::Start(link_position))?;
        output.write_all(&new_header_offset.to_le_bytes())?;
        link_position = new_link_position;
        offset = next;
    }
    ProgressReporter::new(progress).check_cancelled()?;
    output.as_file_mut().sync_all()?;
    output
        .persist_noclobber(destination)
        .map_err(|error| error.error)?;
    Ok(())
}

fn copy_archive_data(
    reader: &mut dyn Read,
    writer: &mut dyn Write,
    progress: Option<&dyn Progress>,
) -> Result<u64, Box<dyn std::error::Error>> {
    if progress.is_none() {
        return Ok(io::copy(reader, writer)?);
    }
    let mut buffer = [0; 64 * 1024];
    let mut total = 0_u64;
    loop {
        ProgressReporter::new(progress).check_cancelled()?;
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            return Ok(total);
        }
        writer.write_all(&buffer[..count])?;
        total = total
            .checked_add(count as u64)
            .ok_or("archive size overflow")?;
    }
}

/// Compacts an overlay into one commit and atomically writes the resulting archive.
/// Encrypted overlay commits require an explicitly encrypted output.
pub fn save_overlay_archive(
    base: &Path,
    overlay: &Path,
    destination: &Path,
    options: EncodingOptions,
    passwords: &PasswordSet,
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
) -> Result<[u8; 4], Box<dyn std::error::Error>> {
    let base_metadata = std::fs::metadata(base)?;
    let overlay_path = overlay.to_str().ok_or("overlay path is not valid UTF-8")?;
    let commits = super::info::info_archive(overlay_path, false)?;
    let first = commits.first().ok_or("overlay contains no commits")?;
    let last = commits.last().ok_or("overlay contains no commits")?;
    if encryption.is_none()
        && commits
            .iter()
            .any(|commit| commit.header_encrypted || commit.data_encrypted)
    {
        return Err("encrypted overlay commits require encrypted output".into());
    }
    if let Some((_, _, scope)) = encryption {
        let protects_header = matches!(
            scope,
            EncryptionScope::Header | EncryptionScope::HeaderAndData
        );
        let protects_data = matches!(
            scope,
            EncryptionScope::Data | EncryptionScope::HeaderAndData
        );
        if commits.iter().any(|commit| {
            (commit.header_encrypted && !protects_header)
                || (commit.data_encrypted && !protects_data)
        }) {
            return Err("output encryption must protect all encrypted overlay content".into());
        }
    }
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let directory = tempfile::tempdir_in(parent)?;
    let assembled = directory.path().join("assembled.zar");
    combine_archives(base, overlay, &assembled)?;

    let saved_id = if first.id == last.id && encryption.is_none() {
        last.id
    } else {
        let first_id = format!("{:08x}", u32::from_le_bytes(first.id));
        let last_id = format!("{:08x}", u32::from_le_bytes(last.id));
        let assembled_path = assembled
            .to_str()
            .ok_or("temporary archive path is not valid UTF-8")?;
        join_archive_with_encryption(
            assembled_path,
            &first_id,
            &last_id,
            options,
            None,
            passwords,
            encryption,
        )?;
        let saved_commits = super::info::info_archive(assembled_path, false)?;
        saved_commits
            .last()
            .ok_or("saved archive contains no commits")?
            .id
    };

    let _ = std::fs::set_permissions(&assembled, File::open(base)?.metadata()?.permissions());
    let current_metadata = std::fs::metadata(base)?;
    if current_metadata.len() != base_metadata.len()
        || current_metadata.modified()? != base_metadata.modified()?
    {
        return Err("base archive changed while saving".into());
    }
    if super::transform::replace_archive(&assembled, destination).is_err() {
        let mut source = File::open(&assembled)?;
        let mut output = tempfile::NamedTempFile::new_in(parent)?;
        io::copy(&mut source, &mut output)?;
        output
            .as_file()
            .set_permissions(File::open(base)?.metadata()?.permissions())?;
        output.as_file_mut().sync_all()?;
        output.persist(destination).map_err(|error| error.error)?;
    }
    Ok(saved_id)
}

pub fn join_archive_with_encoding(
    archive: &str,
    from_id: &str,
    to_id: &str,
    options: EncodingOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    join_archive_with_encoding_progress(archive, from_id, to_id, options, None)
}

pub fn join_archive_with_encoding_progress(
    archive: &str,
    from_id: &str,
    to_id: &str,
    options: EncodingOptions,
    progress: Option<&dyn Progress>,
) -> Result<(), Box<dyn std::error::Error>> {
    join_archive_with_encoding_progress_and_passwords(
        archive,
        from_id,
        to_id,
        options,
        progress,
        &PasswordSet::default(),
    )
}

/// Joins the readable commits in the selected range. Encrypted commits whose
/// password is unavailable are retained unchanged; the joined commit is placed
/// at the first readable commit's position.
pub fn join_archive_with_encoding_progress_and_passwords(
    archive: &str,
    from_id: &str,
    to_id: &str,
    options: EncodingOptions,
    progress: Option<&dyn Progress>,
    passwords: &PasswordSet,
) -> Result<(), Box<dyn std::error::Error>> {
    join_archive_with_encryption(archive, from_id, to_id, options, progress, passwords, None)
}

/// Joins commits and optionally encrypts the resulting commit with a new password.
pub fn join_archive_with_encryption(
    archive: &str,
    from_id: &str,
    to_id: &str,
    mut options: EncodingOptions,
    progress: Option<&dyn Progress>,
    passwords: &PasswordSet,
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let encoding_warning = resolve_encoding_options(&mut options);
    if let (Some(progress), Some(warning)) = (progress, encoding_warning.as_deref()) {
        progress.warning(warning);
    }
    let progress = ProgressReporter::new(progress);
    let from_id = parse_commit_id(from_id)?;
    let to_id = parse_commit_id(to_id)?;
    let archive_path = Path::new(archive);
    let parent = archive_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err(format!("archive parent is not a directory: {}", parent.display()).into());
    }
    let mut input = File::open(archive_path)?;
    let limits = CommitValidationLimits::default();
    let archive_bytes = input.metadata()?.len();
    if let Some(callback) = progress.0 {
        callback.set_total(archive_bytes);
    }
    let mut commits = Vec::new();
    let first_header_offset = read_first_header_offset(&mut input)?;
    let mut header_offset = first_header_offset;
    while header_offset != 0 {
        let (commit_header, data_start, next_header_offset) =
            read_commit_at_offset(&mut input, header_offset, limits)?;
        input.seek(SeekFrom::Start(header_offset))?;
        let (validated, key) = read_header_with_passwords(&mut input, limits, passwords)?;
        let commit_end = header_offset
            .checked_add(u64::from(commit_header.header_frame_size))
            .and_then(|size| size.checked_add(HEADER_OFFSET_SIZE))
            .ok_or("commit end overflow")?;
        progress.inc(commit_end.saturating_sub(data_start));
        progress.set_message(format!(
            "Joining commit {:08x}",
            u32::from_le_bytes(commit_header.commit_id)
        ));
        let (block_indices, block_count) = if let Some(ref lh) = validated.logical_header
            && let Some(ref bi) = lh.block_index
        {
            let indices = bi.iter().map(|e| e.index).collect::<HashSet<_>>();
            let count = bi.len() as u64;
            (indices, count)
        } else {
            input.seek(SeekFrom::Start(data_start))?;
            let mut reader = std::io::BufReader::with_capacity(128 * 1024, &mut input);
            let mut indices = HashSet::new();
            let mut remaining = commit_header.data_section_size;
            while remaining > 0 {
                let metadata =
                    BlockFrame::read_metadata_from(&mut reader, limits.max_frame_payload_size)?;
                indices.insert(metadata.block_index);
                remaining -= metadata.encoded_size;
            }
            let count = indices.len() as u64;
            (indices, count)
        };
        commits.push(JoinCommit {
            header: commit_header,
            data_start,
            commit_end,
            logical_header: validated.logical_header,
            encryption: validated.encryption,
            block_indices,
            block_count,
            key,
        });
        header_offset = next_header_offset;
    }
    let matching_indices = |id: &[u8; 4]| {
        commits
            .iter()
            .enumerate()
            .filter_map(|(index, commit)| (commit.header.commit_id == *id).then_some(index))
            .collect::<Vec<_>>()
    };
    let from_matches = matching_indices(&from_id);
    let to_matches = matching_indices(&to_id);
    if from_matches.len() != 1 || to_matches.len() != 1 {
        return Err("commit IDs must each identify exactly one archive commit".into());
    }
    let first = from_matches[0];
    let last = to_matches[0];
    if first > last {
        return Err("start commit ID must not appear after end commit ID".into());
    }

    let selected: Vec<usize> = (first..=last)
        .filter(|&index| {
            let commit = &commits[index];
            commit.logical_header.is_some()
                && (!commit.encryption.is_data_encrypted() || commit.key.is_some())
        })
        .collect();
    if selected.is_empty() {
        return Err(
            "the selected range contains no commits readable with the supplied passwords".into(),
        );
    }
    if encryption.is_some() && selected.len() != last - first + 1 {
        return Err(
            "all selected commits must be readable to create an encrypted joined commit".into(),
        );
    }

    let selected_ids: std::collections::HashSet<_> = selected
        .iter()
        .map(|&index| commits[index].header.commit_id)
        .collect();
    for (index, commit) in commits.iter().enumerate() {
        if selected.contains(&index) || index < selected[0] {
            continue;
        }
        let header = commit
            .logical_header
            .as_ref()
            .ok_or("cannot check references in an encrypted commit following the joined range")?;
        for change in &header.changes {
            if let TocChange::PutFile { path, entry } = change
                && let Some(source) = entry.source_commit
                && selected_ids.contains(&source)
            {
                return Err(format!("cannot join referenced commit {:08x}: commit {:08x}, path {path} references it", u32::from_le_bytes(source), u32::from_le_bytes(commit.header.commit_id)).into());
            }
        }
    }
    let referenced_selected_ids: std::collections::HashSet<_> = selected
        .iter()
        .flat_map(|&index| {
            commits[index]
                .logical_header
                .as_ref()
                .unwrap()
                .changes
                .iter()
                .filter_map(|change| match change {
                    TocChange::PutFile { entry, .. } => entry
                        .source_commit
                        .filter(|source| selected_ids.contains(source)),
                    _ => None,
                })
        })
        .collect();

    let first_selected = selected[0];
    let mut prefix_toc = Toc::default();
    for commit in &commits[..first_selected] {
        let header = commit
            .logical_header
            .as_ref()
            .ok_or("cannot build join prefix while a preceding commit header is encrypted")?;
        Toc::check_available_blocks(&header.changes, &commit.block_indices)?;
        prefix_toc.apply_archive_commit(commit.header.commit_id, &header.changes)?;
    }
    let mut joined_toc = prefix_toc.clone();
    let mut written_files = BTreeMap::<ArchivePath, Vec<u8>>::new();
    let mut written_references = BTreeMap::<ArchivePath, FileEntry>::new();
    let mut historical_blocks = HashMap::<[u8; 4], HashMap<u32, Vec<u8>>>::new();
    let mut custom_metadata = HashMap::new();
    let mut joined_special = BTreeMap::<ArchivePath, TocEntry>::new();
    let mut joined_metadata = BTreeMap::<ArchivePath, crate::EntryMetadata>::new();
    let limits = CommitValidationLimits::default();
    for &index in &selected {
        let commit = &commits[index];
        let commit_header = &commit.header;
        let header = commit.logical_header.as_ref().unwrap();
        let algorithm = match &commit.encryption {
            crate::EncryptionDescriptor::Unencrypted => None,
            crate::EncryptionDescriptor::Symmetric { algorithm, .. }
            | crate::EncryptionDescriptor::Password { algorithm, .. }
            | crate::EncryptionDescriptor::Recipients { algorithm, .. }
            | crate::EncryptionDescriptor::PasswordAndRecipients { algorithm, .. } => {
                Some(*algorithm)
            }
        };
        input.seek(io::SeekFrom::Start(commit.data_start))?;
        let mut data_reader = (&mut input).take(commit_header.data_section_size);
        let mut blocks = HashMap::with_capacity(commit.block_count as usize);
        while data_reader.limit() != 0 {
            let block = BlockFrame::read_from(&mut data_reader, limits.max_frame_payload_size)?;
            let contents = if block.flags & crate::BLOCK_FLAG_ENCRYPTED != 0 {
                block.uncompressed_encrypted(
                    commit_header,
                    commit
                        .key
                        .as_deref()
                        .ok_or("join password is unavailable")?,
                    algorithm.ok_or("encrypted commit is missing its algorithm")?,
                    limits.max_uncompressed_block_size,
                )?
            } else {
                block.uncompressed_at_most(limits.max_uncompressed_block_size)?
            };
            progress.inc(contents.len() as u64);
            blocks.insert(block.block_index, contents);
        }
        super::meta::merge_custom_metadata(&mut custom_metadata, &header.metadata);
        Toc::check_available_blocks(&header.changes, &commit.block_indices)?;
        joined_toc.apply_archive_commit(commit.header.commit_id, &header.changes)?;
        let metadata: HashMap<_, _> = header
            .entry_metadata
            .iter()
            .map(|entry| (&entry.path, entry))
            .collect();
        for change in &header.changes {
            match change {
                TocChange::PutFile { path, .. }
                | TocChange::PutDirectory { path }
                | TocChange::PutSymlink { path, .. }
                | TocChange::PutNamedPipe { path }
                | TocChange::PutDevice { path, .. } => {
                    if let Some(entry) = metadata.get(path) {
                        joined_metadata.insert(path.clone(), (*entry).clone());
                    } else {
                        joined_metadata.remove(path);
                    }
                }
                TocChange::RemoveFile { path } => {
                    joined_metadata.remove(path);
                }
                TocChange::RemoveDirectory { path } => {
                    let prefix = format!("{}/", path.as_str());
                    joined_metadata.retain(|existing, _| {
                        existing != path && !existing.as_str().starts_with(&prefix)
                    });
                }
            }
            match change {
                TocChange::PutFile { path, entry } => {
                    if let Some(source) = entry.source_commit
                        && !selected_ids.contains(&source)
                    {
                        let source_commit = commits
                            .iter()
                            .find(|candidate| candidate.header.commit_id == source)
                            .ok_or("missing reference source commit")?;
                        let output_data_encrypted = encryption.is_some_and(|(_, _, scope)| {
                            matches!(
                                scope,
                                EncryptionScope::Data | EncryptionScope::HeaderAndData
                            )
                        });
                        if source_commit.encryption.is_data_encrypted() != output_data_encrypted {
                            return Err(format!("cannot preserve reference for {path}: source and joined commit have different data encryption").into());
                        }
                        if source_commit.encryption.is_header_encrypted()
                            || source_commit.encryption.is_data_encrypted()
                        {
                            let password = encryption.map(|(password, _, _)| password).ok_or("cannot preserve an encrypted source reference in an unencrypted joined commit")?;
                            let offset = source_commit
                                .data_start
                                .checked_add(source_commit.header.data_section_size)
                                .ok_or("source header offset overflow")?;
                            input.seek(io::SeekFrom::Start(offset))?;
                            if read_header_with_password(&mut input, limits, Some(password))
                                .is_err()
                            {
                                return Err(format!("cannot preserve reference for {path}: source uses a different password").into());
                            }
                        }
                        written_files.remove(path);
                        written_references.insert(path.clone(), entry.clone());
                        continue;
                    }
                    written_references.remove(path);
                    let source_blocks = match entry.source_commit {
                        Some(source) => historical_blocks
                            .get(&source)
                            .ok_or("missing joined source commit")?,
                        None => &blocks,
                    };
                    let mut contents = Vec::new();
                    for index in &entry.block_indices {
                        let block = source_blocks
                            .get(index)
                            .ok_or_else(|| format!("TOC references unavailable block {index}"))?;
                        if let Some(offset) = entry.offset_in_block {
                            if entry.block_indices.len() != 1 {
                                return Err(format!(
                                    "packed file {path} must reference exactly one block"
                                )
                                .into());
                            }
                            let start = usize::try_from(offset)
                                .map_err(|_| format!("packed file offset overflow for {path}"))?;
                            let end = start
                                .checked_add(
                                    usize::try_from(entry.size)
                                        .map_err(|_| format!("file size overflow for {path}"))?,
                                )
                                .filter(|&end| end <= block.len())
                                .ok_or_else(|| {
                                    format!("packed file range exceeds block for {path}")
                                })?;
                            contents.extend_from_slice(&block[start..end]);
                        } else {
                            contents.extend_from_slice(block);
                        }
                    }
                    if contents.len() as u64 != entry.size {
                        return Err(format!("decoded size mismatch for {path}").into());
                    }
                    written_files.insert(path.clone(), contents);
                }
                TocChange::RemoveFile { path } => {
                    written_files.remove(path);
                    written_references.remove(path);
                    joined_special.remove(path);
                }
                TocChange::RemoveDirectory { path } => {
                    let prefix = format!("{}/", path.as_str());
                    written_files.retain(|existing, _| {
                        existing != path && !existing.as_str().starts_with(&prefix)
                    });
                    written_references.retain(|existing, _| {
                        existing != path && !existing.as_str().starts_with(&prefix)
                    });
                    joined_special.retain(|existing, _| {
                        existing != path && !existing.as_str().starts_with(&prefix)
                    });
                }
                TocChange::PutDirectory { .. } => {}
                TocChange::PutSymlink { path, target } => {
                    joined_special.insert(
                        path.clone(),
                        TocEntry::Symlink {
                            target: target.clone(),
                        },
                    );
                }
                TocChange::PutNamedPipe { path } => {
                    joined_special.insert(path.clone(), TocEntry::NamedPipe);
                }
                TocChange::PutDevice {
                    path,
                    kind,
                    major,
                    minor,
                } => {
                    joined_special.insert(
                        path.clone(),
                        TocEntry::Device {
                            kind: *kind,
                            major: *major,
                            minor: *minor,
                        },
                    );
                }
            }
        }
        if referenced_selected_ids.contains(&commit_header.commit_id) {
            historical_blocks.insert(commit_header.commit_id, blocks);
        }
    }
    let mut changes = Vec::new();
    let mut rewritten_paths = std::collections::HashSet::new();
    for (path, entry) in prefix_toc.iter() {
        match (entry, joined_toc.get(path)) {
            (TocEntry::Directory, Some(TocEntry::Directory))
            | (TocEntry::File(_), Some(TocEntry::File(_)))
            | (TocEntry::Symlink { .. }, Some(TocEntry::Symlink { .. }))
            | (TocEntry::NamedPipe, Some(TocEntry::NamedPipe))
            | (TocEntry::Device { .. }, Some(TocEntry::Device { .. })) => {}
            (TocEntry::Directory, _) => {
                changes.push(TocChange::RemoveDirectory { path: path.clone() })
            }
            (
                TocEntry::File(_)
                | TocEntry::Symlink { .. }
                | TocEntry::NamedPipe
                | TocEntry::Device { .. },
                _,
            ) => {
                changes.push(TocChange::RemoveFile { path: path.clone() });
                rewritten_paths.insert(path.clone());
            }
        }
    }
    for (path, entry) in joined_toc.iter() {
        if !matches!(entry, TocEntry::File(_) | TocEntry::Directory) {
            let unchanged = prefix_toc.get(path) == Some(entry) && !rewritten_paths.contains(path);
            if !unchanged {
                match entry {
                    TocEntry::Symlink { target } => changes.push(TocChange::PutSymlink {
                        path: path.clone(),
                        target: target.clone(),
                    }),
                    TocEntry::NamedPipe => {
                        changes.push(TocChange::PutNamedPipe { path: path.clone() })
                    }
                    TocEntry::Device { kind, major, minor } => changes.push(TocChange::PutDevice {
                        path: path.clone(),
                        kind: *kind,
                        major: *major,
                        minor: *minor,
                    }),
                    TocEntry::File(_) | TocEntry::Directory => unreachable!(),
                }
            }
            continue;
        }
        match entry {
            TocEntry::Directory if !matches!(prefix_toc.get(path), Some(TocEntry::Directory)) => {
                changes.push(TocChange::PutDirectory { path: path.clone() });
            }
            TocEntry::File(file_entry)
                if !matches!(prefix_toc.get(path), Some(TocEntry::File(_)))
                    || written_files.contains_key(path)
                    || written_references.contains_key(path) =>
            {
                changes.push(TocChange::PutFile {
                    path: path.clone(),
                    entry: file_entry.clone(),
                });
            }
            _ => {}
        }
    }

    let id = TEMP_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let unique_id = unique_temp_file_id()?;
    let stem = archive_path
        .file_name()
        .ok_or("archive path has no filename")?
        .to_string_lossy();
    let _staged_commit = parent.join(format!(".{stem}.zarc-join-{unique_id}-{id}.tmp"));
    let staged_archive = parent.join(format!(".{stem}.zarc-join-full-{unique_id}-{id}.tmp"));
    let mut cleanup = TemporaryFiles(Vec::new());
    let mut data = Vec::new();
    let mut next_block_index = 0_u32;
    let mut block_sizes = std::collections::HashMap::<u32, u64>::new();
    let mut block_locations = Vec::new();
    let mut small_packer = SmallFilePacker::new(options.block_size);
    let mut joined_dedup = HashMap::<(u64, u64), Vec<(ArchivePath, FileEntry)>>::new();
    let new_commit_id = next_commit_id(archive_path)?;
    let (dek, descriptor, scope, data_encryption) = match encryption {
        Some((password, algorithm, scope)) => {
            if password.is_empty() {
                return Err("the encryption password must not be empty".into());
            }
            let dek = Zeroizing::new(generate_dek()?);
            let descriptor = EncryptionDescriptor::Password {
                scope,
                algorithm,
                nonce: generate_nonce()?,
                key_wrap: wrap_dek_with_password(password, &dek, algorithm, 65_536, 3, 1)?,
            }
            .encode()?;
            let provisional = CommitHeader::new(0, new_commit_id, 0, 0)?;
            let data_encryption = matches!(
                scope,
                EncryptionScope::Data | EncryptionScope::HeaderAndData
            )
            .then(|| EncryptionContext::new(Zeroizing::new(*dek), provisional, algorithm));
            (Some(dek), descriptor, Some(scope), data_encryption)
        }
        None => (None, UNENCRYPTED_DESCRIPTOR.to_vec(), None, None),
    };
    for change in &mut changes {
        if let TocChange::PutFile { path, entry } = change {
            if let Some(reference) = written_references.get(path) {
                *entry = reference.clone();
                continue;
            }
            let contents = written_files
                .get(path)
                .ok_or_else(|| format!("missing data for joined file {path}"))?;
            let mut hasher = Xxh3::new();
            hasher.update(contents);
            let checksum = hasher.digest();
            if let Some(existing) =
                joined_dedup
                    .get(&(entry.size, checksum))
                    .and_then(|candidates| {
                        candidates
                            .iter()
                            .find(|(candidate, _)| written_files.get(candidate) == Some(contents))
                    })
            {
                *entry = existing.1.clone();
                continue;
            }
            if entry.size > 8 && entry.size < options.block_size as u64 {
                let (block_index, offset_in_block, completed) = small_packer
                    .add(contents.clone(), checksum, false, || {
                        let index = next_block_index;
                        next_block_index = next_block_index
                            .checked_add(1)
                            .ok_or_else(|| "too many joined blocks".to_owned())?;
                        Ok(index)
                    })
                    .map_err(io::Error::other)?;
                if let Some(block) = completed {
                    write_joined_block(
                        &mut data,
                        &mut block_sizes,
                        &mut block_locations,
                        block,
                        options.compression_level,
                        data_encryption.as_ref(),
                    )?;
                }
                *entry = FileEntry::new(entry.size, vec![block_index])?
                    .with_block_offset(offset_in_block)
                    .with_checksum(checksum);
                joined_dedup
                    .entry((entry.size, checksum))
                    .or_default()
                    .push((path.clone(), entry.clone()));
                continue;
            }
            let mut indices = Vec::new();
            for chunk in contents.chunks(options.block_size) {
                let index = next_block_index;
                next_block_index = next_block_index
                    .checked_add(1)
                    .ok_or("too many joined blocks")?;
                let block = encode_block(
                    index,
                    chunk,
                    options.compression_level,
                    false,
                    data_encryption.as_ref(),
                )?;
                block_sizes.insert(index, block.payload.len() as u64);
                write_joined_frame(&mut data, &mut block_locations, &block)?;
                indices.push(index);
            }
            *entry = FileEntry::new(entry.size, indices)?;
            if entry.size != 0 {
                entry.checksum = Some(checksum);
            }
            joined_dedup
                .entry((entry.size, checksum))
                .or_default()
                .push((path.clone(), entry.clone()));
        }
    }
    if let Some(block) = small_packer.take_block().map_err(io::Error::other)? {
        write_joined_block(
            &mut data,
            &mut block_sizes,
            &mut block_locations,
            block,
            options.compression_level,
            data_encryption.as_ref(),
        )?;
    }
    let entry_metadata = changes
        .iter()
        .filter_map(|change| match change {
            TocChange::PutFile { path, .. }
            | TocChange::PutDirectory { path }
            | TocChange::PutSymlink { path, .. }
            | TocChange::PutNamedPipe { path }
            | TocChange::PutDevice { path, .. } => joined_metadata.remove(path),
            TocChange::RemoveFile { .. } | TocChange::RemoveDirectory { .. } => None,
        })
        .collect();
    let logical_header = LogicalHeader::with_block_size(changes.clone(), options.block_size as u32)
        .with_block_index(block_locations)
        .with_entry_metadata(entry_metadata);
    let mut group_sizes = group_size_metadata(&logical_header, &block_sizes)?;
    group_sizes.extend(custom_metadata);
    group_sizes.extend(commit_metadata(None)?);
    let header_bytes = logical_header.with_metadata(group_sizes).encode()?;
    let data_section_size = data.len() as u64;
    let temp_ch = CommitHeader::new(0, new_commit_id, data_section_size, 0)?;
    let encrypted_header = matches!(
        scope,
        Some(EncryptionScope::Header | EncryptionScope::HeaderAndData)
    );
    let make_header = |commit_header| -> Result<HeaderFrame, Box<dyn std::error::Error>> {
        if encrypted_header {
            Ok(HeaderFrame::from_uncompressed_encrypted(
                commit_header,
                &header_bytes,
                descriptor.clone(),
                options.compression_level,
                dek.as_deref()
                    .ok_or("encrypted output is missing its key")?,
            )?)
        } else {
            Ok(HeaderFrame::from_uncompressed(
                commit_header,
                &header_bytes,
                descriptor.clone(),
                options.compression_level,
            )?)
        }
    };
    let staged_header = make_header(temp_ch)?;
    let header_size = u32::try_from(staged_header.encoded_size()?)?;
    let commit_header = CommitHeader::new(0, new_commit_id, data_section_size, header_size)?;
    let header = make_header(commit_header.clone())?;

    let selected_set = selected
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let merged_body_start = data.len() as u64;
    let mut merged_body = data;
    header.write_to(&mut merged_body)?;
    let merged_body = merged_body;

    let mut output = OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(&staged_archive)?;
    cleanup.0.push(staged_archive.clone());
    crate::ContainerHeader::new(0).write_to(&mut output)?;
    let mut previous_link_position = None;
    let mut output_commits = 0usize;

    for (index, commit) in commits.iter().enumerate() {
        if index == first_selected {
            let link_position = output.stream_position()?;
            output.write_all(&0u64.to_le_bytes())?;
            let data_start = output.stream_position()?;
            output.write_all(&merged_body)?;
            let merged_header_offset = data_start
                .checked_add(merged_body_start)
                .ok_or("joined header offset overflow")?;
            output.write_all(&merged_header_offset.to_le_bytes())?;
            if let Some(position) = previous_link_position {
                let end = output.stream_position()?;
                output.seek(SeekFrom::Start(position))?;
                output.write_all(&merged_header_offset.to_le_bytes())?;
                output.seek(SeekFrom::Start(end))?;
            } else {
                output.seek(SeekFrom::Start(4))?;
                output.write_all(&merged_header_offset.to_le_bytes())?;
                output.seek(SeekFrom::End(0))?;
            }
            previous_link_position = Some(link_position);
            output_commits += 1;
        }
        if selected_set.contains(&index) {
            continue;
        }

        let link_position = output.stream_position()?;
        output.write_all(&0u64.to_le_bytes())?;
        let data_start = output.stream_position()?;
        let body_size = commit
            .commit_end
            .checked_sub(commit.data_start)
            .and_then(|size| size.checked_sub(HEADER_OFFSET_SIZE))
            .ok_or("source commit body size underflow")?;
        input.seek(SeekFrom::Start(commit.data_start))?;
        io::copy(&mut (&mut input).take(body_size), &mut output)?;
        let output_header_offset = data_start
            .checked_add(commit.header.data_section_size)
            .ok_or("copied header offset overflow")?;
        output.write_all(&output_header_offset.to_le_bytes())?;
        if let Some(position) = previous_link_position {
            let end = output.stream_position()?;
            output.seek(SeekFrom::Start(position))?;
            output.write_all(&output_header_offset.to_le_bytes())?;
            output.seek(SeekFrom::Start(end))?;
        } else {
            output.seek(SeekFrom::Start(4))?;
            output.write_all(&output_header_offset.to_le_bytes())?;
            output.seek(SeekFrom::End(0))?;
        }
        previous_link_position = Some(link_position);
        output_commits += 1;
    }

    if output_commits == 0 {
        return Err("joining would leave an archive with no commits".into());
    }
    output.flush()?;
    drop(output);

    let mut validation = File::open(&staged_archive)?;
    let mut validation_passwords = PasswordSet::default();
    for commit in &commits {
        if let Some(password) = passwords.password_for(&commit.header.commit_id) {
            validation_passwords.insert_for_commit(commit.header.commit_id, password.to_vec());
        }
    }
    if let Some((password, _, _)) = encryption {
        validation_passwords.insert_for_commit(new_commit_id, password.to_vec());
    }
    let mut output_offset = read_first_header_offset(&mut validation)?;
    let mut validation_count = 0usize;
    while output_offset != 0 {
        let (ch, _, next_offset) = read_commit_at_offset(&mut validation, output_offset, limits)?;
        validation.seek(SeekFrom::Start(output_offset))?;
        let (validated, _) =
            read_header_with_passwords(&mut validation, limits, &validation_passwords)?;
        if ch.commit_id == new_commit_id && validated.logical_header.is_none() {
            return Err("joined commit failed logical-header validation".into());
        }
        validation_count += 1;
        output_offset = next_offset;
    }
    if validation_count != output_commits {
        return Err("staged archive has an unexpected number of commits".into());
    }
    let _ = load_toc_through_with_passwords(
        staged_archive
            .to_str()
            .ok_or("archive path is not valid UTF-8")?,
        None,
        &validation_passwords,
    )?;
    drop(validation);
    drop(input);
    std::fs::rename(&staged_archive, archive_path)?;
    cleanup.0.retain(|path| path != &staged_archive);
    drop(cleanup);
    Ok(())
}
