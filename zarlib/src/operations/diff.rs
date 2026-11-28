use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fs::File;

use super::common::{parse_commit_id, read_commit_at_offset, read_first_header_offset};
use super::{PasswordSet, PreparedExtraction};
use crate::{ArchivePath, CommitValidationLimits, TocEntry};

/// Sizes of an entry before and after a comparison, in uncompressed bytes.
/// A missing side is `None`; non-file entries have size zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffEntry {
    pub old_size: Option<u64>,
    pub new_size: Option<u64>,
}

/// Differences between two complete archive snapshots, keyed by archive path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffResult {
    pub added: BTreeMap<ArchivePath, DiffEntry>,
    pub removed: BTreeMap<ArchivePath, DiffEntry>,
    pub changed: BTreeMap<ArchivePath, DiffEntry>,
}

/// Compare two commit snapshots, using the default password if provided.
pub fn diff_archive(
    archive: &str,
    start: &str,
    end: &str,
    password: Option<&[u8]>,
) -> Result<DiffResult, Box<dyn std::error::Error>> {
    diff_archive_with_passwords(
        archive,
        start,
        end,
        &PasswordSet::new(password.map(<[u8]>::to_vec)),
    )
}

/// Compare two commit snapshots, requiring passwords for all selected headers.
/// Equal IDs compare the selected commit with its predecessor, or an empty
/// snapshot if it is the first commit.
pub fn diff_archive_with_passwords(
    archive: &str,
    start: &str,
    end: &str,
    passwords: &PasswordSet,
) -> Result<DiffResult, Box<dyn std::error::Error>> {
    let after = PreparedExtraction::new_complete(archive, Some(end), passwords)?;
    let before_id = if parse_commit_id(start)? == parse_commit_id(end)? {
        previous_commit(archive, parse_commit_id(end)?)?
    } else {
        Some(start.to_owned())
    };
    let before = before_id
        .as_deref()
        .map(|id| PreparedExtraction::new_complete(archive, Some(id), passwords))
        .transpose()?;
    if let Some(before) = &before {
        before.ensure_unchanged()?;
    }
    let mut result = DiffResult::default();
    if let Some(before) = &before {
        for (path, old) in before.toc().iter() {
            match after.toc().get(path) {
                None => {
                    result.removed.insert(
                        path.clone(),
                        DiffEntry {
                            old_size: Some(entry_size(old)),
                            new_size: None,
                        },
                    );
                }
                Some(new) => {
                    let changed = before.entry_metadata().get(path)
                        != after.entry_metadata().get(path)
                        || entries_differ(before, &after, path, old, new)?;
                    if changed {
                        result.changed.insert(
                            path.clone(),
                            DiffEntry {
                                old_size: Some(entry_size(old)),
                                new_size: Some(entry_size(new)),
                            },
                        );
                    }
                }
            }
        }
    }
    for (path, new) in after.toc().iter() {
        if before
            .as_ref()
            .is_none_or(|before| before.toc().get(path).is_none())
        {
            result.added.insert(
                path.clone(),
                DiffEntry {
                    old_size: None,
                    new_size: Some(entry_size(new)),
                },
            );
        }
    }
    if let Some(before) = &before {
        before.ensure_unchanged()?;
    }
    after.ensure_unchanged()?;
    Ok(result)
}

fn previous_commit(
    archive: &str,
    requested: [u8; 4],
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let mut file = File::open(archive)?;
    let mut offset = read_first_header_offset(&mut file)?;
    let mut previous = None;
    let mut visited = HashSet::new();
    while offset != 0 {
        if !visited.insert(offset) {
            return Err("archive commit chain contains a cycle".into());
        }
        let (header, _, next) =
            read_commit_at_offset(&mut file, offset, CommitValidationLimits::default())?;
        if header.commit_id == requested {
            return Ok(previous);
        }
        previous = Some(format!("{:08x}", u32::from_le_bytes(header.commit_id)));
        offset = next;
    }
    Err("commit ID not found".into())
}

fn entry_size(entry: &TocEntry) -> u64 {
    match entry {
        TocEntry::File(file) => file.size,
        _ => 0,
    }
}

fn entries_differ(
    before: &PreparedExtraction,
    after: &PreparedExtraction,
    path: &ArchivePath,
    old: &TocEntry,
    new: &TocEntry,
) -> Result<bool, Box<dyn std::error::Error>> {
    let (TocEntry::File(old_file), TocEntry::File(new_file)) = (old, new) else {
        return Ok(old != new);
    };
    if old_file.size != new_file.size {
        return Ok(true);
    }
    if old_file == new_file && before.toc().file_origin(path) == after.toc().file_origin(path) {
        return Ok(false);
    }
    if matches!((old_file.checksum, new_file.checksum), (Some(a), Some(b)) if a != b) {
        return Ok(true);
    }
    let mut offset = 0;
    while offset < old_file.size {
        let length = (old_file.size - offset).min(1024 * 1024);
        let old_bytes = before.read_file_range(path.as_str(), offset, length)?;
        let new_bytes = after.read_file_range(path.as_str(), offset, length)?;
        if old_bytes != new_bytes {
            return Ok(true);
        }
        if old_bytes.len() as u64 != length {
            return Err(format!("short read while comparing {path}").into());
        }
        offset += length;
    }
    Ok(false)
}
