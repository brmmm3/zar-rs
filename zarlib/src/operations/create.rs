use super::common::*;
use super::filters::FilterSet;
use super::info::group_size_metadata;
use super::pipeline::*;
use crate::{
    CommitHeader, CommitValidationLimits, ContentAead, EncryptionDescriptor, EncryptionScope,
    HEADER_OFFSET_SIZE, HeaderFrame, LogicalHeader, UNENCRYPTED_DESCRIPTOR, ValidatedHeader,
    generate_dek, generate_nonce, wrap_dek_with_password,
};
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CreateInputOptions {
    pub save_metadata: bool,
    pub save_dates: bool,
    pub include_special_files: bool,
    pub overwrite_existing: bool,
    pub verbose: bool,
}

#[derive(Clone, Copy)]
pub struct CreateFilterOptions<'a> {
    pub input: CreateInputOptions,
    pub filters: &'a FilterSet,
}

pub fn create_archive(archive: &str, paths: &[PathBuf]) -> Result<(), Box<dyn std::error::Error>> {
    create_archive_with_encoding(archive, paths, false, EncodingOptions::default()).map(|_| ())
}

pub fn create_archive_with_options(
    archive: &str,
    paths: &[PathBuf],
    strict: bool,
) -> Result<CreateReport, Box<dyn std::error::Error>> {
    create_archive_with_encoding(archive, paths, strict, EncodingOptions::default())
}

pub fn create_archive_with_encoding(
    archive: &str,
    paths: &[PathBuf],
    strict: bool,
    options: EncodingOptions,
) -> Result<CreateReport, Box<dyn std::error::Error>> {
    create_archive_with_password(archive, paths, strict, options, None)
}

pub fn create_archive_with_password(
    archive: &str,
    paths: &[PathBuf],
    strict: bool,
    options: EncodingOptions,
    password: Option<&[u8]>,
) -> Result<CreateReport, Box<dyn std::error::Error>> {
    create_archive_with_password_progress(archive, paths, strict, options, password, None, false)
}

pub fn create_archive_with_password_progress(
    archive: &str,
    paths: &[PathBuf],
    strict: bool,
    options: EncodingOptions,
    password: Option<&[u8]>,
    progress: Option<&dyn Progress>,
    save_metadata: bool,
) -> Result<CreateReport, Box<dyn std::error::Error>> {
    create_archive_with_password_progress_and_special_files(
        archive,
        paths,
        strict,
        options,
        password,
        progress,
        CreateInputOptions {
            save_metadata,
            save_dates: false,
            include_special_files: true,
            overwrite_existing: false,
            verbose: false,
        },
    )
}

pub fn create_archive_with_password_progress_and_special_files(
    archive: &str,
    paths: &[PathBuf],
    strict: bool,
    options: EncodingOptions,
    password: Option<&[u8]>,
    progress: Option<&dyn Progress>,
    input_options: CreateInputOptions,
) -> Result<CreateReport, Box<dyn std::error::Error>> {
    create_archive_with_password_progress_and_filters(
        archive,
        paths,
        strict,
        options,
        password,
        progress,
        CreateFilterOptions {
            input: input_options,
            filters: &FilterSet::default(),
        },
    )
}

pub fn create_archive_with_password_progress_and_filters(
    archive: &str,
    paths: &[PathBuf],
    strict: bool,
    options: EncodingOptions,
    password: Option<&[u8]>,
    progress: Option<&dyn Progress>,
    filter_options: CreateFilterOptions<'_>,
) -> Result<CreateReport, Box<dyn std::error::Error>> {
    create_archive_with_password_progress_and_filters_and_aead(
        archive,
        paths,
        strict,
        options,
        password.map(|password| (password, ContentAead::Aes256Gcm)),
        progress,
        filter_options,
    )
}

pub fn create_archive_with_password_progress_and_filters_and_aead(
    archive: &str,
    paths: &[PathBuf],
    strict: bool,
    options: EncodingOptions,
    encryption: Option<(&[u8], ContentAead)>,
    progress: Option<&dyn Progress>,
    filter_options: CreateFilterOptions<'_>,
) -> Result<CreateReport, Box<dyn std::error::Error>> {
    create_archive_with_password_progress_and_filters_and_encryption(
        archive,
        paths,
        strict,
        options,
        encryption
            .map(|(password, algorithm)| (password, algorithm, EncryptionScope::HeaderAndData)),
        progress,
        filter_options,
    )
}

pub fn create_archive_with_password_progress_and_filters_and_encryption(
    archive: &str,
    paths: &[PathBuf],
    strict: bool,
    mut options: EncodingOptions,
    encryption: Option<(&[u8], ContentAead, EncryptionScope)>,
    progress: Option<&dyn Progress>,
    filter_options: CreateFilterOptions<'_>,
) -> Result<CreateReport, Box<dyn std::error::Error>> {
    let CreateFilterOptions {
        input: input_options,
        filters,
    } = filter_options;
    let CreateInputOptions {
        save_metadata,
        save_dates,
        include_special_files,
        overwrite_existing,
        verbose,
    } = input_options;
    let encoding_warning = resolve_encoding_options(&mut options);
    let archive_path = Path::new(archive);
    if archive_path.exists() && !overwrite_existing {
        return Err(format!("archive already exists: {}", archive_path.display()).into());
    }
    if archive_path.file_name().is_none() {
        return Err("archive path has no filename".into());
    }
    let parent = archive_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err(format!("archive parent is not a directory: {}", parent.display()).into());
    }
    let cwd = std::env::current_dir()?;
    let id = TEMP_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let unique_id = unique_temp_file_id()?;
    let stem = archive_path
        .file_name()
        .ok_or("archive path has no filename")?
        .to_string_lossy();
    let temp_archive = parent.join(format!(".{stem}.zarc-{unique_id}-{id}.tmp"));
    let mut cleanup = TemporaryFiles(Vec::new());
    let mut changes = Vec::new();
    let mut next_block_index = 0_u32;
    let mut block_sizes = std::collections::HashMap::<u32, u64>::new();
    let mut seen_paths = std::collections::BTreeSet::new();
    let mut entry_metadata = Vec::new();
    let mut scan_policy = ScanPolicy {
        strict,
        include_special_files,
        verbose,
        warnings: false,
        warning_messages: Vec::new(),
    };

    let mut archive = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&temp_archive)?;
    cleanup.0.push(temp_archive.clone());

    crate::write_container_magic(&mut archive)?;
    let container_header_offset = 0u64.to_le_bytes();
    archive.write_all(&container_header_offset)?;
    let commit_data_start = archive.stream_position()?;
    archive.write_all(&0u64.to_le_bytes())?;

    let cid = commit_id(1);
    let (encryption, dek, descriptor_bytes, encryption_scope) =
        if let Some((password, algorithm, scope)) = encryption {
            let dek = Zeroizing::new(generate_dek()?);
            let descriptor = EncryptionDescriptor::Password {
                scope,
                algorithm,
                nonce: generate_nonce()?,
                key_wrap: wrap_dek_with_password(password, &dek, algorithm, 65_536, 3, 1)?,
            };
            let descriptor_bytes = descriptor.encode()?;
            let provisional_ch = CommitHeader::new(0, cid, 0, 0)?;
            let data_encryption = matches!(
                scope,
                EncryptionScope::Data | EncryptionScope::HeaderAndData
            )
            .then(|| EncryptionContext::new(Zeroizing::new(*dek), provisional_ch, algorithm));
            (
                data_encryption,
                Some(dek),
                Some(descriptor_bytes),
                Some(scope),
            )
        } else {
            (None, None, None, None)
        };

    let mut scan_context = ScanContext {
        changes: &mut changes,
        data: &mut archive,
        next_block_index: &mut next_block_index,
        block_sizes: &mut block_sizes,
        seen_paths: &mut seen_paths,
        entry_metadata: &mut entry_metadata,
        policy: &mut scan_policy,
        options,
        compression_workers: compression_worker_count(options.block_size),
        progress: ProgressReporter::new(progress),
        save_metadata,
        action: "Creating",
        encryption,
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
    if changes.is_empty() {
        return Err("no supported files or directories were found to archive".into());
    }

    archive.flush()?;
    let data_section_size = archive.stream_position()? - commit_data_start - HEADER_OFFSET_SIZE;

    let logical_header = LogicalHeader::with_block_size(changes, options.block_size as u32)
        .with_entry_metadata(finalize_entry_metadata(entry_metadata, save_dates));
    let group_sizes = group_size_metadata(&logical_header, &block_sizes)?;
    let header_bytes = logical_header.with_metadata(group_sizes).encode()?;

    let (header, commit_header, dek) = if let (Some(dek), Some(descriptor_bytes), Some(scope)) =
        (dek, descriptor_bytes, encryption_scope)
    {
        let temp_ch = CommitHeader::new(0, cid, data_section_size, 0)?;
        let temp_header = if matches!(
            scope,
            EncryptionScope::Header | EncryptionScope::HeaderAndData
        ) {
            HeaderFrame::from_uncompressed_encrypted(
                temp_ch,
                &header_bytes,
                descriptor_bytes.clone(),
                options.compression_level,
                &dek,
            )?
        } else {
            HeaderFrame::from_uncompressed(
                temp_ch,
                &header_bytes,
                descriptor_bytes.clone(),
                options.compression_level,
            )?
        };
        let header_size = u32::try_from(temp_header.encoded_size()?)?;
        let commit_header = CommitHeader::new(0, cid, data_section_size, header_size)?;
        let header = if matches!(
            scope,
            EncryptionScope::Header | EncryptionScope::HeaderAndData
        ) {
            HeaderFrame::from_uncompressed_encrypted(
                commit_header.clone(),
                &header_bytes,
                descriptor_bytes,
                options.compression_level,
                &dek,
            )?
        } else {
            HeaderFrame::from_uncompressed(
                commit_header.clone(),
                &header_bytes,
                descriptor_bytes,
                options.compression_level,
            )?
        };
        (header, commit_header, Some(dek))
    } else {
        let temp_ch = CommitHeader::new(0, cid, data_section_size, 0)?;
        let staged_header = HeaderFrame::from_uncompressed(
            temp_ch,
            &header_bytes,
            UNENCRYPTED_DESCRIPTOR.to_vec(),
            options.compression_level,
        )?;
        let header_size = u32::try_from(staged_header.encoded_size()?)?;
        let commit_header = CommitHeader::new(0, cid, data_section_size, header_size)?;
        let header = HeaderFrame::from_uncompressed(
            commit_header.clone(),
            &header_bytes,
            UNENCRYPTED_DESCRIPTOR.to_vec(),
            options.compression_level,
        )?;
        (header, commit_header, None)
    };

    let header_offset = archive.stream_position()?;
    header.write_to(&mut archive)?;
    archive.write_all(&header_offset.to_le_bytes())?;
    archive.flush()?;

    archive.seek(SeekFrom::Start(4))?;
    archive.write_all(&header_offset.to_le_bytes())?;

    archive.seek(SeekFrom::Start(header_offset))?;
    let limits = CommitValidationLimits::default();
    let validated_header = if let Some(dek) = dek.as_ref() {
        ValidatedHeader::read_header_with_key(&mut archive, limits, dek)?
    } else {
        ValidatedHeader::read_header(&mut archive, limits)?
    };
    if validated_header.logical_header.is_none() {
        return Err("created archive failed validation".into());
    }
    let expected_end = header_offset
        .checked_add(u64::from(commit_header.header_frame_size))
        .and_then(|size| size.checked_add(HEADER_OFFSET_SIZE))
        .ok_or("archive size overflow")?;
    let actual_end = archive.seek(SeekFrom::End(0))?;
    if actual_end != expected_end {
        return Err("created archive failed validation".into());
    }

    if overwrite_existing {
        replace_archive(&temp_archive, archive_path)?;
    } else {
        std::fs::hard_link(&temp_archive, archive_path)?;
    }
    drop(cleanup);
    let mut warnings = scan_policy.warning_messages;
    warnings.extend(encoding_warning);
    Ok(CreateReport {
        warnings,
        block_size: options.block_size,
    })
}

#[cfg(unix)]
fn replace_archive(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_archive(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let succeeded = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if succeeded == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(any(unix, windows)))]
fn replace_archive(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::rename(source, destination)
}
