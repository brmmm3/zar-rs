use super::common::{
    EncodingOptions, MAX_CREATE_BLOCK_SIZE, MIN_CREATE_BLOCK_SIZE, commit_metadata,
    parse_compression_level,
};
use super::info::group_size_metadata;
use super::pipeline::{EncryptionContext, encode_block};
use crate::{
    ArchivePath, CommitHeader, CommitValidationLimits, ContentAead, EncryptionDescriptor,
    EncryptionScope, FileEntry, HEADER_OFFSET_SIZE, HeaderBlockIndexEntry, HeaderFrame,
    LogicalHeader, Toc, TocChange, UNENCRYPTED_DESCRIPTOR, ValidatedHeader, generate_dek,
    generate_nonce, wrap_dek_with_password,
};
use std::collections::{BTreeSet, HashMap};
use std::io::{self, Read, Seek, SeekFrom, Write};
use xxhash_rust::xxh64::Xxh64;
use zeroize::Zeroizing;

/// A regular file to add to a new archive. The reader is consumed to EOF.
pub struct StreamInput<'a> {
    pub path: &'a str,
    pub reader: &'a mut dyn Read,
}

/// Optional password encryption for a streamed archive.
#[derive(Clone, Copy)]
pub struct StreamEncryption<'a> {
    pub password: &'a [u8],
    pub algorithm: ContentAead,
    pub scope: EncryptionScope,
}

/// Creates a single-commit ZAR archive from regular-file readers.
///
/// `archive` must be empty and support writing at seeked positions (not append
/// mode). Files are read sequentially in bounded blocks; this API does not
/// deduplicate or preserve filesystem metadata. An error may leave a partial
/// archive in the destination. The caller is responsible for flushing it.
pub fn create_archive_from_readers(
    archive: &mut (impl Write + Seek),
    inputs: &mut [StreamInput<'_>],
    options: EncodingOptions,
    encryption: Option<StreamEncryption<'_>>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !(MIN_CREATE_BLOCK_SIZE..=MAX_CREATE_BLOCK_SIZE).contains(&options.block_size) {
        return Err("invalid streaming block size".into());
    }
    parse_compression_level(&options.compression_level.to_string())?;
    let mut paths = Vec::with_capacity(inputs.len());
    let mut seen = BTreeSet::new();
    for input in inputs.iter() {
        let path = ArchivePath::new(input.path)?;
        if !seen.insert(path.clone()) {
            return Err(format!("duplicate archive path: {path}").into());
        }
        paths.push(path);
    }
    let mut check_toc = Toc::default();
    let placeholder = paths
        .iter()
        .map(|path| {
            Ok(TocChange::PutFile {
                path: path.clone(),
                entry: FileEntry::new(0, Vec::new())?,
            })
        })
        .collect::<Result<Vec<_>, crate::TocError>>()?;
    check_toc.apply_archive_commit(1_u32.to_le_bytes(), &placeholder)?;

    if archive.seek(SeekFrom::End(0))? != 0 {
        return Err("streaming archive destination must be empty".into());
    }
    archive.seek(SeekFrom::Start(0))?;
    crate::write_container_magic(archive)?;
    archive.write_all(&0_u64.to_le_bytes())?;
    archive.write_all(&0_u64.to_le_bytes())?;

    let cid = 1_u32.to_le_bytes();
    let (descriptor_bytes, dek, scope, data_encryption) = match encryption {
        Some(encryption) => {
            let dek = Zeroizing::new(generate_dek()?);
            let descriptor = EncryptionDescriptor::Password {
                scope: encryption.scope,
                algorithm: encryption.algorithm,
                nonce: generate_nonce()?,
                key_wrap: wrap_dek_with_password(
                    encryption.password,
                    &dek,
                    encryption.algorithm,
                    65_536,
                    3,
                    1,
                )?,
            };
            let data_encryption = matches!(
                encryption.scope,
                EncryptionScope::Data | EncryptionScope::HeaderAndData
            )
            .then(|| {
                EncryptionContext::new(
                    Zeroizing::new(*dek),
                    CommitHeader::new(0, cid, 0, 0).expect("valid commit identity"),
                    encryption.algorithm,
                )
            });
            (
                descriptor.encode()?,
                Some(dek),
                Some(encryption.scope),
                data_encryption,
            )
        }
        None => (UNENCRYPTED_DESCRIPTOR.to_vec(), None, None, None),
    };

    let limits = CommitValidationLimits::default();
    let mut changes = Vec::with_capacity(inputs.len());
    let mut block_index = Vec::new();
    let mut block_sizes = HashMap::new();
    let mut buffer = vec![0_u8; options.block_size];
    let mut data_size = 0_u64;
    for (input, path) in inputs.iter_mut().zip(paths) {
        let mut size = 0_u64;
        let mut indices = Vec::new();
        let mut checksum = Xxh64::new(0);
        loop {
            let mut filled = 0;
            while filled < buffer.len() {
                match input.reader.read(&mut buffer[filled..]) {
                    Ok(0) => break,
                    Ok(count) => filled += count,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            if filled == 0 {
                break;
            }
            if block_index.len() as u64 >= limits.max_blocks {
                return Err("archive exceeds the block limit".into());
            }
            let index = u32::try_from(block_index.len())?;
            let bytes = &buffer[..filled];
            size = size
                .checked_add(filled as u64)
                .ok_or("file size overflow")?;
            checksum.update(bytes);
            let frame = encode_block(
                index,
                bytes,
                options.compression_level,
                false,
                data_encryption.as_ref(),
            )?;
            let encoded_size = u64::try_from(frame.encoded_size()?)?;
            frame.write_to(archive)?;
            block_index.push(HeaderBlockIndexEntry {
                index,
                offset: data_size,
                encoded_size,
            });
            data_size = data_size
                .checked_add(encoded_size)
                .ok_or("archive size overflow")?;
            block_sizes.insert(index, frame.payload.len() as u64);
            indices.push(index);
        }
        let mut entry = FileEntry::new(size, indices)?;
        if size != 0 {
            entry = entry.with_checksum(checksum.digest());
        }
        changes.push(TocChange::PutFile { path, entry });
    }

    let logical = LogicalHeader::with_block_size(changes, options.block_size as u32)
        .with_block_index(block_index);
    let mut metadata = group_size_metadata(&logical, &block_sizes)?;
    metadata.extend(commit_metadata(None)?);
    let header_bytes = logical.with_metadata(metadata).encode()?;
    let build_header = |commit_header| {
        if matches!(
            scope,
            Some(EncryptionScope::Header | EncryptionScope::HeaderAndData)
        ) {
            HeaderFrame::from_uncompressed_encrypted(
                commit_header,
                &header_bytes,
                descriptor_bytes.clone(),
                options.compression_level,
                dek.as_deref().expect("encrypted header has a key"),
            )
        } else {
            HeaderFrame::from_uncompressed(
                commit_header,
                &header_bytes,
                descriptor_bytes.clone(),
                options.compression_level,
            )
        }
    };
    let provisional = build_header(CommitHeader::new(0, cid, data_size, 0)?)?;
    let header_size = u32::try_from(provisional.encoded_size()?)?;
    let header = build_header(CommitHeader::new(0, cid, data_size, header_size)?)?;
    ValidatedHeader::from_header_frame(header.clone(), limits, dek.as_deref())?;
    let header_offset = crate::CONTAINER_HEADER_SIZE
        .checked_add(HEADER_OFFSET_SIZE)
        .and_then(|offset| offset.checked_add(data_size))
        .ok_or("archive size overflow")?;
    header.write_to(archive)?;
    archive.write_all(&header_offset.to_le_bytes())?;
    let end = archive.stream_position()?;
    archive.seek(SeekFrom::Start(4))?;
    archive.write_all(&header_offset.to_le_bytes())?;
    if archive.stream_position()? != crate::CONTAINER_HEADER_SIZE {
        return Err("archive destination did not write at the requested position".into());
    }
    archive.seek(SeekFrom::Start(end))?;
    Ok(())
}
