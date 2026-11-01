use crate::{
    ArchivePath, CommitHeader, CommitValidationLimits, ContainerHeader, ContentAead, DEK_SIZE,
    EncryptionDescriptor, EntryMetadata, HEADER_OFFSET_SIZE, HeaderFrame, ValidatedCommit,
    ValidatedHeader, unwrap_dek_with_password_zeroizing,
};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use sysinfo::System;
use zeroize::Zeroizing;

pub const SKIP_COMPRESS_EXTENSIONS: [&str; 21] = [
    "gz", "tgz", "bz2", "tbz2", "lzo", "lz4", "lzma", "z", "snappy", "zip", "7z", "rar", "jpg",
    "jpeg", "png", "gif", "mpg", "mpeg", "avi", "mp3", "zar",
];

pub fn should_skip_compression(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            SKIP_COMPRESS_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

/// Error raised when strict input scanning rejects unsupported file types.
#[derive(Debug)]
pub struct StrictModeWarning;

impl std::fmt::Display for StrictModeWarning {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("strict mode rejected an unsupported input type")
    }
}

impl std::error::Error for StrictModeWarning {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateReport {
    pub warnings: Vec<String>,
    pub block_size: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputSummary {
    pub files: u64,
    pub empty_files: u64,
    pub tiny_files: u64,
    pub small_files: u64,
    pub big_files: u64,
    pub directories: u64,
    pub symlinks: u64,
    pub named_pipes: u64,
    pub devices: u64,
    pub other_special: u64,
    pub total_bytes: u64,
}

/// Callback receiving progress events while an archive operation runs.
pub trait Progress: Send + Sync {
    fn message(&self, message: &str);
    fn advance(&self, amount: u64);
    /// Set the operation's estimated total work in bytes.
    fn set_total(&self, _total: u64) {}
    fn start_phase(&self, message: &str, total: u64) {
        self.message(message);
        self.set_total(total);
    }
    fn reset_phase(&self, message: &str, total: u64) {
        self.message(message);
        self.set_total(total);
    }
    fn add_summary(&self, _summary: &InputSummary) {}
    fn extract_summary(&self, summary: &InputSummary) {
        self.add_summary(summary);
    }
    fn is_interactive(&self) -> bool {
        false
    }
    fn warning(&self, _message: &str) {}
    fn confirm(&self, _prompt: &str) -> Result<bool, Box<dyn std::error::Error>> {
        Err("interactive confirmation is unavailable".into())
    }
}

pub struct ProgressReporter<'a>(pub Option<&'a dyn Progress>);

impl<'a> ProgressReporter<'a> {
    pub fn new(callback: Option<&'a dyn Progress>) -> Self {
        Self(callback)
    }

    pub fn inc(&self, amount: u64) {
        if let Some(progress) = self.0 {
            progress.advance(amount);
        }
    }

    pub fn set_total(&self, total: u64) {
        if let Some(progress) = self.0 {
            progress.set_total(total);
        }
    }

    pub fn start_phase(&self, message: &str, total: u64) {
        if let Some(progress) = self.0 {
            progress.start_phase(message, total);
        }
    }

    pub fn reset_phase(&self, message: &str, total: u64) {
        if let Some(progress) = self.0 {
            progress.reset_phase(message, total);
        }
    }

    pub fn input_summary(&self, summary: &InputSummary) {
        if let Some(progress) = self.0 {
            progress.add_summary(summary);
        }
    }

    pub fn extraction_summary(&self, summary: &InputSummary) {
        if let Some(progress) = self.0 {
            progress.extract_summary(summary);
        }
    }

    pub fn set_message(&self, message: impl Into<String>) {
        if let Some(progress) = self.0 {
            progress.message(&message.into());
        }
    }
}

pub const CREATE_BLOCK_SIZE: usize = 8 * 1024 * 1024;
pub const AUTO_MIN_BLOCK_SIZE: usize = 1024 * 1024;
pub const MIN_CREATE_BLOCK_SIZE: usize = 64 * 1024;
pub const MAX_CREATE_BLOCK_SIZE: usize = 32 * 1024 * 1024;
pub const DEFAULT_MEMORY_BUDGET_PERCENT: usize = 50;
pub const COMPRESSION_WORKSPACE_MULTIPLIER: usize = 2;
pub const QUEUED_OUTPUT_MULTIPLIER: usize = 2;
pub const FIXED_WORKER_OVERHEAD: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncodingOptions {
    pub block_size: usize,
    pub compression_level: i32,
}

impl Default for EncodingOptions {
    fn default() -> Self {
        Self {
            block_size: CREATE_BLOCK_SIZE,
            compression_level: 3,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncodingBudget {
    pub block_size: usize,
    pub workers: usize,
    pub budget_bytes: usize,
    pub estimated_bytes: usize,
    pub warning: bool,
}

pub fn estimate_worker_memory(block_size: usize) -> usize {
    block_size
        .saturating_mul(1 + COMPRESSION_WORKSPACE_MULTIPLIER + QUEUED_OUTPUT_MULTIPLIER)
        .saturating_add(FIXED_WORKER_OVERHEAD)
}

pub fn select_compression_workers(
    block_size: usize,
    available_memory: usize,
    requested_workers: usize,
) -> usize {
    let budget_bytes = available_memory.saturating_mul(DEFAULT_MEMORY_BUDGET_PERCENT) / 100;
    let per_worker_bytes = estimate_worker_memory(block_size).max(1);
    let memory_limited_workers = (budget_bytes / per_worker_bytes).max(1);
    requested_workers.max(1).min(memory_limited_workers)
}

pub fn select_automatic_encoding(
    available_memory: usize,
    requested_workers: usize,
) -> EncodingBudget {
    let budget_bytes = available_memory.saturating_mul(DEFAULT_MEMORY_BUDGET_PERCENT) / 100;
    let requested_workers = requested_workers.max(1);
    let mut workers = requested_workers;

    loop {
        let per_worker_budget = budget_bytes / workers;
        let affordable = per_worker_budget.saturating_sub(FIXED_WORKER_OVERHEAD)
            / (1 + COMPRESSION_WORKSPACE_MULTIPLIER + QUEUED_OUTPUT_MULTIPLIER);
        if affordable >= AUTO_MIN_BLOCK_SIZE {
            let block_size =
                affordable.min(CREATE_BLOCK_SIZE) / AUTO_MIN_BLOCK_SIZE * AUTO_MIN_BLOCK_SIZE;
            let estimated_bytes = estimate_worker_memory(block_size).saturating_mul(workers);
            return EncodingBudget {
                block_size,
                workers,
                budget_bytes,
                estimated_bytes,
                warning: false,
            };
        }
        if workers == 1 {
            let estimated_bytes = estimate_worker_memory(AUTO_MIN_BLOCK_SIZE);
            return EncodingBudget {
                block_size: AUTO_MIN_BLOCK_SIZE,
                workers: 1,
                budget_bytes,
                estimated_bytes,
                warning: estimated_bytes > budget_bytes,
            };
        }
        workers -= 1;
    }
}

pub fn available_memory_bytes() -> usize {
    static CACHED: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *CACHED.get_or_init(|| {
        if let Ok(mut system) = System::new() {
            system.refresh_memory();
            usize::try_from(system.available_memory()).unwrap_or(usize::MAX)
        } else {
            usize::MAX
        }
    })
}

pub fn available_compression_workers() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

pub fn compression_worker_count(block_size: usize) -> usize {
    select_compression_workers(
        block_size,
        available_memory_bytes(),
        available_compression_workers(),
    )
}

pub fn resolve_encoding_options(options: &mut EncodingOptions) -> Option<String> {
    if options.block_size != 0 {
        return None;
    }
    let budget =
        select_automatic_encoding(available_memory_bytes(), available_compression_workers());
    options.block_size = budget.block_size;
    budget.warning.then(|| {
        format!(
            "warning: automatic block size requires an estimated {} bytes, exceeding the {}-byte memory budget",
            budget.estimated_bytes, budget.budget_bytes
        )
    })
}

pub fn parse_block_size(value: &str) -> Result<usize, Box<dyn std::error::Error>> {
    if value == "auto" {
        return Ok(0);
    }
    let (digits, multiplier) = match value.as_bytes().last().copied() {
        Some(b'k' | b'K') => (&value[..value.len() - 1], 1024_usize),
        Some(b'm' | b'M') => (&value[..value.len() - 1], 1024 * 1024),
        _ => (value, 1),
    };
    let size = digits
        .parse::<usize>()
        .map_err(|_| format!("invalid block size `{value}`"))?
        .checked_mul(multiplier)
        .ok_or_else(|| format!("invalid block size `{value}`: value is too large"))?;
    if !(MIN_CREATE_BLOCK_SIZE..=MAX_CREATE_BLOCK_SIZE).contains(&size) {
        return Err(format!(
            "invalid block size `{value}`: choose {MIN_CREATE_BLOCK_SIZE} bytes through {MAX_CREATE_BLOCK_SIZE} bytes"
        )
        .into());
    }
    Ok(size)
}

pub fn parse_compression_level(value: &str) -> Result<i32, Box<dyn std::error::Error>> {
    let level = value
        .parse::<i32>()
        .map_err(|_| format!("invalid compression level `{value}`"))?;
    if !(-7..=22).contains(&level) {
        return Err("invalid compression level: choose a value from -7 through 22".into());
    }
    Ok(level)
}

pub static TEMP_FILE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn unique_temp_file_id() -> Result<u64, getrandom::Error> {
    let mut id = [0; 8];
    getrandom::fill(&mut id)?;
    Ok(u64::from_ne_bytes(id))
}

pub struct TemporaryFiles(pub Vec<PathBuf>);

impl Drop for TemporaryFiles {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub fn scan_input_summary(
    paths: &[PathBuf],
    cwd: &Path,
) -> Result<InputSummary, Box<dyn std::error::Error>> {
    fn walk(path: &Path, summary: &mut InputSummary) -> Result<(), Box<dyn std::error::Error>> {
        let metadata = std::fs::symlink_metadata(path)?;
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            summary.symlinks += 1;
        } else if metadata.is_dir() {
            if path.parent().is_some() {
                summary.directories += 1;
            }
            for child in std::fs::read_dir(path)? {
                walk(&child?.path(), summary)?;
            }
        } else if metadata.is_file() {
            summary.files += 1;
            summary.total_bytes = summary
                .total_bytes
                .checked_add(metadata.len())
                .ok_or("input size estimate overflow")?;
            if metadata.len() == 0 {
                summary.empty_files += 1;
            } else if metadata.len() <= 8 {
                summary.tiny_files += 1;
            } else if metadata.len() < 8 * 1024 * 1024 {
                summary.small_files += 1;
            } else {
                summary.big_files += 1;
            }
        } else {
            #[cfg(unix)]
            {
                use std::os::unix::fs::FileTypeExt;
                if file_type.is_fifo() {
                    summary.named_pipes += 1;
                } else if file_type.is_char_device() || file_type.is_block_device() {
                    summary.devices += 1;
                } else {
                    summary.other_special += 1;
                }
            }
            #[cfg(not(unix))]
            {
                summary.other_special += 1;
            }
        }
        Ok(())
    }
    let mut summary = InputSummary::default();
    for input in paths {
        let source = if input.is_absolute() {
            input.clone()
        } else {
            cwd.join(input)
        };
        walk(&source, &mut summary)?;
    }
    Ok(summary)
}

#[cfg(unix)]
pub fn platform_identifier() -> &'static str {
    "unix"
}

#[cfg(windows)]
pub fn platform_identifier() -> &'static str {
    "windows"
}

#[cfg(not(any(unix, windows)))]
pub fn platform_identifier() -> &'static str {
    "other"
}

pub fn modified_unix_millis(metadata: &std::fs::Metadata) -> Option<i64> {
    let modified = metadata.modified().ok()?;
    let millis = match modified.duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => i128::try_from(duration.as_millis()).ok()?,
        Err(error) => -(error.duration().as_millis() as i128),
    };
    i64::try_from(millis).ok()
}

/// Drops modification times unless requested, and omits records that carry no data.
pub fn finalize_entry_metadata(
    mut entries: Vec<EntryMetadata>,
    save_dates: bool,
) -> Vec<EntryMetadata> {
    if !save_dates {
        for entry in &mut entries {
            entry.modified_unix_millis = None;
        }
    }
    entries.retain(|entry| {
        entry.owner.is_some()
            || entry.group.is_some()
            || entry.permissions.is_some()
            || entry.modified_unix_millis.is_some()
    });
    entries
}

pub fn entry_date_metadata(path: &ArchivePath, modified_unix_millis: Option<i64>) -> EntryMetadata {
    EntryMetadata {
        path: path.clone(),
        platform: platform_identifier().to_owned(),
        owner: None,
        group: None,
        permissions: None,
        modified_unix_millis,
    }
}

#[cfg(unix)]
pub fn capture_entry_metadata(
    path: &ArchivePath,
    metadata: &std::fs::Metadata,
    save_metadata: bool,
) -> Result<EntryMetadata, Box<dyn std::error::Error>> {
    use std::os::unix::fs::MetadataExt;
    Ok(EntryMetadata {
        path: path.clone(),
        platform: platform_identifier().to_owned(),
        owner: save_metadata.then(|| metadata.uid()),
        group: save_metadata.then(|| metadata.gid()),
        permissions: save_metadata.then(|| metadata.mode() & 0o7777),
        modified_unix_millis: modified_unix_millis(metadata),
    })
}

#[cfg(windows)]
pub fn capture_entry_metadata(
    path: &ArchivePath,
    metadata: &std::fs::Metadata,
    save_metadata: bool,
) -> Result<EntryMetadata, Box<dyn std::error::Error>> {
    Ok(EntryMetadata {
        path: path.clone(),
        platform: platform_identifier().to_owned(),
        owner: None,
        group: None,
        permissions: save_metadata.then(|| u32::from(metadata.permissions().readonly())),
        modified_unix_millis: modified_unix_millis(metadata),
    })
}

#[cfg(not(any(unix, windows)))]
pub fn capture_entry_metadata(
    path: &ArchivePath,
    metadata: &std::fs::Metadata,
    save_metadata: bool,
) -> Result<EntryMetadata, Box<dyn std::error::Error>> {
    if save_metadata {
        return Err("metadata preservation is unsupported on this platform".into());
    }
    Ok(entry_date_metadata(path, modified_unix_millis(metadata)))
}

pub fn push_unique_path(
    path: ArchivePath,
    seen_paths: &mut std::collections::BTreeSet<ArchivePath>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !seen_paths.insert(path.clone()) {
        return Err(format!("duplicate archive path: {path}").into());
    }
    Ok(())
}

pub fn commit_id(value: u32) -> [u8; 4] {
    value.to_le_bytes()
}

pub fn parse_commit_id(value: &str) -> Result<[u8; 4], Box<dyn std::error::Error>> {
    if value.len() != 8 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("commit ID must contain exactly 8 hexadecimal digits".into());
    }
    Ok(u32::from_str_radix(value, 16)?.to_le_bytes())
}

pub fn next_commit_id(archive: &Path) -> Result<[u8; 4], Box<dyn std::error::Error>> {
    let mut file = File::open(archive)?;
    let limits = CommitValidationLimits::default();
    let first_header_offset = read_first_header_offset(&mut file)?;
    let mut highest_id = 0_u32;
    let mut header_offset = first_header_offset;
    while header_offset != 0 {
        let (commit_header, _, next_header_offset) =
            read_commit_at_offset(&mut file, header_offset, limits)?;
        highest_id = highest_id.max(u32::from_le_bytes(commit_header.commit_id));
        header_offset = next_header_offset;
    }
    Ok(highest_id
        .checked_add(1)
        .ok_or("commit ID counter exhausted")?
        .to_le_bytes())
}

pub fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

pub fn skip_exact(file: &mut File, mut remaining: u64) -> io::Result<()> {
    let mut buffer = [0_u8; 8192];
    while remaining > 0 {
        let chunk = remaining.min(buffer.len() as u64) as usize;
        file.read_exact(&mut buffer[..chunk])?;
        remaining -= chunk as u64;
    }
    Ok(())
}

pub fn read_first_header_offset(file: &mut File) -> Result<u64, Box<dyn std::error::Error>> {
    let container = ContainerHeader::read_from(file)?;
    Ok(container.first_header_offset)
}

pub fn read_commit_at_offset(
    file: &mut File,
    header_offset: u64,
    limits: CommitValidationLimits,
) -> Result<(CommitHeader, u64, u64), Box<dyn std::error::Error>> {
    if header_offset == 0 {
        return Err("invalid header offset: 0".into());
    }
    file.seek(SeekFrom::Start(header_offset))?;
    let header_frame = HeaderFrame::read_from(file, limits.max_frame_payload_size)?;
    let commit_header = header_frame.commit_header.clone();
    let data_start = header_offset
        .checked_sub(commit_header.data_section_size)
        .ok_or("data section offset underflow")?;
    let next_offset_pos = data_start
        .checked_sub(HEADER_OFFSET_SIZE)
        .ok_or("next_header_offset position underflow")?;
    file.seek(SeekFrom::Start(next_offset_pos))?;
    let mut next_offset_bytes = [0u8; 8];
    file.read_exact(&mut next_offset_bytes)?;
    let next_header_offset = u64::from_le_bytes(next_offset_bytes);
    file.seek(SeekFrom::Start(data_start))?;
    Ok((commit_header, data_start, next_header_offset))
}

pub fn find_last_commit(
    file: &mut File,
    limits: CommitValidationLimits,
) -> Result<(u64, CommitHeader), Box<dyn std::error::Error>> {
    let file_len = file.metadata()?.len();
    if file_len < crate::CONTAINER_HEADER_SIZE + HEADER_OFFSET_SIZE {
        return Err("archive is too small to contain a commit".into());
    }
    let this_offset_pos = file_len - HEADER_OFFSET_SIZE;
    file.seek(SeekFrom::Start(this_offset_pos))?;
    let mut bytes = [0u8; 8];
    file.read_exact(&mut bytes)?;
    let header_offset = u64::from_le_bytes(bytes);
    if header_offset == 0 || header_offset >= file_len {
        return Err("archive has no valid last commit".into());
    }
    file.seek(SeekFrom::Start(header_offset))?;
    let header_frame = HeaderFrame::read_from(file, limits.max_frame_payload_size)?;
    Ok((header_offset, header_frame.commit_header))
}

pub fn ensure_archive_unencrypted(archive_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::open(archive_path)?;
    let limits = CommitValidationLimits::default();
    let first_header_offset = read_first_header_offset(&mut file)?;
    let mut header_offset = first_header_offset;
    while header_offset != 0 {
        file.seek(SeekFrom::Start(header_offset))?;
        let header_frame = HeaderFrame::read_from(&mut file, limits.max_frame_payload_size)?;
        let (descriptor, _) = EncryptionDescriptor::decode(&header_frame.encryption_descriptor)?;
        if descriptor.is_header_encrypted() || descriptor.is_data_encrypted() {
            return Err(
                "mutations on archives containing encrypted commits are unsupported".into(),
            );
        }
        let (_, _, next_header_offset) = read_commit_at_offset(&mut file, header_offset, limits)?;
        header_offset = next_header_offset;
    }
    Ok(())
}

pub type UnlockedCommit = (ValidatedCommit, Option<Zeroizing<[u8; DEK_SIZE]>>);
pub type UnlockedHeader = (ValidatedHeader, Option<Zeroizing<[u8; DEK_SIZE]>>);

#[derive(Default)]
pub struct PasswordSet {
    default: Option<Zeroizing<Vec<u8>>>,
    by_commit: BTreeMap<[u8; 4], Zeroizing<Vec<u8>>>,
}

impl PasswordSet {
    pub fn new(default: Option<Vec<u8>>) -> Self {
        Self {
            default: default.map(Zeroizing::new),
            by_commit: BTreeMap::new(),
        }
    }

    pub fn insert_for_commit(&mut self, commit_id: [u8; 4], password: Vec<u8>) {
        self.by_commit.insert(commit_id, Zeroizing::new(password));
    }

    pub fn password_for(&self, commit_id: &[u8; 4]) -> Option<&[u8]> {
        self.by_commit
            .get(commit_id)
            .or(self.default.as_ref())
            .map(|password| password.as_slice())
    }
}

pub fn read_header_with_password(
    file: &mut File,
    limits: CommitValidationLimits,
    password: Option<&[u8]>,
) -> Result<UnlockedHeader, Box<dyn std::error::Error>> {
    let passwords = PasswordSet::new(password.map(<[u8]>::to_vec));
    read_header_with_passwords(file, limits, &passwords)
}

pub fn read_header_with_passwords(
    file: &mut File,
    limits: CommitValidationLimits,
    passwords: &PasswordSet,
) -> Result<UnlockedHeader, Box<dyn std::error::Error>> {
    let body_start = file.stream_position()?;
    let header_frame = HeaderFrame::read_from(file, limits.max_frame_payload_size)?;
    let password = passwords.password_for(&header_frame.commit_header.commit_id);
    let (descriptor, _) = EncryptionDescriptor::decode(&header_frame.encryption_descriptor)?;
    let key = match (password, descriptor) {
        (Some(password), EncryptionDescriptor::Password { key_wrap, .. }) => {
            Some(unwrap_dek_with_password_zeroizing(password, &key_wrap)?)
        }
        (Some(password), EncryptionDescriptor::PasswordAndRecipients { password: wrap, .. }) => {
            Some(unwrap_dek_with_password_zeroizing(password, &wrap)?)
        }
        (Some(_), descriptor)
            if descriptor.is_header_encrypted() || descriptor.is_data_encrypted() =>
        {
            return Err("this encryption descriptor cannot be unlocked with a password".into());
        }
        _ => None,
    };
    file.seek(SeekFrom::Start(body_start))?;
    let validated = match key.as_ref() {
        Some(key) => ValidatedHeader::read_header_with_key(file, limits, key)?,
        None => ValidatedHeader::read_header(file, limits)?,
    };
    Ok((validated, key))
}

pub fn read_commit_with_password(
    file: &mut File,
    commit_header: CommitHeader,
    limits: CommitValidationLimits,
    password: Option<&[u8]>,
) -> Result<UnlockedCommit, Box<dyn std::error::Error>> {
    let passwords = PasswordSet::new(password.map(<[u8]>::to_vec));
    read_commit_with_passwords(file, commit_header, limits, &passwords)
}

pub fn read_commit_with_passwords(
    file: &mut File,
    commit_header: CommitHeader,
    limits: CommitValidationLimits,
    passwords: &PasswordSet,
) -> Result<UnlockedCommit, Box<dyn std::error::Error>> {
    let data_start = file.stream_position()?;
    let header_offset = data_start
        .checked_add(commit_header.data_section_size)
        .ok_or("header offset overflow")?;
    file.seek(SeekFrom::Start(header_offset))?;
    let header = HeaderFrame::read_from(file, limits.max_frame_payload_size)?;
    let password = passwords.password_for(&commit_header.commit_id);
    let (descriptor, _) = EncryptionDescriptor::decode(&header.encryption_descriptor)?;
    let key = match (password, descriptor) {
        (Some(password), EncryptionDescriptor::Password { key_wrap, .. }) => {
            Some(unwrap_dek_with_password_zeroizing(password, &key_wrap)?)
        }
        (Some(password), EncryptionDescriptor::PasswordAndRecipients { password: wrap, .. }) => {
            Some(unwrap_dek_with_password_zeroizing(password, &wrap)?)
        }
        (Some(_), descriptor)
            if descriptor.is_header_encrypted() || descriptor.is_data_encrypted() =>
        {
            return Err("this encryption descriptor cannot be unlocked with a password".into());
        }
        _ => None,
    };
    file.seek(SeekFrom::Start(data_start))?;
    let validated = match key.as_ref() {
        Some(key) => ValidatedCommit::read_body_with_key(file, commit_header, limits, key)?,
        None => ValidatedCommit::read_body(file, commit_header, limits)?,
    };
    Ok((validated, key))
}

pub fn extraction_content_nonce(descriptor: &EncryptionDescriptor) -> Option<[u8; 12]> {
    match descriptor {
        EncryptionDescriptor::Unencrypted => None,
        EncryptionDescriptor::Symmetric { nonce, .. }
        | EncryptionDescriptor::Password { nonce, .. }
        | EncryptionDescriptor::Recipients { nonce, .. }
        | EncryptionDescriptor::PasswordAndRecipients { nonce, .. } => Some(*nonce),
    }
}

pub fn encryption_algorithm(descriptor: &EncryptionDescriptor) -> Option<ContentAead> {
    match descriptor {
        EncryptionDescriptor::Unencrypted => None,
        EncryptionDescriptor::Symmetric { algorithm, .. }
        | EncryptionDescriptor::Password { algorithm, .. }
        | EncryptionDescriptor::Recipients { algorithm, .. }
        | EncryptionDescriptor::PasswordAndRecipients { algorithm, .. } => Some(*algorithm),
    }
}
