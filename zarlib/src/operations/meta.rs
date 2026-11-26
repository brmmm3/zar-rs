use super::common::*;
use super::remove::append_header_only_commit;
use crate::{CommitValidationLimits, LogicalHeader, MetadataType};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{Seek, SeekFrom};
use std::path::Path;

/// Keys starting with this prefix are reserved for the library.
pub const RESERVED_KEY_PREFIX: &str = "zar.";
/// Marks a custom key as removed by a later commit.
const REMOVED_KEY_PREFIX: &str = "zar.removed.";

/// Applies the custom metadata of one commit onto `target`, later commits winning.
/// Removal markers are kept so that they still hide keys set in earlier commits.
pub(crate) fn merge_custom_metadata(
    target: &mut HashMap<String, MetadataType>,
    source: &HashMap<String, MetadataType>,
) {
    for (key, value) in source {
        if let Some(removed) = key.strip_prefix(REMOVED_KEY_PREFIX) {
            target.remove(removed);
            target.insert(key.clone(), value.clone());
        } else if !key.starts_with(RESERVED_KEY_PREFIX) {
            target.remove(&format!("{REMOVED_KEY_PREFIX}{key}"));
            target.insert(key.clone(), value.clone());
        }
    }
}

/// Returns the effective custom metadata of an archive (all commits folded in order).
pub fn read_custom_metadata(
    archive: &str,
) -> Result<BTreeMap<String, MetadataType>, Box<dyn std::error::Error>> {
    let mut merged = HashMap::new();
    merge_archive_metadata(archive, &mut merged)?;
    Ok(visible_metadata(merged))
}

/// Returns effective custom metadata after applying an overlay to a base archive.
pub fn read_custom_metadata_with_overlay(
    base: &str,
    overlay: &str,
) -> Result<BTreeMap<String, MetadataType>, Box<dyn std::error::Error>> {
    let mut merged = HashMap::new();
    merge_archive_metadata(base, &mut merged)?;
    merge_archive_metadata(overlay, &mut merged)?;
    Ok(visible_metadata(merged))
}

fn merge_archive_metadata(
    archive: &str,
    merged: &mut HashMap<String, MetadataType>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut input = File::open(archive)?;
    let limits = CommitValidationLimits::default();
    let mut header_offset = read_first_header_offset(&mut input)?;
    let empty_passwords = PasswordSet::default();
    while header_offset != 0 {
        let (_, _, next_header_offset) = read_commit_at_offset(&mut input, header_offset, limits)?;
        input.seek(SeekFrom::Start(header_offset))?;
        if let Ok((validated, _)) = read_header_with_passwords(&mut input, limits, &empty_passwords)
            && let Some(header) = &validated.logical_header
        {
            merge_custom_metadata(merged, &header.metadata);
        }
        header_offset = next_header_offset;
    }
    Ok(())
}

fn visible_metadata(merged: HashMap<String, MetadataType>) -> BTreeMap<String, MetadataType> {
    merged
        .into_iter()
        .filter(|(key, _)| !key.starts_with(RESERVED_KEY_PREFIX))
        .collect()
}

/// Parses `text` as an integer, then a float, and otherwise keeps it as a string.
pub fn parse_metadata_value(text: &str) -> MetadataType {
    if let Ok(value) = text.parse::<i64>() {
        MetadataType::I64(value)
    } else if let Ok(value) = text.parse::<f64>()
        && value.is_finite()
    {
        MetadataType::F64(value)
    } else {
        MetadataType::String(text.to_owned())
    }
}

pub fn format_metadata_value(value: &MetadataType) -> String {
    match value {
        MetadataType::String(text) => text.clone(),
        MetadataType::I64(number) => number.to_string(),
        MetadataType::F64(number) => number.to_string(),
    }
}

/// Appends a commit without file changes that sets and/or removes custom metadata keys.
pub fn update_custom_metadata(
    archive: &str,
    set: &[(String, MetadataType)],
    remove: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let current = read_custom_metadata(archive)?;
    update_custom_metadata_with_current(archive, set, remove, &current)
}

/// Appends a metadata change to an overlay, validating against the combined state.
pub fn update_overlay_custom_metadata(
    base: &str,
    overlay: &str,
    set: &[(String, MetadataType)],
    remove: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    ensure_archive_unencrypted(Path::new(base))?;
    let current = read_custom_metadata_with_overlay(base, overlay)?;
    update_custom_metadata_with_current(overlay, set, remove, &current)
}

fn update_custom_metadata_with_current(
    archive: &str,
    set: &[(String, MetadataType)],
    remove: &[String],
    current: &BTreeMap<String, MetadataType>,
) -> Result<(), Box<dyn std::error::Error>> {
    if set.is_empty() && remove.is_empty() {
        return Err("nothing to change".into());
    }
    let archive_path = Path::new(archive);
    ensure_archive_unencrypted(archive_path)?;
    let mut header_metadata = HashMap::new();
    for (key, value) in set {
        validate_key(key)?;
        if remove.contains(key) {
            return Err(format!("key `{key}` is both set and removed").into());
        }
        if header_metadata.insert(key.clone(), value.clone()).is_some() {
            return Err(format!("key `{key}` is set more than once").into());
        }
    }
    for key in remove {
        validate_key(key)?;
        if !current.contains_key(key) {
            return Err(format!("metadata key not found: {key}").into());
        }
        header_metadata.insert(format!("{REMOVED_KEY_PREFIX}{key}"), MetadataType::I64(1));
    }
    header_metadata.extend(commit_metadata(None)?);
    let header_bytes = LogicalHeader::new(Vec::new())
        .with_metadata(header_metadata)
        .encode()?;
    append_header_only_commit(archive_path, &header_bytes)
}

fn validate_key(key: &str) -> Result<(), Box<dyn std::error::Error>> {
    if key.is_empty() {
        return Err("metadata key must not be empty".into());
    }
    if key.starts_with(RESERVED_KEY_PREFIX) {
        return Err(format!("metadata keys must not start with `{RESERVED_KEY_PREFIX}`").into());
    }
    Ok(())
}
