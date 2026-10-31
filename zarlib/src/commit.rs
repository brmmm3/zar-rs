use std::collections::HashSet;
use std::io::Read;

use crate::{
    BlockFrame, CommitHeader, ContentAead, EncryptionDescriptor, EncryptionScope, FramingError,
    HeaderFrame, LogicalHeader, LogicalHeaderLimits,
};

const DEFAULT_MAX_FRAME_PAYLOAD_SIZE: usize = 64 * 1024 * 1024;
const DEFAULT_MAX_BLOCKS: u64 = 1_000_000;
const DEFAULT_MAX_LOGICAL_HEADER_SIZE: usize = 64 * 1024 * 1024;
const DEFAULT_MAX_UNCOMPRESSED_BLOCK_SIZE: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitValidationLimits {
    pub max_frame_payload_size: usize,
    pub max_uncompressed_block_size: usize,
    pub max_blocks: u64,
    pub max_logical_header_size: usize,
}

impl Default for CommitValidationLimits {
    fn default() -> Self {
        Self {
            max_frame_payload_size: DEFAULT_MAX_FRAME_PAYLOAD_SIZE,
            max_uncompressed_block_size: DEFAULT_MAX_UNCOMPRESSED_BLOCK_SIZE,
            max_blocks: DEFAULT_MAX_BLOCKS,
            max_logical_header_size: DEFAULT_MAX_LOGICAL_HEADER_SIZE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedHeader {
    pub commit_header: CommitHeader,
    pub encryption: EncryptionDescriptor,
    pub extensions: Vec<crate::DescriptorExtension>,
    pub logical_header: Option<LogicalHeader>,
    pub header_compressed_size: u64,
    pub header_uncompressed_size: Option<u64>,
}

impl ValidatedHeader {
    pub fn validate_references(&self, available_blocks: &HashSet<u32>) -> Result<(), FramingError> {
        if let Some(header) = &self.logical_header {
            for change in &header.changes {
                if let crate::TocChange::PutFile { path, entry } = change
                    && let Some(&block_index) = entry
                        .block_indices
                        .iter()
                        .find(|&&index| !available_blocks.contains(&index))
                {
                    return Err(FramingError::InvalidLogicalHeader(format!(
                        "file {path} references unavailable block {block_index}"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Read and validate only the commit header. The reader must be positioned
    /// at the start of the header frame (i.e. at `header_offset`).
    pub fn read_header(
        reader: &mut (impl Read + std::io::Seek),
        limits: CommitValidationLimits,
    ) -> Result<Self, FramingError> {
        Self::read_header_internal(reader, limits, None)
    }

    /// Read the commit header and decrypt it when a content key is supplied.
    pub fn read_header_with_key(
        reader: &mut (impl Read + std::io::Seek),
        limits: CommitValidationLimits,
        key: &[u8; crate::DEK_SIZE],
    ) -> Result<Self, FramingError> {
        Self::read_header_internal(reader, limits, Some(key))
    }

    fn read_header_internal(
        reader: &mut (impl Read + std::io::Seek),
        limits: CommitValidationLimits,
        key: Option<&[u8; crate::DEK_SIZE]>,
    ) -> Result<Self, FramingError> {
        let header = HeaderFrame::read_from(reader, limits.max_frame_payload_size)?;
        let commit_header = header.commit_header.clone();
        commit_header.validate()?;
        let (encryption, extensions) = EncryptionDescriptor::decode(&header.encryption_descriptor)?;
        let header_compressed_size = header.payload.len() as u64;
        let (logical_header, header_uncompressed_size) = if encryption.header_encrypted() {
            match key {
                Some(key) => {
                    let bytes =
                        header.uncompressed_encrypted(key, limits.max_logical_header_size)?;
                    let uncompressed_size = bytes.len() as u64;
                    (
                        Some(decode_logical_header(&bytes, limits)?),
                        Some(uncompressed_size),
                    )
                }
                None => (None, None),
            }
        } else {
            let bytes = header.uncompressed(limits.max_logical_header_size)?;
            let uncompressed_size = bytes.len() as u64;
            (
                Some(decode_logical_header(&bytes, limits)?),
                Some(uncompressed_size),
            )
        };
        Ok(Self {
            commit_header,
            encryption,
            extensions,
            logical_header,
            header_compressed_size,
            header_uncompressed_size,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedCommit {
    pub commit_header: CommitHeader,
    pub encryption: EncryptionDescriptor,
    pub extensions: Vec<crate::DescriptorExtension>,
    pub logical_header: Option<LogicalHeader>,
    pub block_count: u64,
    pub block_indices: HashSet<u32>,
}

impl ValidatedCommit {
    /// Validate a commit body. The reader must be positioned at the start of
    /// the data section (i.e. at `header_offset - data_section_size`).
    pub fn read_body(
        reader: &mut (impl Read + std::io::Seek),
        commit_header: CommitHeader,
        limits: CommitValidationLimits,
    ) -> Result<Self, FramingError> {
        Self::read_body_internal(reader, commit_header, limits, None)
    }

    /// Validate a commit body and authenticate/decompress encrypted content with its DEK.
    pub fn read_body_with_key(
        reader: &mut (impl Read + std::io::Seek),
        commit_header: CommitHeader,
        limits: CommitValidationLimits,
        key: &[u8; crate::DEK_SIZE],
    ) -> Result<Self, FramingError> {
        Self::read_body_internal(reader, commit_header, limits, Some(key))
    }

    fn read_body_internal(
        reader: &mut (impl Read + std::io::Seek),
        commit_header: CommitHeader,
        limits: CommitValidationLimits,
        key: Option<&[u8; crate::DEK_SIZE]>,
    ) -> Result<Self, FramingError> {
        commit_header.validate()?;

        // Read data blocks first (they precede the header in the new format).
        // We need to peek at the encryption descriptor to know if data is
        // encrypted. Read the header frame first, then seek back to data.
        let data_start = reader.stream_position()?;
        // Seek past data blocks to the header.
        reader.seek(std::io::SeekFrom::Current(
            commit_header.data_section_size as i64,
        ))?;
        let header = HeaderFrame::read_from(reader, limits.max_frame_payload_size)?;
        if header.encoded_size()? != commit_header.header_frame_size as usize {
            return Err(FramingError::InvalidSize);
        }
        let (encryption, extensions) = EncryptionDescriptor::decode(&header.encryption_descriptor)?;
        // Seek back to data start.
        reader.seek(std::io::SeekFrom::Start(data_start))?;

        let logical_header = if encryption.header_encrypted() {
            match key {
                Some(key) => {
                    let logical_header_bytes =
                        header.uncompressed_encrypted(key, limits.max_logical_header_size)?;
                    Some(decode_logical_header(&logical_header_bytes, limits)?)
                }
                None => None,
            }
        } else {
            let logical_header_bytes = header.uncompressed(limits.max_logical_header_size)?;
            Some(decode_logical_header(&logical_header_bytes, limits)?)
        };

        // Now validate data blocks.
        let mut seen_blocks = HashSet::new();
        let mut seen_nonces = HashSet::new();
        if encryption.header_encrypted()
            && let Some(nonce) = encryption.content_nonce()
        {
            seen_nonces.insert(*nonce);
        }

        let mut data_reader = reader.take(commit_header.data_section_size);
        let mut block_count = 0_u64;
        while data_reader.limit() != 0 {
            if block_count >= limits.max_blocks {
                return Err(FramingError::InvalidSize);
            }
            let remaining_before = data_reader.limit();
            let frame = BlockFrame::read_from(&mut data_reader, limits.max_frame_payload_size)?;
            let consumed = remaining_before - data_reader.limit();
            if consumed == 0 || frame.encoded_size()? as u64 != consumed {
                return Err(FramingError::InvalidSize);
            }
            if !seen_blocks.insert(frame.block_index) {
                return Err(FramingError::InvalidSize);
            }
            let encrypted = frame.flags & crate::BLOCK_FLAG_ENCRYPTED != 0;
            if encrypted != encryption.data_encrypted() {
                return Err(FramingError::InvalidDescriptor);
            }
            if let Some(nonce) = frame.nonce
                && !seen_nonces.insert(nonce)
            {
                return Err(FramingError::InvalidDescriptor);
            }
            if encrypted && let Some(key) = key {
                let algorithm =
                    content_algorithm(&encryption).ok_or(FramingError::InvalidDescriptor)?;
                frame.uncompressed_encrypted(
                    &commit_header,
                    key,
                    algorithm,
                    limits.max_uncompressed_block_size,
                )?;
            } else if !encrypted && !data_is_encrypted_inner(&encryption) {
                frame.uncompressed_at_most(limits.max_uncompressed_block_size)?;
            }
            block_count += 1;
        }

        if let Some(logical_header) = &logical_header {
            for change in &logical_header.changes {
                if let crate::TocChange::PutFile { path, entry } = change
                    && let Some(&block_index) = entry
                        .block_indices
                        .iter()
                        .find(|&&index| !seen_blocks.contains(&index))
                {
                    return Err(FramingError::InvalidLogicalHeader(format!(
                        "file {path} references unavailable block {block_index}"
                    )));
                }
            }
        }

        Ok(Self {
            commit_header,
            encryption,
            extensions,
            logical_header,
            block_count,
            block_indices: seen_blocks,
        })
    }
}

fn data_is_encrypted_inner(encryption: &EncryptionDescriptor) -> bool {
    matches!(
        encryption.encryption_scope(),
        Some(EncryptionScope::Data | EncryptionScope::HeaderAndData)
    )
}

fn decode_logical_header(
    bytes: &[u8],
    limits: CommitValidationLimits,
) -> Result<LogicalHeader, FramingError> {
    LogicalHeader::decode_with_limits(
        bytes,
        LogicalHeaderLimits {
            max_encoded_size: limits.max_logical_header_size,
            ..LogicalHeaderLimits::default()
        },
    )
    .map_err(|error| FramingError::InvalidLogicalHeader(error.to_string()))
}

fn content_algorithm(descriptor: &EncryptionDescriptor) -> Option<ContentAead> {
    match descriptor {
        EncryptionDescriptor::Unencrypted => None,
        EncryptionDescriptor::Symmetric { algorithm, .. }
        | EncryptionDescriptor::Password { algorithm, .. }
        | EncryptionDescriptor::Recipients { algorithm, .. }
        | EncryptionDescriptor::PasswordAndRecipients { algorithm, .. } => Some(*algorithm),
    }
}

impl EncryptionDescriptor {
    fn encryption_scope(&self) -> Option<EncryptionScope> {
        match self {
            Self::Unencrypted => None,
            Self::Symmetric { scope, .. }
            | Self::Password { scope, .. }
            | Self::Recipients { scope, .. }
            | Self::PasswordAndRecipients { scope, .. } => Some(*scope),
        }
    }

    pub(crate) fn header_encrypted(&self) -> bool {
        matches!(
            self.encryption_scope(),
            Some(EncryptionScope::Header | EncryptionScope::HeaderAndData)
        )
    }

    pub(crate) fn data_encrypted(&self) -> bool {
        matches!(
            self.encryption_scope(),
            Some(EncryptionScope::Data | EncryptionScope::HeaderAndData)
        )
    }

    fn content_nonce(&self) -> Option<&[u8; 12]> {
        match self {
            Self::Unencrypted => None,
            Self::Symmetric { nonce, .. }
            | Self::Password { nonce, .. }
            | Self::Recipients { nonce, .. }
            | Self::PasswordAndRecipients { nonce, .. } => Some(nonce),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    use crate::frames::HEADER_FRAME_FIXED_SIZE;
    use crate::{CommitHeader, ContentAead, EncryptionScope, UNENCRYPTED_DESCRIPTOR};
    use xxhash_rust::xxh64::xxh64;

    fn empty_logical_header() -> Vec<u8> {
        LogicalHeader::default().encode().unwrap()
    }

    /// Build a commit body in the new format: data blocks, then header frame.
    /// Returns (body_bytes, commit_header).
    fn build_commit(
        descriptor: &[u8],
        logical_header: &[u8],
        data: &[u8],
    ) -> (Vec<u8>, CommitHeader) {
        let header_payload = zstd::encode_all(logical_header, 1).unwrap();
        let header_frame_size = HEADER_FRAME_FIXED_SIZE + descriptor.len() + header_payload.len();
        let commit_header = CommitHeader::new(
            0,
            [0; 4],
            data.len() as u64,
            u32::try_from(header_frame_size).unwrap(),
        )
        .unwrap();
        let mut body = Vec::new();
        body.extend_from_slice(data);
        // Write header frame manually
        commit_header.write_fixed(&mut body).unwrap();
        body.extend_from_slice(&(header_payload.len() as u32).to_le_bytes());
        body.extend_from_slice(&xxh64(&header_payload, 0).to_le_bytes());
        body.extend_from_slice(&(descriptor.len() as u32).to_le_bytes());
        body.extend_from_slice(descriptor);
        body.extend_from_slice(&header_payload);
        (body, commit_header)
    }

    fn validate_body(descriptor: &[u8], data: &[u8]) -> Result<ValidatedCommit, FramingError> {
        let (body, commit_header) = build_commit(descriptor, &empty_logical_header(), data);
        ValidatedCommit::read_body(
            &mut Cursor::new(body),
            commit_header,
            CommitValidationLimits::default(),
        )
    }

    #[test]
    fn validates_unencrypted_commit_frames() {
        let frame = BlockFrame::from_uncompressed(0, b"block", 1).unwrap();
        let mut data = Vec::new();
        frame.write_to(&mut data).unwrap();
        let commit = validate_body(&UNENCRYPTED_DESCRIPTOR, &data).unwrap();
        assert_eq!(commit.block_count, 1);
        assert_eq!(commit.encryption, EncryptionDescriptor::Unencrypted);
    }

    #[test]
    fn validates_encryption_scope_and_rejects_reused_nonces() {
        let descriptor = EncryptionDescriptor::Symmetric {
            scope: EncryptionScope::Data,
            algorithm: ContentAead::Aes256Gcm,
            nonce: [1; 12],
        }
        .encode()
        .unwrap();
        let logical_header = empty_logical_header();
        let (_body, commit_header) = build_commit(&descriptor, &logical_header, &[]);
        let mut frame = BlockFrame::from_uncompressed_encrypted(
            0,
            b"ciphertext",
            1,
            &commit_header,
            &[2; crate::DEK_SIZE],
            ContentAead::Aes256Gcm,
        )
        .unwrap();
        frame.nonce = Some([2; crate::AEAD_NONCE_SIZE]);
        let mut data = Vec::new();
        frame.write_to(&mut data).unwrap();
        assert_eq!(validate_body(&descriptor, &data).unwrap().block_count, 1);

        // Test header-and-data encryption with duplicate nonce
        let descriptor_hd = EncryptionDescriptor::Symmetric {
            scope: EncryptionScope::HeaderAndData,
            algorithm: ContentAead::Aes256Gcm,
            nonce: [2; crate::AEAD_NONCE_SIZE],
        }
        .encode()
        .unwrap();
        let compressed_header = zstd::encode_all(logical_header.as_slice(), 1).unwrap();
        let header_frame_size =
            HEADER_FRAME_FIXED_SIZE + descriptor_hd.len() + compressed_header.len() + 16;
        let data_size = data.len();
        let commit_header2 = CommitHeader::new(
            0,
            [0; 4],
            data_size as u64,
            u32::try_from(header_frame_size).unwrap(),
        )
        .unwrap();
        let header = HeaderFrame::from_uncompressed_encrypted(
            commit_header2.clone(),
            &logical_header,
            descriptor_hd.clone(),
            1,
            &[2; crate::DEK_SIZE],
        )
        .unwrap();
        let mut duplicate_nonce_frame = BlockFrame::from_uncompressed_encrypted(
            0,
            b"ciphertext",
            1,
            &commit_header2,
            &[2; crate::DEK_SIZE],
            ContentAead::Aes256Gcm,
        )
        .unwrap();
        duplicate_nonce_frame.nonce = Some([2; crate::AEAD_NONCE_SIZE]);
        let mut body = Vec::new();
        duplicate_nonce_frame.write_to(&mut body).unwrap();
        header.write_to(&mut body).unwrap();
        assert_eq!(
            ValidatedCommit::read_body(
                &mut Cursor::new(body),
                commit_header2,
                CommitValidationLimits::default(),
            ),
            Err(FramingError::InvalidDescriptor)
        );

        let descriptor_unenc = EncryptionDescriptor::Unencrypted.encode().unwrap();
        assert_eq!(
            validate_body(&descriptor_unenc, &data),
            Err(FramingError::InvalidDescriptor)
        );
    }

    #[test]
    fn decrypts_encrypted_header_when_key_is_provided() {
        let key = [7; crate::DEK_SIZE];
        let descriptor = EncryptionDescriptor::Symmetric {
            scope: EncryptionScope::Header,
            algorithm: ContentAead::Aes256Gcm,
            nonce: [8; crate::AEAD_NONCE_SIZE],
        }
        .encode()
        .unwrap();
        let logical_header = empty_logical_header();
        let compressed_len = zstd::encode_all(logical_header.as_slice(), 1)
            .unwrap()
            .len();
        let header_size = HEADER_FRAME_FIXED_SIZE + descriptor.len() + compressed_len + 16;
        let commit_header =
            CommitHeader::new(0, [4; 4], 0, u32::try_from(header_size).unwrap()).unwrap();
        let header = HeaderFrame::from_uncompressed_encrypted(
            commit_header.clone(),
            &logical_header,
            descriptor,
            1,
            &key,
        )
        .unwrap();
        let mut body = Vec::new();
        header.write_to(&mut body).unwrap();

        let commit = ValidatedCommit::read_body_with_key(
            &mut Cursor::new(body.clone()),
            commit_header.clone(),
            CommitValidationLimits::default(),
            &key,
        )
        .unwrap();
        assert_eq!(commit.logical_header, Some(LogicalHeader::default()));

        assert_eq!(
            ValidatedCommit::read_body_with_key(
                &mut Cursor::new(body),
                commit_header,
                CommitValidationLimits::default(),
                &[9; crate::DEK_SIZE],
            ),
            Err(FramingError::AuthenticationFailed)
        );
    }

    #[test]
    fn authenticates_encrypted_blocks_when_key_is_provided() {
        let key = [10; crate::DEK_SIZE];
        let descriptor = EncryptionDescriptor::Symmetric {
            scope: EncryptionScope::Data,
            algorithm: ContentAead::ChaCha20Poly1305,
            nonce: [11; crate::AEAD_NONCE_SIZE],
        }
        .encode()
        .unwrap();
        let logical_header = empty_logical_header();
        let plaintext_block = b"block";
        let encoded_block = zstd::encode_all(&plaintext_block[..], 1).unwrap();
        let ciphertext_len = (if encoded_block.len() < plaintext_block.len() {
            encoded_block.len()
        } else {
            plaintext_block.len()
        }) + 16;
        let data_size = 20 + crate::AEAD_NONCE_SIZE + ciphertext_len;
        let header_frame_size = HEADER_FRAME_FIXED_SIZE
            + descriptor.len()
            + zstd::encode_all(logical_header.as_slice(), 1)
                .unwrap()
                .len();
        let commit_header = CommitHeader::new(
            0,
            [12; 4],
            data_size as u64,
            u32::try_from(header_frame_size).unwrap(),
        )
        .unwrap();
        let header =
            HeaderFrame::from_uncompressed(commit_header.clone(), &logical_header, descriptor, 1)
                .unwrap();
        let frame = BlockFrame::from_uncompressed_encrypted(
            0,
            b"block",
            1,
            &commit_header,
            &key,
            ContentAead::ChaCha20Poly1305,
        )
        .unwrap();
        let mut body = Vec::new();
        frame.write_to(&mut body).unwrap();
        header.write_to(&mut body).unwrap();

        let commit = ValidatedCommit::read_body_with_key(
            &mut Cursor::new(body.clone()),
            commit_header.clone(),
            CommitValidationLimits::default(),
            &key,
        )
        .unwrap();
        assert_eq!(commit.block_count, 1);

        assert_eq!(
            ValidatedCommit::read_body_with_key(
                &mut Cursor::new(body.clone()),
                commit_header.clone(),
                CommitValidationLimits::default(),
                &[13; crate::DEK_SIZE],
            ),
            Err(FramingError::AuthenticationFailed)
        );

        let mut corrupted = body;
        // Corrupt the block payload (after the 28-byte block header + nonce)
        let payload_offset = 28 + crate::AEAD_NONCE_SIZE + 8;
        corrupted[payload_offset] ^= 1;
        assert_eq!(
            ValidatedCommit::read_body_with_key(
                &mut Cursor::new(corrupted),
                commit_header,
                CommitValidationLimits::default(),
                &key,
            ),
            Err(FramingError::ChecksumMismatch)
        );
    }
}
