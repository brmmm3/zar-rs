use super::common::*;
use super::remove::append_header_only_commit;
use crate::{CommitValidationLimits, LogicalHeader, MetadataType, ValidatedCommit};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
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
    let mut input = File::open(archive)?;
    let limits = CommitValidationLimits::default();
    let mut merged = HashMap::new();
    let mut header_offset = read_first_header_offset(&mut input)?;
    while header_offset != 0 {
        let (commit_header, _, next_header_offset) =
            read_commit_at_offset(&mut input, header_offset, limits)?;
        let validated = ValidatedCommit::read_body(&mut input, commit_header, limits)?;
        if let Some(header) = &validated.logical_header {
            merge_custom_metadata(&mut merged, &header.metadata);
        }
        header_offset = next_header_offset;
    }
    Ok(merged
        .into_iter()
        .filter(|(key, _)| !key.starts_with(RESERVED_KEY_PREFIX))
        .collect())
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
    if set.is_empty() && remove.is_empty() {
        return Err("nothing to change".into());
    }
    let archive_path = Path::new(archive);
    ensure_archive_unencrypted(archive_path)?;
    let current = read_custom_metadata(archive)?;
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
