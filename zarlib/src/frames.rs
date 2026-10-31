use std::io::{Read, Write};

/// Compresses one block with a per-thread reusable zstd context.
///
/// Streaming `encode_all` does not know the input size, so zstd sizes and zeroes
/// context tables for the maximum window on every call, which dominates the cost
/// of small files. The bulk API knows the size and the context is reused.
fn compress_block(bytes: &[u8], level: i32) -> std::io::Result<Vec<u8>> {
    thread_local! {
        static COMPRESSOR: std::cell::RefCell<Option<(i32, zstd::bulk::Compressor<'static>)>> =
            const { std::cell::RefCell::new(None) };
    }
    COMPRESSOR.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.as_ref().is_none_or(|(current, _)| *current != level) {
            *slot = Some((level, zstd::bulk::Compressor::new(level)?));
        }
        let (_, compressor) = slot.as_mut().expect("compressor initialised above");
        compressor.compress(bytes)
    })
}

use xxhash_rust::xxh64::xxh64;

use crate::{CommitHeader, FramingError};

pub const MAX_ENCRYPTION_DESCRIPTOR_SIZE: usize = 64 * 1024;
pub const UNENCRYPTED_DESCRIPTOR: [u8; 1] = [0];
pub const BLOCK_NONCE_SIZE: usize = 12;
pub const BLOCK_FLAG_ENCRYPTED: u16 = 1;
pub const BLOCK_FLAG_NONCE_PRESENT: u16 = 2;
/// Payload is stored as raw, uncompressed block data.
pub const BLOCK_FLAG_UNCOMPRESSED: u16 = 4;
const BLOCK_KNOWN_FLAGS: u16 =
    BLOCK_FLAG_ENCRYPTED | BLOCK_FLAG_NONCE_PRESENT | BLOCK_FLAG_UNCOMPRESSED;
const BLOCK_FIXED_SIZE: usize = 20;

/// Fixed metadata size for the header frame: commit-header fields (22) +
/// compressed_header_len(4) + header_checksum(8) + encryption_descriptor_len(4) = 38.
pub const HEADER_FRAME_FIXED_SIZE: usize = crate::COMMIT_HEADER_FIXED_SIZE + 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderFrame {
    pub commit_header: CommitHeader,
    pub encryption_descriptor: Vec<u8>,
    pub payload: Vec<u8>,
    checksum: u64,
    uncompressed_size: Option<u64>,
}

impl HeaderFrame {
    pub fn from_uncompressed(
        commit_header: CommitHeader,
        bytes: &[u8],
        encryption_descriptor: Vec<u8>,
        compression_level: i32,
    ) -> Result<Self, FramingError> {
        validate_descriptor_size(encryption_descriptor.len())?;
        let (descriptor, _) = crate::EncryptionDescriptor::decode(&encryption_descriptor)
            .map_err(|_| FramingError::UnsupportedEncryption)?;
        if descriptor.header_encrypted() {
            return Err(FramingError::UnsupportedEncryption);
        }
        let payload = compress_block(bytes, compression_level)
            .map_err(|error| FramingError::Io(error.to_string()))?;
        if payload.len() > u32::MAX as usize {
            return Err(FramingError::InvalidSize);
        }
        let checksum = xxh64(&payload, 0);
        Ok(Self {
            commit_header,
            encryption_descriptor,
            payload,
            checksum,
            uncompressed_size: Some(bytes.len() as u64),
        })
    }

    pub fn from_uncompressed_encrypted(
        commit_header: CommitHeader,
        bytes: &[u8],
        encryption_descriptor: Vec<u8>,
        compression_level: i32,
        key: &[u8; crate::DEK_SIZE],
    ) -> Result<Self, FramingError> {
        validate_descriptor_size(encryption_descriptor.len())?;
        let compressed = compress_block(bytes, compression_level)
            .map_err(|error| FramingError::Io(error.to_string()))?;
        let (descriptor, _) = crate::EncryptionDescriptor::decode(&encryption_descriptor)
            .map_err(|_| FramingError::UnsupportedEncryption)?;
        let nonce = match descriptor {
            crate::EncryptionDescriptor::Symmetric {
                scope,
                algorithm,
                nonce,
            }
            | crate::EncryptionDescriptor::Password {
                scope,
                algorithm,
                nonce,
                ..
            }
            | crate::EncryptionDescriptor::Recipients {
                scope,
                algorithm,
                nonce,
                ..
            }
            | crate::EncryptionDescriptor::PasswordAndRecipients {
                scope,
                algorithm,
                nonce,
                ..
            } if matches!(
                scope,
                crate::EncryptionScope::Header | crate::EncryptionScope::HeaderAndData
            ) =>
            {
                (algorithm, nonce)
            }
            _ => return Err(FramingError::InvalidDescriptor),
        };
        let encrypted_len = compressed
            .len()
            .checked_add(16)
            .ok_or(FramingError::InvalidSize)?;
        if encrypted_len > u32::MAX as usize {
            return Err(FramingError::InvalidSize);
        }
        let mut aad = commit_header.fixed_bytes()?.to_vec();
        aad.extend_from_slice(&(encrypted_len as u32).to_le_bytes());
        aad.extend_from_slice(&(encryption_descriptor.len() as u32).to_le_bytes());
        aad.extend_from_slice(&encryption_descriptor);
        let payload = crate::encrypt_content(nonce.0, key, &nonce.1, &aad, &compressed)?;
        let checksum = xxh64(&payload, 0);
        Ok(Self {
            commit_header,
            encryption_descriptor,
            payload,
            checksum,
            uncompressed_size: Some(bytes.len() as u64),
        })
    }

    pub fn uncompressed_encrypted(
        &self,
        key: &[u8; crate::DEK_SIZE],
        max_output_size: usize,
    ) -> Result<Vec<u8>, FramingError> {
        validate_descriptor_size(self.encryption_descriptor.len())?;
        let (descriptor, _) = crate::EncryptionDescriptor::decode(&self.encryption_descriptor)
            .map_err(|_| FramingError::UnsupportedEncryption)?;
        let (algorithm, nonce) = match descriptor {
            crate::EncryptionDescriptor::Symmetric {
                scope,
                algorithm,
                nonce,
            }
            | crate::EncryptionDescriptor::Password {
                scope,
                algorithm,
                nonce,
                ..
            }
            | crate::EncryptionDescriptor::Recipients {
                scope,
                algorithm,
                nonce,
                ..
            }
            | crate::EncryptionDescriptor::PasswordAndRecipients {
                scope,
                algorithm,
                nonce,
                ..
            } if matches!(
                scope,
                crate::EncryptionScope::Header | crate::EncryptionScope::HeaderAndData
            ) =>
            {
                (algorithm, nonce)
            }
            _ => return Err(FramingError::InvalidDescriptor),
        };
        if self.payload.len() > u32::MAX as usize {
            return Err(FramingError::InvalidSize);
        }
        self.verify_checksum_payload()?;
        let mut aad = self.commit_header.fixed_bytes()?.to_vec();
        aad.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        aad.extend_from_slice(&(self.encryption_descriptor.len() as u32).to_le_bytes());
        aad.extend_from_slice(&self.encryption_descriptor);
        let compressed = crate::decrypt_content(algorithm, key, &nonce, &aad, &self.payload)?;
        let decoder = zstd::stream::read::Decoder::new(compressed.as_slice())
            .map_err(|error| FramingError::Io(error.to_string()))?;
        read_limited(decoder, max_output_size)
    }

    pub fn uncompressed(&self, max_output_size: usize) -> Result<Vec<u8>, FramingError> {
        validate_descriptor_size(self.encryption_descriptor.len())?;
        let (descriptor, _) = crate::EncryptionDescriptor::decode(&self.encryption_descriptor)
            .map_err(|_| FramingError::UnsupportedEncryption)?;
        if descriptor.header_encrypted() {
            return Err(FramingError::UnsupportedEncryption);
        }
        if self.payload.len() > u32::MAX as usize {
            return Err(FramingError::InvalidSize);
        }
        self.verify_checksum_payload()?;
        let decoder = zstd::stream::read::Decoder::new(self.payload.as_slice())
            .map_err(|error| FramingError::Io(error.to_string()))?;
        read_limited(decoder, max_output_size)
    }

    pub fn encoded_size(&self) -> Result<usize, FramingError> {
        validate_descriptor_size(self.encryption_descriptor.len())?;
        if self.payload.len() > u32::MAX as usize {
            return Err(FramingError::InvalidSize);
        }
        self.verify_checksum_payload()?;
        HEADER_FRAME_FIXED_SIZE
            .checked_add(self.encryption_descriptor.len())
            .and_then(|size| size.checked_add(self.payload.len()))
            .ok_or(FramingError::InvalidSize)
    }

    pub fn write_to(&self, writer: &mut (impl Write + ?Sized)) -> Result<(), FramingError> {
        self.encoded_size()?;
        let payload_len = self.payload.len() as u32;
        let descriptor_len = self.encryption_descriptor.len() as u32;
        writer.write_all(&self.commit_header.fixed_bytes()?)?;
        writer.write_all(&payload_len.to_le_bytes())?;
        writer.write_all(&self.checksum.to_le_bytes())?;
        writer.write_all(&descriptor_len.to_le_bytes())?;
        writer.write_all(&self.encryption_descriptor)?;
        writer.write_all(&self.payload)?;
        Ok(())
    }

    pub fn read_from(
        reader: &mut impl Read,
        max_payload_size: usize,
    ) -> Result<Self, FramingError> {
        let commit_header = CommitHeader::read_fixed(reader)?;
        let payload_len = read_u32(reader)? as usize;
        let checksum = read_u64(reader)?;
        let descriptor_len = read_u32(reader)? as usize;
        validate_descriptor_size(descriptor_len)?;
        if payload_len > max_payload_size {
            return Err(FramingError::InvalidSize);
        }
        let mut encryption_descriptor = vec![0; descriptor_len];
        reader.read_exact(&mut encryption_descriptor)?;
        let mut payload = vec![0; payload_len];
        reader.read_exact(&mut payload)?;
        if xxh64(&payload, 0) != checksum {
            return Err(FramingError::ChecksumMismatch);
        }
        Ok(Self {
            commit_header,
            encryption_descriptor,
            payload,
            checksum,
            uncompressed_size: None,
        })
    }

    pub fn checksum(&self) -> u64 {
        self.checksum
    }

    pub fn uncompressed_size(&self) -> Option<u64> {
        self.uncompressed_size
    }

    fn verify_checksum_payload(&self) -> Result<(), FramingError> {
        if xxh64(&self.payload, 0) != self.checksum {
            return Err(FramingError::ChecksumMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockFrameMetadata {
    pub flags: u16,
    pub block_index: u32,
    pub nonce: Option<[u8; BLOCK_NONCE_SIZE]>,
    pub payload_size: u32,
    pub encoded_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockFrame {
    pub flags: u16,
    pub block_index: u32,
    pub nonce: Option<[u8; BLOCK_NONCE_SIZE]>,
    pub payload: Vec<u8>,
    checksum: u64,
}

impl BlockFrame {
    /// Read a block's fixed metadata and seek past its payload without loading it.
    /// Payload checksums and decompression are intentionally not performed.
    pub fn read_metadata_from(
        reader: &mut (impl Read + std::io::Seek),
        max_payload_size: usize,
    ) -> Result<BlockFrameMetadata, FramingError> {
        let payload_size = read_u32(reader)?;
        let flags = read_u16(reader)?;
        let reserved = read_u16(reader)?;
        let block_index = read_u32(reader)?;
        if payload_size as usize > max_payload_size || reserved != 0 {
            return Err(FramingError::InvalidSize);
        }
        validate_block_flags(flags)?;
        let has_nonce = flags & BLOCK_FLAG_NONCE_PRESENT != 0;
        let nonce = if has_nonce {
            let mut nonce = [0_u8; BLOCK_NONCE_SIZE];
            reader.read_exact(&mut nonce)?;
            Some(nonce)
        } else {
            None
        };
        let _checksum = read_u64(reader)?;
        reader.seek(std::io::SeekFrom::Current(i64::from(payload_size)))?;
        let encoded_size = (BLOCK_FIXED_SIZE as u64)
            .checked_add(if has_nonce {
                BLOCK_NONCE_SIZE as u64
            } else {
                0
            })
            .and_then(|size| size.checked_add(u64::from(payload_size)))
            .ok_or(FramingError::InvalidSize)?;
        Ok(BlockFrameMetadata {
            flags,
            block_index,
            nonce,
            payload_size,
            encoded_size,
        })
    }

    pub fn from_stored(block_index: u32, bytes: &[u8]) -> Result<Self, FramingError> {
        if bytes.len() > u32::MAX as usize {
            return Err(FramingError::InvalidSize);
        }
        let payload = bytes.to_vec();
        let checksum = xxh64(&payload, 0);
        Ok(Self {
            flags: BLOCK_FLAG_UNCOMPRESSED,
            block_index,
            nonce: None,
            payload,
            checksum,
        })
    }

    pub fn from_uncompressed(
        block_index: u32,
        bytes: &[u8],
        compression_level: i32,
    ) -> Result<Self, FramingError> {
        let compressed = compress_block(bytes, compression_level)
            .map_err(|error| FramingError::Io(error.to_string()))?;
        let (flags, payload) = if compressed.len() < bytes.len() {
            (0, compressed)
        } else {
            (BLOCK_FLAG_UNCOMPRESSED, bytes.to_vec())
        };
        if payload.len() > u32::MAX as usize {
            return Err(FramingError::InvalidSize);
        }
        let checksum = xxh64(&payload, 0);
        Ok(Self {
            flags,
            block_index,
            nonce: None,
            payload,
            checksum,
        })
    }

    pub fn from_uncompressed_encrypted(
        block_index: u32,
        bytes: &[u8],
        compression_level: i32,
        commit_header: &CommitHeader,
        key: &[u8; crate::DEK_SIZE],
        algorithm: crate::ContentAead,
    ) -> Result<Self, FramingError> {
        let compressed = compress_block(bytes, compression_level)
            .map_err(|error| FramingError::Io(error.to_string()))?;
        let (payload, mode_flags) = if compressed.len() < bytes.len() {
            (compressed, 0)
        } else {
            (bytes.to_vec(), BLOCK_FLAG_UNCOMPRESSED)
        };
        Self::from_compressed_encrypted(
            block_index,
            &payload,
            BLOCK_FLAG_ENCRYPTED | BLOCK_FLAG_NONCE_PRESENT | mode_flags,
            commit_header,
            key,
            algorithm,
        )
    }

    pub fn from_staged_encrypted(
        frame: &Self,
        commit_header: &CommitHeader,
        key: &[u8; crate::DEK_SIZE],
        algorithm: crate::ContentAead,
    ) -> Result<Self, FramingError> {
        if frame.flags & BLOCK_FLAG_ENCRYPTED != 0 || frame.nonce.is_some() {
            return Err(FramingError::InvalidDescriptor);
        }
        let flags = (frame.flags & BLOCK_FLAG_UNCOMPRESSED)
            | BLOCK_FLAG_ENCRYPTED
            | BLOCK_FLAG_NONCE_PRESENT;
        Self::from_compressed_encrypted(
            frame.block_index,
            &frame.payload,
            flags,
            commit_header,
            key,
            algorithm,
        )
    }

    fn from_compressed_encrypted(
        block_index: u32,
        compressed: &[u8],
        flags: u16,
        commit_header: &CommitHeader,
        key: &[u8; crate::DEK_SIZE],
        algorithm: crate::ContentAead,
    ) -> Result<Self, FramingError> {
        let nonce = crate::generate_nonce()?;
        let payload_len = compressed
            .len()
            .checked_add(16)
            .ok_or(FramingError::InvalidSize)?;
        if payload_len > u32::MAX as usize {
            return Err(FramingError::InvalidSize);
        }
        let mut aad = commit_header.identity_bytes()?.to_vec();
        aad.extend_from_slice(&(payload_len as u32).to_le_bytes());
        aad.extend_from_slice(&flags.to_le_bytes());
        aad.extend_from_slice(&0_u16.to_le_bytes());
        aad.extend_from_slice(&block_index.to_le_bytes());
        aad.extend_from_slice(&nonce);
        let payload = crate::encrypt_content(algorithm, key, &nonce, &aad, compressed)?;
        if payload.len() != payload_len {
            return Err(FramingError::InvalidSize);
        }
        let checksum = xxh64(&payload, 0);
        Ok(Self {
            flags,
            block_index,
            nonce: Some(nonce),
            payload,
            checksum,
        })
    }

    pub fn write_to(&self, writer: &mut (impl Write + ?Sized)) -> Result<(), FramingError> {
        self.validate()?;
        writer.write_all(&(self.payload.len() as u32).to_le_bytes())?;
        writer.write_all(&self.flags.to_le_bytes())?;
        writer.write_all(&0_u16.to_le_bytes())?;
        writer.write_all(&self.block_index.to_le_bytes())?;
        if let Some(nonce) = self.nonce {
            writer.write_all(&nonce)?;
        }
        writer.write_all(&self.checksum.to_le_bytes())?;
        writer.write_all(&self.payload)?;
        Ok(())
    }

    pub fn read_from(
        reader: &mut impl Read,
        max_payload_size: usize,
    ) -> Result<Self, FramingError> {
        let payload_len = read_u32(reader)? as usize;
        let flags = read_u16(reader)?;
        let reserved = read_u16(reader)?;
        let block_index = read_u32(reader)?;
        if payload_len > max_payload_size || reserved != 0 {
            return Err(FramingError::InvalidSize);
        }
        validate_block_flags(flags)?;
        let nonce = if flags & BLOCK_FLAG_NONCE_PRESENT != 0 {
            let mut nonce = [0; BLOCK_NONCE_SIZE];
            reader.read_exact(&mut nonce)?;
            Some(nonce)
        } else {
            None
        };
        let checksum = read_u64(reader)?;
        let mut payload = vec![0; payload_len];
        reader.read_exact(&mut payload)?;
        if xxh64(&payload, 0) != checksum {
            return Err(FramingError::ChecksumMismatch);
        }
        Ok(Self {
            flags,
            block_index,
            nonce,
            payload,
            checksum,
        })
    }

    pub fn uncompressed(
        &self,
        expected_size: usize,
        max_output_size: usize,
    ) -> Result<Vec<u8>, FramingError> {
        let output = self.uncompressed_at_most(max_output_size)?;
        if output.len() != expected_size {
            return Err(FramingError::InvalidSize);
        }
        Ok(output)
    }

    pub fn uncompressed_encrypted(
        &self,
        commit_header: &CommitHeader,
        key: &[u8; crate::DEK_SIZE],
        algorithm: crate::ContentAead,
        max_output_size: usize,
    ) -> Result<Vec<u8>, FramingError> {
        self.validate()?;
        if self.flags & BLOCK_FLAG_ENCRYPTED == 0 {
            return Err(FramingError::InvalidDescriptor);
        }
        let nonce = self.nonce.ok_or(FramingError::InvalidSize)?;
        let mut aad = commit_header.identity_bytes()?.to_vec();
        aad.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        aad.extend_from_slice(&self.flags.to_le_bytes());
        aad.extend_from_slice(&0_u16.to_le_bytes());
        aad.extend_from_slice(&self.block_index.to_le_bytes());
        aad.extend_from_slice(&nonce);
        let compressed = crate::decrypt_content(algorithm, key, &nonce, &aad, &self.payload)?;
        if self.flags & BLOCK_FLAG_UNCOMPRESSED != 0 {
            if compressed.len() > max_output_size {
                return Err(FramingError::InvalidSize);
            }
            return Ok(compressed);
        }
        let decoder = zstd::stream::read::Decoder::new(compressed.as_slice())
            .map_err(|error| FramingError::Io(error.to_string()))?;
        read_limited(decoder, max_output_size)
    }

    pub fn uncompressed_at_most(&self, max_output_size: usize) -> Result<Vec<u8>, FramingError> {
        self.validate()?;
        if self.flags & BLOCK_FLAG_ENCRYPTED != 0 {
            return Err(FramingError::UnsupportedEncryption);
        }
        if self.flags & BLOCK_FLAG_UNCOMPRESSED != 0 {
            if self.payload.len() > max_output_size {
                return Err(FramingError::InvalidSize);
            }
            return Ok(self.payload.clone());
        }
        let decoder = zstd::stream::read::Decoder::new(self.payload.as_slice())
            .map_err(|error| FramingError::Io(error.to_string()))?;
        read_limited(decoder, max_output_size)
    }

    pub fn encoded_size(&self) -> Result<usize, FramingError> {
        self.validate()?;
        BLOCK_FIXED_SIZE
            .checked_add(if self.nonce.is_some() {
                BLOCK_NONCE_SIZE
            } else {
                0
            })
            .and_then(|size| size.checked_add(self.payload.len()))
            .ok_or(FramingError::InvalidSize)
    }

    pub fn checksum(&self) -> u64 {
        self.checksum
    }

    fn validate(&self) -> Result<(), FramingError> {
        if self.payload.len() > u32::MAX as usize {
            return Err(FramingError::InvalidSize);
        }
        if xxh64(&self.payload, 0) != self.checksum {
            return Err(FramingError::ChecksumMismatch);
        }
        validate_block_flags(self.flags)?;
        if (self.flags & BLOCK_FLAG_NONCE_PRESENT != 0) != self.nonce.is_some() {
            return Err(FramingError::InvalidSize);
        }
        Ok(())
    }
}

fn validate_descriptor_size(size: usize) -> Result<(), FramingError> {
    if size > MAX_ENCRYPTION_DESCRIPTOR_SIZE {
        return Err(FramingError::InvalidSize);
    }
    Ok(())
}

fn validate_block_flags(flags: u16) -> Result<(), FramingError> {
    if flags & !BLOCK_KNOWN_FLAGS != 0
        || (flags & BLOCK_FLAG_ENCRYPTED == 0 && flags & BLOCK_FLAG_NONCE_PRESENT != 0)
        || (flags & BLOCK_FLAG_ENCRYPTED != 0 && flags & BLOCK_FLAG_NONCE_PRESENT == 0)
    {
        return Err(FramingError::InvalidSize);
    }
    Ok(())
}

fn read_limited(mut reader: impl Read, limit: usize) -> Result<Vec<u8>, FramingError> {
    let mut output = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(output);
        }
        if output
            .len()
            .checked_add(read)
            .ok_or(FramingError::InvalidSize)?
            > limit
        {
            return Err(FramingError::InvalidSize);
        }
        output.extend_from_slice(&buffer[..read]);
    }
}

fn read_u16(reader: &mut impl Read) -> Result<u16, FramingError> {
    let mut bytes = [0; 2];
    reader.read_exact(&mut bytes)?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32(reader: &mut impl Read) -> Result<u32, FramingError> {
    let mut bytes = [0; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(reader: &mut impl Read) -> Result<u64, FramingError> {
    let mut bytes = [0; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn header_frame_round_trips_and_checks_payload() {
        let commit_header = CommitHeader::new(0, [1; 4], 0, 64).unwrap();
        let frame =
            HeaderFrame::from_uncompressed(commit_header, b"logical header", vec![0], 1).unwrap();
        let mut encoded = Vec::new();
        frame.write_to(&mut encoded).unwrap();
        assert_eq!(frame.encoded_size().unwrap(), encoded.len());
        let decoded = HeaderFrame::read_from(&mut Cursor::new(encoded), 1024).unwrap();
        assert_eq!(decoded.encryption_descriptor, UNENCRYPTED_DESCRIPTOR);
        assert_eq!(decoded.uncompressed(128).unwrap(), b"logical header");
    }

    #[test]
    fn encrypted_header_frame_round_trips_and_authenticates_prefix() {
        let key = [7; crate::DEK_SIZE];
        let descriptor = crate::EncryptionDescriptor::Symmetric {
            scope: crate::EncryptionScope::Header,
            algorithm: crate::ContentAead::Aes256Gcm,
            nonce: [9; 12],
        }
        .encode()
        .unwrap();
        let compressed = zstd::encode_all(b"encrypted logical header".as_slice(), 1).unwrap();
        let encrypted_size = compressed.len() + 16;
        let header_size = HEADER_FRAME_FIXED_SIZE + descriptor.len() + encrypted_size;
        let commit_header =
            crate::CommitHeader::new(0, [3; 4], 0, u32::try_from(header_size).unwrap()).unwrap();
        let frame = HeaderFrame::from_uncompressed_encrypted(
            commit_header.clone(),
            b"encrypted logical header",
            descriptor,
            1,
            &key,
        )
        .unwrap();
        assert_eq!(
            frame.uncompressed_encrypted(&key, 1024).unwrap(),
            b"encrypted logical header"
        );
        assert_eq!(
            frame.uncompressed(1024),
            Err(FramingError::UnsupportedEncryption)
        );

        let altered_header =
            crate::CommitHeader::new(0, [4; 4], 0, u32::try_from(header_size).unwrap()).unwrap();
        let mut altered_frame = frame.clone();
        altered_frame.commit_header = altered_header;
        assert_eq!(
            altered_frame.uncompressed_encrypted(&key, 1024),
            Err(FramingError::AuthenticationFailed)
        );
        assert_eq!(
            frame.uncompressed_encrypted(&[8; crate::DEK_SIZE], 1024),
            Err(FramingError::AuthenticationFailed)
        );

        let mut encoded = Vec::new();
        frame.write_to(&mut encoded).unwrap();
        *encoded.last_mut().unwrap() ^= 1;
        assert_eq!(
            HeaderFrame::read_from(&mut Cursor::new(encoded), 1024),
            Err(FramingError::ChecksumMismatch)
        );
    }

    #[test]
    fn header_frame_decompresses_data_only_encryption_descriptor() {
        let descriptor = crate::EncryptionDescriptor::Symmetric {
            scope: crate::EncryptionScope::Data,
            algorithm: crate::ContentAead::Aes256Gcm,
            nonce: [1; 12],
        }
        .encode()
        .unwrap();
        let commit_header = CommitHeader::new(0, [1; 4], 0, 64).unwrap();
        let frame = HeaderFrame::from_uncompressed(commit_header, b"logical header", descriptor, 1)
            .unwrap();
        assert_eq!(frame.uncompressed(128).unwrap(), b"logical header");
    }

    #[test]
    fn header_frame_rejects_encrypted_header_decompression() {
        let descriptor = crate::EncryptionDescriptor::Symmetric {
            scope: crate::EncryptionScope::HeaderAndData,
            algorithm: crate::ContentAead::Aes256Gcm,
            nonce: [1; 12],
        }
        .encode()
        .unwrap();
        let commit_header = CommitHeader::new(0, [1; 4], 0, 64).unwrap();
        let frame = HeaderFrame {
            commit_header,
            encryption_descriptor: descriptor,
            payload: vec![1, 2, 3],
            checksum: xxh64(&[1, 2, 3], 0),
            uncompressed_size: None,
        };
        assert_eq!(
            frame.uncompressed(128),
            Err(FramingError::UnsupportedEncryption)
        );
    }

    #[test]
    fn header_frame_preserves_unsupported_descriptor_for_copy_through() {
        let descriptor = vec![1, 2, 3];
        let payload = vec![4, 5, 6];
        let checksum = xxh64(&payload, 0);
        let commit_header = CommitHeader::new(0, [1; 4], 0, 64).unwrap();
        let mut encoded = Vec::new();
        commit_header.write_fixed(&mut encoded).unwrap();
        encoded.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        encoded.extend_from_slice(&checksum.to_le_bytes());
        encoded.extend_from_slice(&(descriptor.len() as u32).to_le_bytes());
        encoded.extend_from_slice(&descriptor);
        encoded.extend_from_slice(&payload);

        let frame = HeaderFrame::read_from(&mut Cursor::new(encoded), 1024).unwrap();
        assert_eq!(frame.encryption_descriptor, descriptor);
        assert_eq!(frame.payload, payload);

        let mut copied = Vec::new();
        frame.write_to(&mut copied).unwrap();
        let copied_frame = HeaderFrame::read_from(&mut Cursor::new(copied), 1024).unwrap();
        assert_eq!(copied_frame, frame);
        assert_eq!(
            frame.uncompressed(1024),
            Err(FramingError::UnsupportedEncryption)
        );
    }

    #[test]
    fn header_frame_rejects_mutated_payload() {
        let commit_header = CommitHeader::new(0, [1; 4], 0, 64).unwrap();
        let mut frame = HeaderFrame::from_uncompressed(
            commit_header,
            b"header",
            UNENCRYPTED_DESCRIPTOR.to_vec(),
            1,
        )
        .unwrap();
        frame.payload[0] ^= 1;
        assert_eq!(frame.uncompressed(128), Err(FramingError::ChecksumMismatch));
        assert_eq!(
            frame.write_to(&mut Vec::new()),
            Err(FramingError::ChecksumMismatch)
        );
    }

    #[test]
    fn header_frame_enforces_descriptor_and_payload_limits() {
        let commit_header = CommitHeader::new(0, [1; 4], 0, 64).unwrap();
        let frame =
            HeaderFrame::from_uncompressed(commit_header, b"x", UNENCRYPTED_DESCRIPTOR.to_vec(), 1)
                .unwrap();
        let mut encoded = Vec::new();
        frame.write_to(&mut encoded).unwrap();
        assert_eq!(
            HeaderFrame::read_from(&mut Cursor::new(encoded), 0),
            Err(FramingError::InvalidSize)
        );
        let commit_header2 = CommitHeader::new(0, [1; 4], 0, 64).unwrap();
        assert_eq!(
            HeaderFrame::from_uncompressed(
                commit_header2,
                b"x",
                vec![0; MAX_ENCRYPTION_DESCRIPTOR_SIZE + 1],
                1
            ),
            Err(FramingError::InvalidSize)
        );
    }

    #[test]
    fn block_frame_falls_back_to_uncompressed_when_zstd_expands() {
        let bytes = (0..256).map(|value| value as u8).collect::<Vec<_>>();
        let frame = BlockFrame::from_uncompressed(0, &bytes, 3).unwrap();
        assert_ne!(frame.flags & BLOCK_FLAG_UNCOMPRESSED, 0);
        assert_eq!(frame.payload, bytes);
        assert_eq!(frame.uncompressed_at_most(256).unwrap(), bytes);
        assert!(
            BlockFrame::from_uncompressed(1, b"a repeated block a repeated block", 3)
                .unwrap()
                .payload
                .len()
                < b"a repeated block a repeated block".len()
        );
    }

    #[test]
    fn block_frame_round_trips_and_checks_uncompressed_length() {
        let frame = BlockFrame::from_uncompressed(3, b"block data", 1).unwrap();
        let mut encoded = Vec::new();
        frame.write_to(&mut encoded).unwrap();
        assert_eq!(frame.encoded_size().unwrap(), encoded.len());
        let decoded = BlockFrame::read_from(&mut Cursor::new(encoded), 1024).unwrap();
        assert_eq!(decoded.block_index, 3);
        assert_eq!(decoded.uncompressed(10, 1024).unwrap(), b"block data");
        assert_eq!(
            decoded.uncompressed(9, 1024),
            Err(FramingError::InvalidSize)
        );
    }

    #[test]
    fn encrypted_block_frame_round_trips_and_authenticates_prefix_and_index() {
        let key = [7; crate::DEK_SIZE];
        let algorithm = crate::ContentAead::ChaCha20Poly1305;
        let _compressed = zstd::encode_all(b"encrypted block bytes".as_slice(), 1).unwrap();
        let original = b"encrypted block bytes";
        let encoded = zstd::encode_all(original.as_slice(), 1).unwrap();
        let expected_flags = if encoded.len() < original.len() {
            BLOCK_FLAG_ENCRYPTED | BLOCK_FLAG_NONCE_PRESENT
        } else {
            BLOCK_FLAG_ENCRYPTED | BLOCK_FLAG_NONCE_PRESENT | BLOCK_FLAG_UNCOMPRESSED
        };
        let expected_payload_len = if encoded.len() < original.len() {
            encoded.len()
        } else {
            original.len()
        };
        let commit_header = crate::CommitHeader::new(0, [3; 4], 64, 1).unwrap();
        let frame = BlockFrame::from_uncompressed_encrypted(
            5,
            b"encrypted block bytes",
            1,
            &commit_header,
            &key,
            algorithm,
        )
        .unwrap();
        assert_eq!(frame.payload.len(), expected_payload_len + 16);
        assert_eq!(frame.flags, expected_flags);
        assert_eq!(
            frame
                .uncompressed_encrypted(&commit_header, &key, algorithm, 1024)
                .unwrap(),
            b"encrypted block bytes"
        );
        assert_eq!(
            frame.uncompressed_at_most(1024),
            Err(FramingError::UnsupportedEncryption)
        );

        let altered_header = crate::CommitHeader::new(0, [4; 4], 64, 1).unwrap();
        assert_eq!(
            frame.uncompressed_encrypted(&altered_header, &key, algorithm, 1024),
            Err(FramingError::AuthenticationFailed)
        );
        let mut altered_index = frame.clone();
        altered_index.block_index += 1;
        assert_eq!(
            altered_index.uncompressed_encrypted(&commit_header, &key, algorithm, 1024),
            Err(FramingError::AuthenticationFailed)
        );
        assert_eq!(
            frame.uncompressed_encrypted(&commit_header, &[8; crate::DEK_SIZE], algorithm, 1024),
            Err(FramingError::AuthenticationFailed)
        );

        let mut encoded = Vec::new();
        frame.write_to(&mut encoded).unwrap();
        *encoded.last_mut().unwrap() ^= 1;
        assert_eq!(
            BlockFrame::read_from(&mut Cursor::new(encoded), 1024),
            Err(FramingError::ChecksumMismatch)
        );
    }

    #[test]
    fn block_frame_rejects_invalid_flags_and_reserved_fields() {
        let mut frame = BlockFrame::from_uncompressed(0, b"data", 1).unwrap();
        frame.flags = 2;
        assert_eq!(frame.encoded_size(), Err(FramingError::InvalidSize));
        frame.flags = 0;
        let mut encoded = Vec::new();
        frame.write_to(&mut encoded).unwrap();
        encoded[6] = 1;
        assert_eq!(
            BlockFrame::read_from(&mut Cursor::new(encoded), 1024),
            Err(FramingError::InvalidSize)
        );
    }

    #[test]
    fn block_frame_rejects_mutated_payload() {
        let mut frame = BlockFrame::from_uncompressed(0, b"data", 1).unwrap();
        frame.payload[0] ^= 1;
        assert_eq!(frame.encoded_size(), Err(FramingError::ChecksumMismatch));
        assert_eq!(
            frame.uncompressed(4, 1024),
            Err(FramingError::ChecksumMismatch)
        );
    }

    #[test]
    fn block_frame_checksum_rejects_payload_corruption() {
        let frame = BlockFrame::from_uncompressed(0, b"data", 1).unwrap();
        let mut encoded = Vec::new();
        frame.write_to(&mut encoded).unwrap();
        *encoded.last_mut().unwrap() ^= 1;
        assert_eq!(
            BlockFrame::read_from(&mut Cursor::new(encoded), 1024),
            Err(FramingError::ChecksumMismatch)
        );
    }
}
