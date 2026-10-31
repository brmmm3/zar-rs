use std::{collections::HashMap, fmt};

use crate::{ArchivePath, ArchivePathError, FileEntry, TocChange};

const HEADER_MAGIC: [u8; 4] = *b"ZHDR";
const FILE_TYPE_HEADER_VERSION: u16 = 6;
const ENTRY_METADATA_HEADER_VERSION: u16 = 5;
const METADATA_HEADER_VERSION: u16 = 4;
const HEADER_VERSION: u16 = 3;
const CHECKSUM_HEADER_VERSION: u16 = 2;
const BLOCK_SIZE_HEADER_VERSION: u16 = 1;
const LEGACY_HEADER_VERSION: u16 = 0;
const RECORD_PUT_FILE: u8 = 1;
const RECORD_PUT_DIRECTORY: u8 = 2;
const RECORD_REMOVE_FILE: u8 = 3;
const RECORD_REMOVE_DIRECTORY: u8 = 4;
const RECORD_PUT_SYMLINK: u8 = 5;
const RECORD_PUT_NAMED_PIPE: u8 = 6;
const RECORD_PUT_DEVICE: u8 = 7;
const DEVICE_CHARACTER: u8 = 1;
const DEVICE_BLOCK: u8 = 2;
const LEGACY_HEADER_PREFIX_SIZE: usize = 10;
const HEADER_PREFIX_SIZE: usize = 14;
const METADATA_HEADER_PREFIX_SIZE: usize = 18;
const METADATA_STRING: u8 = 1;
const METADATA_I64: u8 = 2;
const METADATA_F64: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogicalHeaderLimits {
    pub max_encoded_size: usize,
    pub max_records: usize,
    pub max_path_size: usize,
    pub max_block_references_per_file: usize,
    pub max_metadata_entries: usize,
    pub max_metadata_key_size: usize,
    pub max_metadata_string_size: usize,
    pub max_entry_metadata: usize,
}

impl Default for LogicalHeaderLimits {
    fn default() -> Self {
        Self {
            max_encoded_size: 64 * 1024 * 1024,
            max_records: 1_000_000,
            max_path_size: 1024 * 1024,
            max_block_references_per_file: 1_000_000,
            max_metadata_entries: 65_536,
            max_metadata_key_size: 1024 * 1024,
            max_metadata_string_size: 16 * 1024 * 1024,
            max_entry_metadata: 1_000_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogicalHeaderError {
    InvalidMagic,
    UnsupportedVersion(u16),
    InvalidRecord(u8),
    InvalidFileEntry,
    InvalidMetadata,
    InvalidUtf8Path,
    InvalidUtf8Metadata,
    InvalidPath(ArchivePathError),
    LimitExceeded,
    Truncated,
    TrailingData,
}

impl fmt::Display for LogicalHeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic => f.write_str("invalid logical-header magic"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported logical-header version {version}")
            }
            Self::InvalidRecord(tag) => write!(f, "invalid logical-header record tag {tag}"),
            Self::InvalidFileEntry => f.write_str("invalid file metadata or block references"),
            Self::InvalidMetadata => f.write_str("invalid logical-header metadata"),
            Self::InvalidUtf8Path => f.write_str("archive path is not valid UTF-8"),
            Self::InvalidUtf8Metadata => f.write_str("metadata key or value is not valid UTF-8"),
            Self::InvalidPath(error) => write!(f, "invalid archive path: {error}"),
            Self::LimitExceeded => f.write_str("logical header exceeds configured limits"),
            Self::Truncated => f.write_str("truncated logical header"),
            Self::TrailingData => f.write_str("logical header contains trailing data"),
        }
    }
}

impl std::error::Error for LogicalHeaderError {}

#[derive(Debug, Clone)]
pub enum MetadataType {
    String(String),
    I64(i64),
    F64(f64),
}

impl Eq for MetadataType {}

impl PartialEq for MetadataType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::String(left), Self::String(right)) => left == right,
            (Self::I64(left), Self::I64(right)) => left == right,
            (Self::F64(left), Self::F64(right)) => left.to_bits() == right.to_bits(),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogicalHeader {
    pub changes: Vec<TocChange>,
    pub block_size: Option<u32>,
    pub metadata: HashMap<String, MetadataType>,
    pub entry_metadata: Vec<EntryMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryMetadata {
    pub path: ArchivePath,
    pub platform: String,
    pub owner: Option<u32>,
    pub group: Option<u32>,
    pub permissions: Option<u32>,
}

impl LogicalHeader {
    pub fn new(changes: Vec<TocChange>) -> Self {
        Self {
            changes,
            block_size: None,
            metadata: HashMap::new(),
            entry_metadata: Vec::new(),
        }
    }

    pub fn with_block_size(changes: Vec<TocChange>, block_size: u32) -> Self {
        Self {
            changes,
            block_size: Some(block_size),
            metadata: HashMap::new(),
            entry_metadata: Vec::new(),
        }
    }

    pub fn with_metadata(mut self, metadata: HashMap<String, MetadataType>) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn with_entry_metadata(mut self, entry_metadata: Vec<EntryMetadata>) -> Self {
        self.entry_metadata = entry_metadata;
        self
    }

    /// Encode ordered TOC changes, using v1 when block-size metadata is present.
    pub fn encode(&self) -> Result<Vec<u8>, LogicalHeaderError> {
        self.encode_with_limits(LogicalHeaderLimits::default())
    }

    pub fn encode_with_limits(
        &self,
        limits: LogicalHeaderLimits,
    ) -> Result<Vec<u8>, LogicalHeaderError> {
        let has_checksums = self.changes.iter().any(
            |change| matches!(change, TocChange::PutFile { entry, .. } if entry.checksum.is_some()),
        );
        if self.block_size == Some(0) {
            return Err(LogicalHeaderError::InvalidFileEntry);
        }
        let version = if self.changes.iter().any(|change| {
            matches!(
                change,
                TocChange::PutSymlink { .. }
                    | TocChange::PutNamedPipe { .. }
                    | TocChange::PutDevice { .. }
            )
        }) {
            FILE_TYPE_HEADER_VERSION
        } else if !self.entry_metadata.is_empty() {
            ENTRY_METADATA_HEADER_VERSION
        } else if !self.metadata.is_empty() {
            METADATA_HEADER_VERSION
        } else if self.block_size.is_some() && has_checksums {
            HEADER_VERSION
        } else if has_checksums {
            CHECKSUM_HEADER_VERSION
        } else if self.block_size.is_some() {
            BLOCK_SIZE_HEADER_VERSION
        } else {
            LEGACY_HEADER_VERSION
        };
        let prefix_size = match version {
            METADATA_HEADER_VERSION | ENTRY_METADATA_HEADER_VERSION | FILE_TYPE_HEADER_VERSION => {
                METADATA_HEADER_PREFIX_SIZE
            }
            BLOCK_SIZE_HEADER_VERSION | HEADER_VERSION => HEADER_PREFIX_SIZE,
            _ => LEGACY_HEADER_PREFIX_SIZE,
        };
        if prefix_size > limits.max_encoded_size {
            return Err(LogicalHeaderError::LimitExceeded);
        }
        if self.changes.len() > limits.max_records || self.changes.len() > u32::MAX as usize {
            return Err(LogicalHeaderError::LimitExceeded);
        }
        validate_entry_metadata(&self.changes, &self.entry_metadata, limits)?;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&HEADER_MAGIC);
        bytes.extend_from_slice(&version.to_le_bytes());
        if version == BLOCK_SIZE_HEADER_VERSION || version == HEADER_VERSION {
            bytes.extend_from_slice(
                &self
                    .block_size
                    .ok_or(LogicalHeaderError::InvalidFileEntry)?
                    .to_le_bytes(),
            );
        } else if version == METADATA_HEADER_VERSION
            || version == ENTRY_METADATA_HEADER_VERSION
            || version == FILE_TYPE_HEADER_VERSION
        {
            bytes.extend_from_slice(&self.block_size.unwrap_or(0).to_le_bytes());
            write_metadata(&mut bytes, &self.metadata, limits)?;
            if version >= ENTRY_METADATA_HEADER_VERSION {
                write_entry_metadata(&mut bytes, &self.entry_metadata, limits)?;
            }
        }
        bytes.extend_from_slice(&(self.changes.len() as u32).to_le_bytes());

        for change in &self.changes {
            let (tag, path) = match change {
                TocChange::PutFile { path, .. } => (RECORD_PUT_FILE, path),
                TocChange::PutDirectory { path } => (RECORD_PUT_DIRECTORY, path),
                TocChange::RemoveFile { path } => (RECORD_REMOVE_FILE, path),
                TocChange::RemoveDirectory { path } => (RECORD_REMOVE_DIRECTORY, path),
                TocChange::PutSymlink { path, .. } => (RECORD_PUT_SYMLINK, path),
                TocChange::PutNamedPipe { path } => (RECORD_PUT_NAMED_PIPE, path),
                TocChange::PutDevice { path, .. } => (RECORD_PUT_DEVICE, path),
            };
            bytes.push(tag);
            write_path(&mut bytes, path, limits)?;
            match change {
                TocChange::PutSymlink { target, .. } => {
                    write_link_target(&mut bytes, target, limits)?
                }
                TocChange::PutDevice {
                    kind, major, minor, ..
                } => {
                    bytes.push(match kind {
                        crate::DeviceKind::Character => DEVICE_CHARACTER,
                        crate::DeviceKind::Block => DEVICE_BLOCK,
                    });
                    bytes.extend_from_slice(&major.to_le_bytes());
                    bytes.extend_from_slice(&minor.to_le_bytes());
                }
                _ => {}
            }
            if let TocChange::PutFile { entry, .. } = change {
                validate_file_entry(entry, limits)?;
                bytes.extend_from_slice(&entry.size.to_le_bytes());
                bytes.extend_from_slice(&(entry.block_indices.len() as u32).to_le_bytes());
                for index in &entry.block_indices {
                    bytes.extend_from_slice(&index.to_le_bytes());
                }
                if version >= CHECKSUM_HEADER_VERSION && entry.size != 0 {
                    match entry.checksum {
                        Some(checksum) => {
                            bytes.push(1);
                            bytes.extend_from_slice(&checksum.to_le_bytes());
                        }
                        None => bytes.push(0),
                    }
                }
            }
            if bytes.len() > limits.max_encoded_size {
                return Err(LogicalHeaderError::LimitExceeded);
            }
        }
        Ok(bytes)
    }

    pub fn decode(input: &[u8]) -> Result<Self, LogicalHeaderError> {
        Self::decode_with_limits(input, LogicalHeaderLimits::default())
    }

    pub fn decode_with_limits(
        input: &[u8],
        limits: LogicalHeaderLimits,
    ) -> Result<Self, LogicalHeaderError> {
        if input.len() > limits.max_encoded_size {
            return Err(LogicalHeaderError::LimitExceeded);
        }
        let mut reader = SliceReader::new(input);
        if reader.take(4)? != HEADER_MAGIC {
            return Err(LogicalHeaderError::InvalidMagic);
        }
        let version = reader.u16()?;
        let (block_size, has_file_checksums, metadata, entry_metadata) = match version {
            LEGACY_HEADER_VERSION => (None, false, HashMap::new(), Vec::new()),
            CHECKSUM_HEADER_VERSION => (None, true, HashMap::new(), Vec::new()),
            BLOCK_SIZE_HEADER_VERSION => {
                let block_size = reader.u32()?;
                if block_size == 0 {
                    return Err(LogicalHeaderError::InvalidFileEntry);
                }
                (Some(block_size), false, HashMap::new(), Vec::new())
            }
            HEADER_VERSION => {
                let block_size = reader.u32()?;
                if block_size == 0 {
                    return Err(LogicalHeaderError::InvalidFileEntry);
                }
                (Some(block_size), true, HashMap::new(), Vec::new())
            }
            METADATA_HEADER_VERSION => {
                let block_size = reader.u32()?;
                let metadata = read_metadata(&mut reader, limits)?;
                (
                    if block_size == 0 {
                        None
                    } else {
                        Some(block_size)
                    },
                    true,
                    metadata,
                    Vec::new(),
                )
            }
            ENTRY_METADATA_HEADER_VERSION => {
                let block_size = reader.u32()?;
                let metadata = read_metadata(&mut reader, limits)?;
                let entry_metadata = read_entry_metadata(&mut reader, limits)?;
                (
                    if block_size == 0 {
                        None
                    } else {
                        Some(block_size)
                    },
                    true,
                    metadata,
                    entry_metadata,
                )
            }
            FILE_TYPE_HEADER_VERSION => {
                let block_size = reader.u32()?;
                let metadata = read_metadata(&mut reader, limits)?;
                let entry_metadata = read_entry_metadata(&mut reader, limits)?;
                (
                    if block_size == 0 {
                        None
                    } else {
                        Some(block_size)
                    },
                    true,
                    metadata,
                    entry_metadata,
                )
            }
            other => return Err(LogicalHeaderError::UnsupportedVersion(other)),
        };
        let record_count = reader.u32()? as usize;
        if record_count > limits.max_records {
            return Err(LogicalHeaderError::LimitExceeded);
        }
        let mut changes = Vec::with_capacity(record_count.min(reader.remaining() / 5));
        for _ in 0..record_count {
            let tag = reader.u8()?;
            let path_len = reader.u32()? as usize;
            if path_len > limits.max_path_size {
                return Err(LogicalHeaderError::LimitExceeded);
            }
            let path_bytes = reader.take(path_len)?;
            let path_text =
                std::str::from_utf8(path_bytes).map_err(|_| LogicalHeaderError::InvalidUtf8Path)?;
            let path = ArchivePath::try_from(path_text).map_err(LogicalHeaderError::InvalidPath)?;
            let change = match tag {
                RECORD_PUT_FILE => {
                    let size = reader.u64()?;
                    let block_count = reader.u32()? as usize;
                    if block_count > limits.max_block_references_per_file {
                        return Err(LogicalHeaderError::LimitExceeded);
                    }
                    let required_bytes = block_count
                        .checked_mul(std::mem::size_of::<u32>())
                        .ok_or(LogicalHeaderError::LimitExceeded)?;
                    if required_bytes > reader.remaining() {
                        return Err(LogicalHeaderError::Truncated);
                    }
                    let mut blocks = Vec::with_capacity(block_count);
                    for _ in 0..block_count {
                        blocks.push(reader.u32()?);
                    }
                    let mut entry = FileEntry::new(size, blocks)
                        .map_err(|_| LogicalHeaderError::InvalidFileEntry)?;
                    if has_file_checksums && entry.size != 0 {
                        entry.checksum = match reader.u8()? {
                            0 => None,
                            1 => Some(reader.u64()?),
                            _ => return Err(LogicalHeaderError::InvalidFileEntry),
                        };
                    }
                    TocChange::PutFile { path, entry }
                }
                RECORD_PUT_DIRECTORY => TocChange::PutDirectory { path },
                RECORD_REMOVE_FILE => TocChange::RemoveFile { path },
                RECORD_REMOVE_DIRECTORY => TocChange::RemoveDirectory { path },
                RECORD_PUT_SYMLINK if version >= FILE_TYPE_HEADER_VERSION => {
                    TocChange::PutSymlink {
                        path,
                        target: read_link_target(&mut reader, limits)?,
                    }
                }
                RECORD_PUT_NAMED_PIPE if version >= FILE_TYPE_HEADER_VERSION => {
                    TocChange::PutNamedPipe { path }
                }
                RECORD_PUT_DEVICE if version >= FILE_TYPE_HEADER_VERSION => {
                    let kind = match reader.u8()? {
                        DEVICE_CHARACTER => crate::DeviceKind::Character,
                        DEVICE_BLOCK => crate::DeviceKind::Block,
                        _ => return Err(LogicalHeaderError::InvalidRecord(tag)),
                    };
                    TocChange::PutDevice {
                        path,
                        kind,
                        major: reader.u32()?,
                        minor: reader.u32()?,
                    }
                }
                other => return Err(LogicalHeaderError::InvalidRecord(other)),
            };
            changes.push(change);
        }
        if !reader.is_empty() {
            return Err(LogicalHeaderError::TrailingData);
        }
        validate_entry_metadata(&changes, &entry_metadata, limits)?;
        Ok(Self {
            changes,
            block_size,
            metadata,
            entry_metadata,
        })
    }
}

fn write_metadata(
    output: &mut Vec<u8>,
    metadata: &HashMap<String, MetadataType>,
    limits: LogicalHeaderLimits,
) -> Result<(), LogicalHeaderError> {
    if metadata.len() > limits.max_metadata_entries || metadata.len() > u32::MAX as usize {
        return Err(LogicalHeaderError::LimitExceeded);
    }
    output.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
    let mut entries: Vec<_> = metadata.iter().collect();
    entries.sort_unstable_by_key(|(key, _)| *key);
    for (key, value) in entries {
        let key = key.as_bytes();
        if key.len() > limits.max_metadata_key_size || key.len() > u32::MAX as usize {
            return Err(LogicalHeaderError::InvalidMetadata);
        }
        output.extend_from_slice(&(key.len() as u32).to_le_bytes());
        output.extend_from_slice(key);
        match value {
            MetadataType::String(value) => {
                let value = value.as_bytes();
                if value.len() > limits.max_metadata_string_size || value.len() > u32::MAX as usize
                {
                    return Err(LogicalHeaderError::LimitExceeded);
                }
                output.push(METADATA_STRING);
                output.extend_from_slice(&(value.len() as u32).to_le_bytes());
                output.extend_from_slice(value);
            }
            MetadataType::I64(value) => {
                output.push(METADATA_I64);
                output.extend_from_slice(&value.to_le_bytes());
            }
            MetadataType::F64(value) => {
                output.push(METADATA_F64);
                output.extend_from_slice(&value.to_bits().to_le_bytes());
            }
        }
        if output.len() > limits.max_encoded_size {
            return Err(LogicalHeaderError::LimitExceeded);
        }
    }
    Ok(())
}

fn read_metadata(
    reader: &mut SliceReader<'_>,
    limits: LogicalHeaderLimits,
) -> Result<HashMap<String, MetadataType>, LogicalHeaderError> {
    let count = reader.u32()? as usize;
    if count > limits.max_metadata_entries {
        return Err(LogicalHeaderError::LimitExceeded);
    }
    let mut metadata = HashMap::with_capacity(count.min(reader.remaining() / 5));
    for _ in 0..count {
        let key_len = reader.u32()? as usize;
        if key_len > limits.max_metadata_key_size {
            return Err(LogicalHeaderError::LimitExceeded);
        }
        let key = std::str::from_utf8(reader.take(key_len)?)
            .map_err(|_| LogicalHeaderError::InvalidUtf8Metadata)?
            .to_owned();
        let value = match reader.u8()? {
            METADATA_STRING => {
                let value_len = reader.u32()? as usize;
                if value_len > limits.max_metadata_string_size {
                    return Err(LogicalHeaderError::LimitExceeded);
                }
                MetadataType::String(
                    std::str::from_utf8(reader.take(value_len)?)
                        .map_err(|_| LogicalHeaderError::InvalidUtf8Metadata)?
                        .to_owned(),
                )
            }
            METADATA_I64 => MetadataType::I64(i64::from_le_bytes(
                reader.take(8)?.try_into().expect("fixed length"),
            )),
            METADATA_F64 => MetadataType::F64(f64::from_bits(reader.u64()?)),
            _ => return Err(LogicalHeaderError::InvalidMetadata),
        };
        if metadata.insert(key, value).is_some() {
            return Err(LogicalHeaderError::InvalidMetadata);
        }
    }
    Ok(metadata)
}

fn validate_entry_metadata(
    changes: &[TocChange],
    entries: &[EntryMetadata],
    limits: LogicalHeaderLimits,
) -> Result<(), LogicalHeaderError> {
    if entries.len() > limits.max_entry_metadata || entries.len() > u32::MAX as usize {
        return Err(LogicalHeaderError::LimitExceeded);
    }
    let record_paths = changes
        .iter()
        .filter_map(|change| match change {
            TocChange::PutFile { path, .. }
            | TocChange::PutDirectory { path }
            | TocChange::PutSymlink { path, .. }
            | TocChange::PutNamedPipe { path }
            | TocChange::PutDevice { path, .. } => Some(path),
            TocChange::RemoveFile { .. } | TocChange::RemoveDirectory { .. } => None,
        })
        .collect::<std::collections::HashSet<_>>();
    let mut metadata_paths = std::collections::HashSet::with_capacity(entries.len());
    for entry in entries {
        if !record_paths.contains(&entry.path) || !metadata_paths.insert(&entry.path) {
            return Err(LogicalHeaderError::InvalidMetadata);
        }
        if entry.platform.is_empty() || entry.platform.len() > 64 {
            return Err(LogicalHeaderError::InvalidMetadata);
        }
    }
    Ok(())
}

fn write_entry_metadata(
    output: &mut Vec<u8>,
    entries: &[EntryMetadata],
    limits: LogicalHeaderLimits,
) -> Result<(), LogicalHeaderError> {
    if entries.len() > limits.max_entry_metadata || entries.len() > u32::MAX as usize {
        return Err(LogicalHeaderError::LimitExceeded);
    }
    output.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    let mut entries = entries.iter().collect::<Vec<_>>();
    entries.sort_unstable_by_key(|entry| entry.path.as_str());
    let mut seen_paths = std::collections::HashSet::with_capacity(entries.len());
    for entry in entries {
        if !seen_paths.insert(&entry.path) {
            return Err(LogicalHeaderError::InvalidMetadata);
        }
        write_path(output, &entry.path, limits)?;
        let platform = entry.platform.as_bytes();
        if platform.is_empty() || platform.len() > 64 {
            return Err(LogicalHeaderError::InvalidMetadata);
        }
        output.extend_from_slice(&(platform.len() as u16).to_le_bytes());
        output.extend_from_slice(platform);
        for value in [entry.owner, entry.group, entry.permissions] {
            match value {
                Some(value) => {
                    output.push(1);
                    output.extend_from_slice(&value.to_le_bytes());
                }
                None => output.push(0),
            }
        }
        if output.len() > limits.max_encoded_size {
            return Err(LogicalHeaderError::LimitExceeded);
        }
    }
    Ok(())
}

fn read_entry_metadata(
    reader: &mut SliceReader<'_>,
    limits: LogicalHeaderLimits,
) -> Result<Vec<EntryMetadata>, LogicalHeaderError> {
    let count = reader.u32()? as usize;
    if count > limits.max_entry_metadata {
        return Err(LogicalHeaderError::LimitExceeded);
    }
    let mut entries = Vec::with_capacity(count.min(reader.remaining() / 7));
    let mut paths = std::collections::HashSet::with_capacity(count.min(reader.remaining() / 7));
    for _ in 0..count {
        let path_len = reader.u32()? as usize;
        if path_len > limits.max_path_size {
            return Err(LogicalHeaderError::LimitExceeded);
        }
        let path_text = std::str::from_utf8(reader.take(path_len)?)
            .map_err(|_| LogicalHeaderError::InvalidUtf8Path)?;
        let path = ArchivePath::try_from(path_text).map_err(LogicalHeaderError::InvalidPath)?;
        if !paths.insert(path.clone()) {
            return Err(LogicalHeaderError::InvalidMetadata);
        }
        let platform_len = reader.u16()? as usize;
        if platform_len > 64 {
            return Err(LogicalHeaderError::LimitExceeded);
        }
        let platform = std::str::from_utf8(reader.take(platform_len)?)
            .map_err(|_| LogicalHeaderError::InvalidUtf8Metadata)?
            .to_owned();
        if platform.is_empty() {
            return Err(LogicalHeaderError::InvalidMetadata);
        }
        if reader.remaining() < 3 {
            return Err(LogicalHeaderError::Truncated);
        }
        let mut read_value = || -> Result<Option<u32>, LogicalHeaderError> {
            match reader.u8()? {
                0 => Ok(None),
                1 => Ok(Some(reader.u32()?)),
                _ => Err(LogicalHeaderError::InvalidMetadata),
            }
        };
        entries.push(EntryMetadata {
            path,
            platform,
            owner: read_value()?,
            group: read_value()?,
            permissions: read_value()?,
        });
    }
    Ok(entries)
}

fn write_link_target(
    output: &mut Vec<u8>,
    target: &str,
    limits: LogicalHeaderLimits,
) -> Result<(), LogicalHeaderError> {
    let bytes = target.as_bytes();
    if bytes.is_empty()
        || bytes.contains(&0)
        || bytes.len() > limits.max_path_size
        || bytes.len() > u32::MAX as usize
    {
        return Err(LogicalHeaderError::InvalidMetadata);
    }
    output.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

fn read_link_target(
    reader: &mut SliceReader<'_>,
    limits: LogicalHeaderLimits,
) -> Result<String, LogicalHeaderError> {
    let len = reader.u32()? as usize;
    if len == 0 || len > limits.max_path_size {
        return Err(LogicalHeaderError::LimitExceeded);
    }
    let target =
        std::str::from_utf8(reader.take(len)?).map_err(|_| LogicalHeaderError::InvalidUtf8Path)?;
    if target.as_bytes().contains(&0) {
        return Err(LogicalHeaderError::InvalidMetadata);
    }
    Ok(target.to_owned())
}

fn write_path(
    output: &mut Vec<u8>,
    path: &ArchivePath,
    limits: LogicalHeaderLimits,
) -> Result<(), LogicalHeaderError> {
    let path = path.as_str().as_bytes();
    if path.len() > limits.max_path_size || path.len() > u32::MAX as usize {
        return Err(LogicalHeaderError::LimitExceeded);
    }
    output.extend_from_slice(&(path.len() as u32).to_le_bytes());
    output.extend_from_slice(path);
    Ok(())
}

fn validate_file_entry(
    entry: &FileEntry,
    limits: LogicalHeaderLimits,
) -> Result<(), LogicalHeaderError> {
    if (entry.size == 0) != entry.block_indices.is_empty()
        || entry.block_indices.len() > limits.max_block_references_per_file
        || entry.block_indices.len() > u32::MAX as usize
    {
        return Err(LogicalHeaderError::InvalidFileEntry);
    }
    let mut unique = entry.block_indices.clone();
    unique.sort_unstable();
    unique.dedup();
    if unique.len() != entry.block_indices.len() {
        return Err(LogicalHeaderError::InvalidFileEntry);
    }
    Ok(())
}

struct SliceReader<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> SliceReader<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self { input, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], LogicalHeaderError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(LogicalHeaderError::LimitExceeded)?;
        let result = self
            .input
            .get(self.offset..end)
            .ok_or(LogicalHeaderError::Truncated)?;
        self.offset = end;
        Ok(result)
    }

    fn u8(&mut self) -> Result<u8, LogicalHeaderError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, LogicalHeaderError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("fixed length"),
        ))
    }

    fn u32(&mut self) -> Result<u32, LogicalHeaderError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("fixed length"),
        ))
    }

    fn u64(&mut self) -> Result<u64, LogicalHeaderError> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("fixed length"),
        ))
    }

    fn is_empty(&self) -> bool {
        self.offset == self.input.len()
    }

    fn remaining(&self) -> usize {
        self.input.len() - self.offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(value: &str) -> ArchivePath {
        ArchivePath::try_from(value).unwrap()
    }

    fn sample_header() -> LogicalHeader {
        LogicalHeader::new(vec![
            TocChange::PutDirectory { path: path("docs") },
            TocChange::PutFile {
                path: path("docs/readme.txt"),
                entry: FileEntry::new(9, vec![2, 0]).unwrap(),
            },
            TocChange::RemoveFile {
                path: path("old.txt"),
            },
            TocChange::RemoveDirectory {
                path: path("archive/old"),
            },
        ])
    }

    #[test]
    fn logical_header_round_trips_metadata_values_and_block_size() {
        let mut metadata = HashMap::new();
        metadata.insert("author".to_owned(), MetadataType::String("Ada".to_owned()));
        metadata.insert("count".to_owned(), MetadataType::I64(-42));
        metadata.insert("ratio".to_owned(), MetadataType::F64(1.25));
        let header = LogicalHeader::with_block_size(Vec::new(), 64 * 1024).with_metadata(metadata);

        let encoded = header.encode().unwrap();
        assert_eq!(&encoded[4..6], &METADATA_HEADER_VERSION.to_le_bytes());
        assert_eq!(LogicalHeader::decode(&encoded).unwrap(), header);
    }

    #[test]
    fn metadata_encoding_is_deterministic_and_rejects_duplicate_keys_and_bad_tags() {
        let mut metadata = HashMap::new();
        metadata.insert("z".to_owned(), MetadataType::I64(1));
        metadata.insert("a".to_owned(), MetadataType::String("v".to_owned()));
        let header = LogicalHeader::new(Vec::new()).with_metadata(metadata);
        assert_eq!(header.encode().unwrap(), header.encode().unwrap());

        let mut duplicate = header.encode().unwrap();
        duplicate[29] = b'a';
        assert_eq!(
            LogicalHeader::decode(&duplicate),
            Err(LogicalHeaderError::InvalidMetadata)
        );

        let mut invalid_tag = header.encode().unwrap();
        invalid_tag[30] = 99;
        assert_eq!(
            LogicalHeader::decode(&invalid_tag),
            Err(LogicalHeaderError::InvalidMetadata)
        );
    }

    #[test]
    fn logical_header_round_trips_special_file_records() {
        let header = LogicalHeader::with_block_size(
            vec![
                TocChange::PutSymlink {
                    path: path("link"),
                    target: "target/file".to_owned(),
                },
                TocChange::PutNamedPipe { path: path("pipe") },
                TocChange::PutDevice {
                    path: path("device"),
                    kind: crate::DeviceKind::Character,
                    major: 1,
                    minor: 7,
                },
            ],
            64 * 1024,
        );
        let encoded = header.encode().unwrap();
        assert_eq!(&encoded[4..6], &FILE_TYPE_HEADER_VERSION.to_le_bytes());
        assert_eq!(LogicalHeader::decode(&encoded).unwrap(), header);
    }

    #[test]
    fn logical_header_round_trips_platform_entry_metadata() {
        let header = LogicalHeader::with_block_size(
            vec![
                TocChange::PutDirectory { path: path("docs") },
                TocChange::PutFile {
                    path: path("docs/file.txt"),
                    entry: FileEntry::new(1, vec![0]).unwrap(),
                },
            ],
            64 * 1024,
        )
        .with_entry_metadata(vec![
            EntryMetadata {
                path: path("docs"),
                platform: "unix".to_owned(),
                owner: Some(1000),
                group: Some(100),
                permissions: Some(0o755),
            },
            EntryMetadata {
                path: path("docs/file.txt"),
                platform: "unix".to_owned(),
                owner: Some(1000),
                group: Some(100),
                permissions: Some(0o640),
            },
        ]);

        let encoded = header.encode().unwrap();
        assert_eq!(&encoded[4..6], &ENTRY_METADATA_HEADER_VERSION.to_le_bytes());
        assert_eq!(LogicalHeader::decode(&encoded).unwrap(), header);
    }

    #[test]
    fn rejects_duplicate_entry_metadata_paths() {
        let entry = EntryMetadata {
            path: path("file"),
            platform: "unix".to_owned(),
            owner: Some(1),
            group: Some(1),
            permissions: Some(0o600),
        };
        let header = LogicalHeader::new(vec![TocChange::PutFile {
            path: path("file"),
            entry: FileEntry::new(0, vec![]).unwrap(),
        }])
        .with_entry_metadata(vec![entry.clone(), entry]);
        assert_eq!(header.encode(), Err(LogicalHeaderError::InvalidMetadata));
    }

    #[test]
    fn logical_header_round_trips_ordered_operations_and_block_references() {
        let header = sample_header();
        let encoded = header.encode().unwrap();
        assert_eq!(&encoded[..4], b"ZHDR");
        assert_eq!(LogicalHeader::decode(&encoded).unwrap(), header);
    }

    fn hex_bytes(value: &str) -> Vec<u8> {
        assert_eq!(value.len() % 2, 0);
        (0..value.len())
            .step_by(2)
            .map(|offset| u8::from_str_radix(&value[offset..offset + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn canonical_v0_compatibility_vector_is_stable_and_decodable() {
        let expected = hex_bytes(concat!(
            "5a484452000004000000",                     // magic, version, record count
            "0204000000646f6373",                       // put directory: tag, path length, path
            "010f000000646f63732f726561646d652e747874", // put file path
            "0900000000000000020000000200000000000000", // size, block count, indices
            "03070000006f6c642e747874",                 // remove file
            "040b000000617263686976652f6f6c64",         // remove directory
        ));
        let header = sample_header();

        assert_eq!(header.encode().unwrap(), expected);
        assert_eq!(LogicalHeader::decode(&expected).unwrap(), header);
    }

    #[test]
    fn rejects_unknown_version_tag_truncation_and_trailing_bytes() {
        let encoded = sample_header().encode().unwrap();
        let mut invalid_version = encoded.clone();
        invalid_version[4] = 7;
        assert!(matches!(
            LogicalHeader::decode(&invalid_version),
            Err(LogicalHeaderError::UnsupportedVersion(7))
        ));

        let mut invalid_tag = encoded.clone();
        invalid_tag[10] = 99;
        assert!(matches!(
            LogicalHeader::decode(&invalid_tag),
            Err(LogicalHeaderError::InvalidRecord(99))
        ));
        assert_eq!(
            LogicalHeader::decode(&encoded[..encoded.len() - 1]),
            Err(LogicalHeaderError::Truncated)
        );
        let mut trailing = encoded;
        trailing.push(0);
        assert_eq!(
            LogicalHeader::decode(&trailing),
            Err(LogicalHeaderError::TrailingData)
        );
    }

    #[test]
    fn encodes_record_tags_before_path_lengths_and_checks_empty_header_limit() {
        let encoded = LogicalHeader::new(vec![TocChange::PutDirectory { path: path("docs") }])
            .encode()
            .unwrap();
        assert_eq!(&encoded[..4], b"ZHDR");
        assert_eq!(&encoded[4..6], &0u16.to_le_bytes());
        assert_eq!(&encoded[6..10], &1u32.to_le_bytes());
        assert_eq!(encoded[10], RECORD_PUT_DIRECTORY);
        assert_eq!(&encoded[11..15], &4u32.to_le_bytes());
        assert_eq!(&encoded[15..], b"docs");

        let block_sized = LogicalHeader::with_block_size(
            vec![TocChange::PutDirectory { path: path("docs") }],
            64 * 1024,
        )
        .encode()
        .unwrap();
        assert_eq!(&block_sized[4..6], &BLOCK_SIZE_HEADER_VERSION.to_le_bytes());
        assert_eq!(&block_sized[6..10], &(64_u32 * 1024).to_le_bytes());
        assert_eq!(
            LogicalHeader::decode(&block_sized).unwrap().block_size,
            Some(64 * 1024)
        );

        let limits = LogicalHeaderLimits {
            max_encoded_size: LEGACY_HEADER_PREFIX_SIZE - 1,
            ..LogicalHeaderLimits::default()
        };
        assert_eq!(
            LogicalHeader::default().encode_with_limits(limits),
            Err(LogicalHeaderError::LimitExceeded)
        );
    }

    #[test]
    fn rejects_declared_block_references_that_do_not_fit_input_before_allocation() {
        let mut encoded = LogicalHeader::new(vec![TocChange::PutFile {
            path: path("file"),
            entry: FileEntry::new(1, vec![0]).unwrap(),
        }])
        .encode()
        .unwrap();
        let block_count_offset = encoded.len() - std::mem::size_of::<u32>() - 4;
        encoded.truncate(block_count_offset + 4);
        encoded[block_count_offset..block_count_offset + 4]
            .copy_from_slice(&u32::MAX.to_le_bytes());

        let limits = LogicalHeaderLimits {
            max_block_references_per_file: u32::MAX as usize,
            ..LogicalHeaderLimits::default()
        };
        assert_eq!(
            LogicalHeader::decode_with_limits(&encoded, limits),
            Err(LogicalHeaderError::Truncated)
        );
    }

    #[test]
    fn rejects_non_utf8_archive_paths() {
        let encoded = [
            b'Z',
            b'H',
            b'D',
            b'R',
            0,
            0,
            1,
            0,
            0,
            0,
            RECORD_REMOVE_FILE,
            1,
            0,
            0,
            0,
            0xff,
        ];
        assert_eq!(
            LogicalHeader::decode(&encoded),
            Err(LogicalHeaderError::InvalidUtf8Path)
        );
    }

    #[test]
    fn enforces_record_path_and_block_reference_limits() {
        let header = sample_header();
        let small = LogicalHeaderLimits {
            max_records: 1,
            ..LogicalHeaderLimits::default()
        };
        assert_eq!(
            header.encode_with_limits(small),
            Err(LogicalHeaderError::LimitExceeded)
        );
        assert_eq!(
            LogicalHeader::decode_with_limits(&header.encode().unwrap(), small),
            Err(LogicalHeaderError::LimitExceeded)
        );

        let blocks_limit = LogicalHeaderLimits {
            max_block_references_per_file: 1,
            ..LogicalHeaderLimits::default()
        };
        assert_eq!(
            LogicalHeader::new(vec![TocChange::PutFile {
                path: path("file"),
                entry: FileEntry::new(1, vec![0, 1]).unwrap(),
            }])
            .encode_with_limits(blocks_limit),
            Err(LogicalHeaderError::InvalidFileEntry)
        );
    }
}
