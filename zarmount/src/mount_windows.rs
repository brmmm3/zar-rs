use clap::Parser;
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Cursor, Read},
    ops::ControlFlow,
    path::{Path, PathBuf},
};
use windows_projfs::{
    DirectoryEntry, DirectoryInfo, FileInfo, Notification, ProjectedFileSystem,
    ProjectedFileSystemSource,
};
use zarlib::{
    TocEntry,
    operations::{PasswordSet, PreparedExtraction, parse_commit_id},
};
use zeroize::Zeroizing;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Parser, Debug, Clone)]
#[command(
    version,
    about = "Mount a ZAR archive as a read-only Windows ProjFS filesystem"
)]
pub struct Options {
    /// Archive to mount.
    pub archive: PathBuf,
    /// Mount path or drive letter (e.g. C:\mount or Z:).
    #[arg(required = false)]
    pub target1: Option<String>,
    /// Optional drive letter when target1 is a path (e.g. Z:).
    #[arg(required = false)]
    pub target2: Option<String>,
    /// Open dialog with mount options.
    #[arg(long)]
    pub ask: bool,
    /// Select the archive state through this eight-digit commit ID.
    #[arg(long)]
    pub to: Option<String>,
    /// File containing the default password (one optional trailing newline is removed).
    #[arg(long)]
    pub password_file: Option<PathBuf>,
    /// Commit-specific password file, as COMMIT_ID=PATH. May be repeated.
    #[arg(long, value_name = "COMMIT_ID=PATH")]
    pub password_for: Vec<String>,
}

#[allow(dead_code)]
pub fn password_file(path: &Path) -> Result<Vec<u8>> {
    let mut password = Zeroizing::new(fs::read(path)?);
    if password.last() == Some(&b'\n') {
        password.pop();
        if password.last() == Some(&b'\r') {
            password.pop();
        }
    }
    if password.is_empty() {
        return Err(format!("password file {} is empty", path.display()).into());
    }
    Ok(std::mem::take(&mut *password))
}

/// Build a [`PasswordSet`] from the password-related options.
#[allow(dead_code)]
pub fn build_passwords(
    password_file_path: Option<&Path>,
    password_for: &[String],
) -> Result<PasswordSet> {
    let default = password_file_path.map(password_file).transpose()?;
    let mut passwords = PasswordSet::new(default);
    let mut ids = BTreeMap::new();
    for spec in password_for {
        let (id, path) = spec
            .split_once('=')
            .ok_or("--password-for must be COMMIT_ID=PATH")?;
        if path.is_empty() {
            return Err("--password-for requires a password file path".into());
        }
        let commit = parse_commit_id(id)?;
        if ids.insert(commit, ()).is_some() {
            return Err(format!("password for commit {id} specified more than once").into());
        }
        passwords.insert_for_commit(commit, password_file(Path::new(path))?);
    }
    Ok(passwords)
}

/// The resolved target of a symlink.
enum ResolvedTarget {
    File(u64),
    Directory,
}

/// Resolve a symlink target relative to the symlink's location in the archive.
///
/// Follows the chain of symlinks up to a maximum depth to prevent infinite loops.
/// Returns `None` if the target cannot be resolved (doesn't exist in the archive).
fn resolve_symlink(target: &str, symlink_path: &str, toc: &zarlib::Toc) -> Option<ResolvedTarget> {
    let mut visited = std::collections::HashSet::new();
    let mut current_target = target.to_string();
    let mut current_symlink_path = symlink_path.to_string();
    let max_depth = 64;

    for _ in 0..max_depth {
        if !visited.insert(current_target.clone()) {
            // Cycle detected
            return None;
        }

        // Resolve the target relative to the symlink's parent directory
        let resolved = if current_target.starts_with('/') {
            // Absolute path in the archive — strip leading slash
            current_target.trim_start_matches('/').to_string()
        } else {
            // Relative path — resolve against the symlink's parent
            let parent = match current_symlink_path.rsplit_once('/') {
                Some((parent, _)) => parent.to_string(),
                None => String::new(),
            };
            let mut result = if parent.is_empty() {
                String::new()
            } else {
                parent + "/"
            };
            // Normalize the relative path (handle .. and .)
            for component in current_target.split('/') {
                match component {
                    "." | "" => {}
                    ".." => {
                        let trimmed = result.trim_end_matches('/');
                        if let Some(pos) = trimmed.rfind('/') {
                            result.truncate(pos + 1);
                        } else {
                            result.clear();
                        }
                    }
                    _ => result.push_str(component),
                }
                if !result.is_empty() && !result.ends_with('/') {
                    result.push('/');
                }
            }
            // Remove trailing slash
            if result.ends_with('/') && result.len() > 1 {
                result.truncate(result.len() - 1);
            }
            result
        };

        // Look up the resolved path in the TOC
        let archive_path = zarlib::ArchivePath::new(&resolved).ok()?;
        match toc.get(&archive_path)? {
            TocEntry::File(file) => return Some(ResolvedTarget::File(file.size)),
            TocEntry::Directory => return Some(ResolvedTarget::Directory),
            TocEntry::Symlink { target } => {
                // Follow the chain
                current_target = target.clone();
                current_symlink_path = resolved;
            }
            TocEntry::NamedPipe | TocEntry::Device { .. } => return None,
        }
    }

    None
}

pub struct ZarSource {
    archive: PreparedExtraction,
    /// Maps path -> entry info (symlinks resolved to their target)
    entries: BTreeMap<String, EntryInfo>,
}

struct EntryInfo {
    is_directory: bool,
    size: u64,
}

impl ZarSource {
    fn new(archive: PreparedExtraction) -> Result<Self> {
        let toc = archive.toc();
        let mut entries: BTreeMap<String, EntryInfo> = BTreeMap::new();

        // Add root directory
        entries.insert(
            String::new(),
            EntryInfo {
                is_directory: true,
                size: 0,
            },
        );

        // First pass: collect all paths and their raw entries
        struct RawEntry {
            entry: TocEntry,
            path: String,
        }
        let mut raw_entries: Vec<RawEntry> = Vec::new();

        for (archive_path, entry) in toc.iter() {
            let path_str = archive_path.as_str().to_string();
            match entry {
                TocEntry::NamedPipe | TocEntry::Device { .. } => {
                    return Err(format!("unsupported special file in archive: {path_str}").into());
                }
                _ => {}
            }
            raw_entries.push(RawEntry {
                entry: entry.clone(),
                path: path_str,
            });

            // Ensure all parent directory entries exist
            let mut parent = String::new();
            for component in archive_path.as_str().split('/') {
                if !parent.is_empty() {
                    parent.push('/');
                }
                parent.push_str(component);
                entries.entry(parent.clone()).or_insert(EntryInfo {
                    is_directory: true,
                    size: 0,
                });
            }
        }

        // Second pass: resolve symlinks and insert resolved entries
        for raw in &raw_entries {
            match &raw.entry {
                TocEntry::File(file) => {
                    entries.insert(
                        raw.path.clone(),
                        EntryInfo {
                            is_directory: false,
                            size: file.size,
                        },
                    );
                }
                TocEntry::Directory => {
                    entries.insert(
                        raw.path.clone(),
                        EntryInfo {
                            is_directory: true,
                            size: 0,
                        },
                    );
                }
                TocEntry::Symlink { target } => {
                    let resolved = resolve_symlink(target, &raw.path, toc);
                    match resolved {
                        Some(ResolvedTarget::File(size)) => {
                            entries.insert(
                                raw.path.clone(),
                                EntryInfo {
                                    is_directory: false,
                                    size,
                                },
                            );
                        }
                        Some(ResolvedTarget::Directory) => {
                            entries.insert(
                                raw.path.clone(),
                                EntryInfo {
                                    is_directory: true,
                                    size: 0,
                                },
                            );
                        }
                        None => {
                            // Target doesn't exist in archive — skip the symlink entry
                            // so it won't appear in the filesystem
                        }
                    }
                }
                TocEntry::NamedPipe | TocEntry::Device { .. } => {
                    // Already rejected above, unreachable
                }
            }
        }

        Ok(Self { archive, entries })
    }

    fn list_children(&self, path: &Path) -> Vec<DirectoryEntry> {
        let prefix = path_to_zar_path(path);

        let mut children: Vec<DirectoryEntry> = Vec::new();
        let prefix_slash = if prefix.is_empty() {
            String::new()
        } else {
            format!("{prefix}/")
        };

        for (entry_path, info) in &self.entries {
            if entry_path == &prefix {
                continue;
            }
            if !entry_path.starts_with(&prefix_slash) {
                continue;
            }
            // Only direct children (one level deep)
            let rest = &entry_path[prefix_slash.len()..];
            if rest.contains('/') {
                continue;
            }
            if info.is_directory {
                children.push(DirectoryEntry::Directory(DirectoryInfo {
                    directory_name: rest.to_string(),
                    ..Default::default()
                }));
            } else {
                children.push(DirectoryEntry::File(FileInfo {
                    file_name: rest.to_string(),
                    file_size: info.size,
                    ..Default::default()
                }));
            }
        }
        children
    }
}

/// Convert a Windows path (relative to the ProjFS root) to a ZAR archive path.
fn path_to_zar_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    // Strip leading backslash and convert backslashes to forward slashes
    let s = s.trim_start_matches('\\');
    s.replace('\\', "/")
}

impl ProjectedFileSystemSource for ZarSource {
    fn list_directory(&self, path: &Path) -> Vec<DirectoryEntry> {
        self.list_children(path)
    }

    fn get_directory_entry(&self, path: &Path) -> Option<DirectoryEntry> {
        let zar_path = path_to_zar_path(path);
        if zar_path.is_empty() {
            // Root directory
            return Some(DirectoryEntry::Directory(DirectoryInfo {
                directory_name: String::new(),
                ..Default::default()
            }));
        }
        self.entries.get(&zar_path).map(|info| {
            if info.is_directory {
                DirectoryEntry::Directory(DirectoryInfo {
                    directory_name: zar_path.rsplit('/').next().unwrap_or(&zar_path).to_string(),
                    ..Default::default()
                })
            } else {
                DirectoryEntry::File(FileInfo {
                    file_name: zar_path.rsplit('/').next().unwrap_or(&zar_path).to_string(),
                    file_size: info.size,
                    ..Default::default()
                })
            }
        })
    }

    fn stream_file_content(
        &self,
        path: &Path,
        byte_offset: usize,
        length: usize,
    ) -> io::Result<Box<dyn Read>> {
        let zar_path = path_to_zar_path(path);
        let data = self
            .archive
            .read_file_range(&zar_path, byte_offset as u64, length as u64)
            .map_err(|e| io::Error::other(e.to_string()))?;
        Ok(Box::new(Cursor::new(data)))
    }

    fn handle_notification(&self, notification: &Notification) -> ControlFlow<()> {
        // Cancel all cancelable notifications to make the filesystem read-only
        if notification.is_cancelable() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }
}

/// Mount a ZAR archive onto a directory using ProjFS.
///
/// The returned `ProjectedFileSystem` must be kept alive for the mount to
/// remain active. Dropping it unmounts the filesystem.
pub fn mount_archive(
    archive: &Path,
    mountpoint: &Path,
    to: Option<&str>,
    passwords: &PasswordSet,
) -> Result<ProjectedFileSystem> {
    // ProjFS requires the root directory to exist and be empty.
    // Create it if it doesn't exist yet.
    if !mountpoint.exists() {
        fs::create_dir_all(mountpoint)?;
    } else if !mountpoint.is_dir() {
        return Err(format!("mountpoint must be a directory: {}", mountpoint.display()).into());
    } else if mountpoint.read_dir()?.next().is_some() {
        return Err(format!("mountpoint must be empty: {}", mountpoint.display()).into());
    }

    if archive.starts_with(mountpoint) {
        return Err("archive must not be inside the mountpoint".into());
    }

    let archive_name = archive.to_str().ok_or("archive path must be UTF-8")?;
    let prepared = PreparedExtraction::new_complete(archive_name, to, passwords)?;
    let source = ZarSource::new(prepared)?;

    eprintln!(
        "zarmount: mounting {} on {} (ProjFS)",
        archive.display(),
        mountpoint.display()
    );

    let pfs = ProjectedFileSystem::new(mountpoint, source)?;
    eprintln!("zarmount: mounted successfully");
    Ok(pfs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing_archive_only() {
        let args = ["zarmount", "archive.zar"];
        let opts = Options::try_parse_from(args).unwrap();
        assert_eq!(opts.archive, PathBuf::from("archive.zar"));
        assert_eq!(opts.target1, None);
        assert_eq!(opts.target2, None);
        assert!(!opts.ask);
    }

    #[test]
    fn test_cli_parsing_archive_and_path() {
        let args = ["zarmount", "archive.zar", "/path/to/mount"];
        let opts = Options::try_parse_from(args).unwrap();
        assert_eq!(opts.archive, PathBuf::from("archive.zar"));
        assert_eq!(opts.target1, Some("/path/to/mount".into()));
        assert_eq!(opts.target2, None);
        assert!(!opts.ask);
    }

    #[test]
    fn test_cli_parsing_archive_and_drive() {
        let args = ["zarmount", "archive.zar", "Z:"];
        let opts = Options::try_parse_from(args).unwrap();
        assert_eq!(opts.archive, PathBuf::from("archive.zar"));
        assert_eq!(opts.target1, Some("Z:".into()));
        assert_eq!(opts.target2, None);
        assert!(!opts.ask);
    }

    #[test]
    fn test_cli_parsing_archive_path_and_drive() {
        let args = ["zarmount", "archive.zar", "path/to/mount", "Z:"];
        let opts = Options::try_parse_from(args).unwrap();
        assert_eq!(opts.archive, PathBuf::from("archive.zar"));
        assert_eq!(opts.target1, Some("path/to/mount".into()));
        assert_eq!(opts.target2, Some("Z:".into()));
        assert!(!opts.ask);
    }

    #[test]
    fn test_cli_parsing_ask_flag() {
        let args1 = ["zarmount", "archive.zar", "--ask"];
        let opts1 = Options::try_parse_from(args1).unwrap();
        assert_eq!(opts1.archive, PathBuf::from("archive.zar"));
        assert!(opts1.ask);

        let args2 = ["zarmount", "--ask", "archive.zar"];
        let opts2 = Options::try_parse_from(args2).unwrap();
        assert_eq!(opts2.archive, PathBuf::from("archive.zar"));
        assert!(opts2.ask);
    }
}
