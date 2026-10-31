use std::collections::{BTreeMap, HashSet};
use std::fmt;

use crate::ArchivePath;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TocError {
    InvalidFileBlocks,
    MissingBlockReference { path: ArchivePath, block_index: u32 },
    MissingPath(ArchivePath),
    WrongEntryKind(ArchivePath),
    FileAncestor(ArchivePath),
    FileHasDescendants(ArchivePath),
}

impl fmt::Display for TocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFileBlocks => f.write_str("file block references are invalid"),
            Self::MissingBlockReference { path, block_index } => {
                write!(f, "file {path} references missing block {block_index}")
            }
            Self::MissingPath(path) => write!(f, "archive path does not exist: {path}"),
            Self::WrongEntryKind(path) => {
                write!(f, "archive path has the wrong entry kind: {path}")
            }
            Self::FileAncestor(path) => write!(f, "a file blocks this archive path: {path}"),
            Self::FileHasDescendants(path) => {
                write!(
                    f,
                    "cannot replace a directory with descendants by a file: {path}"
                )
            }
        }
    }
}

impl std::error::Error for TocError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileGroupKind {
    EmptyFiles,
    TinyFiles,
    SmallFiles,
    BigFiles,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub size: u64,
    pub block_indices: Vec<u32>,
    pub checksum: Option<u64>,
}

impl FileEntry {
    pub fn new(size: u64, block_indices: Vec<u32>) -> Result<Self, TocError> {
        if (size == 0) != block_indices.is_empty() {
            return Err(TocError::InvalidFileBlocks);
        }
        let mut unique = block_indices.clone();
        unique.sort_unstable();
        unique.dedup();
        if unique.len() != block_indices.len() {
            return Err(TocError::InvalidFileBlocks);
        }
        Ok(Self {
            size,
            block_indices,
            checksum: None,
        })
    }

    pub fn with_checksum(mut self, checksum: u64) -> Self {
        self.checksum = Some(checksum);
        self
    }

    pub fn group_kind(&self, block_size: u32) -> FileGroupKind {
        assert!(block_size > 0, "block size must be nonzero");
        if self.size >= u64::from(block_size) {
            FileGroupKind::BigFiles
        } else {
            match self.size {
                0 => FileGroupKind::EmptyFiles,
                1..=8 => FileGroupKind::TinyFiles,
                _ => FileGroupKind::SmallFiles,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Character,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TocEntry {
    File(FileEntry),
    Directory,
    Symlink {
        target: String,
    },
    NamedPipe,
    Device {
        kind: DeviceKind,
        major: u32,
        minor: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TocChange {
    PutFile {
        path: ArchivePath,
        entry: FileEntry,
    },
    PutDirectory {
        path: ArchivePath,
    },
    PutSymlink {
        path: ArchivePath,
        target: String,
    },
    PutNamedPipe {
        path: ArchivePath,
    },
    PutDevice {
        path: ArchivePath,
        kind: DeviceKind,
        major: u32,
        minor: u32,
    },
    RemoveFile {
        path: ArchivePath,
    },
    RemoveDirectory {
        path: ArchivePath,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Toc {
    entries: BTreeMap<ArchivePath, TocEntry>,
}

impl Toc {
    pub fn get(&self, path: &ArchivePath) -> Option<&TocEntry> {
        self.entries.get(path)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ArchivePath, &TocEntry)> {
        self.entries.iter()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Apply changes after checking new file references against this commit's blocks.
    /// Existing TOC entries are not rechecked because they may reference earlier commits.
    pub fn apply_commit_with_available_blocks(
        &mut self,
        changes: &[TocChange],
        available_blocks: &HashSet<u32>,
    ) -> Result<(), TocError> {
        Self::check_available_blocks(changes, available_blocks)?;
        self.apply_commit(changes)
    }

    /// Verify that every file in `changes` only references blocks in `available_blocks`.
    pub fn check_available_blocks(
        changes: &[TocChange],
        available_blocks: &HashSet<u32>,
    ) -> Result<(), TocError> {
        for change in changes {
            if let TocChange::PutFile { path, entry } = change
                && let Some(&block_index) = entry
                    .block_indices
                    .iter()
                    .find(|&&index| !available_blocks.contains(&index))
            {
                return Err(TocError::MissingBlockReference {
                    path: path.clone(),
                    block_index,
                });
            }
        }
        Ok(())
    }

    /// Apply one commit's ordered changes atomically; on error the TOC is unchanged.
    pub fn apply_commit(&mut self, changes: &[TocChange]) -> Result<(), TocError> {
        let mut updated = self.entries.clone();
        for change in changes {
            apply_change(&mut updated, change)?;
        }
        self.entries = updated;
        Ok(())
    }
}

fn apply_change(
    entries: &mut BTreeMap<ArchivePath, TocEntry>,
    change: &TocChange,
) -> Result<(), TocError> {
    match change {
        TocChange::PutFile { path, entry } => {
            if (entry.size == 0) != entry.block_indices.is_empty() {
                return Err(TocError::InvalidFileBlocks);
            }
            let mut unique = entry.block_indices.clone();
            unique.sort_unstable();
            unique.dedup();
            if unique.len() != entry.block_indices.len() {
                return Err(TocError::InvalidFileBlocks);
            }
            ensure_no_non_directory_ancestor(entries, path)?;
            if has_descendant(entries, path) {
                return Err(TocError::FileHasDescendants(path.clone()));
            }
            entries.insert(path.clone(), TocEntry::File(entry.clone()));
        }
        TocChange::PutDirectory { path } => {
            ensure_no_non_directory_ancestor(entries, path)?;
            entries.insert(path.clone(), TocEntry::Directory);
        }
        TocChange::PutSymlink { path, target } => {
            if target.is_empty() || target.as_bytes().contains(&0) {
                return Err(TocError::InvalidFileBlocks);
            }
            ensure_no_non_directory_ancestor(entries, path)?;
            if has_descendant(entries, path) {
                return Err(TocError::FileHasDescendants(path.clone()));
            }
            entries.insert(
                path.clone(),
                TocEntry::Symlink {
                    target: target.clone(),
                },
            );
        }
        TocChange::PutNamedPipe { path } => {
            ensure_no_non_directory_ancestor(entries, path)?;
            if has_descendant(entries, path) {
                return Err(TocError::FileHasDescendants(path.clone()));
            }
            entries.insert(path.clone(), TocEntry::NamedPipe);
        }
        TocChange::PutDevice {
            path,
            kind,
            major,
            minor,
        } => {
            if !matches!(kind, DeviceKind::Character | DeviceKind::Block) {
                return Err(TocError::InvalidFileBlocks);
            }
            ensure_no_non_directory_ancestor(entries, path)?;
            if has_descendant(entries, path) {
                return Err(TocError::FileHasDescendants(path.clone()));
            }
            entries.insert(
                path.clone(),
                TocEntry::Device {
                    kind: *kind,
                    major: *major,
                    minor: *minor,
                },
            );
        }
        TocChange::RemoveFile { path } => match entries.get(path) {
            None => {}
            Some(
                TocEntry::File(_)
                | TocEntry::Symlink { .. }
                | TocEntry::NamedPipe
                | TocEntry::Device { .. },
            ) => {
                entries.remove(path);
            }
            Some(TocEntry::Directory) => return Err(TocError::WrongEntryKind(path.clone())),
        },
        TocChange::RemoveDirectory { path } => match entries.get(path) {
            None => {}
            Some(
                TocEntry::File(_)
                | TocEntry::Symlink { .. }
                | TocEntry::NamedPipe
                | TocEntry::Device { .. },
            ) => return Err(TocError::WrongEntryKind(path.clone())),
            Some(TocEntry::Directory) => {
                entries.retain(|existing, _| existing != path && !is_descendant(existing, path));
            }
        },
    }
    Ok(())
}

fn ensure_no_non_directory_ancestor(
    entries: &BTreeMap<ArchivePath, TocEntry>,
    path: &ArchivePath,
) -> Result<(), TocError> {
    ensure_no_entry_ancestor(entries, path, |entry| !matches!(entry, TocEntry::Directory))
}

fn ensure_no_entry_ancestor(
    entries: &BTreeMap<ArchivePath, TocEntry>,
    path: &ArchivePath,
    is_blocking: impl Fn(&TocEntry) -> bool,
) -> Result<(), TocError> {
    let mut parent = path.as_str();
    while let Some(separator) = parent.rfind('/') {
        parent = &parent[..separator];
        let parent_path = ArchivePath::new(parent.to_owned()).expect("validated path prefix");
        if entries.get(&parent_path).is_some_and(&is_blocking) {
            return Err(TocError::FileAncestor(parent_path));
        }
    }
    Ok(())
}

fn has_descendant(entries: &BTreeMap<ArchivePath, TocEntry>, ancestor: &ArchivePath) -> bool {
    let upper_bound = ArchivePath::new(format!("{}/\u{10ffff}", ancestor.as_str()))
        .expect("the upper-bound path component is valid");
    entries
        .range(ancestor.clone()..=upper_bound)
        .any(|(existing, _)| is_descendant(existing, ancestor))
}

fn is_descendant(candidate: &ArchivePath, ancestor: &ArchivePath) -> bool {
    candidate
        .as_str()
        .strip_prefix(ancestor.as_str())
        .is_some_and(|suffix| suffix.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(value: &str) -> ArchivePath {
        ArchivePath::try_from(value).unwrap()
    }

    fn file(size: u64, block_indices: &[u32]) -> FileEntry {
        FileEntry::new(size, block_indices.to_vec()).unwrap()
    }

    #[test]
    fn special_file_entries_conflict_with_descendants_and_replace_in_order() {
        let mut toc = Toc::default();
        toc.apply_commit(&[TocChange::PutSymlink {
            path: path("node"),
            target: "target".to_owned(),
        }])
        .unwrap();
        assert!(
            toc.apply_commit(&[TocChange::PutFile {
                path: path("node/child"),
                entry: FileEntry::new(1, vec![0]).unwrap()
            }])
            .is_err()
        );
        toc.apply_commit(&[TocChange::PutNamedPipe { path: path("node") }])
            .unwrap();
        assert_eq!(toc.get(&path("node")), Some(&TocEntry::NamedPipe));
        toc.apply_commit(&[TocChange::RemoveFile { path: path("node") }])
            .unwrap();
        assert!(toc.get(&path("node")).is_none());
    }

    #[test]
    fn file_group_kinds_cover_tiny_small_and_big_boundaries() {
        let block_size = 64 * 1024;
        assert_eq!(
            file(0, &[]).group_kind(block_size),
            FileGroupKind::EmptyFiles
        );
        for size in 1..=8 {
            assert_eq!(
                file(size, &[0]).group_kind(block_size),
                FileGroupKind::TinyFiles
            );
        }
        assert_eq!(
            file(9, &[0]).group_kind(block_size),
            FileGroupKind::SmallFiles
        );
        assert_eq!(
            file(u64::from(block_size) - 1, &[0]).group_kind(block_size),
            FileGroupKind::SmallFiles
        );
        assert_eq!(
            file(u64::from(block_size), &[0]).group_kind(block_size),
            FileGroupKind::BigFiles
        );
    }

    #[test]
    fn applies_ordered_changes_and_directory_tombstones() {
        let mut toc = Toc::default();
        toc.apply_commit(&[
            TocChange::PutDirectory { path: path("docs") },
            TocChange::PutFile {
                path: path("docs/old.txt"),
                entry: file(4, &[0]),
            },
            TocChange::PutFile {
                path: path("docs/new.txt"),
                entry: file(5, &[1]),
            },
            TocChange::RemoveDirectory { path: path("docs") },
            TocChange::PutDirectory { path: path("docs") },
            TocChange::PutFile {
                path: path("docs/final.txt"),
                entry: file(5, &[2]),
            },
        ])
        .unwrap();

        assert_eq!(toc.len(), 2);
        assert_eq!(toc.get(&path("docs")), Some(&TocEntry::Directory));
        assert_eq!(
            toc.get(&path("docs/final.txt")),
            Some(&TocEntry::File(file(5, &[2])))
        );
        assert!(toc.get(&path("docs/old.txt")).is_none());
        assert!(toc.get(&path("docs/new.txt")).is_none());
    }

    #[test]
    fn removals_of_missing_paths_are_noops_and_file_replacement_is_ordered() {
        let mut toc = Toc::default();
        toc.apply_commit(&[
            TocChange::RemoveFile {
                path: path("missing"),
            },
            TocChange::RemoveDirectory {
                path: path("missing-dir"),
            },
            TocChange::PutFile {
                path: path("item"),
                entry: file(2, &[0]),
            },
            TocChange::PutFile {
                path: path("item"),
                entry: file(3, &[1]),
            },
        ])
        .unwrap();
        assert_eq!(toc.len(), 1);
        assert_eq!(toc.get(&path("item")), Some(&TocEntry::File(file(3, &[1]))));
    }

    #[test]
    fn rejects_invalid_file_references_and_file_directory_conflicts_atomically() {
        assert_eq!(FileEntry::new(0, vec![1]), Err(TocError::InvalidFileBlocks));
        assert_eq!(FileEntry::new(1, vec![]), Err(TocError::InvalidFileBlocks));
        assert_eq!(
            FileEntry::new(2, vec![1, 1]),
            Err(TocError::InvalidFileBlocks)
        );

        let mut toc = Toc::default();
        toc.apply_commit(&[TocChange::PutFile {
            path: path("parent"),
            entry: file(1, &[0]),
        }])
        .unwrap();
        let before = toc.clone();
        assert_eq!(
            toc.apply_commit(&[TocChange::PutFile {
                path: path("parent/child"),
                entry: file(1, &[1]),
            }]),
            Err(TocError::FileAncestor(path("parent")))
        );
        assert_eq!(toc, before);

        let mut toc = Toc::default();
        toc.apply_commit(&[
            TocChange::PutDirectory {
                path: path("folder"),
            },
            TocChange::PutFile {
                path: path("folder/child"),
                entry: file(1, &[0]),
            },
        ])
        .unwrap();
        let before = toc.clone();
        assert_eq!(
            toc.apply_commit(&[TocChange::PutFile {
                path: path("folder"),
                entry: file(1, &[1]),
            }]),
            Err(TocError::FileHasDescendants(path("folder")))
        );
        assert_eq!(toc, before);
    }

    #[test]
    fn commit_changes_validate_references_atomically() {
        let mut toc = Toc::default();
        let available_blocks = HashSet::from([3, 7]);
        let changes = [
            TocChange::PutFile {
                path: path("valid.bin"),
                entry: file(2, &[3, 7]),
            },
            TocChange::PutFile {
                path: path("missing.bin"),
                entry: file(1, &[9]),
            },
        ];

        assert_eq!(
            toc.apply_commit_with_available_blocks(&changes, &available_blocks),
            Err(TocError::MissingBlockReference {
                path: path("missing.bin"),
                block_index: 9,
            })
        );
        assert!(toc.is_empty());

        toc.apply_commit_with_available_blocks(&changes[..1], &available_blocks)
            .unwrap();
        assert_eq!(toc.len(), 1);
    }

    #[test]
    fn validating_new_changes_allows_references_to_prior_commits() {
        let mut toc = Toc::default();
        toc.apply_commit(&[TocChange::PutFile {
            path: path("existing.bin"),
            entry: file(1, &[2]),
        }])
        .unwrap();

        toc.apply_commit_with_available_blocks(
            &[TocChange::PutFile {
                path: path("new.bin"),
                entry: file(1, &[5]),
            }],
            &HashSet::from([5]),
        )
        .unwrap();
        assert_eq!(toc.len(), 2);
    }

    #[test]
    fn removing_a_path_with_wrong_kind_is_rejected() {
        let mut toc = Toc::default();
        toc.apply_commit(&[TocChange::PutDirectory {
            path: path("folder"),
        }])
        .unwrap();
        assert_eq!(
            toc.apply_commit(&[TocChange::RemoveFile {
                path: path("folder"),
            }]),
            Err(TocError::WrongEntryKind(path("folder")))
        );
    }
}
