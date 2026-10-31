//! Binary framing primitives for the ZAR archive format.
//!
//! Provides the ZAR wire format, validation primitives, and high-level archive
//! operations. Use [`operations`] for public create, add, remove, join, check,
//! info, list, and extract APIs; the CLI is a presentation layer over them.

use std::io::{self, Read, Write};

mod archive_path;
mod commit;
mod crypto;
mod encryption;
mod frames;
mod header;
pub mod operations;
mod toc;

pub use archive_path::{ArchivePath, ArchivePathError};
pub use commit::{CommitValidationLimits, ValidatedCommit, ValidatedHeader};
pub use crypto::{
    AEAD_NONCE_SIZE, DEK_SIZE, decrypt_content, derive_password_key, encrypt_content, generate_dek,
    generate_nonce, unwrap_dek_with_password, unwrap_dek_with_password_zeroizing,
    wrap_dek_with_password,
};

pub use encryption::{
    ContentAead, DescriptorExtension, EncryptionDescriptor, EncryptionScope, HpkeSuite,
    PasswordKeyWrap, RecipientKeyWrap,
};
pub use frames::{
    BLOCK_FLAG_ENCRYPTED, BLOCK_FLAG_NONCE_PRESENT, BLOCK_FLAG_UNCOMPRESSED, BlockFrame,
    BlockFrameMetadata, HeaderFrame, MAX_ENCRYPTION_DESCRIPTOR_SIZE, UNENCRYPTED_DESCRIPTOR,
};
pub use header::{
    EntryMetadata, LogicalHeader, LogicalHeaderError, LogicalHeaderLimits, MetadataType,
};
pub use toc::{DeviceKind, FileEntry, FileGroupKind, Toc, TocChange, TocEntry, TocError};

pub const CONTAINER_MAGIC: [u8; 4] = *b"ZAR0";
pub const COMMIT_MAGIC: [u8; 4] = *b"ZCMT";
pub const FRAMING_VERSION: u8 = 0;

/// Size of the container header: 4-byte magic + 8-byte first_header_offset.
pub const CONTAINER_HEADER_SIZE: u64 = 12;

/// Size of the fixed commit-header fields (magic through header_frame_size).
/// magic[4] + framing_version[1] + flags[1] + commit_id[4] +
/// data_section_size[8] + header_frame_size[4] = 22 bytes.
pub const COMMIT_HEADER_FIXED_SIZE: usize = 22;

/// Size of the next_header_offset / this_header_offset fields (u64).
pub const HEADER_OFFSET_SIZE: u64 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerHeader {
    pub first_header_offset: u64,
}

impl ContainerHeader {
    pub fn new(first_header_offset: u64) -> Self {
        Self {
            first_header_offset,
        }
    }

    pub fn encode(&self) -> [u8; CONTAINER_HEADER_SIZE as usize] {
        let mut bytes = [0u8; CONTAINER_HEADER_SIZE as usize];
        bytes[..4].copy_from_slice(&CONTAINER_MAGIC);
        bytes[4..12].copy_from_slice(&self.first_header_offset.to_le_bytes());
        bytes
    }

    pub fn decode(bytes: &[u8; CONTAINER_HEADER_SIZE as usize]) -> Result<Self, FramingError> {
        if bytes[..4] != CONTAINER_MAGIC {
            return Err(FramingError::BadMagic);
        }
        Ok(Self {
            first_header_offset: u64::from_le_bytes(bytes[4..12].try_into().expect("fixed slice")),
        })
    }

    pub fn read_from(reader: &mut impl Read) -> Result<Self, FramingError> {
        let mut bytes = [0u8; CONTAINER_HEADER_SIZE as usize];
        reader.read_exact(&mut bytes)?;
        Self::decode(&bytes)
    }

    pub fn write_to(&self, writer: &mut (impl Write + ?Sized)) -> Result<(), FramingError> {
        writer.write_all(&self.encode())?;
        Ok(())
    }
}

/// Commit-level metadata carried in the header frame. Replaces the old
/// `CommitPrefix`. The header frame is written at the *end* of each commit,
/// so all sizes are known before encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitHeader {
    pub framing_version: u8,
    pub flags: u8,
    pub commit_id: [u8; 4],
    pub data_section_size: u64,
    pub header_frame_size: u32,
}

impl CommitHeader {
    pub fn new(
        flags: u8,
        commit_id: [u8; 4],
        data_section_size: u64,
        header_frame_size: u32,
    ) -> Result<Self, FramingError> {
        let header = Self {
            framing_version: FRAMING_VERSION,
            flags,
            commit_id,
            data_section_size,
            header_frame_size,
        };
        header.validate()?;
        Ok(header)
    }

    pub fn validate(&self) -> Result<(), FramingError> {
        if self.framing_version != FRAMING_VERSION {
            return Err(FramingError::UnsupportedVersion(self.framing_version));
        }
        Ok(())
    }

    /// Validates that the header_frame_size is non-zero (used after the size is known).
    pub fn validate_with_frame_size(&self) -> Result<(), FramingError> {
        self.validate()?;
        if self.header_frame_size == 0 {
            return Err(FramingError::InvalidSize);
        }
        Ok(())
    }

    /// Serialize the fixed commit-header fields (22 bytes).
    pub fn fixed_bytes(&self) -> Result<[u8; COMMIT_HEADER_FIXED_SIZE], FramingError> {
        self.validate()?;
        let mut bytes = [0u8; COMMIT_HEADER_FIXED_SIZE];
        bytes[..4].copy_from_slice(&COMMIT_MAGIC);
        bytes[4] = self.framing_version;
        bytes[5] = self.flags;
        bytes[6..10].copy_from_slice(&self.commit_id);
        bytes[10..18].copy_from_slice(&self.data_section_size.to_le_bytes());
        bytes[18..22].copy_from_slice(&self.header_frame_size.to_le_bytes());
        Ok(bytes)
    }

    /// Serialize the commit-identity fields (10 bytes): magic, version, flags, commit_id.
    /// This excludes `data_section_size` and `header_frame_size` so that block
    /// encryption AAD can be computed before the data section size is known.
    pub fn identity_bytes(&self) -> Result<[u8; 10], FramingError> {
        self.validate()?;
        let mut bytes = [0u8; 10];
        bytes[..4].copy_from_slice(&COMMIT_MAGIC);
        bytes[4] = self.framing_version;
        bytes[5] = self.flags;
        bytes[6..10].copy_from_slice(&self.commit_id);
        Ok(bytes)
    }

    pub fn decode_fixed(bytes: &[u8; COMMIT_HEADER_FIXED_SIZE]) -> Result<Self, FramingError> {
        if bytes[..4] != COMMIT_MAGIC {
            return Err(FramingError::BadMagic);
        }
        let header = Self {
            framing_version: bytes[4],
            flags: bytes[5],
            commit_id: bytes[6..10].try_into().expect("fixed slice"),
            data_section_size: u64::from_le_bytes(bytes[10..18].try_into().expect("fixed slice")),
            header_frame_size: u32::from_le_bytes(bytes[18..22].try_into().expect("fixed slice")),
        };
        header.validate_with_frame_size()?;
        Ok(header)
    }

    /// Read the fixed commit-header fields from the reader.
    pub fn read_fixed(reader: &mut impl Read) -> Result<Self, FramingError> {
        let mut bytes = [0u8; COMMIT_HEADER_FIXED_SIZE];
        reader.read_exact(&mut bytes)?;
        Self::decode_fixed(&bytes)
    }

    /// Write the fixed commit-header fields to the writer.
    pub fn write_fixed(&self, writer: &mut (impl Write + ?Sized)) -> Result<(), FramingError> {
        writer.write_all(&self.fixed_bytes()?)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FramingError {
    Io(String),
    BadMagic,
    UnsupportedVersion(u8),
    InvalidSize,
    ChecksumMismatch,
    AuthenticationFailed,
    InvalidDescriptor,
    InvalidLogicalHeader(String),
    UnsupportedEncryption,
}

impl std::fmt::Display for FramingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(message) => write!(f, "I/O error: {message}"),
            Self::BadMagic => write!(f, "invalid commit prefix magic"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported framing version {version}")
            }
            Self::InvalidSize => write!(f, "invalid commit size fields"),
            Self::ChecksumMismatch => write!(f, "frame checksum mismatch"),
            Self::AuthenticationFailed => write!(f, "encrypted payload authentication failed"),
            Self::InvalidDescriptor => write!(f, "invalid encryption descriptor"),
            Self::InvalidLogicalHeader(message) => write!(f, "invalid logical header: {message}"),
            Self::UnsupportedEncryption => {
                write!(f, "unsupported encrypted frame for this operation")
            }
        }
    }
}

impl std::error::Error for FramingError {}

impl From<io::Error> for FramingError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

pub fn write_container_magic(writer: &mut impl Write) -> Result<(), FramingError> {
    writer.write_all(&CONTAINER_MAGIC)?;
    Ok(())
}

pub fn read_container_magic(reader: &mut impl Read) -> Result<(), FramingError> {
    let mut magic = [0; 4];
    reader.read_exact(&mut magic)?;
    if magic != CONTAINER_MAGIC {
        return Err(FramingError::BadMagic);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
