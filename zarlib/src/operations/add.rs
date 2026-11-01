use super::common::*;
use super::filters::FilterSet;
use super::info::group_size_metadata;
use super::list::load_toc_through_with_passwords;
use super::pipeline::*;
use crate::{
    CommitHeader, CommitValidationLimits, ContentAead, EncryptionDescriptor, EncryptionScope,
    HEADER_OFFSET_SIZE, HeaderFrame, LogicalHeader, UNENCRYPTED_DESCRIPTOR, ValidatedCommit,
    generate_dek, generate_nonce, wrap_dek_with_password,
};
use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AddInputOptions {
    pub save_metadata: bool,
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
    mut options: EncodingOptions,
    progress: Option<&dyn Progress>,
    input_options: AddInputOptions,
    filters: &FilterSet,
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
    passwords: &PasswordSet,
) -> Result<(), Box<dyn std::error::Error>> {
    let AddInputOptions {
        save_metadata,
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
    let mut toc = load_toc_through_with_passwords(archive, None, passwords)?;
    let cwd = std::env::current_dir()?;
    let limits = CommitValidationLimits::default();
    let new_commit_id = next_commit_id(archive_path)?;
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
    let mut scan_context = ScanContext {
        changes: &mut changes,
        data: &mut data,
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
    };
    let mut all_tree_entries = Vec::new();
    for input in paths {
        let source = if input.is_absolute() {
            input.clone()
        } else {
            cwd.join(input)
        };
        let prefix = archive_prefix(input, &cwd)?;
        let tree_entries = scan_tree(&source, &prefix)?;
        all_tree_entries.extend(tree_entries);
    }
    let all_tree_entries = filter_tree_entries(all_tree_entries, filters);
    if let Some(progress) = progress {
        let summary = summarize_tree_entries(&all_tree_entries)?;
        ProgressReporter::new(Some(progress)).input_summary(&summary);
        ProgressReporter::new(Some(progress)).set_total(summary.total_bytes);
    }
    process_tree_entries(&all_tree_entries, &mut scan_context)?;
    if let Some(progress) = progress {
        for warning in &scan_policy.warning_messages {
            progress.warning(warning);
        }
    }
    if changes.is_empty() {
        return Err("no supported files or directories were found to add".into());
    }

    let logical_header = LogicalHeader::with_block_size(changes.clone(), options.block_size as u32)
        .with_entry_metadata(entry_metadata);
    let group_sizes = group_size_metadata(&logical_header, &block_sizes)?;
    let header_bytes = logical_header.with_metadata(group_sizes).encode()?;
    let data_size = data.stream_position()?;
    data.flush()?;
    drop(data);

    let data_section_size = data_size;

    let mut existing = File::open(archive_path)?;
    let first_header_offset = read_first_header_offset(&mut existing)?;
    let prev_last = if first_header_offset != 0 {
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
    io::copy(&mut File::open(archive_path)?, &mut combined)?;
    combined.write_all(&0u64.to_le_bytes())?;
    io::copy(&mut File::open(&staged_data)?, &mut combined)?;
    let new_header_offset = combined.stream_position()?;
    header.write_to(&mut combined)?;
    combined.write_all(&new_header_offset.to_le_bytes())?;

    if let Some((prev_header_offset, prev_commit_header)) = prev_last {
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

    let mut commit_reader = File::open(&staged_archive)?;
    let new_data_start = new_header_offset
        .checked_sub(data_section_size)
        .ok_or("new commit data start underflow")?;
    commit_reader.seek(SeekFrom::Start(new_data_start))?;
    let validated = match dek.as_ref() {
        Some(dek) => ValidatedCommit::read_body_with_key(
            &mut commit_reader,
            commit_header.clone(),
            limits,
            dek,
        )?,
        None => ValidatedCommit::read_body(&mut commit_reader, commit_header.clone(), limits)?,
    };
    toc.apply_commit_with_available_blocks(&changes, &validated.block_indices)?;

    let mut validation = File::open(&staged_archive)?;
    let first_header_offset = read_first_header_offset(&mut validation)?;
    let mut header_offset = first_header_offset;
    while header_offset != 0 {
        let (ch, _, next_header_offset) =
            read_commit_at_offset(&mut validation, header_offset, limits)?;
        if ch.commit_id == new_commit_id {
            validation.seek(SeekFrom::Start(header_offset))?;
            let (validated_header, _) =
                read_header_with_password(&mut validation, limits, new_commit_password)?;
            if validated_header.logical_header.is_none() {
                return Err("new commit failed header validation".into());
            }
            let data_start = header_offset
                .checked_sub(ch.data_section_size)
                .ok_or("new commit data start underflow")?;
            validation.seek(SeekFrom::Start(data_start))?;
            if let Some(dek) = dek.as_ref() {
                ValidatedCommit::read_body_with_key(&mut validation, ch, limits, dek)?;
            } else {
                ValidatedCommit::read_body(&mut validation, ch, limits)?;
            }
        } else {
            let (validated, dek) =
                read_commit_with_passwords(&mut validation, ch, limits, passwords)?;
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
    drop(commit_reader);
    drop(validation);
    std::fs::rename(&staged_archive, archive_path)?;
    cleanup.0.retain(|path| path != &staged_archive);
    drop(cleanup);
    Ok(())
}
