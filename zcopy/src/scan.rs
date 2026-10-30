//! File-tree scanning with `scandir` and manifest comparison.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::net::Res;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Meta {
    pub size: u64,
    /// Modification time in whole seconds since the Unix epoch.
    pub mtime: i64,
    pub dir: bool,
}

pub type Tree = BTreeMap<String, Meta>;

/// One manifest record: path, size, mtime in seconds, is-directory.
pub type Entry = (String, u64, i64, bool);

pub fn mtime_secs(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() as i64,
        Err(error) => -(error.duration().as_secs() as i64),
    }
}

pub fn system_time(secs: i64) -> SystemTime {
    if secs >= 0 {
        UNIX_EPOCH + Duration::from_secs(secs as u64)
    } else {
        UNIX_EPOCH - Duration::from_secs(secs.unsigned_abs())
    }
}

/// Scans `root` recursively. Paths use `/` separators and are relative to
/// `root`. Symlinks, special files, and top-level names in `skip` are ignored.
pub fn scan_tree(root: &Path, skip: &[&str]) -> Res<Tree> {
    let mut scanner = scandir::Scandir::new(root, None)
        .map_err(|error| format!("cannot scan {}: {error}", root.display()))?
        .return_type(scandir::ReturnType::Ext);
    let results = scanner
        .collect()
        .map_err(|error| format!("cannot scan {}: {error}", root.display()))?;
    // scandir reports the root itself first, by its own name, and all other
    // paths relative to the root.
    let root_name = root
        .canonicalize()?
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut tree = Tree::new();
    for (index, result) in results.results.iter().enumerate() {
        let Some(entry) = result.ext() else { continue };
        if entry.is_symlink || !(entry.is_dir || entry.is_file) {
            continue;
        }
        let path = entry.path.replace('\\', "/");
        let relative = path.trim_matches('/');
        if relative.is_empty() || (index == 0 && entry.is_dir && relative == root_name) {
            continue;
        }
        if skip
            .iter()
            .any(|name| relative.split('/').next() == Some(name))
        {
            continue;
        }
        tree.insert(
            relative.to_owned(),
            Meta {
                size: if entry.is_dir { 0 } else { entry.st_size },
                mtime: entry.st_mtime.map(mtime_secs).unwrap_or(0),
                dir: entry.is_dir,
            },
        );
    }
    Ok(tree)
}

pub fn to_entries(tree: &Tree) -> Vec<Entry> {
    tree.iter()
        .map(|(path, meta)| (path.clone(), meta.size, meta.mtime, meta.dir))
        .collect()
}

/// Directories without any child entry.
pub fn empty_dirs(tree: &Tree) -> Vec<String> {
    let mut parents = BTreeSet::new();
    for path in tree.keys() {
        if let Some((parent, _)) = path.rsplit_once('/') {
            parents.insert(parent);
        }
    }
    tree.iter()
        .filter(|(path, meta)| meta.dir && !parents.contains(path.as_str()))
        .map(|(path, _)| path.clone())
        .collect()
}

/// Paths the remote side lacks or holds in an older version.
pub struct Plan {
    pub files: Vec<String>,
    pub dirs: Vec<String>,
    pub skipped: u64,
}

pub fn plan_transfer(local: &Tree, remote: &Tree) -> Plan {
    let mut plan = Plan {
        files: Vec::new(),
        dirs: Vec::new(),
        skipped: 0,
    };
    for (path, meta) in local.iter().filter(|(_, meta)| !meta.dir) {
        match remote.get(path) {
            Some(existing) if !existing.dir && meta.mtime <= existing.mtime => plan.skipped += 1,
            _ => plan.files.push(path.clone()),
        }
    }
    for path in empty_dirs(local) {
        if !remote.contains_key(&path) {
            plan.dirs.push(path);
        }
    }
    plan
}

/// Rejects absolute, parent-relative, and backslash paths from the peer.
pub fn is_safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains('\0')
        && !path.split('/').any(|part| part.is_empty() || part == "..")
        && !path.contains(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(mtime: i64) -> Meta {
        Meta {
            size: 1,
            mtime,
            dir: false,
        }
    }

    fn dir() -> Meta {
        Meta {
            size: 0,
            mtime: 0,
            dir: true,
        }
    }

    #[test]
    fn plans_missing_and_newer_files_and_empty_dirs() {
        let local = Tree::from([
            ("same".to_owned(), file(10)),
            ("newer".to_owned(), file(20)),
            ("older".to_owned(), file(5)),
            ("missing".to_owned(), file(1)),
            ("d".to_owned(), dir()),
            ("d/f".to_owned(), file(1)),
            ("empty".to_owned(), dir()),
        ]);
        let remote = Tree::from([
            ("same".to_owned(), file(10)),
            ("newer".to_owned(), file(10)),
            ("older".to_owned(), file(9)),
        ]);
        let plan = plan_transfer(&local, &remote);
        assert_eq!(plan.files, ["d/f", "missing", "newer"]);
        assert_eq!(plan.dirs, ["empty"]);
        assert_eq!(plan.skipped, 2);
    }

    #[test]
    fn rejects_unsafe_paths() {
        assert!(is_safe_relative("a/b.txt"));
        for path in ["", "/a", "../a", "a/../b", "a\\b", "a//b", "C:/x"] {
            assert!(!is_safe_relative(path), "{path}");
        }
    }
}
