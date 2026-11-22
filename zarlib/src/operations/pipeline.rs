use super::common::*;
use super::filters::FilterSet;
use crate::{
    ArchivePath, BlockFrame, CommitHeader, ContentAead, DeviceKind, EntryMetadata, FileEntry,
    TocChange,
};
#[cfg(not(target_os = "wasi"))]
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
#[cfg(not(target_os = "wasi"))]
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use xxhash_rust::xxh64::Xxh64;
use zeroize::Zeroizing;

pub fn archive_prefix(input: &Path, cwd: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let relative = if input.is_absolute() {
        input
            .strip_prefix(cwd)
            .unwrap_or_else(|_| input.file_name().map(Path::new).unwrap_or(input))
    } else {
        input
    };
    let mut components = Vec::new();
    for component in relative.components() {
        match component {
            std::path::Component::Normal(value) => {
                components.push(value.to_str().ok_or("input path is not valid UTF-8")?)
            }
            std::path::Component::Prefix(_)
            | std::path::Component::RootDir
            | std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                return Err("input paths must not contain '..'".into());
            }
        }
    }
    Ok(components.join("/"))
}

pub struct ScanPolicy {
    pub strict: bool,
    pub include_special_files: bool,
    pub verbose: bool,
    pub warnings: bool,
    pub warning_messages: Vec<String>,
}

/// A single entry collected from the file tree by the `scandir` walker.
#[derive(Clone)]
pub struct TreeEntry {
    pub archive_path: String,
    pub source_path: PathBuf,
    pub kind: TreeEntryKind,
    pub size: u64,
    pub modified_unix_millis: Option<i64>,
    pub created_unix_millis: Option<i64>,
    pub accessed_unix_millis: Option<i64>,
}

#[derive(Clone, Debug)]
#[cfg_attr(not(unix), allow(dead_code))]
pub enum TreeEntryKind {
    Directory,
    File,
    Symlink {
        target: String,
    },
    NamedPipe,
    Device {
        kind: DeviceKind,
        major: u32,
        minor: u32,
    },
    Unsupported,
}

pub fn filter_tree_entries(entries: Vec<TreeEntry>, filters: &FilterSet) -> Vec<TreeEntry> {
    let selected_paths: Vec<String> = entries
        .iter()
        .filter(|entry| {
            !matches!(entry.kind, TreeEntryKind::Directory)
                && filters.includes(&entry.archive_path, false)
        })
        .map(|entry| entry.archive_path.clone())
        .collect();
    let mut required_directories = std::collections::BTreeSet::new();
    for path in selected_paths {
        for (index, _) in path.match_indices('/') {
            required_directories.insert(path[..index].to_owned());
        }
    }

    entries
        .into_iter()
        .filter(|entry| match entry.kind {
            TreeEntryKind::Directory => {
                filters.includes(&entry.archive_path, true)
                    || required_directories.contains(&entry.archive_path)
            }
            _ => filters.includes(&entry.archive_path, false),
        })
        .collect()
}

pub fn summarize_tree_entries(
    entries: &[TreeEntry],
) -> Result<InputSummary, Box<dyn std::error::Error>> {
    let mut summary = InputSummary::default();
    for entry in entries {
        match entry.kind {
            TreeEntryKind::Directory => summary.directories += 1,
            TreeEntryKind::File => {
                summary.files += 1;
                summary.total_bytes = summary
                    .total_bytes
                    .checked_add(entry.size)
                    .ok_or("input size estimate overflow")?;
                if entry.size == 0 {
                    summary.empty_files += 1;
                } else if entry.size <= 8 {
                    summary.tiny_files += 1;
                } else if entry.size < 8 * 1024 * 1024 {
                    summary.small_files += 1;
                } else {
                    summary.big_files += 1;
                }
            }
            TreeEntryKind::Symlink { .. } => summary.symlinks += 1,
            TreeEntryKind::NamedPipe => summary.named_pipes += 1,
            TreeEntryKind::Device { .. } => summary.devices += 1,
            TreeEntryKind::Unsupported => summary.other_special += 1,
        }
    }
    Ok(summary)
}

/// Walks the file tree rooted at `source` and returns a flat, sorted list of
/// [`TreeEntry`] items. The `prefix` is the archive-relative path for the root
/// (empty string for the working directory).
///
/// For a single file (non-directory), a single entry is returned directly.
#[cfg(not(target_os = "wasi"))]
pub fn scan_tree(
    source: &Path,
    prefix: &str,
) -> Result<Vec<TreeEntry>, Box<dyn std::error::Error>> {
    let metadata = std::fs::symlink_metadata(source)?;
    if !metadata.is_dir() {
        let archive_path = if prefix.is_empty() {
            source
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or("input path has no filename")?
                .to_owned()
        } else {
            prefix.to_owned()
        };
        let kind = classify_entry(source, &metadata)?;
        return Ok(vec![TreeEntry {
            archive_path,
            source_path: source.to_owned(),
            kind,
            size: metadata.len(),
            modified_unix_millis: modified_unix_millis(&metadata),
            created_unix_millis: created_unix_millis(&metadata),
            accessed_unix_millis: accessed_unix_millis(&metadata),
        }]);
    }

    let mut entries = Vec::new();

    let archive_path = if prefix.is_empty() {
        String::new()
    } else {
        prefix.to_owned()
    };
    if !archive_path.is_empty() {
        entries.push(TreeEntry {
            archive_path: archive_path.clone(),
            source_path: source.to_owned(),
            kind: TreeEntryKind::Directory,
            size: 0,
            modified_unix_millis: modified_unix_millis(&metadata),
            created_unix_millis: created_unix_millis(&metadata),
            accessed_unix_millis: accessed_unix_millis(&metadata),
        });
    }

    let mut scandir = scandir::Scandir::new(source, None)?
        .sorted(true)
        .return_type(scandir::ReturnType::Ext);
    let results = scandir.collect()?;
    #[cfg(windows)]
    let root_identity =
        jwalk_meta::DirEntry::<((), ())>::from_path(0, source, true, true, false, None)
            .ok()
            .and_then(|entry| entry.metadata_ext);

    for result in &results.results {
        if let Some(ext) = result.ext() {
            #[cfg(unix)]
            if is_scanned_root_entry(ext, &metadata) {
                continue;
            }
            #[cfg(windows)]
            if is_scanned_root_entry(ext, root_identity.as_ref()) {
                continue;
            }
            let normalized_path = ext.path.trim_matches(['/', '\\']);
            let scanned_path = Path::new(normalized_path);
            let relative_components = scanned_path
                .components()
                .filter_map(|component| match component {
                    std::path::Component::Normal(name) => {
                        Some(name.to_str().ok_or("file name is not valid UTF-8"))
                    }
                    std::path::Component::CurDir => None,
                    _ => Some(Err("scanned path escaped input root")),
                })
                .collect::<Result<Vec<_>, _>>()?;
            let relative = relative_components.join("/");
            if relative.is_empty() {
                continue;
            }
            let source_path = source.join(&relative);
            let metadata = std::fs::symlink_metadata(&source_path)?;
            let archive_path = if prefix.is_empty() {
                relative
            } else {
                format!("{prefix}/{relative}")
            };

            let kind = if ext.is_symlink {
                let target = std::fs::read_link(&source_path)
                    .map_err(|e| format!("cannot read symlink {}: {e}", source_path.display()))?;
                let target = target
                    .to_str()
                    .ok_or("symbolic-link target is not valid UTF-8")?
                    .to_owned();
                if target.is_empty() || target.as_bytes().contains(&0) {
                    return Err("symbolic-link target is empty or contains NUL".into());
                }
                TreeEntryKind::Symlink { target }
            } else if ext.is_dir {
                TreeEntryKind::Directory
            } else if ext.is_file {
                TreeEntryKind::File
            } else {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::FileTypeExt;
                    let file_type = metadata.file_type();
                    if file_type.is_fifo() {
                        TreeEntryKind::NamedPipe
                    } else if file_type.is_char_device() || file_type.is_block_device() {
                        use std::os::unix::fs::MetadataExt;
                        let device_kind = if file_type.is_char_device() {
                            DeviceKind::Character
                        } else {
                            DeviceKind::Block
                        };
                        TreeEntryKind::Device {
                            kind: device_kind,
                            major: {
                                #[allow(clippy::useless_conversion)]
                                let rdev = metadata.rdev().try_into().unwrap();
                                libc::major(rdev) as u32
                            },
                            minor: {
                                #[allow(clippy::useless_conversion)]
                                let rdev = metadata.rdev().try_into().unwrap();
                                libc::minor(rdev) as u32
                            },
                        }
                    } else {
                        TreeEntryKind::Unsupported
                    }
                }
                #[cfg(not(unix))]
                {
                    TreeEntryKind::Unsupported
                }
            };

            entries.push(TreeEntry {
                archive_path,
                source_path,
                kind,
                size: ext.st_size,
                modified_unix_millis: modified_unix_millis(&metadata),
                created_unix_millis: created_unix_millis(&metadata),
                accessed_unix_millis: accessed_unix_millis(&metadata),
            });
        }
    }
    Ok(entries)
}

#[cfg(all(not(target_os = "wasi"), unix))]
fn is_scanned_root_entry(entry: &scandir::DirEntryExt, root_metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;

    entry.is_dir && entry.st_dev == root_metadata.dev() && entry.st_ino == root_metadata.ino()
}

#[cfg(all(not(target_os = "wasi"), windows))]
fn is_scanned_root_entry(
    entry: &scandir::DirEntryExt,
    root_identity: Option<&jwalk_meta::MetaDataExt>,
) -> bool {
    entry.is_dir
        && root_identity.is_some_and(|identity| {
            identity
                .volume_serial_number
                .is_some_and(|device| entry.st_dev == u64::from(device))
                && identity
                    .file_index
                    .is_some_and(|inode| entry.st_ino == inode)
        })
}

#[cfg(target_os = "wasi")]
pub fn scan_tree(
    source: &Path,
    prefix: &str,
) -> Result<Vec<TreeEntry>, Box<dyn std::error::Error>> {
    let metadata = std::fs::symlink_metadata(source)?;
    if !metadata.is_dir() {
        let archive_path = if prefix.is_empty() {
            source
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("input path has no filename")?
                .to_owned()
        } else {
            prefix.to_owned()
        };
        return Ok(vec![TreeEntry {
            archive_path,
            source_path: source.to_owned(),
            kind: classify_entry(source, &metadata)?,
            size: metadata.len(),
            modified_unix_millis: modified_unix_millis(&metadata),
            created_unix_millis: created_unix_millis(&metadata),
            accessed_unix_millis: accessed_unix_millis(&metadata),
        }]);
    }

    let mut entries = Vec::new();
    if !prefix.is_empty() {
        entries.push(TreeEntry {
            archive_path: prefix.to_owned(),
            source_path: source.to_owned(),
            kind: TreeEntryKind::Directory,
            size: 0,
            modified_unix_millis: modified_unix_millis(&metadata),
            created_unix_millis: created_unix_millis(&metadata),
            accessed_unix_millis: accessed_unix_millis(&metadata),
        });
    }
    scan_wasi_directory(source, prefix, &mut entries)?;
    entries.sort_by(|left, right| left.archive_path.cmp(&right.archive_path));
    Ok(entries)
}

#[cfg(target_os = "wasi")]
fn scan_wasi_directory(
    directory: &Path,
    prefix: &str,
    entries: &mut Vec<TreeEntry>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut children = std::fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    children.sort();

    for path in children {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("file name is not valid UTF-8")?;
        let relative = if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix}/{name}")
        };
        let metadata = std::fs::symlink_metadata(&path)?;
        let kind = classify_entry(&path, &metadata)?;
        entries.push(TreeEntry {
            archive_path: relative.clone(),
            source_path: path.clone(),
            kind: kind.clone(),
            size: metadata.len(),
            modified_unix_millis: modified_unix_millis(&metadata),
            created_unix_millis: created_unix_millis(&metadata),
            accessed_unix_millis: accessed_unix_millis(&metadata),
        });
        if matches!(kind, TreeEntryKind::Directory) {
            scan_wasi_directory(&path, &relative, entries)?;
        }
    }
    Ok(())
}

fn classify_entry(
    source: &Path,
    metadata: &std::fs::Metadata,
) -> Result<TreeEntryKind, Box<dyn std::error::Error>> {
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        let target = std::fs::read_link(source)
            .map_err(|e| format!("cannot read symlink {}: {e}", source.display()))?;
        let target = target
            .to_str()
            .ok_or("symbolic-link target is not valid UTF-8")?
            .to_owned();
        if target.is_empty() || target.as_bytes().contains(&0) {
            return Err("symbolic-link target is empty or contains NUL".into());
        }
        Ok(TreeEntryKind::Symlink { target })
    } else if file_type.is_dir() {
        Ok(TreeEntryKind::Directory)
    } else if file_type.is_file() {
        Ok(TreeEntryKind::File)
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt;
            if file_type.is_fifo() {
                Ok(TreeEntryKind::NamedPipe)
            } else if file_type.is_char_device() || file_type.is_block_device() {
                use std::os::unix::fs::MetadataExt;
                let device_kind = if file_type.is_char_device() {
                    DeviceKind::Character
                } else {
                    DeviceKind::Block
                };
                Ok(TreeEntryKind::Device {
                    kind: device_kind,
                    major: {
                        #[allow(clippy::useless_conversion)]
                        let rdev = metadata.rdev().try_into().unwrap();
                        libc::major(rdev) as u32
                    },
                    minor: {
                        #[allow(clippy::useless_conversion)]
                        let rdev = metadata.rdev().try_into().unwrap();
                        libc::minor(rdev) as u32
                    },
                })
            } else {
                Ok(TreeEntryKind::Unsupported)
            }
        }
        #[cfg(not(unix))]
        {
            Ok(TreeEntryKind::Unsupported)
        }
    }
}

#[derive(Debug, Clone)]
pub struct DedupCandidate {
    pub checksum: Option<u64>,
    pub tiny_contents: Vec<u8>,
    pub block_indices: Vec<u32>,
}

pub fn duplicate_blocks(
    candidates_by_size: &HashMap<u64, Vec<DedupCandidate>>,
    size: u64,
    checksum: Option<u64>,
    tiny_contents: &[u8],
) -> Option<Vec<u32>> {
    candidates_by_size.get(&size).and_then(|candidates| {
        candidates
            .iter()
            .find(|candidate| match checksum {
                Some(checksum) => candidate.checksum == Some(checksum),
                None => candidate.checksum.is_none() && candidate.tiny_contents == tiny_contents,
            })
            .map(|candidate| candidate.block_indices.clone())
    })
}

pub struct ScanContext<'a> {
    pub changes: &'a mut Vec<TocChange>,
    pub data: &'a mut dyn WriteSeek,
    pub block_index: &'a mut Vec<crate::HeaderBlockIndexEntry>,
    pub next_block_index: &'a mut u32,
    pub block_sizes: &'a mut HashMap<u32, u64>,
    pub seen_paths: &'a mut std::collections::BTreeSet<ArchivePath>,
    pub entry_metadata: &'a mut Vec<EntryMetadata>,
    pub save_metadata: bool,
    pub policy: &'a mut ScanPolicy,
    pub options: EncodingOptions,
    pub compression_workers: usize,
    pub progress: ProgressReporter<'a>,
    pub action: &'static str,
    pub encryption: Option<EncryptionContext>,
    pub historical_duplicates: Option<&'a HashMap<PathBuf, FileEntry>>,
}

fn write_indexed_block(
    data: &mut dyn WriteSeek,
    block_index: &mut Vec<crate::HeaderBlockIndexEntry>,
    frame: &BlockFrame,
) -> Result<(), Box<dyn std::error::Error>> {
    let offset = match block_index.last() {
        Some(entry) => entry
            .offset
            .checked_add(entry.encoded_size)
            .ok_or("block offset overflow")?,
        None => 0,
    };
    let encoded_size = u64::try_from(frame.encoded_size()?)?;
    frame.write_to(data)?;
    block_index.push(crate::HeaderBlockIndexEntry {
        index: frame.block_index,
        offset,
        encoded_size,
    });
    Ok(())
}

#[cfg(not(target_os = "wasi"))]
fn skip_permission_denied_file(
    entry: &TreeEntry,
    error: &io::Error,
    strict: bool,
    warnings: &std::sync::Mutex<Vec<String>>,
) -> bool {
    if error.kind() != io::ErrorKind::PermissionDenied || strict {
        return false;
    }

    warnings.lock().unwrap().push(format!(
        "warning: W_PERMISSION_DENIED: skipping unreadable file {}",
        entry.source_path.display()
    ));
    true
}

pub struct EncryptionContext {
    pub dek: Zeroizing<[u8; crate::DEK_SIZE]>,
    pub commit_header: CommitHeader,
    pub algorithm: ContentAead,
}

impl EncryptionContext {
    pub fn new(
        dek: Zeroizing<[u8; crate::DEK_SIZE]>,
        commit_header: CommitHeader,
        algorithm: ContentAead,
    ) -> Self {
        Self {
            dek,
            commit_header,
            algorithm,
        }
    }
}

pub trait WriteSeek: Write + Seek {
    #[allow(dead_code)]
    fn set_len(&mut self, len: u64) -> io::Result<()>;
}

impl WriteSeek for File {
    #[allow(dead_code)]
    fn set_len(&mut self, len: u64) -> io::Result<()> {
        File::set_len(self, len)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HardLinkId {
    #[cfg(unix)]
    Inode { dev: u64, ino: u64 },
    #[cfg(not(unix))]
    Canonical(PathBuf),
}

pub fn get_hard_link_id(
    #[allow(unused)] path: &Path,
    #[allow(unused)] metadata: &std::fs::Metadata,
) -> Option<HardLinkId> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(HardLinkId::Inode {
            dev: metadata.dev(),
            ino: metadata.ino(),
        })
    }
    #[cfg(not(unix))]
    {
        std::fs::canonicalize(path).ok().map(HardLinkId::Canonical)
    }
}

pub struct PendingHardLink {
    pub archive_path: ArchivePath,
    pub target_path: ArchivePath,
    pub source_path: PathBuf,
    pub modified_unix_millis: Option<i64>,
    pub created_unix_millis: Option<i64>,
    pub accessed_unix_millis: Option<i64>,
}

pub fn process_tree_entries(
    entries: &[TreeEntry],
    context: &mut ScanContext<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut regular_files: Vec<&TreeEntry> = Vec::new();
    let mut pending_hard_links: Vec<PendingHardLink> = Vec::new();
    let mut seen_hard_links: HashMap<HardLinkId, ArchivePath> = HashMap::new();
    let mut entry_by_path: HashMap<ArchivePath, FileEntry> = HashMap::new();

    for entry in entries {
        context.progress.check_cancelled()?;
        if entry.archive_path.contains('\\') {
            if context.policy.strict {
                return Err(Box::new(StrictModeWarning));
            }
            context.policy.warning_messages.push(format!(
                "warning: W_INVALID_ARCHIVE_PATH: skipping path containing a backslash: {}",
                entry.source_path.display()
            ));
            context.policy.warnings = true;
            continue;
        }

        match &entry.kind {
            TreeEntryKind::Directory => {
                if !entry.archive_path.is_empty() {
                    process_directory_entry(entry, context)?;
                }
            }
            TreeEntryKind::Symlink { target } => {
                process_symlink_entry(entry, target, context)?;
            }
            TreeEntryKind::NamedPipe => {
                if context.policy.include_special_files {
                    process_named_pipe_entry(entry, context)?;
                } else {
                    warn_skipped_special_file(entry, context)?;
                }
            }
            TreeEntryKind::Device { kind, major, minor } => {
                if context.policy.include_special_files {
                    process_device_entry(entry, *kind, *major, *minor, context)?;
                } else {
                    warn_skipped_special_file(entry, context)?;
                }
            }
            TreeEntryKind::Unsupported => {
                if context.policy.strict {
                    return Err(Box::new(StrictModeWarning));
                }
                context.policy.warning_messages.push(format!(
                    "warning: W_UNSUPPORTED_TYPE: skipping special file {}",
                    entry.source_path.display()
                ));
                context.policy.warnings = true;
            }
            TreeEntryKind::File => {
                let path = ArchivePath::new(entry.archive_path.clone())?;
                push_unique_path(path.clone(), context.seen_paths)?;

                let metadata = std::fs::symlink_metadata(&entry.source_path)?;

                if let Some(id) = get_hard_link_id(&entry.source_path, &metadata) {
                    if let Some(target_path) = seen_hard_links.get(&id) {
                        pending_hard_links.push(PendingHardLink {
                            archive_path: path,
                            target_path: target_path.clone(),
                            source_path: entry.source_path.clone(),
                            modified_unix_millis: entry.modified_unix_millis,
                            created_unix_millis: entry.created_unix_millis,
                            accessed_unix_millis: entry.accessed_unix_millis,
                        });
                        continue;
                    }
                    seen_hard_links.insert(id, path.clone());
                }

                if entry.size == 0 {
                    let file_entry = FileEntry::new(0, vec![])?;
                    entry_by_path.insert(path.clone(), file_entry.clone());
                    context.changes.push(TocChange::PutFile {
                        path: path.clone(),
                        entry: file_entry,
                    });
                    context.entry_metadata.push(capture_entry_metadata(
                        &path,
                        &metadata,
                        context.save_metadata,
                    )?);
                    continue;
                }

                if let Some(file_entry) = context
                    .historical_duplicates
                    .and_then(|entries| entries.get(&entry.source_path))
                {
                    entry_by_path.insert(path.clone(), file_entry.clone());
                    context.changes.push(TocChange::PutFile {
                        path: path.clone(),
                        entry: file_entry.clone(),
                    });
                    context.entry_metadata.push(if context.save_metadata {
                        capture_entry_metadata(&path, &metadata, true)?
                    } else {
                        entry_date_metadata(
                            &path,
                            entry.modified_unix_millis,
                            entry.created_unix_millis,
                            entry.accessed_unix_millis,
                        )
                    });
                    context.progress.inc(entry.size);
                    continue;
                }

                regular_files.push(entry);
            }
        }
    }

    if !regular_files.is_empty() {
        process_file_entries_parallel(&regular_files, context, &mut entry_by_path)?;
    }

    for hard_link in pending_hard_links {
        let target_entry = entry_by_path
            .get(&hard_link.target_path)
            .ok_or_else(|| format!("hard link target {} not found", hard_link.target_path))?
            .clone();
        context.changes.push(TocChange::PutFile {
            path: hard_link.archive_path.clone(),
            entry: target_entry,
        });
        let metadata = if context.save_metadata {
            Some(std::fs::symlink_metadata(&hard_link.source_path)?)
        } else {
            None
        };
        context
            .entry_metadata
            .push(if let Some(metadata) = metadata {
                capture_entry_metadata(&hard_link.archive_path, &metadata, true)?
            } else {
                entry_date_metadata(
                    &hard_link.archive_path,
                    hard_link.modified_unix_millis,
                    hard_link.created_unix_millis,
                    hard_link.accessed_unix_millis,
                )
            });
    }

    Ok(())
}

fn warn_skipped_special_file(
    entry: &TreeEntry,
    context: &mut ScanContext<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    if context.policy.strict {
        return Err(Box::new(StrictModeWarning));
    }
    context.policy.warning_messages.push(format!(
        "warning: W_SPECIAL_FILE_SKIPPED: skipping special file {}",
        entry.source_path.display()
    ));
    context.policy.warnings = true;
    Ok(())
}

pub fn process_directory_entry(
    entry: &TreeEntry,
    context: &mut ScanContext<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = ArchivePath::new(entry.archive_path.clone())?;
    let metadata = if context.save_metadata {
        Some(std::fs::symlink_metadata(&entry.source_path)?)
    } else {
        None
    };
    context
        .entry_metadata
        .push(if let Some(metadata) = metadata {
            capture_entry_metadata(&path, &metadata, true)?
        } else {
            entry_date_metadata(
                &path,
                entry.modified_unix_millis,
                entry.created_unix_millis,
                entry.accessed_unix_millis,
            )
        });
    push_unique_path(
        ArchivePath::new(entry.archive_path.clone())?,
        context.seen_paths,
    )?;
    context.changes.push(TocChange::PutDirectory {
        path: ArchivePath::new(entry.archive_path.clone())?,
    });
    Ok(())
}

pub fn process_symlink_entry(
    entry: &TreeEntry,
    target: &str,
    context: &mut ScanContext<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    if context.policy.strict {
        return Err(Box::new(StrictModeWarning));
    }
    if context.policy.verbose {
        context.policy.warning_messages.push(format!(
            "warning: W_SYMLINK_STORED: storing symbolic link {} (restoration requires --symlinks)",
            entry.source_path.display()
        ));
        context.policy.warnings = true;
    }
    let path = ArchivePath::new(entry.archive_path.clone())?;
    push_unique_path(path.clone(), context.seen_paths)?;
    let metadata = if context.save_metadata {
        Some(std::fs::symlink_metadata(&entry.source_path)?)
    } else {
        None
    };
    context
        .entry_metadata
        .push(if let Some(metadata) = metadata {
            capture_entry_metadata(&path, &metadata, true)?
        } else {
            entry_date_metadata(
                &path,
                entry.modified_unix_millis,
                entry.created_unix_millis,
                entry.accessed_unix_millis,
            )
        });
    context.changes.push(TocChange::PutSymlink {
        path,
        target: target.to_owned(),
    });
    Ok(())
}

pub fn process_named_pipe_entry(
    entry: &TreeEntry,
    context: &mut ScanContext<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    context.policy.warning_messages.push(format!(
        "warning: W_SPECIAL_FILE: storing special file {}",
        entry.source_path.display()
    ));
    context.policy.warnings = true;
    let path = ArchivePath::new(entry.archive_path.clone())?;
    push_unique_path(path.clone(), context.seen_paths)?;
    let metadata = if context.save_metadata {
        Some(std::fs::symlink_metadata(&entry.source_path)?)
    } else {
        None
    };
    context
        .entry_metadata
        .push(if let Some(metadata) = metadata {
            capture_entry_metadata(&path, &metadata, true)?
        } else {
            entry_date_metadata(
                &path,
                entry.modified_unix_millis,
                entry.created_unix_millis,
                entry.accessed_unix_millis,
            )
        });
    context.changes.push(TocChange::PutNamedPipe { path });
    Ok(())
}

pub fn process_device_entry(
    entry: &TreeEntry,
    kind: DeviceKind,
    major: u32,
    minor: u32,
    context: &mut ScanContext<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    context.policy.warning_messages.push(format!(
        "warning: W_SPECIAL_FILE: storing special file {}",
        entry.source_path.display()
    ));
    context.policy.warnings = true;
    let path = ArchivePath::new(entry.archive_path.clone())?;
    push_unique_path(path.clone(), context.seen_paths)?;
    let metadata = if context.save_metadata {
        Some(std::fs::symlink_metadata(&entry.source_path)?)
    } else {
        None
    };
    context
        .entry_metadata
        .push(if let Some(metadata) = metadata {
            capture_entry_metadata(&path, &metadata, true)?
        } else {
            entry_date_metadata(
                &path,
                entry.modified_unix_millis,
                entry.created_unix_millis,
                entry.accessed_unix_millis,
            )
        });
    context.changes.push(TocChange::PutDevice {
        path,
        kind,
        major,
        minor,
    });
    Ok(())
}

pub struct BlockWorkItem {
    pub block_index: u32,
    pub data: Vec<u8>,
    pub skip_compression: bool,
}

#[derive(Debug)]
struct SmallFileCandidate {
    checksum: u64,
    block_index: u32,
    offset_in_block: u32,
}

#[derive(Debug)]
pub(super) struct SmallFilePacker {
    block_size: usize,
    data: Vec<u8>,
    block_index: Option<u32>,
    skip_compression: bool,
    candidates_by_size: HashMap<u64, Vec<SmallFileCandidate>>,
}

impl SmallFilePacker {
    pub(super) fn new(block_size: usize) -> Self {
        Self {
            block_size,
            data: Vec::with_capacity(block_size),
            block_index: None,
            skip_compression: true,
            candidates_by_size: HashMap::new(),
        }
    }

    pub(super) fn add(
        &mut self,
        contents: Vec<u8>,
        checksum: u64,
        skip_compression: bool,
        mut allocate_block_index: impl FnMut() -> Result<u32, String>,
    ) -> Result<(u32, u32, Option<BlockWorkItem>), String> {
        if contents.is_empty() || contents.len() >= self.block_size {
            return Err("invalid size for packed small file".to_owned());
        }
        let size = contents.len() as u64;
        if let Some(candidate) = self.candidates_by_size.get(&size).and_then(|candidates| {
            candidates
                .iter()
                .find(|candidate| candidate.checksum == checksum)
        }) {
            return Ok((candidate.block_index, candidate.offset_in_block, None));
        }

        let mut completed = None;
        if self.block_index.is_some()
            && (self.data.len().saturating_add(contents.len()) > self.block_size
                || self.skip_compression != skip_compression)
        {
            completed = self.take_block()?;
        }
        if self.block_index.is_none() {
            self.block_index = Some(allocate_block_index()?);
            self.skip_compression = skip_compression;
        } else {
            self.skip_compression &= skip_compression;
        }
        let block_index = self.block_index.ok_or("small-file block index missing")?;
        let offset_in_block =
            u32::try_from(self.data.len()).map_err(|_| "small-file block offset overflow")?;
        self.data.extend_from_slice(&contents);
        self.candidates_by_size
            .entry(size)
            .or_default()
            .push(SmallFileCandidate {
                checksum,
                block_index,
                offset_in_block,
            });
        Ok((block_index, offset_in_block, completed))
    }

    pub(super) fn take_block(&mut self) -> Result<Option<BlockWorkItem>, String> {
        if self.data.is_empty() {
            return Ok(None);
        }
        let block_index = self
            .block_index
            .take()
            .ok_or_else(|| "small-file block index missing".to_owned())?;
        Ok(Some(BlockWorkItem {
            block_index,
            data: std::mem::replace(&mut self.data, Vec::with_capacity(self.block_size)),
            skip_compression: std::mem::replace(&mut self.skip_compression, true),
        }))
    }
}

#[cfg(not(target_os = "wasi"))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WorkerPreference {
    SmallTinyFirst,
    BigFileFirst,
}

#[cfg(not(target_os = "wasi"))]
pub fn worker_loop(
    preference: WorkerPreference,
    small_rx: &crossbeam_channel::Receiver<BlockWorkItem>,
    big_rx: &crossbeam_channel::Receiver<BlockWorkItem>,
    writer_tx: &crossbeam_channel::Sender<Result<BlockFrame, String>>,
    compression_level: i32,
    encryption: Option<&EncryptionContext>,
) {
    loop {
        let item = match preference {
            WorkerPreference::SmallTinyFirst => {
                small_rx.try_recv().ok().or_else(|| big_rx.try_recv().ok())
            }
            WorkerPreference::BigFileFirst => {
                big_rx.try_recv().ok().or_else(|| small_rx.try_recv().ok())
            }
        };

        let item = match item {
            Some(item) => item,
            None => match preference {
                WorkerPreference::SmallTinyFirst => {
                    crossbeam_channel::select! {
                        recv(small_rx) -> msg => match msg {
                            Ok(item) => item,
                            Err(_) => match big_rx.recv() {
                                Ok(item) => item,
                                Err(_) => break,
                            },
                        },
                        recv(big_rx) -> msg => match msg {
                            Ok(item) => item,
                            Err(_) => match small_rx.recv() {
                                Ok(item) => item,
                                Err(_) => break,
                            },
                        },
                    }
                }
                WorkerPreference::BigFileFirst => {
                    crossbeam_channel::select! {
                        recv(big_rx) -> msg => match msg {
                            Ok(item) => item,
                            Err(_) => match small_rx.recv() {
                                Ok(item) => item,
                                Err(_) => break,
                            },
                        },
                        recv(small_rx) -> msg => match msg {
                            Ok(item) => item,
                            Err(_) => match big_rx.recv() {
                                Ok(item) => item,
                                Err(_) => break,
                            },
                        },
                    }
                }
            },
        };

        let block = encode_block(
            item.block_index,
            &item.data,
            compression_level,
            item.skip_compression,
            encryption,
        )
        .map_err(|e| e.to_string());

        if writer_tx.send(block).is_err() {
            break;
        }
    }
}

fn checksum_file(file: &mut File, progress: &ProgressReporter<'_>) -> io::Result<(u64, u64)> {
    let mut hasher = Xxh64::new(0);
    let mut buffer = vec![0u8; 128 * 1024];
    let mut total_read = 0u64;
    loop {
        progress.check_cancelled()?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        total_read = total_read
            .checked_add(count as u64)
            .ok_or_else(|| io::Error::other("input file size overflow"))?;
        progress.inc(count as u64);
    }
    Ok((total_read, hasher.digest()))
}

pub fn encode_block(
    index: u32,
    data: &[u8],
    compression_level: i32,
    skip_compression: bool,
    encryption: Option<&EncryptionContext>,
) -> Result<BlockFrame, crate::FramingError> {
    if let Some(enc) = encryption {
        if skip_compression {
            BlockFrame::from_staged_encrypted(
                &BlockFrame::from_stored(index, data)?,
                &enc.commit_header,
                &enc.dek,
                enc.algorithm,
            )
        } else {
            BlockFrame::from_uncompressed_encrypted(
                index,
                data,
                compression_level,
                &enc.commit_header,
                &enc.dek,
                enc.algorithm,
            )
        }
    } else if skip_compression {
        BlockFrame::from_stored(index, data)
    } else {
        BlockFrame::from_uncompressed(index, data, compression_level)
    }
}

#[cfg(not(target_os = "wasi"))]
pub fn process_file_entries_parallel(
    entries: &[&TreeEntry],
    context: &mut ScanContext<'_>,
    entry_by_path: &mut HashMap<ArchivePath, FileEntry>,
) -> Result<(), Box<dyn std::error::Error>> {
    let num_workers = context.compression_workers.max(1);
    let buffer_size = context.options.block_size;
    let compression_level = context.options.compression_level;

    let queue_capacity = (num_workers * 2).max(4);
    let (small_tx, small_rx) = crossbeam_channel::bounded::<BlockWorkItem>(queue_capacity);
    let (big_tx, big_rx) = crossbeam_channel::bounded::<BlockWorkItem>(queue_capacity);
    let (writer_tx, writer_rx) =
        crossbeam_channel::bounded::<Result<BlockFrame, String>>(queue_capacity);

    let work_items: Vec<(usize, &TreeEntry)> = entries.iter().copied().enumerate().rev().collect();
    let work_queue = std::sync::Arc::new(std::sync::Mutex::new(work_items));

    let dedup_map = std::sync::Arc::new(std::sync::Mutex::new(
        HashMap::<u64, Vec<DedupCandidate>>::new(),
    ));
    let small_packer =
        std::sync::Arc::new(std::sync::Mutex::new(SmallFilePacker::new(buffer_size)));

    let next_block_index = AtomicU32::new(*context.next_block_index);

    let file_results = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(
        usize,
        ArchivePath,
        FileEntry,
        Option<EntryMetadata>,
    )>::new()));
    let read_warnings = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));

    let reader_count = num_workers;
    let remaining_readers = AtomicUsize::new(reader_count);
    let encryption = context.encryption.as_ref();
    let initial_expected_index = *context.next_block_index;
    let strict = context.policy.strict;
    let priority = super::worker_priority::worker_priority();

    std::thread::scope(|scope| -> Result<(), Box<dyn std::error::Error>> {
        let small_workers = if num_workers > 1 {
            (num_workers / 2).max(1)
        } else {
            1
        };
        let big_workers = num_workers
            .saturating_sub(small_workers)
            .max(if num_workers > 1 { 1 } else { 0 });

        let mut worker_handles = Vec::with_capacity(num_workers);

        for _ in 0..small_workers {
            let s_rx = small_rx.clone();
            let b_rx = big_rx.clone();
            let w_tx = writer_tx.clone();
            worker_handles.push(scope.spawn(move || {
                if let Some(priority) = priority {
                    super::worker_priority::apply_worker_priority(priority);
                }
                worker_loop(
                    WorkerPreference::SmallTinyFirst,
                    &s_rx,
                    &b_rx,
                    &w_tx,
                    compression_level,
                    encryption,
                );
            }));
        }

        for _ in 0..big_workers {
            let s_rx = small_rx.clone();
            let b_rx = big_rx.clone();
            let w_tx = writer_tx.clone();
            worker_handles.push(scope.spawn(move || {
                if let Some(priority) = priority {
                    super::worker_priority::apply_worker_priority(priority);
                }
                worker_loop(
                    WorkerPreference::BigFileFirst,
                    &s_rx,
                    &b_rx,
                    &w_tx,
                    compression_level,
                    encryption,
                );
            }));
        }

        let mut reader_handles = Vec::with_capacity(reader_count);
        for _ in 0..reader_count {
            let wq = work_queue.clone();
            let s_tx = small_tx.clone();
            let b_tx = big_tx.clone();
            let w_tx = writer_tx.clone();
            let dm = dedup_map.clone();
            let nbi = &next_block_index;
            let fr = file_results.clone();
            let warnings = read_warnings.clone();
            let packer = small_packer.clone();
            let remaining = &remaining_readers;
            let progress = &context.progress;
            let action = context.action;
            let save_metadata = context.save_metadata;

            reader_handles.push(scope.spawn(move || {
                if let Some(priority) = priority {
                    super::worker_priority::apply_worker_priority(priority);
                }
                loop {
                    if progress.is_cancelled() {
                        let _ = w_tx.send(Err("operation cancelled".to_owned()));
                        break;
                    }
                    let next = {
                        let mut q = wq.lock().unwrap();
                        q.pop()
                    };
                    let Some((orig_index, entry)) = next else {
                        break;
                    };

                    let path = match ArchivePath::new(entry.archive_path.clone()) {
                        Ok(p) => p,
                        Err(e) => {
                            let _ = w_tx.send(Err(e.to_string()));
                            break;
                        }
                    };

                    progress.set_message(format!("{action} {}", entry.archive_path));

                    let metadata = if save_metadata {
                        match std::fs::symlink_metadata(&entry.source_path) {
                            Ok(md) => match capture_entry_metadata(&path, &md, true) {
                                Ok(m) => Some(m),
                                Err(e) => {
                                    let _ = w_tx.send(Err(e.to_string()));
                                    break;
                                }
                            },
                            Err(e) => {
                                if skip_permission_denied_file(entry, &e, strict, &warnings) {
                                    continue;
                                }
                                let _ = w_tx.send(Err(format!(
                                    "cannot read metadata for {}: {e}",
                                    entry.source_path.display()
                                )));
                                break;
                            }
                        }
                    } else {
                        Some(entry_date_metadata(
                            &path,
                            entry.modified_unix_millis,
                            entry.created_unix_millis,
                            entry.accessed_unix_millis,
                        ))
                    };

                    let skip_compression = should_skip_compression(&entry.source_path);
                    let is_big = entry.size >= buffer_size as u64;
                    let is_tiny = !is_big && entry.size <= 8;

                    if is_tiny {
                        let mut file = match File::open(&entry.source_path) {
                            Ok(f) => f,
                            Err(e) => {
                                if skip_permission_denied_file(entry, &e, strict, &warnings) {
                                    continue;
                                }
                                let _ = w_tx.send(Err(format!(
                                    "cannot read input file {}: {e}",
                                    entry.source_path.display()
                                )));
                                break;
                            }
                        };
                        let mut buf = vec![0u8; entry.size as usize];
                        if let Err(e) = file.read_exact(&mut buf) {
                            if skip_permission_denied_file(entry, &e, strict, &warnings) {
                                continue;
                            }
                            let _ = w_tx.send(Err(format!(
                                "cannot read input file {}: {e}",
                                entry.source_path.display()
                            )));
                            break;
                        }
                        progress.inc(buf.len() as u64);

                        let (is_dup, indices) = {
                            let mut map = dm.lock().unwrap();
                            if let Some(existing) = duplicate_blocks(&map, entry.size, None, &buf) {
                                (true, existing)
                            } else {
                                let idx = nbi.fetch_add(1, Ordering::SeqCst);
                                let indices = vec![idx];
                                map.entry(entry.size).or_default().push(DedupCandidate {
                                    checksum: None,
                                    tiny_contents: buf.clone(),
                                    block_indices: indices.clone(),
                                });
                                (false, indices)
                            }
                        };

                        if !is_dup {
                            let send_res = s_tx.send(BlockWorkItem {
                                block_index: indices[0],
                                data: buf,
                                skip_compression,
                            });
                            if send_res.is_err() {
                                break;
                            }
                        }

                        let file_entry = match FileEntry::new(entry.size, indices) {
                            Ok(fe) => fe,
                            Err(e) => {
                                let _ = w_tx.send(Err(e.to_string()));
                                break;
                            }
                        };

                        fr.lock()
                            .unwrap()
                            .push((orig_index, path, file_entry, metadata));
                        continue;
                    }

                    if !is_big {
                        let mut file = match File::open(&entry.source_path) {
                            Ok(f) => f,
                            Err(e) => {
                                if skip_permission_denied_file(entry, &e, strict, &warnings) {
                                    continue;
                                }
                                let _ = w_tx.send(Err(format!(
                                    "cannot read input file {}: {e}",
                                    entry.source_path.display()
                                )));
                                break;
                            }
                        };
                        let mut buf = vec![0u8; entry.size as usize];
                        if let Err(e) = file.read_exact(&mut buf) {
                            if skip_permission_denied_file(entry, &e, strict, &warnings) {
                                continue;
                            }
                            let _ = w_tx.send(Err(format!(
                                "cannot read input file {}: {e}",
                                entry.source_path.display()
                            )));
                            break;
                        }
                        progress.inc(buf.len() as u64);

                        let checksum = {
                            let mut hasher = Xxh64::new(0);
                            hasher.update(&buf);
                            hasher.digest()
                        };
                        let packed = {
                            let mut packer = packer.lock().unwrap();
                            packer.add(buf, checksum, skip_compression, || {
                                nbi.try_update(Ordering::SeqCst, Ordering::SeqCst, |current| {
                                    current.checked_add(1)
                                })
                                .map_err(|_| "archive has too many blocks".to_owned())
                            })
                        };
                        let (block_index, offset_in_block, completed) = match packed {
                            Ok(result) => result,
                            Err(error) => {
                                let _ = w_tx.send(Err(error));
                                break;
                            }
                        };
                        if let Some(completed) = completed
                            && s_tx.send(completed).is_err()
                        {
                            break;
                        }

                        let file_entry = match FileEntry::new(entry.size, vec![block_index]) {
                            Ok(fe) => fe
                                .with_block_offset(offset_in_block)
                                .with_checksum(checksum),
                            Err(e) => {
                                let _ = w_tx.send(Err(e.to_string()));
                                break;
                            }
                        };

                        fr.lock()
                            .unwrap()
                            .push((orig_index, path, file_entry, metadata));
                        continue;
                    }

                    let mut file = match File::open(&entry.source_path) {
                        Ok(f) => f,
                        Err(e) => {
                            if skip_permission_denied_file(entry, &e, strict, &warnings) {
                                continue;
                            }
                            let _ = w_tx.send(Err(format!(
                                "cannot read input file {}: {e}",
                                entry.source_path.display()
                            )));
                            break;
                        }
                    };
                    let (total_read, checksum) = match checksum_file(&mut file, progress) {
                        Ok(result) => result,
                        Err(error) => {
                            if skip_permission_denied_file(entry, &error, strict, &warnings) {
                                continue;
                            }
                            let _ = w_tx.send(Err(format!(
                                "cannot read input file {}: {error}",
                                entry.source_path.display()
                            )));
                            break;
                        }
                    };
                    if total_read != entry.size {
                        let _ = w_tx.send(Err(format!(
                            "input file changed while being archived: {}",
                            entry.source_path.display()
                        )));
                        break;
                    }

                    let (is_dup, indices) = {
                        let mut map = dm.lock().unwrap();
                        if let Some(existing) =
                            duplicate_blocks(&map, total_read, Some(checksum), &[])
                        {
                            (true, existing)
                        } else {
                            let block_count = total_read.div_ceil(buffer_size as u64);
                            let Ok(num_blocks) = u32::try_from(block_count) else {
                                let _ = w_tx.send(Err("input file has too many blocks".to_owned()));
                                break;
                            };
                            let start_idx = match nbi.try_update(
                                Ordering::SeqCst,
                                Ordering::SeqCst,
                                |current| current.checked_add(num_blocks),
                            ) {
                                Ok(index) => index,
                                Err(_) => {
                                    let _ =
                                        w_tx.send(Err("archive has too many blocks".to_owned()));
                                    break;
                                }
                            };
                            let end_idx = start_idx + num_blocks;
                            let indices: Vec<u32> = (start_idx..end_idx).collect();
                            map.entry(total_read).or_default().push(DedupCandidate {
                                checksum: Some(checksum),
                                tiny_contents: Vec::new(),
                                block_indices: indices.clone(),
                            });
                            (false, indices)
                        }
                    };

                    if !is_dup {
                        let mut file = match File::open(&entry.source_path) {
                            Ok(file) => file,
                            Err(error) => {
                                let _ = w_tx.send(Err(error.to_string()));
                                break;
                            }
                        };
                        let mut verification_hasher = Xxh64::new(0);
                        let mut remaining = total_read;
                        for block_index in indices.iter().copied() {
                            if progress.is_cancelled() {
                                let _ = w_tx.send(Err("operation cancelled".to_owned()));
                                break;
                            }
                            let chunk_size = remaining.min(buffer_size as u64) as usize;
                            let mut chunk = vec![0u8; chunk_size];
                            if let Err(error) = file.read_exact(&mut chunk) {
                                let _ = w_tx.send(Err(format!(
                                    "input file changed while being archived: {} ({error})",
                                    entry.source_path.display()
                                )));
                                break;
                            }
                            verification_hasher.update(&chunk);
                            if b_tx
                                .send(BlockWorkItem {
                                    block_index,
                                    data: chunk,
                                    skip_compression,
                                })
                                .is_err()
                            {
                                break;
                            }
                            remaining -= chunk_size as u64;
                        }
                        if remaining != 0 {
                            continue;
                        }
                        let mut extra = [0u8; 1];
                        match file.read(&mut extra) {
                            Ok(0) if verification_hasher.digest() == checksum => {}
                            Ok(_) | Err(_) => {
                                let _ = w_tx.send(Err(format!(
                                    "input file changed while being archived: {}",
                                    entry.source_path.display()
                                )));
                                break;
                            }
                        }
                        if verification_hasher.digest() != checksum {
                            let _ = w_tx.send(Err(format!(
                                "input file changed while being archived: {}",
                                entry.source_path.display()
                            )));
                            break;
                        }
                    }

                    let file_entry = match FileEntry::new(total_read, indices) {
                        Ok(fe) => fe.with_checksum(checksum),
                        Err(e) => {
                            let _ = w_tx.send(Err(e.to_string()));
                            break;
                        }
                    };

                    fr.lock()
                        .unwrap()
                        .push((orig_index, path, file_entry, metadata));
                }
                if remaining.fetch_sub(1, Ordering::SeqCst) == 1 {
                    let final_block = packer.lock().unwrap().take_block();
                    match final_block {
                        Ok(Some(block)) => {
                            if s_tx.send(block).is_err() {
                                let _ =
                                    w_tx.send(Err("small-file worker queue is closed".to_owned()));
                            }
                        }
                        Ok(None) => {}
                        Err(error) => {
                            let _ = w_tx.send(Err(error));
                        }
                    }
                }
            }));
        }

        drop(small_tx);
        drop(big_tx);
        drop(writer_tx);

        let mut pending_blocks: BTreeMap<u32, BlockFrame> = BTreeMap::new();
        let mut next_expected_index = initial_expected_index;
        let mut writer_error = None;

        while let Ok(res) = writer_rx.recv() {
            let block = match res {
                Ok(block) => block,
                Err(message) => {
                    if writer_error.is_none() {
                        writer_error = Some(io::Error::other(message));
                    }
                    continue;
                }
            };
            if writer_error.is_some() {
                continue;
            }
            pending_blocks.insert(block.block_index, block);
            while let Some(block) = pending_blocks.remove(&next_expected_index) {
                context
                    .block_sizes
                    .insert(block.block_index, block.payload.len() as u64);
                if let Err(error) = write_indexed_block(context.data, context.block_index, &block) {
                    writer_error = Some(io::Error::other(error.to_string()));
                    break;
                }
                next_expected_index += 1;
            }
        }

        for handle in reader_handles {
            let _ = handle.join();
        }

        for handle in worker_handles {
            let _ = handle.join();
        }

        context
            .policy
            .warning_messages
            .extend(read_warnings.lock().unwrap().drain(..));
        context.policy.warnings |= !context.policy.warning_messages.is_empty();

        if let Some(error) = writer_error {
            return Err(Box::new(error));
        }

        while let Some(block) = pending_blocks.remove(&next_expected_index) {
            context
                .block_sizes
                .insert(block.block_index, block.payload.len() as u64);
            write_indexed_block(context.data, context.block_index, &block)?;
            next_expected_index += 1;
        }

        if !pending_blocks.is_empty() {
            return Err(Box::new(io::Error::other(
                "blocks missing in writer sequence",
            )));
        }

        *context.next_block_index = next_expected_index;

        let mut results = file_results.lock().unwrap();
        results.sort_by_key(|(orig_index, _, _, _)| *orig_index);
        for (_, path, file_entry, metadata) in results.drain(..) {
            entry_by_path.insert(path.clone(), file_entry.clone());
            context.changes.push(TocChange::PutFile {
                path,
                entry: file_entry,
            });
            if let Some(m) = metadata {
                context.entry_metadata.push(m);
            }
        }

        Ok(())
    })
}

#[cfg(target_os = "wasi")]
pub fn process_file_entries_parallel(
    entries: &[&TreeEntry],
    context: &mut ScanContext<'_>,
    entry_by_path: &mut HashMap<ArchivePath, FileEntry>,
) -> Result<(), Box<dyn std::error::Error>> {
    let buffer_size = context.options.block_size;
    let compression_level = context.options.compression_level;
    let mut next_block_index = *context.next_block_index;
    let mut candidates = HashMap::<u64, Vec<DedupCandidate>>::new();
    let mut small_packer = SmallFilePacker::new(buffer_size);
    let mut warnings = Vec::new();

    for entry in entries {
        let path = ArchivePath::new(entry.archive_path.clone())?;
        context
            .progress
            .set_message(format!("{} {}", context.action, entry.archive_path));

        let metadata = if context.save_metadata {
            match std::fs::symlink_metadata(&entry.source_path) {
                Ok(metadata) => Some(capture_entry_metadata(&path, &metadata, true)?),
                Err(error)
                    if error.kind() == io::ErrorKind::PermissionDenied
                        && !context.policy.strict =>
                {
                    warnings.push(format!(
                        "warning: cannot read metadata for {}: {error}",
                        entry.source_path.display()
                    ));
                    continue;
                }
                Err(error) => {
                    return Err(format!(
                        "cannot read metadata for {}: {error}",
                        entry.source_path.display()
                    )
                    .into());
                }
            }
        } else {
            Some(entry_date_metadata(
                &path,
                entry.modified_unix_millis,
                entry.created_unix_millis,
                entry.accessed_unix_millis,
            ))
        };

        let skip_compression = should_skip_compression(&entry.source_path);
        let (file_entry, new_blocks) = if entry.size <= 8 {
            let mut file = match File::open(&entry.source_path) {
                Ok(file) => file,
                Err(error)
                    if error.kind() == io::ErrorKind::PermissionDenied
                        && !context.policy.strict =>
                {
                    warnings.push(format!(
                        "warning: cannot read input file {}: {error}",
                        entry.source_path.display()
                    ));
                    continue;
                }
                Err(error) => {
                    return Err(format!(
                        "cannot read input file {}: {error}",
                        entry.source_path.display()
                    )
                    .into());
                }
            };
            let mut contents = vec![0; entry.size as usize];
            if let Err(error) = file.read_exact(&mut contents) {
                if error.kind() == io::ErrorKind::PermissionDenied && !context.policy.strict {
                    warnings.push(format!(
                        "warning: cannot read input file {}: {error}",
                        entry.source_path.display()
                    ));
                    continue;
                }
                return Err(format!(
                    "cannot read input file {}: {error}",
                    entry.source_path.display()
                )
                .into());
            }
            context.progress.inc(entry.size);

            if let Some(indices) = duplicate_blocks(&candidates, entry.size, None, &contents) {
                (FileEntry::new(entry.size, indices)?, Vec::new())
            } else {
                let index = next_block_index;
                next_block_index = next_block_index
                    .checked_add(1)
                    .ok_or("archive has too many blocks")?;
                candidates
                    .entry(entry.size)
                    .or_default()
                    .push(DedupCandidate {
                        checksum: None,
                        tiny_contents: contents.clone(),
                        block_indices: vec![index],
                    });
                (
                    FileEntry::new(entry.size, vec![index])?,
                    vec![BlockWorkItem {
                        block_index: index,
                        data: contents,
                        skip_compression,
                    }],
                )
            }
        } else if entry.size < buffer_size as u64 {
            let Some(mut file) = open_input_file(entry, context.policy.strict, &mut warnings)?
            else {
                continue;
            };
            let size = usize::try_from(entry.size).map_err(|_| "input file is too large")?;
            let mut contents = vec![0; size];
            if let Err(error) = file.read_exact(&mut contents) {
                if error.kind() == io::ErrorKind::PermissionDenied && !context.policy.strict {
                    warnings.push(format!(
                        "warning: cannot read input file {}: {error}",
                        entry.source_path.display()
                    ));
                    continue;
                }
                return Err(format!(
                    "cannot read input file {}: {error}",
                    entry.source_path.display()
                )
                .into());
            }
            context.progress.inc(entry.size);
            let mut hasher = Xxh64::new(0);
            hasher.update(&contents);
            let checksum = hasher.digest();

            let (block_index, offset_in_block, completed) = small_packer
                .add(contents, checksum, skip_compression, || {
                    let index = next_block_index;
                    next_block_index = next_block_index
                        .checked_add(1)
                        .ok_or_else(|| "archive has too many blocks".to_owned())?;
                    Ok(index)
                })
                .map_err(io::Error::other)?;
            (
                FileEntry::new(entry.size, vec![block_index])?
                    .with_block_offset(offset_in_block)
                    .with_checksum(checksum),
                completed.into_iter().collect(),
            )
        } else {
            let Some(mut file) = open_input_file(entry, context.policy.strict, &mut warnings)?
            else {
                continue;
            };
            let (total_read, checksum) = match checksum_file(&mut file, &context.progress) {
                Ok(result) => result,
                Err(error)
                    if error.kind() == io::ErrorKind::PermissionDenied
                        && !context.policy.strict =>
                {
                    warnings.push(format!(
                        "warning: cannot read input file {}: {error}",
                        entry.source_path.display()
                    ));
                    continue;
                }
                Err(error) => {
                    return Err(format!(
                        "cannot read input file {}: {error}",
                        entry.source_path.display()
                    )
                    .into());
                }
            };
            if total_read != entry.size {
                return Err(format!(
                    "input file changed while being archived: {}",
                    entry.source_path.display()
                )
                .into());
            }

            if let Some(indices) = duplicate_blocks(&candidates, total_read, Some(checksum), &[]) {
                (
                    FileEntry::new(total_read, indices)?.with_checksum(checksum),
                    Vec::new(),
                )
            } else {
                let block_count = u32::try_from(total_read.div_ceil(buffer_size as u64))
                    .map_err(|_| "input file has too many blocks")?;
                let start_index = next_block_index;
                next_block_index = next_block_index
                    .checked_add(block_count)
                    .ok_or("archive has too many blocks")?;
                let indices: Vec<u32> = (start_index..next_block_index).collect();
                candidates
                    .entry(total_read)
                    .or_default()
                    .push(DedupCandidate {
                        checksum: Some(checksum),
                        tiny_contents: Vec::new(),
                        block_indices: indices.clone(),
                    });

                let mut file = File::open(&entry.source_path)?;
                let mut verification_hasher = Xxh64::new(0);
                let mut remaining = total_read;
                for index in indices.iter().copied() {
                    let chunk_size = remaining.min(buffer_size as u64) as usize;
                    let mut chunk = vec![0; chunk_size];
                    file.read_exact(&mut chunk).map_err(|error| {
                        format!(
                            "input file changed while being archived: {} ({error})",
                            entry.source_path.display()
                        )
                    })?;
                    verification_hasher.update(&chunk);
                    remaining -= chunk_size as u64;
                    {
                        let block = encode_block(
                            index,
                            &chunk,
                            compression_level,
                            skip_compression,
                            context.encryption.as_ref(),
                        )?;
                        context
                            .block_sizes
                            .insert(index, block.payload.len() as u64);
                        write_indexed_block(context.data, context.block_index, &block)?;
                    }
                }
                let mut extra = [0; 1];
                if remaining != 0
                    || file.read(&mut extra)? != 0
                    || verification_hasher.digest() != checksum
                {
                    return Err(format!(
                        "input file changed while being archived: {}",
                        entry.source_path.display()
                    )
                    .into());
                }
                (
                    FileEntry::new(total_read, indices)?.with_checksum(checksum),
                    Vec::new(),
                )
            }
        };

        for block in new_blocks {
            {
                let block = encode_block(
                    block.block_index,
                    &block.data,
                    compression_level,
                    block.skip_compression,
                    context.encryption.as_ref(),
                )?;
                context
                    .block_sizes
                    .insert(block.block_index, block.payload.len() as u64);
                write_indexed_block(context.data, context.block_index, &block)?;
            }
        }

        entry_by_path.insert(path.clone(), file_entry.clone());
        context.changes.push(TocChange::PutFile {
            path,
            entry: file_entry,
        });
        if let Some(metadata) = metadata {
            context.entry_metadata.push(metadata);
        }
    }

    if let Some(block) = small_packer.take_block().map_err(io::Error::other)? {
        let frame = encode_block(
            block.block_index,
            &block.data,
            compression_level,
            block.skip_compression,
            context.encryption.as_ref(),
        )?;
        context
            .block_sizes
            .insert(block.block_index, frame.payload.len() as u64);
        write_indexed_block(context.data, context.block_index, &frame)?;
    }

    *context.next_block_index = next_block_index;
    context.policy.warnings |= !warnings.is_empty();
    context.policy.warning_messages.extend(warnings);
    Ok(())
}

#[cfg(target_os = "wasi")]
fn open_input_file(
    entry: &TreeEntry,
    strict: bool,
    warnings: &mut Vec<String>,
) -> Result<Option<File>, Box<dyn std::error::Error>> {
    match File::open(&entry.source_path) {
        Ok(file) => Ok(Some(file)),
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied && !strict => {
            warnings.push(format!(
                "warning: cannot read input file {}: {error}",
                entry.source_path.display()
            ));
            Ok(None)
        }
        Err(error) => Err(format!(
            "cannot read input file {}: {error}",
            entry.source_path.display()
        )
        .into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ScanContext, ScanPolicy, SmallFilePacker, TreeEntry, TreeEntryKind, archive_prefix,
        process_file_entries_parallel, process_tree_entries, scan_tree,
        skip_permission_denied_file,
    };
    use crate::operations::{EncodingOptions, ProgressReporter};
    use crate::{DeviceKind, TocChange};
    use std::collections::{BTreeSet, HashMap};
    use std::io::Write;
    use std::path::Path;
    use std::sync::Mutex;

    #[cfg(unix)]
    #[test]
    fn scan_individual_symlinks_and_special_files() {
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::net::UnixListener;

        let directory = tempfile::tempdir().unwrap();
        let link = directory.path().join("link");
        std::os::unix::fs::symlink("target", &link).unwrap();
        let entries = scan_tree(&link, "").unwrap();
        assert_eq!(entries[0].archive_path, "link");
        assert!(
            matches!(&entries[0].kind, TreeEntryKind::Symlink { target } if target == "target")
        );

        let invalid = directory.path().join("invalid");
        std::os::unix::fs::symlink(std::ffi::OsStr::from_bytes(b"\xff"), &invalid).unwrap();
        assert!(
            scan_tree(&invalid, "")
                .err()
                .unwrap()
                .to_string()
                .contains("UTF-8")
        );

        let socket = directory.path().join("socket");
        let _listener = UnixListener::bind(&socket).unwrap();
        assert!(matches!(
            scan_tree(&socket, "").unwrap()[0].kind,
            TreeEntryKind::Unsupported
        ));

        let pipe = directory.path().join("pipe");
        let name = std::ffi::CString::new(pipe.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        assert!(matches!(
            scan_tree(&pipe, "").unwrap()[0].kind,
            TreeEntryKind::NamedPipe
        ));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn scan_character_device_reports_major_and_minor() {
        let entries = scan_tree(Path::new("/dev/null"), "null").unwrap();
        assert!(matches!(
            entries[0].kind,
            TreeEntryKind::Device {
                kind: DeviceKind::Character,
                major: 1,
                minor: 3
            }
        ));
    }

    #[test]
    fn scan_tree_preserves_child_named_like_input_root() {
        let temporary = tempfile::tempdir().unwrap();
        let source = temporary.path().join("duplicates");
        let nested = source.join("duplicates/128x128/file.png");
        std::fs::create_dir_all(nested.parent().unwrap()).unwrap();
        std::fs::write(&nested, b"icon").unwrap();

        let entries = scan_tree(&source, "duplicates").unwrap();
        let unique_paths = entries
            .iter()
            .map(|entry| entry.archive_path.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            unique_paths.len(),
            entries.len(),
            "duplicate paths: {:?}",
            entries
                .iter()
                .map(|entry| &entry.archive_path)
                .collect::<Vec<_>>()
        );
        let entry = entries
            .iter()
            .find(|entry| entry.archive_path == "duplicates/duplicates/128x128/file.png")
            .unwrap();
        assert_eq!(entry.source_path, nested);
    }

    #[test]
    fn special_entries_follow_scan_policy_and_record_metadata() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let entry = |name: &str, kind| TreeEntry {
            archive_path: name.to_owned(),
            source_path: file.path().to_owned(),
            kind,
            size: 0,
            modified_unix_millis: Some(123),
            created_unix_millis: None,
            accessed_unix_millis: None,
        };
        let entries = vec![
            entry(
                "link",
                TreeEntryKind::Symlink {
                    target: "target".into(),
                },
            ),
            entry("pipe", TreeEntryKind::NamedPipe),
            entry(
                "device",
                TreeEntryKind::Device {
                    kind: DeviceKind::Character,
                    major: 2,
                    minor: 3,
                },
            ),
            entry("unsupported", TreeEntryKind::Unsupported),
            entry("bad\\path", TreeEntryKind::Unsupported),
        ];
        for (include_special_files, save_metadata) in [(true, true), (false, false)] {
            let mut changes = Vec::new();
            let mut data = tempfile::tempfile().unwrap();
            let mut block_index = Vec::new();
            let mut next_block_index = 0;
            let mut block_sizes = HashMap::new();
            let mut seen_paths = BTreeSet::new();
            let mut metadata = Vec::new();
            let mut policy = ScanPolicy {
                strict: false,
                include_special_files,
                verbose: true,
                warnings: false,
                warning_messages: Vec::new(),
            };
            let mut context = ScanContext {
                changes: &mut changes,
                data: &mut data,
                block_index: &mut block_index,
                next_block_index: &mut next_block_index,
                block_sizes: &mut block_sizes,
                seen_paths: &mut seen_paths,
                entry_metadata: &mut metadata,
                save_metadata,
                policy: &mut policy,
                options: EncodingOptions::default(),
                compression_workers: 1,
                progress: ProgressReporter(None),
                action: "Adding",
                encryption: None,
                historical_duplicates: None,
            };
            process_tree_entries(&entries, &mut context).unwrap();
            context.policy.strict = true;
            for entry in [&entries[0], &entries[3], &entries[4]] {
                assert!(process_tree_entries(std::slice::from_ref(entry), &mut context).is_err());
            }
            if !include_special_files {
                assert!(process_tree_entries(&entries[1..2], &mut context).is_err());
            }
            assert!(matches!(changes[0], TocChange::PutSymlink { .. }));
            assert_eq!(changes.len(), if include_special_files { 3 } else { 1 });
            assert_eq!(metadata.len(), changes.len());
            if include_special_files {
                assert!(matches!(changes[1], TocChange::PutNamedPipe { .. }));
                assert!(matches!(
                    changes[2],
                    TocChange::PutDevice {
                        major: 2,
                        minor: 3,
                        ..
                    }
                ));
            }
            assert!(policy.warnings);
            assert_eq!(policy.warning_messages.len(), 5);
        }
    }

    #[test]
    fn repeated_source_paths_share_blocks_and_preserve_metadata() {
        let mut source = tempfile::NamedTempFile::new().unwrap();
        source.write_all(b"shared hard link contents").unwrap();
        let entries = ["original", "duplicate"].map(|name| TreeEntry {
            archive_path: name.to_owned(),
            source_path: source.path().to_owned(),
            kind: TreeEntryKind::File,
            size: 25,
            modified_unix_millis: Some(123),
            created_unix_millis: None,
            accessed_unix_millis: None,
        });
        for save_metadata in [false, true] {
            let mut changes = Vec::new();
            let mut data = tempfile::tempfile().unwrap();
            let mut block_index = Vec::new();
            let mut next_block_index = 0;
            let mut block_sizes = HashMap::new();
            let mut seen_paths = BTreeSet::new();
            let mut metadata = Vec::new();
            let mut policy = ScanPolicy {
                strict: false,
                include_special_files: false,
                verbose: false,
                warnings: false,
                warning_messages: Vec::new(),
            };
            let mut context = ScanContext {
                changes: &mut changes,
                data: &mut data,
                block_index: &mut block_index,
                next_block_index: &mut next_block_index,
                block_sizes: &mut block_sizes,
                seen_paths: &mut seen_paths,
                entry_metadata: &mut metadata,
                save_metadata,
                policy: &mut policy,
                options: EncodingOptions::default(),
                compression_workers: 1,
                progress: ProgressReporter(None),
                action: "Adding",
                encryption: None,
                historical_duplicates: None,
            };
            process_tree_entries(&entries, &mut context).unwrap();
            assert_eq!(changes.len(), 2);
            let TocChange::PutFile {
                entry: original, ..
            } = &changes[0]
            else {
                panic!("expected a file")
            };
            let TocChange::PutFile {
                entry: duplicate, ..
            } = &changes[1]
            else {
                panic!("expected a file")
            };
            assert_eq!(original, duplicate);
            assert_eq!(metadata.len(), 2);
            assert_eq!(metadata[1].path.as_str(), "duplicate");
        }
    }

    #[test]
    fn parallel_pipeline_reports_missing_and_changed_input_files() {
        fn process(entry: &TreeEntry, save_metadata: bool) -> String {
            let mut changes = Vec::new();
            let mut data = tempfile::tempfile().unwrap();
            let mut block_index = Vec::new();
            let mut next_block_index = 0;
            let mut block_sizes = HashMap::new();
            let mut seen_paths = BTreeSet::new();
            let mut metadata = Vec::new();
            let mut policy = ScanPolicy {
                strict: false,
                include_special_files: false,
                verbose: false,
                warnings: false,
                warning_messages: Vec::new(),
            };
            let mut context = ScanContext {
                changes: &mut changes,
                data: &mut data,
                block_index: &mut block_index,
                next_block_index: &mut next_block_index,
                block_sizes: &mut block_sizes,
                seen_paths: &mut seen_paths,
                entry_metadata: &mut metadata,
                save_metadata,
                policy: &mut policy,
                options: EncodingOptions {
                    block_size: 16,
                    compression_level: 1,
                },
                compression_workers: 1,
                progress: ProgressReporter(None),
                action: "Adding",
                encryption: None,
                historical_duplicates: None,
            };
            process_file_entries_parallel(&[entry], &mut context, &mut HashMap::new())
                .unwrap_err()
                .to_string()
        }

        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input");
        std::fs::write(&input, b"data").unwrap();
        let mut entry = TreeEntry {
            archive_path: "input".to_owned(),
            source_path: input,
            kind: TreeEntryKind::File,
            size: 8,
            modified_unix_millis: None,
            created_unix_millis: None,
            accessed_unix_millis: None,
        };
        for size in [8, 12] {
            entry.size = size;
            assert!(process(&entry, false).contains("cannot read input file"));
        }
        entry.size = 16;
        assert!(process(&entry, false).contains("input file changed"));
        entry.source_path = directory.path().join("missing");
        for size in [4, 12, 16] {
            entry.size = size;
            assert!(process(&entry, false).contains("cannot read input file"));
        }
        assert!(process(&entry, true).contains("cannot read metadata"));
        entry.archive_path = "../invalid".to_owned();
        assert!(process(&entry, false).contains(".."));
    }

    #[test]
    fn small_file_packing_accepts_all_reader_completion_orders() {
        let contents = [
            b"hello small world".as_slice(),
            b"second 99".as_slice(),
            b"another small one".as_slice(),
        ];
        let mut first_file_paired = [false; 2];
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let mut packer = SmallFilePacker::new(32);
            let mut next_index = 0;
            let mut indices = [0; 3];
            let mut blocks = Vec::new();
            for file in order {
                let (index, _, completed) = packer
                    .add(contents[file].to_vec(), file as u64, false, || {
                        let index = next_index;
                        next_index += 1;
                        Ok(index)
                    })
                    .unwrap();
                indices[file] = index;
                blocks.extend(completed);
            }
            blocks.extend(packer.take_block().unwrap());
            assert_eq!(blocks.len(), 2);
            assert_ne!(indices[0], indices[2]);
            assert!(indices[1] == indices[0] || indices[1] == indices[2]);
            first_file_paired[usize::from(indices[0] == indices[1])] = true;
        }
        assert_eq!(first_file_paired, [true, true]);
    }

    #[test]
    fn absolute_paths_outside_working_directory_use_the_basename() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("elsewhere/log");
        let cwd = directory.path().join("project");
        assert_eq!(archive_prefix(&input, &cwd).unwrap(), "log");
    }

    #[test]
    fn absolute_paths_inside_working_directory_are_relative_to_it() {
        let directory = tempfile::tempdir().unwrap();
        let cwd = directory.path().join("project");
        let input = cwd.join("src/main.rs");
        assert_eq!(archive_prefix(&input, &cwd).unwrap(), "src/main.rs");
    }

    #[test]
    fn relative_paths_remain_relative() {
        assert_eq!(
            archive_prefix(Path::new("src/main.rs"), Path::new("/home/user/project")).unwrap(),
            "src/main.rs"
        );
    }

    #[test]
    fn permission_denied_is_a_warning_unless_strict_mode_is_enabled() {
        let entry = TreeEntry {
            archive_path: "restricted.txt".to_owned(),
            source_path: Path::new("/restricted.txt").to_owned(),
            kind: TreeEntryKind::File,
            size: 12,
            modified_unix_millis: None,
            created_unix_millis: None,
            accessed_unix_millis: None,
        };
        let warnings = Mutex::new(Vec::new());
        let permission_error = std::io::Error::from(std::io::ErrorKind::PermissionDenied);

        assert!(skip_permission_denied_file(
            &entry,
            &permission_error,
            false,
            &warnings
        ));
        assert_eq!(warnings.lock().unwrap().len(), 1);
        assert!(warnings.lock().unwrap()[0].contains("/restricted.txt"));

        let strict_warnings = Mutex::new(Vec::new());
        assert!(!skip_permission_denied_file(
            &entry,
            &permission_error,
            true,
            &strict_warnings
        ));
        assert!(strict_warnings.lock().unwrap().is_empty());
    }
}
