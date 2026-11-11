use super::common::*;
use super::info::group_size_metadata;
use super::list::load_toc_through_with_passwords;
use super::pipeline::{BlockWorkItem, SmallFilePacker, encode_block};
use crate::{
    ArchivePath, BlockFrame, CommitHeader, CommitValidationLimits, FileEntry, HEADER_OFFSET_SIZE,
    HeaderFrame, LogicalHeader, Toc, TocChange, TocEntry, UNENCRYPTED_DESCRIPTOR, ValidatedCommit,
};
use std::collections::{BTreeMap, HashMap};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use xxhash_rust::xxh64::Xxh64;
use zeroize::Zeroizing;

struct JoinCommit {
    header: CommitHeader,
    data_start: u64,
    commit_end: u64,
    validated: ValidatedCommit,
    key: Option<Zeroizing<[u8; crate::DEK_SIZE]>>,
}

fn write_joined_block(
    data: &mut Vec<u8>,
    block_sizes: &mut HashMap<u32, u64>,
    block: BlockWorkItem,
    compression_level: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    let frame = encode_block(
        block.block_index,
        &block.data,
        compression_level,
        block.skip_compression,
        None,
    )?;
    block_sizes.insert(
        block.block_index,
        u64::try_from(frame.payload.len()).map_err(|_| "joined block size overflow")?,
    );
    frame.write_to(data)?;
    Ok(())
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
    mut options: EncodingOptions,
    progress: Option<&dyn Progress>,
    passwords: &PasswordSet,
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
        let (validated, key) =
            read_commit_with_passwords(&mut input, commit_header.clone(), limits, passwords)?;
        let commit_end = header_offset
            .checked_add(u64::from(commit_header.header_frame_size))
            .and_then(|size| size.checked_add(HEADER_OFFSET_SIZE))
            .ok_or("commit end overflow")?;
        progress.inc(commit_end.saturating_sub(data_start));
        progress.set_message(format!(
            "Joining commit {:08x}",
            u32::from_le_bytes(commit_header.commit_id)
        ));
        commits.push(JoinCommit {
            header: commit_header,
            data_start,
            commit_end,
            validated,
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
            commit.validated.logical_header.is_some()
                && (!commit.validated.encryption.is_data_encrypted() || commit.key.is_some())
        })
        .collect();
    if selected.is_empty() {
        return Err(
            "the selected range contains no commits readable with the supplied passwords".into(),
        );
    }

    let first_selected = selected[0];
    let mut prefix_toc = Toc::default();
    for commit in &commits[..first_selected] {
        let header = commit
            .validated
            .logical_header
            .as_ref()
            .ok_or("cannot build join prefix while a preceding commit header is encrypted")?;
        prefix_toc
            .apply_commit_with_available_blocks(&header.changes, &commit.validated.block_indices)?;
    }
    let mut joined_toc = prefix_toc.clone();
    let mut written_files = BTreeMap::<ArchivePath, Vec<u8>>::new();
    let mut custom_metadata = HashMap::new();
    let mut joined_special = BTreeMap::<ArchivePath, TocEntry>::new();
    let limits = CommitValidationLimits::default();
    for &index in &selected {
        let commit = &commits[index];
        let commit_header = &commit.header;
        let header = commit.validated.logical_header.as_ref().unwrap();
        let algorithm = match &commit.validated.encryption {
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
        let mut blocks = HashMap::with_capacity(commit.validated.block_count as usize);
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
        joined_toc
            .apply_commit_with_available_blocks(&header.changes, &commit.validated.block_indices)?;
        for change in &header.changes {
            match change {
                TocChange::PutFile { path, entry } => {
                    let mut contents = Vec::new();
                    for index in &entry.block_indices {
                        let block = blocks
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
                    joined_special.remove(path);
                }
                TocChange::RemoveDirectory { path } => {
                    let prefix = format!("{}/", path.as_str());
                    written_files.retain(|existing, _| {
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
                    || written_files.contains_key(path) =>
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
    let mut small_packer = SmallFilePacker::new(options.block_size);
    for change in &mut changes {
        if let TocChange::PutFile { path, entry } = change {
            let contents = written_files
                .get(path)
                .ok_or_else(|| format!("missing data for joined file {path}"))?;
            if entry.size > 8 && entry.size < options.block_size as u64 {
                let mut hasher = Xxh64::new(0);
                hasher.update(contents);
                let checksum = hasher.digest();
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
                        block,
                        options.compression_level,
                    )?;
                }
                *entry = FileEntry::new(entry.size, vec![block_index])?
                    .with_block_offset(offset_in_block)
                    .with_checksum(checksum);
                continue;
            }
            let mut indices = Vec::new();
            for chunk in contents.chunks(options.block_size) {
                let index = next_block_index;
                next_block_index = next_block_index
                    .checked_add(1)
                    .ok_or("too many joined blocks")?;
                let block = BlockFrame::from_uncompressed(index, chunk, options.compression_level)?;
                block_sizes.insert(index, block.payload.len() as u64);
                block.write_to(&mut data)?;
                indices.push(index);
            }
            *entry = FileEntry::new(entry.size, indices)?;
        }
    }
    if let Some(block) = small_packer.take_block().map_err(io::Error::other)? {
        write_joined_block(
            &mut data,
            &mut block_sizes,
            block,
            options.compression_level,
        )?;
    }
    let logical_header = LogicalHeader::with_block_size(changes.clone(), options.block_size as u32);
    let mut group_sizes = group_size_metadata(&logical_header, &block_sizes)?;
    group_sizes.extend(custom_metadata);
    group_sizes.extend(commit_metadata(None)?);
    let header_bytes = logical_header.with_metadata(group_sizes).encode()?;
    let data_section_size = data.len() as u64;
    let new_commit_id = next_commit_id(archive_path)?;
    let temp_ch = CommitHeader::new(0, new_commit_id, data_section_size, 0)?;
    let staged_header = HeaderFrame::from_uncompressed(
        temp_ch,
        &header_bytes,
        UNENCRYPTED_DESCRIPTOR.to_vec(),
        options.compression_level,
    )?;
    let header_size = u32::try_from(staged_header.encoded_size()?)?;
    let commit_header = CommitHeader::new(0, new_commit_id, data_section_size, header_size)?;
    let header = HeaderFrame::from_uncompressed(
        commit_header.clone(),
        &header_bytes,
        UNENCRYPTED_DESCRIPTOR.to_vec(),
        options.compression_level,
    )?;

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
    let mut output_offset = read_first_header_offset(&mut validation)?;
    let mut validation_count = 0usize;
    while output_offset != 0 {
        let (ch, _, next_offset) = read_commit_at_offset(&mut validation, output_offset, limits)?;
        let (validated, _) =
            read_commit_with_passwords(&mut validation, ch.clone(), limits, passwords)?;
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
        passwords,
    )?;
    drop(validation);
    drop(input);
    std::fs::rename(&staged_archive, archive_path)?;
    cleanup.0.retain(|path| path != &staged_archive);
    drop(cleanup);
    Ok(())
}
