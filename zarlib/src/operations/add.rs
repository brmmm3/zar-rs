use super::common::*;
use super::filters::FilterSet;
use super::info::group_size_metadata;
use super::list::{load_overlay_toc, load_toc_through_with_passwords};
use super::pipeline::*;
use crate::{
    ArchivePath, BlockFrame, CommitHeader, CommitValidationLimits, ContentAead,
    EncryptionDescriptor, EncryptionScope, FileEntry, HEADER_OFFSET_SIZE, HeaderFrame,
    LogicalHeader, Toc, TocChange, UNENCRYPTED_DESCRIPTOR, generate_dek, generate_nonce,
    wrap_dek_with_password,
};
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use xxhash_rust::xxh3::Xxh3;
use zeroize::Zeroizing;

struct HistoricalCandidate {
    archive: PathBuf,
    commit: CommitHeader,
    data_start: u64,
    entry: FileEntry,
    index: Option<std::sync::Arc<Vec<crate::HeaderBlockIndexEntry>>>,
    algorithm: Option<ContentAead>,
    key: Option<Zeroizing<[u8; crate::DEK_SIZE]>>,
}

type HistoricalCandidates = HashMap<(u64, Option<u64>), Vec<HistoricalCandidate>>;

fn historical_candidates(
    archives: &[&str],
    sizes: &HashSet<u64>,
    passwords: &PasswordSet,
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
    progress: &ProgressReporter<'_>,
) -> Result<HistoricalCandidates, Box<dyn std::error::Error>> {
    let mut candidates = HashMap::<(u64, Option<u64>), Vec<HistoricalCandidate>>::new();
    let limits = CommitValidationLimits::default();
    let encrypted_output = encryption.is_some_and(|(_, _, scope)| {
        matches!(
            scope,
            EncryptionScope::Data | EncryptionScope::HeaderAndData
        )
    });
    for archive in archives {
        let mut file = File::open(archive)?;
        let mut offset = read_first_header_offset(&mut file)?;
        while offset != 0 {
            progress.check_cancelled()?;
            let (commit, data_start, next) = read_commit_at_offset(&mut file, offset, limits)?;
            file.seek(SeekFrom::Start(offset))?;
            let (mut validated, mut key) =
                read_header_with_passwords(&mut file, limits, passwords)?;
            let eligible = if encrypted_output {
                if validated.encryption.is_data_encrypted() {
                    file.seek(SeekFrom::Start(offset))?;
                    match read_header_with_password(
                        &mut file,
                        limits,
                        encryption.map(|(password, _, _)| password),
                    ) {
                        Ok((header, unlocked)) if unlocked.is_some() => {
                            validated = header;
                            key = unlocked;
                            true
                        }
                        _ => false,
                    }
                } else {
                    false
                }
            } else {
                !validated.encryption.is_data_encrypted()
                    && !validated.encryption.is_header_encrypted()
            };
            if eligible && let Some(header) = validated.logical_header {
                let algorithm = encryption_algorithm(&validated.encryption);
                let index = header.block_index.map(std::sync::Arc::new);
                for change in header.changes {
                    if let TocChange::PutFile { entry, .. } = change
                        && entry.source_commit.is_none()
                        && entry.size > 0
                        && sizes.contains(&entry.size)
                    {
                        candidates
                            .entry((entry.size, entry.checksum))
                            .or_default()
                            .push(HistoricalCandidate {
                                archive: PathBuf::from(archive),
                                commit: commit.clone(),
                                data_start,
                                entry,
                                index: index.clone(),
                                algorithm,
                                key: key.clone(),
                            });
                    }
                }
            }
            offset = next;
        }
    }
    Ok(candidates)
}

fn candidate_matches(
    candidate: &HistoricalCandidate,
    input: &Path,
    progress: &ProgressReporter<'_>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let limits = CommitValidationLimits::default();
    let mut archive = File::open(&candidate.archive)?;
    let mut offsets = HashMap::new();
    if let Some(index) = &candidate.index {
        for block in index.iter() {
            if block
                .offset
                .checked_add(block.encoded_size)
                .is_none_or(|end| end > candidate.commit.data_section_size)
            {
                return Err("source block index exceeds commit data".into());
            }
            offsets.insert(
                block.index,
                candidate
                    .data_start
                    .checked_add(block.offset)
                    .ok_or("source block offset overflow")?,
            );
        }
    } else {
        archive.seek(SeekFrom::Start(candidate.data_start))?;
        let end = candidate
            .data_start
            .checked_add(candidate.commit.data_section_size)
            .ok_or("source commit size overflow")?;
        while archive.stream_position()? < end {
            let offset = archive.stream_position()?;
            let block =
                BlockFrame::read_metadata_from(&mut archive, limits.max_frame_payload_size)?;
            if archive.stream_position()? > end
                || offsets.insert(block.block_index, offset).is_some()
            {
                return Err("invalid source block data".into());
            }
        }
    }
    let mut input = File::open(input)?;
    let mut total = 0_u64;
    for index in &candidate.entry.block_indices {
        progress.check_cancelled()?;
        archive.seek(SeekFrom::Start(
            *offsets.get(index).ok_or("missing source block")?,
        ))?;
        let frame = BlockFrame::read_from(&mut archive, limits.max_frame_payload_size)?;
        if frame.block_index != *index {
            return Err("source block index mismatch".into());
        }
        let decoded = if frame.flags & crate::BLOCK_FLAG_ENCRYPTED != 0 {
            frame.uncompressed_encrypted(
                &candidate.commit,
                candidate.key.as_ref().ok_or("missing source key")?,
                candidate.algorithm.ok_or("missing source algorithm")?,
                limits.max_uncompressed_block_size,
            )?
        } else {
            frame.uncompressed_at_most(limits.max_uncompressed_block_size)?
        };
        let bytes = if let Some(offset) = candidate.entry.offset_in_block {
            let start = usize::try_from(offset)?;
            let end = start
                .checked_add(usize::try_from(candidate.entry.size)?)
                .filter(|end| *end <= decoded.len())
                .ok_or("invalid source file offset")?;
            &decoded[start..end]
        } else {
            &decoded[..]
        };
        let mut actual = vec![0; bytes.len()];
        if input.read_exact(&mut actual).is_err() || actual != bytes {
            return Ok(false);
        }
        total = total
            .checked_add(bytes.len() as u64)
            .ok_or("source file size overflow")?;
    }
    let mut extra = [0; 1];
    Ok(total == candidate.entry.size && input.read(&mut extra)? == 0)
}

fn find_historical_duplicates(
    entries: &[TreeEntry],
    archives: &[&str],
    passwords: &PasswordSet,
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
    progress: &ProgressReporter<'_>,
) -> Result<HashMap<PathBuf, FileEntry>, Box<dyn std::error::Error>> {
    let sizes = entries
        .iter()
        .filter(|entry| matches!(entry.kind, TreeEntryKind::File) && entry.size > 0)
        .map(|entry| entry.size)
        .collect();
    let candidates = historical_candidates(archives, &sizes, passwords, encryption, progress)?;
    let mut duplicates = HashMap::new();
    for entry in entries
        .iter()
        .filter(|entry| matches!(entry.kind, TreeEntryKind::File) && entry.size > 0)
    {
        if !candidates.keys().any(|(size, _)| *size == entry.size) {
            continue;
        }
        let Ok(mut input) = File::open(&entry.source_path) else {
            continue;
        };
        let mut hasher = Xxh3::new();
        let mut buffer = vec![0; 128 * 1024];
        let mut size = 0_u64;
        loop {
            progress.check_cancelled()?;
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
            size = size
                .checked_add(count as u64)
                .ok_or("input file size overflow")?;
        }
        if size != entry.size {
            return Err(format!(
                "input file changed while being archived: {}",
                entry.source_path.display()
            )
            .into());
        }
        for candidate in candidates
            .get(&(size, Some(hasher.digest())))
            .into_iter()
            .chain(candidates.get(&(size, None)))
            .flatten()
        {
            if candidate_matches(candidate, &entry.source_path, progress)? {
                duplicates.insert(
                    entry.source_path.clone(),
                    candidate
                        .entry
                        .clone()
                        .with_source_commit(candidate.commit.commit_id),
                );
                break;
            }
        }
    }
    Ok(duplicates)
}

fn copy_with_cancellation(
    reader: &mut impl Read,
    writer: &mut impl Write,
    progress: Option<&dyn Progress>,
) -> io::Result<u64> {
    let progress = ProgressReporter::new(progress);
    let mut buffer = vec![0; 128 * 1024];
    let mut copied = 0u64;
    loop {
        progress.check_cancelled()?;
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            return Ok(copied);
        }
        writer.write_all(&buffer[..count])?;
        copied = copied
            .checked_add(count as u64)
            .ok_or_else(|| io::Error::other("copied data size overflow"))?;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AddInputOptions {
    pub save_metadata: bool,
    pub save_modification_time: bool,
    pub save_creation_time: bool,
    pub save_access_time: bool,
    pub save_user: bool,
    pub include_special_files: bool,
    pub strict: bool,
    pub verbose: bool,
}

pub fn add_archive(archive: &str, paths: &[PathBuf]) -> Result<(), Box<dyn std::error::Error>> {
    add_archive_with_encoding(archive, paths, EncodingOptions::default())
}

pub fn add_archive_with_encoding(
    archive: &str,
    paths: &[PathBuf],
    options: EncodingOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    add_archive_with_encoding_progress(archive, paths, options, None, false)
}

pub fn add_archive_with_encoding_progress(
    archive: &str,
    paths: &[PathBuf],
    options: EncodingOptions,
    progress: Option<&dyn Progress>,
    save_metadata: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    add_archive_with_encoding_progress_and_options(
        archive,
        paths,
        options,
        progress,
        AddInputOptions {
            save_metadata,
            save_modification_time: false,
            save_creation_time: false,
            save_access_time: false,
            save_user: false,
            include_special_files: true,
            strict: false,
            verbose: false,
        },
    )
}

pub fn add_archive_with_encoding_progress_and_special_files(
    archive: &str,
    paths: &[PathBuf],
    options: EncodingOptions,
    progress: Option<&dyn Progress>,
    save_metadata: bool,
    include_special_files: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    add_archive_with_encoding_progress_and_options(
        archive,
        paths,
        options,
        progress,
        AddInputOptions {
            save_metadata,
            save_modification_time: false,
            save_creation_time: false,
            save_access_time: false,
            save_user: false,
            include_special_files,
            strict: false,
            verbose: false,
        },
    )
}

pub fn add_archive_with_encoding_progress_and_options(
    archive: &str,
    paths: &[PathBuf],
    options: EncodingOptions,
    progress: Option<&dyn Progress>,
    input_options: AddInputOptions,
) -> Result<(), Box<dyn std::error::Error>> {
    add_archive_with_encoding_progress_and_options_and_filters(
        archive,
        paths,
        options,
        progress,
        input_options,
        &FilterSet::default(),
    )
}

pub fn add_archive_with_encoding_progress_and_options_and_filters(
    archive: &str,
    paths: &[PathBuf],
    options: EncodingOptions,
    progress: Option<&dyn Progress>,
    input_options: AddInputOptions,
    filters: &FilterSet,
) -> Result<(), Box<dyn std::error::Error>> {
    let passwords = PasswordSet::default();
    add_archive_with_encoding_progress_and_filters_and_encryption(
        archive,
        paths,
        options,
        progress,
        input_options,
        filters,
        None,
        &passwords,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn add_archive_with_encoding_progress_and_filters_and_encryption(
    archive: &str,
    paths: &[PathBuf],
    options: EncodingOptions,
    progress: Option<&dyn Progress>,
    input_options: AddInputOptions,
    filters: &FilterSet,
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
    passwords: &PasswordSet,
) -> Result<(), Box<dyn std::error::Error>> {
    add_with_base(
        archive,
        paths,
        options,
        progress,
        input_options,
        filters,
        encryption,
        passwords,
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn add_archive_in_folder_with_encoding_progress_and_filters_and_encryption(
    archive: &str,
    paths: &[PathBuf],
    folder: &ArchivePath,
    options: EncodingOptions,
    progress: Option<&dyn Progress>,
    input_options: AddInputOptions,
    filters: &FilterSet,
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
    passwords: &PasswordSet,
) -> Result<(), Box<dyn std::error::Error>> {
    add_with_base(
        archive,
        paths,
        options,
        progress,
        input_options,
        filters,
        encryption,
        passwords,
        None,
        Some(folder),
    )
}

/// Adds entries to an overlay, validating the resulting state against its base archive.
#[allow(clippy::too_many_arguments)]
pub fn add_overlay_with_encoding_progress_and_filters_and_encryption(
    base: &str,
    overlay: &str,
    paths: &[PathBuf],
    options: EncodingOptions,
    progress: Option<&dyn Progress>,
    input_options: AddInputOptions,
    filters: &FilterSet,
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
    passwords: &PasswordSet,
) -> Result<(), Box<dyn std::error::Error>> {
    add_with_base(
        overlay,
        paths,
        options,
        progress,
        input_options,
        filters,
        encryption,
        passwords,
        Some(base),
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn add_overlay_in_folder_with_encoding_progress_and_filters_and_encryption(
    base: &str,
    overlay: &str,
    paths: &[PathBuf],
    folder: &ArchivePath,
    options: EncodingOptions,
    progress: Option<&dyn Progress>,
    input_options: AddInputOptions,
    filters: &FilterSet,
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
    passwords: &PasswordSet,
) -> Result<(), Box<dyn std::error::Error>> {
    add_with_base(
        overlay,
        paths,
        options,
        progress,
        input_options,
        filters,
        encryption,
        passwords,
        Some(base),
        Some(folder),
    )
}

#[allow(clippy::too_many_arguments)]
fn add_with_base(
    archive: &str,
    paths: &[PathBuf],
    mut options: EncodingOptions,
    progress: Option<&dyn Progress>,
    input_options: AddInputOptions,
    filters: &FilterSet,
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
    passwords: &PasswordSet,
    base: Option<&str>,
    folder: Option<&ArchivePath>,
) -> Result<(), Box<dyn std::error::Error>> {
    let AddInputOptions {
        save_metadata,
        save_modification_time,
        save_creation_time,
        save_access_time,
        save_user,
        include_special_files,
        strict,
        verbose,
    } = input_options;
    let encoding_warning = resolve_encoding_options(&mut options);
    if let (Some(progress), Some(warning)) = (progress, encoding_warning.as_deref()) {
        progress.warning(warning);
    }
    let archive_path = Path::new(archive);
    let parent = archive_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err(format!("archive parent is not a directory: {}", parent.display()).into());
    }
    let mut toc = match base {
        Some(base) => load_overlay_toc(base, archive, passwords)?,
        None => load_toc_through_with_passwords(archive, None, passwords)?,
    };
    let cwd = std::env::current_dir()?;
    let limits = CommitValidationLimits::default();
    let reusable_tail = find_reusable_empty_tail(archive_path, passwords)?;
    let new_commit_id = match reusable_tail {
        Some(tail) => tail.commit_id,
        None => next_commit_id(archive_path)?,
    };
    if reusable_tail.is_some() {
        toc.forget_empty_commit(new_commit_id)?;
    }
    let id = TEMP_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let unique_id = unique_temp_file_id()?;
    let stem = archive_path
        .file_name()
        .ok_or("archive path has no filename")?
        .to_string_lossy();
    let _staged_commit = parent.join(format!(".{stem}.zarc-add-{unique_id}-{id}.tmp"));
    let staged_data = parent.join(format!(".{stem}.zarc-add-data-{unique_id}-{id}.tmp"));
    let staged_archive = parent.join(format!(".{stem}.zarc-add-full-{unique_id}-{id}.tmp"));
    let mut cleanup = TemporaryFiles(Vec::new());

    let mut data = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged_data)?;
    cleanup.0.push(staged_data.clone());
    let mut changes = Vec::new();
    let mut next_block_index = 0_u32;
    let mut block_sizes = std::collections::HashMap::<u32, u64>::new();
    let mut seen_paths = std::collections::BTreeSet::new();
    let mut scan_policy = ScanPolicy {
        strict,
        include_special_files,
        verbose,
        warnings: false,
        warning_messages: Vec::new(),
    };
    let mut entry_metadata = Vec::new();
    let (encryption_context, encryption_descriptor) =
        if let Some((password, algorithm, scope)) = encryption {
            let dek = Zeroizing::new(generate_dek()?);
            let provisional_ch = CommitHeader::new(0, new_commit_id, 0, 0)?;
            let context = matches!(
                scope,
                EncryptionScope::Data | EncryptionScope::HeaderAndData
            )
            .then(|| EncryptionContext::new(Zeroizing::new(*dek), provisional_ch, algorithm));
            let descriptor = EncryptionDescriptor::Password {
                scope,
                algorithm,
                nonce: generate_nonce()?,
                key_wrap: wrap_dek_with_password(password, &dek, algorithm, 65_536, 3, 1)?,
            };
            (context, Some((dek, descriptor.encode()?, scope)))
        } else {
            (None, None)
        };
    let new_commit_password = encryption.map(|(password, _, _)| password);
    let mut block_index = Vec::new();
    ProgressReporter::new(progress).check_cancelled()?;
    let scanned_inputs =
        super::archive_input::scan_input_paths_in_folder(paths, &cwd, parent, progress, folder)?;
    ProgressReporter::new(progress).check_cancelled()?;
    let all_tree_entries = filter_tree_entries(scanned_inputs.entries, filters);
    let archives: Vec<&str> = base.into_iter().chain(std::iter::once(archive)).collect();
    let duplicates = find_historical_duplicates(
        &all_tree_entries,
        &archives,
        passwords,
        encryption,
        &ProgressReporter::new(progress),
    )?;
    let mut scan_context = ScanContext {
        changes: &mut changes,
        data: &mut data,
        block_index: &mut block_index,
        next_block_index: &mut next_block_index,
        block_sizes: &mut block_sizes,
        seen_paths: &mut seen_paths,
        entry_metadata: &mut entry_metadata,
        policy: &mut scan_policy,
        options,
        compression_workers: compression_worker_count(options.block_size),
        progress: ProgressReporter::new(progress),
        save_metadata,
        action: "Adding",
        encryption: encryption_context,
        historical_duplicates: Some(&duplicates),
    };
    if let Some(progress) = progress {
        let summary = summarize_tree_entries(&all_tree_entries)?;
        ProgressReporter::new(Some(progress)).input_summary(&summary);
        ProgressReporter::new(Some(progress)).set_total(summary.total_bytes);
    }
    process_tree_entries(&all_tree_entries, &mut scan_context)?;
    if save_creation_time {
        require_creation_times(&entry_metadata)?;
    }
    if save_access_time {
        require_access_times(&entry_metadata)?;
    }
    ProgressReporter::new(progress).check_cancelled()?;
    if let Some(progress) = progress {
        for warning in &scan_policy.warning_messages {
            progress.warning(warning);
        }
    }
    if changes.is_empty() {
        return Err("no supported files or directories were found to add".into());
    }

    let logical_header = LogicalHeader::with_block_size(changes.clone(), options.block_size as u32)
        .with_block_index(block_index)
        .with_entry_metadata(finalize_entry_metadata(
            entry_metadata,
            save_modification_time,
            save_creation_time,
            save_access_time,
        ));
    let mut group_sizes = group_size_metadata(&logical_header, &block_sizes)?;
    let username = save_user.then(current_username).transpose()?;
    group_sizes.extend(commit_metadata(username.as_deref())?);
    let header_bytes = logical_header.with_metadata(group_sizes).encode()?;
    let data_size = data.stream_position()?;
    data.flush()?;
    drop(data);

    let data_section_size = data_size;

    let mut existing = File::open(archive_path)?;
    let first_header_offset = read_first_header_offset(&mut existing)?;
    let prev_last = if reusable_tail.is_some() {
        None
    } else if first_header_offset != 0 {
        Some(find_last_commit(&mut existing, limits)?)
    } else {
        None
    };
    drop(existing);
    let _existing_size = File::open(archive_path)?.metadata()?.len();

    let temp_ch = CommitHeader::new(0, new_commit_id, data_section_size, 0)?;
    let (dek, descriptor_bytes, scope) = match encryption_descriptor {
        Some((dek, descriptor_bytes, scope)) => (Some(dek), descriptor_bytes, Some(scope)),
        None => (None, UNENCRYPTED_DESCRIPTOR.to_vec(), None),
    };
    let staged_header = if matches!(
        scope,
        Some(EncryptionScope::Header | EncryptionScope::HeaderAndData)
    ) {
        HeaderFrame::from_uncompressed_encrypted(
            temp_ch,
            &header_bytes,
            descriptor_bytes.clone(),
            options.compression_level,
            dek.as_deref()
                .ok_or("encrypted header is missing its key")?,
        )?
    } else {
        HeaderFrame::from_uncompressed(
            temp_ch,
            &header_bytes,
            descriptor_bytes.clone(),
            options.compression_level,
        )?
    };
    let header_size = u32::try_from(staged_header.encoded_size()?)?;
    let commit_header = CommitHeader::new(0, new_commit_id, data_section_size, header_size)?;
    let header = if matches!(
        scope,
        Some(EncryptionScope::Header | EncryptionScope::HeaderAndData)
    ) {
        HeaderFrame::from_uncompressed_encrypted(
            commit_header.clone(),
            &header_bytes,
            descriptor_bytes,
            options.compression_level,
            dek.as_deref()
                .ok_or("encrypted header is missing its key")?,
        )?
    } else {
        HeaderFrame::from_uncompressed(
            commit_header.clone(),
            &header_bytes,
            descriptor_bytes,
            options.compression_level,
        )?
    };

    let mut combined = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged_archive)?;
    cleanup.0.push(staged_archive.clone());
    ProgressReporter::new(progress).check_cancelled()?;
    let mut archive_source = File::open(archive_path)?;
    if let Some(tail) = reusable_tail {
        let mut limited_source = Read::by_ref(&mut archive_source).take(tail.link_position);
        copy_with_cancellation(&mut limited_source, &mut combined, progress)?;
    } else {
        copy_with_cancellation(&mut archive_source, &mut combined, progress)?;
    }
    drop(archive_source);
    combined.write_all(&0u64.to_le_bytes())?;
    copy_with_cancellation(&mut File::open(&staged_data)?, &mut combined, progress)?;
    let new_header_offset = combined.stream_position()?;
    header.write_to(&mut combined)?;
    combined.write_all(&new_header_offset.to_le_bytes())?;

    if let Some(tail) = reusable_tail {
        combined.seek(SeekFrom::Start(tail.incoming_link_position))?;
        combined.write_all(&new_header_offset.to_le_bytes())?;
    } else if let Some((prev_header_offset, prev_commit_header)) = prev_last {
        let next_offset_pos = prev_header_offset
            .checked_sub(prev_commit_header.data_section_size)
            .and_then(|pos| pos.checked_sub(HEADER_OFFSET_SIZE))
            .ok_or("previous commit offset underflow")?;
        combined.seek(SeekFrom::Start(next_offset_pos))?;
        combined.write_all(&new_header_offset.to_le_bytes())?;
    } else {
        combined.seek(SeekFrom::Start(4))?;
        combined.write_all(&new_header_offset.to_le_bytes())?;
    }
    combined.flush()?;
    drop(combined);

    let available_blocks: std::collections::HashSet<u32> = block_sizes.keys().copied().collect();
    Toc::check_available_blocks(&changes, &available_blocks)?;
    toc.apply_archive_commit(commit_header.commit_id, &changes)?;

    let mut validation = File::open(&staged_archive)?;
    let first_header_offset = read_first_header_offset(&mut validation)?;
    let mut header_offset = first_header_offset;
    while header_offset != 0 {
        ProgressReporter::new(progress).check_cancelled()?;
        let (ch, _, next_header_offset) =
            read_commit_at_offset(&mut validation, header_offset, limits)?;
        validation.seek(SeekFrom::Start(header_offset))?;
        if ch.commit_id == new_commit_id {
            let (validated_header, _) =
                read_header_with_password(&mut validation, limits, new_commit_password)?;
            if validated_header.logical_header.is_none() {
                return Err("new commit failed header validation".into());
            }
        } else {
            let (validated, dek) = read_header_with_passwords(&mut validation, limits, passwords)?;
            if (validated.encryption.is_header_encrypted()
                || validated.encryption.is_data_encrypted())
                && dek.is_none()
            {
                return Err(format!(
                    "cannot validate encrypted commit {:08x}; provide its password",
                    u32::from_le_bytes(validated.commit_header.commit_id)
                )
                .into());
            }
        }
        header_offset = next_header_offset;
    }
    let expected_end = new_header_offset
        .checked_add(u64::from(commit_header.header_frame_size))
        .and_then(|size| size.checked_add(HEADER_OFFSET_SIZE))
        .ok_or("archive size overflow")?;
    let actual_end = validation.seek(SeekFrom::End(0))?;
    if actual_end != expected_end {
        return Err("added archive failed validation".into());
    }
    drop(validation);
    ProgressReporter::new(progress).check_cancelled()?;
    std::fs::rename(&staged_archive, archive_path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("cannot replace archive {}: {error}", archive_path.display()),
        )
    })?;
    cleanup.0.retain(|path| path != &staged_archive);
    drop(cleanup);
    Ok(())
}
