use super::common::{
    PasswordSet, TEMP_FILE_ID, TemporaryFiles, parse_commit_id, read_commit_at_offset,
    read_first_header_offset, unique_temp_file_id,
};
use crate::{
    BlockFrame, CommitHeader, CommitValidationLimits, ContentAead, EncryptionDescriptor,
    EncryptionScope, HEADER_OFFSET_SIZE, HeaderFrame, ValidatedCommit,
};
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

const TRANSFORM_COMPRESSION_LEVEL: i32 = 3;

/// Commit range, output destination, password lookup, and missing-password policy.
pub struct CommitRangeOptions<'a> {
    /// Inclusive first commit ID.
    pub from_id: &'a str,
    /// Inclusive last commit ID.
    pub to_id: &'a str,
    /// Destination archive, or `None` to replace the input archive.
    pub output: Option<&'a str>,
    /// Passwords for existing encrypted commits.
    pub passwords: &'a PasswordSet,
    /// Whether a missing password for an encrypted commit aborts the operation.
    pub strict: bool,
}

type ContentKey = Zeroizing<[u8; crate::DEK_SIZE]>;
type RewrittenCommit = (u64, Option<ContentKey>);

struct CommitInfo {
    header_offset: u64,
    data_start: u64,
    commit_header: CommitHeader,
}

enum Transform {
    Encrypt {
        password: Zeroizing<Vec<u8>>,
        algorithm: ContentAead,
        scope: EncryptionScope,
    },
    Reencrypt {
        password: Zeroizing<Vec<u8>>,
        algorithm: ContentAead,
        scope: EncryptionScope,
    },
    Decrypt,
}

/// Encrypts commits in the inclusive commit-ID range.
///
/// Existing encrypted commits require their current password in `passwords`.
/// When `strict` is false, encrypted commits without a password are copied
/// unchanged. `output` may be omitted to replace the input archive atomically.
pub fn encrypt_archive_commits(
    archive: &str,
    range: CommitRangeOptions<'_>,
    new_password: &[u8],
    algorithm: ContentAead,
    scope: EncryptionScope,
) -> Result<(), Box<dyn std::error::Error>> {
    if new_password.is_empty() {
        return Err("the new encryption password must not be empty".into());
    }
    transform_archive_commits(
        archive,
        range,
        Transform::Encrypt {
            password: Zeroizing::new(new_password.to_vec()),
            algorithm,
            scope,
        },
    )
}

/// Decrypts commits in the inclusive commit-ID range.
///
/// When `strict` is false, encrypted commits without a password are copied
/// unchanged. `output` may be omitted to replace the input archive atomically.
pub fn decrypt_archive_commits(
    archive: &str,
    range: CommitRangeOptions<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    transform_archive_commits(archive, range, Transform::Decrypt)
}

/// Re-encrypts only commits that are already encrypted in the selected range.
/// Unencrypted commits and encrypted commits outside the range are preserved.
pub fn change_encryption_for_commits(
    archive: &str,
    range: CommitRangeOptions<'_>,
    new_password: &[u8],
    algorithm: ContentAead,
    scope: EncryptionScope,
) -> Result<(), Box<dyn std::error::Error>> {
    if new_password.is_empty() {
        return Err("the new encryption password must not be empty".into());
    }
    transform_archive_commits(
        archive,
        range,
        Transform::Reencrypt {
            password: Zeroizing::new(new_password.to_vec()),
            algorithm,
            scope,
        },
    )
}

fn transform_archive_commits(
    archive: &str,
    range: CommitRangeOptions<'_>,
    transform: Transform,
) -> Result<(), Box<dyn std::error::Error>> {
    let CommitRangeOptions {
        from_id,
        to_id,
        output,
        passwords,
        strict,
    } = range;
    let from_id = parse_commit_id(from_id)?;
    let to_id = parse_commit_id(to_id)?;
    let archive_path = std::fs::canonicalize(archive)?;
    let destination = match output {
        Some(path) => resolve_output_path(Path::new(path))?,
        None => archive_path.clone(),
    };
    if paths_refer_to_same_file(&archive_path, &destination)? && output.is_some() {
        return Err("output path must not be the same as the input archive".into());
    }
    let destination_parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or("output path has no parent directory")?;
    if !destination_parent.is_dir() {
        return Err(format!(
            "output parent is not a directory: {}",
            destination_parent.display()
        )
        .into());
    }
    if destination.file_name().is_none() {
        return Err("output path has no filename".into());
    }

    let limits = CommitValidationLimits::default();
    let mut input = File::open(&archive_path)?;
    let commits = collect_commits(&mut input, limits)
        .map_err(|error| format!("could not read input commit chain: {error}"))?;
    let matching_indices = |id: &[u8; 4]| {
        commits
            .iter()
            .enumerate()
            .filter_map(|(index, commit)| (commit.commit_header.commit_id == *id).then_some(index))
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
        return Err("--from commit must not appear after --to commit".into());
    }

    let mut selected = vec![false; commits.len()];
    for (index, commit) in commits.iter().enumerate().take(last + 1).skip(first) {
        let header = read_header_descriptor(&mut input, commit, limits)?;
        let (descriptor, _) = EncryptionDescriptor::decode(&header.encryption_descriptor)?;
        let encrypted = descriptor.is_header_encrypted() || descriptor.is_data_encrypted();
        let should_transform = match transform {
            Transform::Encrypt { .. } => true,
            Transform::Reencrypt { .. } | Transform::Decrypt => encrypted,
        };
        if encrypted
            && should_transform
            && passwords
                .password_for(&commit.commit_header.commit_id)
                .is_none()
            && strict
        {
            return Err(format!(
                "cannot transform encrypted commit {:08x} without its password",
                u32::from_le_bytes(commit.commit_header.commit_id)
            )
            .into());
        }
        selected[index] = should_transform;
    }

    let id = TEMP_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let unique_id = unique_temp_file_id()?;
    let stem = destination
        .file_name()
        .ok_or("output path has no filename")?
        .to_string_lossy();
    let staged_archive =
        destination_parent.join(format!(".{stem}.zarc-transform-{unique_id}-{id}.tmp"));
    let mut cleanup = TemporaryFiles(vec![staged_archive.clone()]);
    let mut staged = OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(&staged_archive)?;
    crate::ContainerHeader::new(0).write_to(&mut staged)?;

    let mut previous_next_offset_position = None;
    for (index, commit) in commits.iter().enumerate() {
        let next_offset_position = staged.stream_position()?;
        staged.write_all(&0_u64.to_le_bytes())?;
        let output_data_start = staged.stream_position()?;
        let (output_data_size, output_key, did_transform) = if selected[index]
            && (commit_is_encrypted(&mut input, commit, limits)?
                && passwords
                    .password_for(&commit.commit_header.commit_id)
                    .is_none())
        {
            copy_file_region(
                &mut input,
                &mut staged,
                commit.data_start,
                commit.commit_header.data_section_size,
            )?;
            copy_file_region(
                &mut input,
                &mut staged,
                commit.header_offset,
                u64::from(commit.commit_header.header_frame_size),
            )?;
            (commit.commit_header.data_section_size, None, false)
        } else if selected[index] {
            let (data_size, key) = transform_commit(
                &mut input,
                &mut staged,
                commit,
                &transform,
                passwords,
                limits,
            )
            .map_err(|error| {
                format!(
                    "could not transform commit {:08x}: {error}",
                    u32::from_le_bytes(commit.commit_header.commit_id)
                )
            })?;
            (data_size, key, true)
        } else {
            copy_file_region(
                &mut input,
                &mut staged,
                commit.data_start,
                commit.commit_header.data_section_size,
            )?;
            copy_file_region(
                &mut input,
                &mut staged,
                commit.header_offset,
                u64::from(commit.commit_header.header_frame_size),
            )?;
            (commit.commit_header.data_section_size, None, false)
        };

        let output_header_offset = output_data_start
            .checked_add(output_data_size)
            .ok_or("output header offset overflow")?;
        if selected[index] && !did_transform && !commit_is_encrypted(&mut input, commit, limits)? {
            return Err("selected commit was not transformed".into());
        }
        let current_position = staged.stream_position()?;
        if let Some(position) = previous_next_offset_position {
            staged.seek(SeekFrom::Start(position))?;
            staged.write_all(&output_header_offset.to_le_bytes())?;
            staged.seek(SeekFrom::Start(current_position))?;
        } else {
            staged.seek(SeekFrom::Start(4))?;
            staged.write_all(&output_header_offset.to_le_bytes())?;
            staged.seek(SeekFrom::Start(current_position))?;
        }
        previous_next_offset_position = Some(next_offset_position);

        staged.write_all(&output_header_offset.to_le_bytes())?;
        if output_key.is_some() {
            validate_written_commit(
                &mut staged,
                output_header_offset,
                output_data_size,
                limits,
                output_key.as_deref(),
            )
            .map_err(|error| format!("transformed commit failed validation: {error}"))?;
            staged.seek(SeekFrom::End(0))?;
        }
    }

    if let Some(position) = previous_next_offset_position {
        let current_position = staged.stream_position()?;
        staged.seek(SeekFrom::Start(position))?;
        staged.write_all(&0_u64.to_le_bytes())?;
        staged.seek(SeekFrom::Start(current_position))?;
    }
    staged.flush()?;
    drop(staged);

    validate_staged_archive(&staged_archive, limits)
        .map_err(|error| format!("staged archive failed validation: {error}"))?;
    drop(input);
    replace_archive(&staged_archive, &destination)?;
    cleanup.0.clear();
    Ok(())
}

fn collect_commits(
    input: &mut File,
    limits: CommitValidationLimits,
) -> Result<Vec<CommitInfo>, Box<dyn std::error::Error>> {
    let file_len = input.metadata()?.len();
    let mut header_offset = read_first_header_offset(input)?;
    if header_offset == 0 {
        return Err("archive contains no commits".into());
    }
    let mut commits = Vec::new();
    let mut seen_offsets = HashSet::new();
    while header_offset != 0 {
        if !seen_offsets.insert(header_offset) {
            return Err("archive commit chain contains a cycle".into());
        }
        let (commit_header, data_start, next_header_offset) =
            read_commit_at_offset(input, header_offset, limits)?;
        if data_start < crate::CONTAINER_HEADER_SIZE + HEADER_OFFSET_SIZE {
            return Err("commit data begins before the archive data region".into());
        }
        let commit_end = header_offset
            .checked_add(u64::from(commit_header.header_frame_size))
            .and_then(|offset| offset.checked_add(HEADER_OFFSET_SIZE))
            .ok_or("commit end overflow")?;
        if commit_end > file_len {
            return Err("commit extends beyond the end of the archive".into());
        }
        ValidatedCommit::read_body(input, commit_header.clone(), limits)?;
        commits.push(CommitInfo {
            header_offset,
            data_start,
            commit_header,
        });
        header_offset = next_header_offset;
    }
    Ok(commits)
}

fn read_header_descriptor(
    input: &mut File,
    commit: &CommitInfo,
    limits: CommitValidationLimits,
) -> Result<HeaderFrame, Box<dyn std::error::Error>> {
    input.seek(SeekFrom::Start(commit.header_offset))?;
    Ok(HeaderFrame::read_from(
        input,
        limits.max_frame_payload_size,
    )?)
}

fn commit_is_encrypted(
    input: &mut File,
    commit: &CommitInfo,
    limits: CommitValidationLimits,
) -> Result<bool, Box<dyn std::error::Error>> {
    let header = read_header_descriptor(input, commit, limits)?;
    let (descriptor, _) = EncryptionDescriptor::decode(&header.encryption_descriptor)?;
    Ok(descriptor.is_header_encrypted() || descriptor.is_data_encrypted())
}

fn transform_commit(
    input: &mut File,
    output: &mut File,
    commit: &CommitInfo,
    transform: &Transform,
    passwords: &PasswordSet,
    limits: CommitValidationLimits,
) -> Result<RewrittenCommit, Box<dyn std::error::Error>> {
    let old_header = read_header_descriptor(input, commit, limits)?;
    let (old_descriptor, extensions) =
        EncryptionDescriptor::decode(&old_header.encryption_descriptor)?;
    let old_password = passwords.password_for(&commit.commit_header.commit_id);
    let old_key = if old_descriptor.is_header_encrypted() || old_descriptor.is_data_encrypted() {
        let password = old_password.ok_or_else(|| {
            format!(
                "cannot transform encrypted commit {:08x} without its password",
                u32::from_le_bytes(commit.commit_header.commit_id)
            )
        })?;
        Some(unwrap_commit_key(password, &old_descriptor)?)
    } else {
        None
    };
    input.seek(SeekFrom::Start(commit.data_start))?;
    if let Some(key) = old_key.as_ref() {
        ValidatedCommit::read_body_with_key(input, commit.commit_header.clone(), limits, key)?;
    } else {
        ValidatedCommit::read_body(input, commit.commit_header.clone(), limits)?;
    }
    let header_bytes = Zeroizing::new(if old_descriptor.is_header_encrypted() {
        old_header
            .uncompressed_encrypted(
                old_key
                    .as_deref()
                    .ok_or("encrypted header is missing its key")?,
                limits.max_logical_header_size,
            )
            .map_err(|error| format!("cannot decrypt commit header: {error}"))?
    } else {
        old_header
            .uncompressed(limits.max_logical_header_size)
            .map_err(|error| format!("cannot read commit header: {error}"))?
    });

    let output_data_start = output.stream_position()?;
    let (new_descriptor, new_key) = match transform {
        Transform::Encrypt {
            password,
            algorithm,
            scope,
        }
        | Transform::Reencrypt {
            password,
            algorithm,
            scope,
        } => {
            let key = Zeroizing::new(crate::generate_dek()?);
            let descriptor = EncryptionDescriptor::Password {
                scope: *scope,
                algorithm: *algorithm,
                nonce: crate::generate_nonce()?,
                key_wrap: crate::wrap_dek_with_password(password, &key, *algorithm, 65_536, 3, 1)?,
            };
            (descriptor, Some(key))
        }
        Transform::Decrypt => (EncryptionDescriptor::Unencrypted, None),
    };
    let descriptor_bytes = new_descriptor.encode_with_extensions(&extensions)?;
    let provisional_header = CommitHeader::new(
        commit.commit_header.flags,
        commit.commit_header.commit_id,
        0,
        0,
    )?;
    input.seek(SeekFrom::Start(commit.data_start))?;
    let mut limited_data = input.take(commit.commit_header.data_section_size);
    while limited_data.limit() > 0 {
        let remaining_before = limited_data.limit();
        let frame = BlockFrame::read_from(&mut limited_data, limits.max_frame_payload_size)
            .map_err(|error| format!("cannot read block frame: {error}"))?;
        if remaining_before - limited_data.limit()
            != frame
                .encoded_size()
                .map_err(|error| format!("invalid block frame size: {error}"))?
                as u64
        {
            return Err("invalid block frame size while transforming commit".into());
        }
        let plaintext = Zeroizing::new(if frame.flags & crate::BLOCK_FLAG_ENCRYPTED != 0 {
            let key = old_key
                .as_deref()
                .ok_or("encrypted data block is missing its key")?;
            let algorithm = descriptor_algorithm(&old_descriptor)
                .ok_or("encrypted commit is missing its content algorithm")?;
            frame
                .uncompressed_encrypted(
                    &commit.commit_header,
                    key,
                    algorithm,
                    limits.max_uncompressed_block_size,
                )
                .map_err(|error| format!("cannot decrypt data block: {error}"))?
        } else {
            frame
                .uncompressed_at_most(limits.max_uncompressed_block_size)
                .map_err(|error| format!("cannot read data block: {error}"))?
        });
        let new_frame = if new_descriptor.is_data_encrypted() {
            BlockFrame::from_uncompressed_encrypted(
                frame.block_index,
                &plaintext,
                TRANSFORM_COMPRESSION_LEVEL,
                &provisional_header,
                new_key.as_deref().ok_or("new encryption key is missing")?,
                descriptor_algorithm(&new_descriptor)
                    .ok_or("new encryption algorithm is missing")?,
            )
            .map_err(|error| format!("cannot encrypt data block: {error}"))?
        } else {
            BlockFrame::from_uncompressed(
                frame.block_index,
                &plaintext,
                TRANSFORM_COMPRESSION_LEVEL,
            )
            .map_err(|error| format!("cannot encode data block: {error}"))?
        };
        new_frame.write_to(output)?;
    }

    let output_data_size = output
        .stream_position()?
        .checked_sub(output_data_start)
        .ok_or("output data size underflow")?;
    let commit_header_without_size = CommitHeader::new(
        commit.commit_header.flags,
        commit.commit_header.commit_id,
        output_data_size,
        0,
    )?;
    let probe = if new_descriptor.is_header_encrypted() {
        HeaderFrame::from_uncompressed_encrypted(
            commit_header_without_size.clone(),
            &header_bytes,
            descriptor_bytes.clone(),
            TRANSFORM_COMPRESSION_LEVEL,
            new_key.as_deref().ok_or("new encryption key is missing")?,
        )
        .map_err(|error| format!("cannot encode transformed header: {error}"))?
    } else {
        HeaderFrame::from_uncompressed(
            commit_header_without_size.clone(),
            &header_bytes,
            descriptor_bytes.clone(),
            TRANSFORM_COMPRESSION_LEVEL,
        )
        .map_err(|error| format!("cannot encode transformed header: {error}"))?
    };
    let header_frame_size = u32::try_from(
        probe
            .encoded_size()
            .map_err(|error| format!("invalid transformed header size: {error}"))?,
    )?;
    let commit_header = CommitHeader::new(
        commit_header_without_size.flags,
        commit_header_without_size.commit_id,
        commit_header_without_size.data_section_size,
        header_frame_size,
    )?;
    let header = if new_descriptor.is_header_encrypted() {
        HeaderFrame::from_uncompressed_encrypted(
            commit_header,
            &header_bytes,
            descriptor_bytes,
            TRANSFORM_COMPRESSION_LEVEL,
            new_key.as_deref().ok_or("new encryption key is missing")?,
        )
        .map_err(|error| format!("cannot encode transformed header: {error}"))?
    } else {
        HeaderFrame::from_uncompressed(
            commit_header,
            &header_bytes,
            descriptor_bytes,
            TRANSFORM_COMPRESSION_LEVEL,
        )
        .map_err(|error| format!("cannot encode transformed header: {error}"))?
    };
    header.write_to(output)?;
    Ok((output_data_size, new_key))
}

fn descriptor_algorithm(descriptor: &EncryptionDescriptor) -> Option<ContentAead> {
    match descriptor {
        EncryptionDescriptor::Unencrypted => None,
        EncryptionDescriptor::Symmetric { algorithm, .. }
        | EncryptionDescriptor::Password { algorithm, .. }
        | EncryptionDescriptor::Recipients { algorithm, .. }
        | EncryptionDescriptor::PasswordAndRecipients { algorithm, .. } => Some(*algorithm),
    }
}

fn unwrap_commit_key(
    password: &[u8],
    descriptor: &EncryptionDescriptor,
) -> Result<Zeroizing<[u8; crate::DEK_SIZE]>, Box<dyn std::error::Error>> {
    let wrap = match descriptor {
        EncryptionDescriptor::Password { key_wrap, .. } => key_wrap,
        EncryptionDescriptor::PasswordAndRecipients { password, .. } => password,
        _ => return Err("encrypted commit is not unlockable with a password".into()),
    };
    Ok(crate::unwrap_dek_with_password_zeroizing(password, wrap)?)
}

fn validate_written_commit(
    staged: &mut File,
    header_offset: u64,
    data_size: u64,
    limits: CommitValidationLimits,
    key: Option<&[u8; crate::DEK_SIZE]>,
) -> Result<(), Box<dyn std::error::Error>> {
    staged.seek(SeekFrom::Start(header_offset))?;
    let header = HeaderFrame::read_from(staged, limits.max_frame_payload_size)?;
    let data_start = header_offset
        .checked_sub(data_size)
        .ok_or("staged data start underflow")?;
    staged.seek(SeekFrom::Start(data_start))?;
    if let Some(key) = key {
        ValidatedCommit::read_body_with_key(staged, header.commit_header, limits, key)?;
    } else {
        ValidatedCommit::read_body(staged, header.commit_header, limits)?;
    }
    Ok(())
}

fn validate_staged_archive(
    path: &Path,
    limits: CommitValidationLimits,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut staged = File::open(path)?;
    let file_len = staged.metadata()?.len();
    let mut header_offset = read_first_header_offset(&mut staged)?;
    let mut seen_offsets = HashSet::new();
    while header_offset != 0 {
        if !seen_offsets.insert(header_offset) {
            return Err("staged archive commit chain contains a cycle".into());
        }
        let (commit_header, _, next_header_offset) =
            read_commit_at_offset(&mut staged, header_offset, limits)?;
        let commit_end = header_offset
            .checked_add(u64::from(commit_header.header_frame_size))
            .and_then(|offset| offset.checked_add(HEADER_OFFSET_SIZE))
            .ok_or("staged commit end overflow")?;
        if commit_end > file_len {
            return Err("staged commit extends beyond the end of the archive".into());
        }
        ValidatedCommit::read_body(&mut staged, commit_header, limits)?;
        header_offset = next_header_offset;
    }
    Ok(())
}

fn copy_file_region(
    input: &mut File,
    output: &mut File,
    offset: u64,
    size: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    input.seek(SeekFrom::Start(offset))?;
    let copied = io::copy(&mut input.take(size), output)?;
    if copied != size {
        return Err("archive ended unexpectedly while copying a commit".into());
    }
    Ok(())
}

fn resolve_output_path(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = absolute
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or("output path has no parent directory")?
        .canonicalize()?;
    let filename = absolute.file_name().ok_or("output path has no filename")?;
    Ok(parent.join(filename))
}

fn paths_refer_to_same_file(
    source: &Path,
    destination: &Path,
) -> Result<bool, Box<dyn std::error::Error>> {
    if destination.exists() {
        let destination_canonical = destination.canonicalize()?;
        if source == destination_canonical {
            return Ok(true);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let source_metadata = source.metadata()?;
            let destination_metadata = destination.metadata()?;
            if source_metadata.dev() == destination_metadata.dev()
                && source_metadata.ino() == destination_metadata.ino()
            {
                return Ok(true);
            }
        }
    }
    Ok(source == destination)
}

#[cfg(unix)]
fn replace_archive(source: &Path, destination: &Path) -> io::Result<()> {
    std::fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_archive(source: &Path, destination: &Path) -> io::Result<()> {
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
fn replace_archive(source: &Path, destination: &Path) -> io::Result<()> {
    std::fs::rename(source, destination)
}

#[cfg(test)]
mod tests {
    use super::{
        CommitRangeOptions, change_encryption_for_commits, decrypt_archive_commits,
        encrypt_archive_commits,
    };
    use crate::operations::{
        PasswordSet, add_archive, create_archive, info_archive, list_archive_with_passwords,
    };
    use crate::{ContentAead, EncryptionScope};
    use std::fs;
    use std::path::Path;

    fn range<'a>(
        from_id: &'a str,
        to_id: &'a str,
        output: Option<&'a str>,
        passwords: &'a PasswordSet,
        strict: bool,
    ) -> CommitRangeOptions<'a> {
        CommitRangeOptions {
            from_id,
            to_id,
            output,
            passwords,
            strict,
        }
    }

    fn create_three_commit_archive(directory: &Path) -> std::path::PathBuf {
        let archive = directory.join("archive.zar");
        let first = directory.join("first.txt");
        let second = directory.join("second.txt");
        let third = directory.join("third.txt");
        fs::write(&first, "first contents").unwrap();
        fs::write(&second, "second contents").unwrap();
        fs::write(&third, "third contents").unwrap();
        create_archive(archive.to_str().unwrap(), &[first]).unwrap();
        add_archive(archive.to_str().unwrap(), &[second]).unwrap();
        add_archive(archive.to_str().unwrap(), &[third]).unwrap();
        archive
    }

    #[test]
    fn encrypts_and_decrypts_only_the_inclusive_commit_range() {
        let directory = tempfile::tempdir().unwrap();
        let archive = create_three_commit_archive(directory.path());
        let output = directory.path().join("encrypted.zar");
        let no_passwords = PasswordSet::default();

        encrypt_archive_commits(
            archive.to_str().unwrap(),
            range(
                "00000002",
                "00000003",
                Some(output.to_str().unwrap()),
                &no_passwords,
                true,
            ),
            b"new password",
            ContentAead::Aes256Gcm,
            EncryptionScope::HeaderAndData,
        )
        .unwrap();

        let original_info = info_archive(archive.to_str().unwrap(), false).unwrap();
        let encrypted_info = info_archive(output.to_str().unwrap(), false).unwrap();
        assert_eq!(original_info.len(), 3);
        assert!(original_info.iter().all(|commit| !commit.header_encrypted));
        assert!(!encrypted_info[0].header_encrypted);
        assert!(encrypted_info[1].header_encrypted);
        assert!(encrypted_info[1].data_encrypted);
        assert!(encrypted_info[2].header_encrypted);

        let passwords = PasswordSet::new(Some(b"new password".to_vec()));
        list_archive_with_passwords(output.to_str().unwrap(), None, &passwords).unwrap();
        decrypt_archive_commits(
            output.to_str().unwrap(),
            range("00000002", "00000003", None, &passwords, true),
        )
        .unwrap();
        let decrypted_info = info_archive(output.to_str().unwrap(), false).unwrap();
        assert!(decrypted_info.iter().all(|commit| !commit.header_encrypted));
        list_archive_with_passwords(output.to_str().unwrap(), None, &PasswordSet::default())
            .unwrap();
    }

    #[test]
    fn reencrypts_existing_commits_using_their_current_password() {
        let directory = tempfile::tempdir().unwrap();
        let archive = create_three_commit_archive(directory.path());
        let no_passwords = PasswordSet::default();
        encrypt_archive_commits(
            archive.to_str().unwrap(),
            range("00000002", "00000002", None, &no_passwords, true),
            b"old password",
            ContentAead::Aes256Gcm,
            EncryptionScope::Data,
        )
        .unwrap();

        let before_wrong_password = fs::read(&archive).unwrap();
        let incorrect_passwords = PasswordSet::new(Some(b"incorrect password".to_vec()));
        let wrong_password_error = encrypt_archive_commits(
            archive.to_str().unwrap(),
            range("00000002", "00000002", None, &incorrect_passwords, true),
            b"replacement password",
            ContentAead::ChaCha20Poly1305,
            EncryptionScope::Data,
        )
        .unwrap_err();
        assert!(
            wrong_password_error
                .to_string()
                .contains("authentication failed")
        );
        assert_eq!(fs::read(&archive).unwrap(), before_wrong_password);

        let old_passwords = PasswordSet::new(Some(b"old password".to_vec()));
        encrypt_archive_commits(
            archive.to_str().unwrap(),
            range("00000002", "00000002", None, &old_passwords, true),
            b"new password",
            ContentAead::ChaCha20Poly1305,
            EncryptionScope::HeaderAndData,
        )
        .unwrap();
        let info = info_archive(archive.to_str().unwrap(), false).unwrap();
        assert_eq!(
            info[1].encryption_algorithm,
            Some(ContentAead::ChaCha20Poly1305)
        );
        assert!(info[1].header_encrypted);
        assert!(info[1].data_encrypted);

        let new_passwords = PasswordSet::new(Some(b"new password".to_vec()));
        list_archive_with_passwords(archive.to_str().unwrap(), None, &new_passwords).unwrap();
        decrypt_archive_commits(
            archive.to_str().unwrap(),
            range("00000002", "00000002", None, &new_passwords, true),
        )
        .unwrap();
        assert!(
            info_archive(archive.to_str().unwrap(), false)
                .unwrap()
                .iter()
                .all(|commit| !commit.header_encrypted && !commit.data_encrypted)
        );
    }

    #[test]
    fn change_encryption_rewrites_only_encrypted_commits_in_range() {
        let directory = tempfile::tempdir().unwrap();
        let archive = create_three_commit_archive(directory.path());
        let no_passwords = PasswordSet::default();
        encrypt_archive_commits(
            archive.to_str().unwrap(),
            range("00000002", "00000002", None, &no_passwords, true),
            b"old password",
            ContentAead::Aes256Gcm,
            EncryptionScope::HeaderAndData,
        )
        .unwrap();
        let old_passwords = PasswordSet::new(Some(b"old password".to_vec()));
        change_encryption_for_commits(
            archive.to_str().unwrap(),
            range("00000001", "00000003", None, &old_passwords, true),
            b"new password",
            ContentAead::ChaCha20Poly1305,
            EncryptionScope::Data,
        )
        .unwrap();

        let info = info_archive(archive.to_str().unwrap(), false).unwrap();
        assert!(!info[0].header_encrypted && !info[0].data_encrypted);
        assert!(!info[1].header_encrypted && info[1].data_encrypted);
        assert!(!info[2].header_encrypted && !info[2].data_encrypted);
        assert_eq!(
            info[1].encryption_algorithm,
            Some(ContentAead::ChaCha20Poly1305)
        );
        let new_passwords = PasswordSet::new(Some(b"new password".to_vec()));
        list_archive_with_passwords(archive.to_str().unwrap(), None, &new_passwords).unwrap();

        let output = directory.path().join("changed-copy.zar");
        let old_passwords = PasswordSet::new(Some(b"new password".to_vec()));
        change_encryption_for_commits(
            archive.to_str().unwrap(),
            range(
                "00000002",
                "00000002",
                Some(output.to_str().unwrap()),
                &old_passwords,
                true,
            ),
            b"copy password",
            ContentAead::Aes256Gcm,
            EncryptionScope::Header,
        )
        .unwrap();
        let output_info = info_archive(output.to_str().unwrap(), false).unwrap();
        assert!(!output_info[0].header_encrypted && !output_info[0].data_encrypted);
        assert!(output_info[1].header_encrypted && !output_info[1].data_encrypted);
        assert!(!output_info[2].header_encrypted && !output_info[2].data_encrypted);
        list_archive_with_passwords(
            output.to_str().unwrap(),
            None,
            &PasswordSet::new(Some(b"copy password".to_vec())),
        )
        .unwrap();
        assert!(info_archive(archive.to_str().unwrap(), false).unwrap()[1].data_encrypted);
    }

    #[test]
    fn change_encryption_rejects_missing_or_incorrect_current_password_atomically() {
        let directory = tempfile::tempdir().unwrap();
        let archive = create_three_commit_archive(directory.path());
        let no_passwords = PasswordSet::default();
        encrypt_archive_commits(
            archive.to_str().unwrap(),
            range("00000002", "00000002", None, &no_passwords, true),
            b"old password",
            ContentAead::Aes256Gcm,
            EncryptionScope::Data,
        )
        .unwrap();
        let before = fs::read(&archive).unwrap();
        let missing_error = change_encryption_for_commits(
            archive.to_str().unwrap(),
            range("00000002", "00000002", None, &no_passwords, true),
            b"new password",
            ContentAead::Aes256Gcm,
            EncryptionScope::Data,
        )
        .unwrap_err();
        assert!(missing_error.to_string().contains("without its password"));
        assert_eq!(fs::read(&archive).unwrap(), before);

        let wrong_passwords = PasswordSet::new(Some(b"wrong password".to_vec()));
        assert!(
            change_encryption_for_commits(
                archive.to_str().unwrap(),
                range("00000002", "00000002", None, &wrong_passwords, true),
                b"new password",
                ContentAead::Aes256Gcm,
                EncryptionScope::Data,
            )
            .is_err()
        );
        assert_eq!(fs::read(&archive).unwrap(), before);
    }

    #[test]
    fn strict_missing_password_fails_and_non_strict_copies_the_commit() {
        let directory = tempfile::tempdir().unwrap();
        let archive = create_three_commit_archive(directory.path());
        let no_passwords = PasswordSet::default();
        encrypt_archive_commits(
            archive.to_str().unwrap(),
            range("00000001", "00000001", None, &no_passwords, true),
            b"root password",
            ContentAead::Aes256Gcm,
            EncryptionScope::Header,
        )
        .unwrap();
        let before = fs::read(&archive).unwrap();

        let strict_error = decrypt_archive_commits(
            archive.to_str().unwrap(),
            range("00000001", "00000001", None, &no_passwords, true),
        )
        .unwrap_err();
        assert!(strict_error.to_string().contains("without its password"));
        assert_eq!(fs::read(&archive).unwrap(), before);

        let output = directory.path().join("partial.zar");
        decrypt_archive_commits(
            archive.to_str().unwrap(),
            range(
                "00000001",
                "00000001",
                Some(output.to_str().unwrap()),
                &no_passwords,
                false,
            ),
        )
        .unwrap();
        let info = info_archive(output.to_str().unwrap(), false).unwrap();
        assert!(info[0].header_encrypted);
        assert_eq!(info.len(), 3);

        let root_passwords = PasswordSet::new(Some(b"root password".to_vec()));
        list_archive_with_passwords(output.to_str().unwrap(), None, &root_passwords).unwrap();
        decrypt_archive_commits(
            output.to_str().unwrap(),
            range("00000001", "00000001", None, &root_passwords, true),
        )
        .unwrap();
        assert!(info_archive(archive.to_str().unwrap(), false).unwrap()[0].header_encrypted);
        assert!(!info_archive(output.to_str().unwrap(), false).unwrap()[0].header_encrypted);
    }

    #[test]
    fn rejects_invalid_ranges_and_same_destination_paths() {
        let directory = tempfile::tempdir().unwrap();
        let archive = create_three_commit_archive(directory.path());
        let archive_path = archive.to_str().unwrap();
        let no_passwords = PasswordSet::default();

        let reversed = decrypt_archive_commits(
            archive_path,
            range("00000003", "00000001", None, &no_passwords, true),
        )
        .unwrap_err();
        assert!(reversed.to_string().contains("must not appear after"));

        let missing = decrypt_archive_commits(
            archive_path,
            range("00000004", "00000004", None, &no_passwords, true),
        )
        .unwrap_err();
        assert!(missing.to_string().contains("identify exactly one"));

        let same_path = decrypt_archive_commits(
            archive_path,
            range(
                "00000001",
                "00000001",
                Some(&format!("{}/./archive.zar", directory.path().display())),
                &no_passwords,
                true,
            ),
        )
        .unwrap_err();
        assert!(same_path.to_string().contains("must not be the same"));
    }
}
