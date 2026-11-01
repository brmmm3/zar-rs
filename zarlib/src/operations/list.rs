use super::common::*;
use crate::{
    ArchivePath, BlockFrame, CommitValidationLimits, ContainerHeader, EntryMetadata, Toc,
    TocChange, ValidatedCommit,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{Seek, SeekFrom};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArchiveEntryDetails {
    pub compressed_size: Option<u64>,
    pub owner: Option<u32>,
    pub group: Option<u32>,
    pub permissions: Option<u32>,
    pub metadata_platform: Option<String>,
}

pub fn list_archive_with_details(
    path: &str,
    through: Option<&str>,
    password: Option<&[u8]>,
) -> Result<(Toc, BTreeMap<ArchivePath, ArchiveEntryDetails>), Box<dyn std::error::Error>> {
    let mut file = File::open(path)?;
    let container = ContainerHeader::read_from(&mut file)?;
    let limits = CommitValidationLimits::default();
    let mut toc = Toc::default();
    let mut details = BTreeMap::new();
    let mut header_offset = container.first_header_offset;
    let requested_id = through.map(parse_commit_id).transpose()?;
    let mut matching_commits = 0_u32;
    let mut stopped_at_through = false;

    while header_offset != 0 {
        let (commit_header, data_start, next_header_offset) =
            read_commit_at_offset(&mut file, header_offset, limits)?;
        if requested_id == Some(commit_header.commit_id) {
            matching_commits += 1;
            if matching_commits > 1 {
                return Err("--through commit ID is ambiguous".into());
            }
        }
        if stopped_at_through {
            header_offset = next_header_offset;
            continue;
        }
        file.seek(SeekFrom::Start(header_offset))?;
        let (validated, _) = read_header_with_password(&mut file, limits, password)?;
        let Some(header) = validated.logical_header else {
            return Err(
                "cannot list an archive with a header-encrypted commit; use --password".into(),
            );
        };

        file.seek(SeekFrom::Start(data_start))?;
        let mut block_sizes = HashMap::new();
        let mut remaining = validated.commit_header.data_section_size;
        let mut blocks_seen = 0_u64;
        while remaining > 0 {
            if blocks_seen >= limits.max_blocks {
                return Err(crate::FramingError::InvalidSize.into());
            }
            let before = file.stream_position()?;
            let metadata =
                BlockFrame::read_metadata_from(&mut file, limits.max_frame_payload_size)?;
            let consumed = file
                .stream_position()?
                .checked_sub(before)
                .ok_or("block offset underflow")?;
            if consumed == 0 || consumed > remaining || consumed != metadata.encoded_size {
                return Err(crate::FramingError::InvalidSize.into());
            }
            if block_sizes
                .insert(metadata.block_index, u64::from(metadata.payload_size))
                .is_some()
            {
                return Err(crate::FramingError::InvalidSize.into());
            }
            remaining -= consumed;
            blocks_seen += 1;
        }

        let block_indices = block_sizes.keys().copied().collect::<HashSet<_>>();
        toc.apply_commit_with_available_blocks(&header.changes, &block_indices)?;
        apply_entry_details(
            &mut details,
            &header.changes,
            &header.entry_metadata,
            &block_sizes,
        )?;
        if requested_id == Some(validated.commit_header.commit_id) {
            stopped_at_through = true;
        }
        header_offset = next_header_offset;
    }

    if requested_id.is_some() && matching_commits == 0 {
        return Err("--through commit ID was not found".into());
    }

    Ok((toc, details))
}

fn apply_entry_details(
    details: &mut BTreeMap<ArchivePath, ArchiveEntryDetails>,
    changes: &[TocChange],
    metadata: &[EntryMetadata],
    block_sizes: &HashMap<u32, u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    for change in changes {
        match change {
            TocChange::PutFile { path, entry } => {
                let compressed_size =
                    entry.block_indices.iter().try_fold(0_u64, |total, index| {
                        total
                            .checked_add(*block_sizes.get(index).ok_or("missing block frame size")?)
                            .ok_or("file compressed size overflow")
                    })?;
                details.insert(
                    path.clone(),
                    entry_details(path, metadata, Some(compressed_size)),
                );
            }
            TocChange::PutDirectory { path }
            | TocChange::PutSymlink { path, .. }
            | TocChange::PutNamedPipe { path }
            | TocChange::PutDevice { path, .. } => {
                details.insert(path.clone(), entry_details(path, metadata, None));
            }
            TocChange::RemoveFile { path } => {
                details.remove(path);
            }
            TocChange::RemoveDirectory { path } => {
                details.retain(|entry_path, _| {
                    entry_path != path
                        && !entry_path
                            .as_str()
                            .strip_prefix(path.as_str())
                            .is_some_and(|suffix| suffix.starts_with('/'))
                });
            }
        }
    }
    Ok(())
}

fn entry_details(
    path: &ArchivePath,
    metadata: &[EntryMetadata],
    compressed_size: Option<u64>,
) -> ArchiveEntryDetails {
    let entry_metadata = metadata.iter().rev().find(|entry| entry.path == *path);
    ArchiveEntryDetails {
        compressed_size,
        owner: entry_metadata.and_then(|entry| entry.owner),
        group: entry_metadata.and_then(|entry| entry.group),
        permissions: entry_metadata.and_then(|entry| entry.permissions),
        metadata_platform: entry_metadata.map(|entry| entry.platform.clone()),
    }
}

pub fn load_toc(file: &mut File) -> Result<Toc, Box<dyn std::error::Error>> {
    let container = ContainerHeader::read_from(file)?;
    let limits = CommitValidationLimits::default();
    let mut toc = Toc::default();
    let mut header_offset = container.first_header_offset;
    while header_offset != 0 {
        let (commit_header, _data_start, next_header_offset) =
            read_commit_at_offset(file, header_offset, limits)?;
        let validated = ValidatedCommit::read_body(file, commit_header, limits)?;
        if let Some(header) = validated.logical_header {
            toc.apply_commit_with_available_blocks(&header.changes, &validated.block_indices)?;
        } else {
            return Err("cannot build archive TOC from a header-encrypted commit".into());
        }
        header_offset = next_header_offset;
    }
    Ok(toc)
}

pub fn list_archive(
    path: &str,
    through: Option<&str>,
    password: Option<&[u8]>,
) -> Result<Toc, Box<dyn std::error::Error>> {
    let passwords = PasswordSet::new(password.map(<[u8]>::to_vec));
    list_archive_with_passwords(path, through, &passwords)
}

pub fn list_archive_with_passwords(
    path: &str,
    through: Option<&str>,
    passwords: &PasswordSet,
) -> Result<Toc, Box<dyn std::error::Error>> {
    load_toc_through_with_passwords(path, through, passwords)
}

pub fn load_toc_through(
    archive: &str,
    through: Option<&str>,
    password: Option<&[u8]>,
) -> Result<Toc, Box<dyn std::error::Error>> {
    let passwords = PasswordSet::new(password.map(<[u8]>::to_vec));
    load_toc_through_with_passwords(archive, through, &passwords)
}

pub fn load_toc_through_with_passwords(
    archive: &str,
    through: Option<&str>,
    passwords: &PasswordSet,
) -> Result<Toc, Box<dyn std::error::Error>> {
    load_toc_through_with_passwords_and_progress(archive, through, passwords, None)
}

pub fn load_toc_through_with_progress(
    archive: &str,
    through: Option<&str>,
    password: Option<&[u8]>,
    progress: Option<&ProgressReporter<'_>>,
) -> Result<Toc, Box<dyn std::error::Error>> {
    let passwords = PasswordSet::new(password.map(<[u8]>::to_vec));
    load_toc_through_with_passwords_and_progress(archive, through, &passwords, progress)
}

pub fn load_toc_through_with_passwords_and_progress(
    archive: &str,
    through: Option<&str>,
    passwords: &PasswordSet,
    progress: Option<&ProgressReporter<'_>>,
) -> Result<Toc, Box<dyn std::error::Error>> {
    let mut file = File::open(archive)?;
    let container = ContainerHeader::read_from(&mut file)?;
    let requested_id = through.map(parse_commit_id).transpose()?;
    let mut toc = Toc::default();
    let mut matches = 0_u32;
    let mut stopped_at_through = false;
    let limits = CommitValidationLimits::default();

    let mut header_offset = container.first_header_offset;
    while header_offset != 0 {
        let (commit_header, _data_start, next_header_offset) =
            read_commit_at_offset(&mut file, header_offset, limits)?;
        if requested_id == Some(commit_header.commit_id) {
            matches += 1;
            if matches > 1 {
                return Err("--through commit ID is ambiguous".into());
            }
        }
        if stopped_at_through {
            header_offset = next_header_offset;
            continue;
        }
        let body_start = file.stream_position()?;
        file.seek(SeekFrom::Start(header_offset))?;
        let (validated, _) = read_header_with_passwords(&mut file, limits, passwords)?;
        if let Some(progress) = progress {
            let header_end = file.stream_position()?;
            progress.inc(header_end.saturating_sub(body_start));
            progress.set_message(format!(
                "Reading header for commit {:08x}",
                u32::from_le_bytes(validated.commit_header.commit_id)
            ));
        }
        let Some(header) = validated.logical_header.as_ref() else {
            return Err(
                "cannot list an archive with a header-encrypted commit; use --password".into(),
            );
        };
        toc.apply_commit(&header.changes)?;
        if requested_id == Some(validated.commit_header.commit_id) {
            stopped_at_through = true;
        }
        header_offset = next_header_offset;
    }

    if requested_id.is_some() && matches == 0 {
        return Err("--through commit ID was not found".into());
    }
    Ok(toc)
}
