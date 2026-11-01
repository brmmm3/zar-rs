use super::common::*;
use super::info::group_size_metadata;
use super::list::load_toc;
use crate::{
    ArchivePath, BlockFrame, CommitHeader, CommitValidationLimits, FileEntry, HEADER_OFFSET_SIZE,
    HeaderFrame, LogicalHeader, Toc, TocChange, TocEntry, UNENCRYPTED_DESCRIPTOR, ValidatedCommit,
};
use std::collections::{BTreeMap, HashMap};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub fn join_archive_with_encoding(
    archive: &str,
    from_id: &str,
    through_id: &str,
    options: EncodingOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    join_archive_with_encoding_progress(archive, from_id, through_id, options, None)
}

pub fn join_archive_with_encoding_progress(
    archive: &str,
    from_id: &str,
    through_id: &str,
    mut options: EncodingOptions,
    progress: Option<&dyn Progress>,
) -> Result<(), Box<dyn std::error::Error>> {
    let encoding_warning = resolve_encoding_options(&mut options);
    if let (Some(progress), Some(warning)) = (progress, encoding_warning.as_deref()) {
        progress.warning(warning);
    }
    let progress = ProgressReporter::new(progress);
    let from_id = parse_commit_id(from_id)?;
    let through_id = parse_commit_id(through_id)?;
    let archive_path = Path::new(archive);
    let parent = archive_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err(format!("archive parent is not a directory: {}", parent.display()).into());
    }
    ensure_archive_unencrypted(archive_path)?;

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
        let validated = ValidatedCommit::read_body(&mut input, commit_header.clone(), limits)?;
        let commit_end = header_offset
            .checked_add(u64::from(commit_header.header_frame_size))
            .and_then(|size| size.checked_add(HEADER_OFFSET_SIZE))
            .ok_or("commit end overflow")?;
        progress.inc(commit_end.saturating_sub(data_start));
        progress.set_message(format!(
            "Joining commit {:08x}",
            u32::from_le_bytes(commit_header.commit_id)
        ));
        commits.push((
            header_offset,
            commit_header,
            data_start,
            commit_end,
            validated,
        ));
        header_offset = next_header_offset;
    }
    let matching_indices = |id: &[u8; 4]| {
        commits
            .iter()
            .enumerate()
            .filter_map(|(index, (_, ch, _, _, _commit))| (ch.commit_id == *id).then_some(index))
            .collect::<Vec<_>>()
    };
    let from_matches = matching_indices(&from_id);
    let through_matches = matching_indices(&through_id);
    if from_matches.len() != 1 || through_matches.len() != 1 {
        return Err("commit IDs must each identify exactly one archive commit".into());
    }
    let first = from_matches[0];
    let last = through_matches[0];
    if first > last {
        return Err("--from commit must not appear after --through commit".into());
    }

    let mut prefix_toc = Toc::default();
    for (_, _, _, _, commit) in &commits[..first] {
        let header = commit
            .logical_header
            .as_ref()
            .ok_or("cannot join when preceding commit headers are encrypted")?;
        prefix_toc.apply_commit_with_available_blocks(&header.changes, &commit.block_indices)?;
    }
    let mut joined_toc = prefix_toc.clone();
    let mut written_files = BTreeMap::<ArchivePath, Vec<u8>>::new();
    let mut custom_metadata = HashMap::new();
    let mut joined_special = BTreeMap::<ArchivePath, TocEntry>::new();
    let limits = CommitValidationLimits::default();
    for (_header_offset, commit_header, data_start, _, commit) in &commits[first..=last] {
        let header = commit
            .logical_header
            .as_ref()
            .ok_or("cannot join a commit with an encrypted header")?;
        if commit.encryption.is_data_encrypted() {
            return Err("cannot join data-encrypted commits".into());
        }
        input.seek(io::SeekFrom::Start(*data_start))?;
        let mut data_reader = (&mut input).take(commit_header.data_section_size);
        let mut blocks = HashMap::with_capacity(commit.block_count as usize);
        while data_reader.limit() != 0 {
            let block = BlockFrame::read_from(&mut data_reader, limits.max_frame_payload_size)?;
            let contents = block.uncompressed_at_most(limits.max_uncompressed_block_size)?;
            progress.inc(contents.len() as u64);
            blocks.insert(block.block_index, contents);
        }
        super::meta::merge_custom_metadata(&mut custom_metadata, &header.metadata);
        joined_toc.apply_commit_with_available_blocks(&header.changes, &commit.block_indices)?;
        for change in &header.changes {
            match change {
                TocChange::PutFile { path, entry } => {
                    let mut contents = Vec::new();
                    for index in &entry.block_indices {
                        contents.extend_from_slice(
                            blocks.get(index).ok_or_else(|| {
                                format!("TOC references unavailable block {index}")
                            })?,
                        );
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
    if commits[last + 1..]
        .iter()
        .any(|(_, _, _, _, commit)| commit.logical_header.is_none())
    {
        return Err("cannot validate joined state with a later header-encrypted commit".into());
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
    for change in &mut changes {
        if let TocChange::PutFile { path, entry } = change {
            let contents = written_files
                .get(path)
                .ok_or_else(|| format!("missing data for joined file {path}"))?;
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
    let logical_header = LogicalHeader::with_block_size(changes.clone(), options.block_size as u32);
    let mut group_sizes = group_size_metadata(&logical_header, &block_sizes)?;
    group_sizes.extend(custom_metadata);
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

    let (_, _, range_start, _, _) = &commits[first];
    let range_start = range_start
        .checked_sub(HEADER_OFFSET_SIZE)
        .ok_or("range start underflow")?;
    let (_, _, _, range_end, _) = &commits[last];

    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged_archive)?;
    cleanup.0.push(staged_archive.clone());
    input.seek(io::SeekFrom::Start(0))?;
    io::copy(&mut (&mut input).take(range_start), &mut output)?;
    output.write_all(&0u64.to_le_bytes())?;
    output.write_all(&data)?;
    let new_header_offset = output.stream_position()?;
    header.write_to(&mut output)?;
    output.write_all(&new_header_offset.to_le_bytes())?;
    input.seek(io::SeekFrom::Start(*range_end))?;
    io::copy(&mut input, &mut output)?;
    output.flush()?;
    drop(output);

    if first == 0 {
        let mut f = OpenOptions::new().write(true).open(&staged_archive)?;
        f.seek(SeekFrom::Start(4))?;
        f.write_all(&new_header_offset.to_le_bytes())?;
        f.flush()?;
    } else {
        let (_, prev_commit_header, _, _, _) = &commits[first - 1];
        let prev_next_offset_pos = commits[first - 1]
            .0
            .checked_sub(prev_commit_header.data_section_size)
            .and_then(|pos| pos.checked_sub(HEADER_OFFSET_SIZE))
            .ok_or("previous commit offset underflow")?;
        let mut f = OpenOptions::new().write(true).open(&staged_archive)?;
        f.seek(SeekFrom::Start(prev_next_offset_pos))?;
        f.write_all(&new_header_offset.to_le_bytes())?;
        f.flush()?;
    }

    if last + 1 < commits.len() {
        let header_enc_size =
            u64::try_from(header.encoded_size()?).map_err(|_| "header size overflow")?;
        let new_commit_size = (new_header_offset - range_start)
            .checked_add(header_enc_size)
            .and_then(|size| size.checked_add(HEADER_OFFSET_SIZE))
            .ok_or("new commit size overflow")?;
        let old_range_size = range_end
            .checked_sub(range_start)
            .ok_or("range size underflow")?;
        let delta = (new_commit_size as i128) - (old_range_size as i128);

        let next_commit = &commits[last + 1];
        let next_commit_new_offset = new_header_offset
            .checked_add(header_enc_size)
            .and_then(|size| size.checked_add(HEADER_OFFSET_SIZE))
            .and_then(|size| size.checked_add(HEADER_OFFSET_SIZE))
            .and_then(|size| size.checked_add(next_commit.1.data_section_size))
            .ok_or("next commit offset overflow")?;

        let mut f = OpenOptions::new().write(true).open(&staged_archive)?;
        f.seek(SeekFrom::Start(range_start))?;
        f.write_all(&next_commit_new_offset.to_le_bytes())?;
        f.flush()?;
        drop(f);

        let mut current_offset = next_commit_new_offset;
        for (_suffix_header_offset, suffix_commit_header, _, _, _) in &commits[(last + 1)..] {
            let next_offset_pos = current_offset
                .checked_sub(suffix_commit_header.data_section_size)
                .and_then(|pos| pos.checked_sub(HEADER_OFFSET_SIZE))
                .ok_or("suffix next_header_offset position underflow")?;
            let mut f = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&staged_archive)?;
            f.seek(SeekFrom::Start(next_offset_pos))?;
            let mut next_bytes = [0u8; 8];
            f.read_exact(&mut next_bytes)?;
            let old_next = u64::from_le_bytes(next_bytes);
            let new_next = if old_next == 0 {
                0
            } else {
                let adjusted = (old_next as i128) + delta;
                if adjusted < 0 {
                    return Err("suffix offset underflow during adjustment".into());
                }
                adjusted as u64
            };
            f.seek(SeekFrom::Start(next_offset_pos))?;
            f.write_all(&new_next.to_le_bytes())?;
            f.flush()?;
            drop(f);

            let this_offset_pos = current_offset
                .checked_add(u64::from(suffix_commit_header.header_frame_size))
                .ok_or("suffix this_header_offset position overflow")?;
            let mut f = OpenOptions::new().write(true).open(&staged_archive)?;
            f.seek(SeekFrom::Start(this_offset_pos))?;
            f.write_all(&current_offset.to_le_bytes())?;
            f.flush()?;
            drop(f);

            current_offset = if new_next == 0 {
                break;
            } else {
                new_next
            };
        }
    }

    let mut validation = File::open(&staged_archive)?;
    let first_header_offset = read_first_header_offset(&mut validation)?;
    let mut header_offset = first_header_offset;
    while header_offset != 0 {
        let (ch, _, next_header_offset) =
            read_commit_at_offset(&mut validation, header_offset, limits)?;
        ValidatedCommit::read_body(&mut validation, ch, limits)?;
        header_offset = next_header_offset;
    }
    let _ = load_toc(&mut File::open(&staged_archive)?)?;
    drop(validation);
    drop(input);
    std::fs::rename(&staged_archive, archive_path)?;
    cleanup.0.retain(|path| path != &staged_archive);
    drop(cleanup);
    Ok(())
}
