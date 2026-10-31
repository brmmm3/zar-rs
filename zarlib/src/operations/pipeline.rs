use super::common::*;
use super::filters::FilterSet;
use crate::{
    ArchivePath, BlockFrame, CommitHeader, ContentAead, DeviceKind, EntryMetadata, FileEntry,
    TocChange,
};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
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

/// Walks the file tree rooted at `source` using the `scandir` crate and
/// returns a flat, sorted list of [`TreeEntry`] items. The `prefix` is the
/// archive-relative path for the root (empty string for the working directory).
///
/// For a single file (non-directory), a single entry is returned directly
/// without invoking the directory walker.
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
        });
    }

    let mut scandir = scandir::Scandir::new(source, None)?
        .sorted(true)
        .return_type(scandir::ReturnType::Ext);
    let results = scandir.collect()?;

    for result in &results.results {
        if let Some(ext) = result.ext() {
            let normalized_path = ext.path.trim_matches(['/', '\\']);
            let scanned_path = Path::new(normalized_path);
            let root_name = source.file_name().unwrap_or_default();
            let has_root_prefix = scanned_path
                .components()
                .next()
                .is_some_and(|component| component.as_os_str() == root_name);
            let relative_path = if has_root_prefix {
                scanned_path.strip_prefix(root_name).unwrap_or(scanned_path)
            } else {
                scanned_path
            };
            let relative_components = relative_path
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
            let source_path = source.join(relative_path);
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
                    let file_type = std::fs::symlink_metadata(&source_path)?.file_type();
                    if file_type.is_fifo() {
                        TreeEntryKind::NamedPipe
                    } else if file_type.is_char_device() || file_type.is_block_device() {
                        use std::os::unix::fs::MetadataExt;
                        let md = std::fs::symlink_metadata(&source_path)?;
                        let device_kind = if file_type.is_char_device() {
                            DeviceKind::Character
                        } else {
                            DeviceKind::Block
                        };
                        TreeEntryKind::Device {
                            kind: device_kind,
                            major: libc::major(md.rdev().try_into().unwrap()) as u32,
                            minor: libc::minor(md.rdev().try_into().unwrap()) as u32,
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
            });
        }
    }
    Ok(entries)
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
                    major: libc::major(metadata.rdev().try_into().unwrap()) as u32,
                    minor: libc::minor(metadata.rdev().try_into().unwrap()) as u32,
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
    pub next_block_index: &'a mut u32,
    pub seen_paths: &'a mut std::collections::BTreeSet<ArchivePath>,
    pub entry_metadata: &'a mut Vec<EntryMetadata>,
    pub save_metadata: bool,
    pub policy: &'a mut ScanPolicy,
    pub options: EncodingOptions,
    pub compression_workers: usize,
    pub progress: ProgressReporter<'a>,
    pub action: &'static str,
    pub encryption: Option<EncryptionContext>,
}

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
                    if context.save_metadata {
                        context
                            .entry_metadata
                            .push(capture_entry_metadata(&path, &metadata)?);
                    }
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
        if context.save_metadata {
            let metadata = std::fs::symlink_metadata(&hard_link.source_path)?;
            context
                .entry_metadata
                .push(capture_entry_metadata(&hard_link.archive_path, &metadata)?);
        }
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
    if context.save_metadata {
        let metadata = std::fs::symlink_metadata(&entry.source_path)?;
        context.entry_metadata.push(capture_entry_metadata(
            &ArchivePath::new(entry.archive_path.clone())?,
            &metadata,
        )?);
    }
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
    if context.save_metadata {
        let metadata = std::fs::symlink_metadata(&entry.source_path)?;
        context
            .entry_metadata
            .push(capture_entry_metadata(&path, &metadata)?);
    }
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
    if context.save_metadata {
        let metadata = std::fs::symlink_metadata(&entry.source_path)?;
        context
            .entry_metadata
            .push(capture_entry_metadata(&path, &metadata)?);
    }
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
    if context.save_metadata {
        let metadata = std::fs::symlink_metadata(&entry.source_path)?;
        context
            .entry_metadata
            .push(capture_entry_metadata(&path, &metadata)?);
    }
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WorkerPreference {
    SmallTinyFirst,
    BigFileFirst,
}

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
            BlockFrame::from_uncompressed_encrypted(
                index,
                data,
                0,
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

    let next_block_index = AtomicU32::new(*context.next_block_index);

    let file_results = std::sync::Arc::new(std::sync::Mutex::new(Vec::<(
        usize,
        ArchivePath,
        FileEntry,
        Option<EntryMetadata>,
    )>::new()));
    let read_warnings = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));

    let reader_count = num_workers;
    let encryption = context.encryption.as_ref();
    let initial_expected_index = *context.next_block_index;
    let strict = context.policy.strict;

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
            let progress = &context.progress;
            let action = context.action;
            let save_metadata = context.save_metadata;

            reader_handles.push(scope.spawn(move || {
                loop {
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
                            Ok(md) => match capture_entry_metadata(&path, &md) {
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
                        None
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

                        let (is_dup, indices) = {
                            let mut map = dm.lock().unwrap();
                            if let Some(existing) =
                                duplicate_blocks(&map, entry.size, Some(checksum), &buf)
                            {
                                (true, existing)
                            } else {
                                let idx = nbi.fetch_add(1, Ordering::SeqCst);
                                let indices = vec![idx];
                                map.entry(entry.size).or_default().push(DedupCandidate {
                                    checksum: Some(checksum),
                                    tiny_contents: Vec::new(),
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
                            Ok(fe) => fe.with_checksum(checksum),
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
                if let Err(error) = block.write_to(context.data) {
                    writer_error = Some(io::Error::other(error));
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
            block.write_to(context.data)?;
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

#[cfg(test)]
mod tests {
    use super::{TreeEntry, TreeEntryKind, archive_prefix, skip_permission_denied_file};
    use std::path::Path;
    use std::sync::Mutex;

    #[test]
    fn absolute_paths_outside_working_directory_use_the_basename() {
        assert_eq!(
            archive_prefix(Path::new("/var/log"), Path::new("/home/user/project")).unwrap(),
            "log"
        );
    }

    #[test]
    fn absolute_paths_inside_working_directory_are_relative_to_it() {
        assert_eq!(
            archive_prefix(
                Path::new("/home/user/project/src/main.rs"),
                Path::new("/home/user/project")
            )
            .unwrap(),
            "src/main.rs"
        );
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
