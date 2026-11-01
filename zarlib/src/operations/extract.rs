use super::common::*;
use super::filters::FilterSet;
use super::list::load_toc_through_with_passwords_and_progress;
use crate::{
    ArchivePath, BlockFrame, CommitHeader, CommitValidationLimits, ContentAead, DEK_SIZE,
    DeviceKind, EntryMetadata, FileEntry, HEADER_OFFSET_SIZE, Toc, TocChange, TocEntry,
};
#[cfg(not(target_os = "wasi"))]
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap};
#[cfg(unix)]
use std::ffi::CString;
use std::fs::File;
#[cfg(not(unix))]
use std::fs::OpenOptions;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use xxhash_rust::xxh64::Xxh64;
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct ExtractedBlock {
    pub offset: u64,
    pub block_index: u32,
    pub encoded_size: u64,
    pub uncompressed_size: usize,
    pub commit_header: CommitHeader,
    pub algorithm: Option<ContentAead>,
    pub dek: Option<Zeroizing<[u8; DEK_SIZE]>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractedKind {
    File,
    Symlink(String),
    NamedPipe,
    Device {
        kind: DeviceKind,
        major: u32,
        minor: u32,
    },
}

#[derive(Clone)]
pub struct ExtractedSource {
    pub entry: FileEntry,
    pub blocks: Vec<ExtractedBlock>,
    pub metadata: Option<EntryMetadata>,
    pub kind: ExtractedKind,
}

impl ExtractedSource {
    pub fn symlink(target: String, metadata: Option<EntryMetadata>) -> Self {
        Self {
            entry: FileEntry::new(0, Vec::new()).expect("empty file metadata is valid"),
            blocks: Vec::new(),
            metadata,
            kind: ExtractedKind::Symlink(target),
        }
    }

    pub fn named_pipe(metadata: Option<EntryMetadata>) -> Self {
        Self {
            entry: FileEntry::new(0, Vec::new()).expect("empty file metadata is valid"),
            blocks: Vec::new(),
            metadata,
            kind: ExtractedKind::NamedPipe,
        }
    }

    pub fn device(
        kind: DeviceKind,
        major: u32,
        minor: u32,
        metadata: Option<EntryMetadata>,
    ) -> Self {
        Self {
            entry: FileEntry::new(0, Vec::new()).expect("empty file metadata is valid"),
            blocks: Vec::new(),
            metadata,
            kind: ExtractedKind::Device { kind, major, minor },
        }
    }
}

pub type ExtractionSources = BTreeMap<ArchivePath, ExtractedSource>;
pub type ArchivedEntryMetadata = BTreeMap<ArchivePath, EntryMetadata>;
pub type LoadedExtractionState = (Toc, ExtractionSources, ArchivedEntryMetadata);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollisionPolicy {
    Error,
    Latest,
    First,
    Rename,
    Ask,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExistingPolicy {
    Overwrite,
    Skip,
    Error,
    Ask,
}

fn validate_link_target(target: &str, destination: &Path, output_path: &Path) -> io::Result<()> {
    let relative_target = Path::new(target);
    if relative_target.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "absolute symbolic-link targets are not allowed",
        ));
    }
    let parent = output_path.parent().unwrap_or(destination);
    let mut components = parent
        .strip_prefix(destination)
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "link path escaped extraction root",
            )
        })?
        .components()
        .collect::<Vec<_>>();
    for component in relative_target.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if components.pop().is_none() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "symbolic-link target escapes extraction root",
                    ));
                }
            }
            std::path::Component::Normal(value) => {
                components.push(std::path::Component::Normal(value))
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "unsupported symbolic-link target",
                ));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub struct ExtractionOptions<'a> {
    pub dry_run: bool,
    pub through: Option<&'a str>,
    pub collision_policy: CollisionPolicy,
    pub existing_policy: ExistingPolicy,
    pub restore_metadata: bool,
    pub restore_symlinks: bool,
    pub restore_devices: bool,
    /// Drops the first path component of every archive path.
    pub strip_top_level: bool,
}

pub fn extract_archive(
    archive: &str,
    destination: &Path,
    dry_run: bool,
) -> Result<ExtractionPlan, Box<dyn std::error::Error>> {
    extract_archive_with_progress(
        archive,
        destination,
        ExtractionOptions {
            dry_run,
            through: None,
            collision_policy: CollisionPolicy::Rename,
            existing_policy: ExistingPolicy::Error,
            restore_metadata: false,
            restore_symlinks: false,
            restore_devices: false,
            strip_top_level: false,
        },
        None,
        None,
    )
}

pub fn extract_archive_to_memory(
    archive: &str,
    through: Option<&str>,
    password: Option<&[u8]>,
    size_limit: Option<u64>,
    offset: u64,
) -> Result<HashMap<String, Vec<u8>>, Box<dyn std::error::Error>> {
    extract_archive_to_memory_with_filters(
        archive,
        through,
        password,
        size_limit,
        offset,
        &FilterSet::default(),
    )
}

pub fn extract_archive_to_memory_with_filters(
    archive: &str,
    through: Option<&str>,
    password: Option<&[u8]>,
    size_limit: Option<u64>,
    offset: u64,
    filters: &FilterSet,
) -> Result<HashMap<String, Vec<u8>>, Box<dyn std::error::Error>> {
    let passwords = PasswordSet::new(password.map(<[u8]>::to_vec));
    extract_archive_to_memory_with_passwords_and_filters(
        archive, through, &passwords, size_limit, offset, filters,
    )
}

pub fn extract_archive_to_memory_with_passwords(
    archive: &str,
    through: Option<&str>,
    passwords: &PasswordSet,
    size_limit: Option<u64>,
    offset: u64,
) -> Result<HashMap<String, Vec<u8>>, Box<dyn std::error::Error>> {
    extract_archive_to_memory_with_passwords_and_filters(
        archive,
        through,
        passwords,
        size_limit,
        offset,
        &FilterSet::default(),
    )
}

pub fn extract_archive_to_memory_with_passwords_and_filters(
    archive: &str,
    through: Option<&str>,
    passwords: &PasswordSet,
    size_limit: Option<u64>,
    offset: u64,
    filters: &FilterSet,
) -> Result<HashMap<String, Vec<u8>>, Box<dyn std::error::Error>> {
    let (toc, extraction_sources, _) =
        load_extraction_state_with_passwords(archive, through, passwords)?;
    let toc = filters.filter_toc(&toc)?;
    let mut archive_file = File::open(archive)?;
    let mut files = HashMap::new();

    for (path, entry) in toc.iter() {
        let TocEntry::File(file_entry) = entry else {
            continue;
        };
        let source = extraction_sources
            .get(path)
            .ok_or_else(|| format!("missing extraction source for {path}"))?;
        if !matches!(source.kind, ExtractedKind::File) || source.entry != *file_entry {
            return Err(format!("extraction source mismatch for {path}").into());
        }

        let requested_size = file_entry.size.saturating_sub(offset);
        let selected_size = size_limit.map_or(requested_size, |limit| requested_size.min(limit));
        let selected_capacity = usize::try_from(selected_size)
            .map_err(|_| format!("selected file contents are too large for {path}"))?;
        let mut contents = Vec::new();
        contents.try_reserve_exact(selected_capacity)?;

        let selected_end = offset
            .checked_add(selected_size)
            .ok_or("selected file range overflow")?;
        let read_full_file = offset == 0 && selected_end == file_entry.size;
        let mut file_offset = 0_u64;
        let mut checksum = Xxh64::new(0);

        for block in &source.blocks {
            let block_end = file_offset
                .checked_add(block.uncompressed_size as u64)
                .ok_or("decoded file size overflow")?;
            if file_offset < selected_end && block_end > offset {
                let block_contents = decode_extraction_block(&mut archive_file, block)
                    .map_err(ExtractionFailure::from_send_sync)?;
                if block_contents.len() != block.uncompressed_size {
                    return Err("archive block changed after extraction validation".into());
                }
                if read_full_file {
                    checksum.update(&block_contents);
                }
                let start = usize::try_from(offset.saturating_sub(file_offset))
                    .map_err(|_| "selected block offset is too large")?;
                let end = usize::try_from(selected_end.min(block_end) - file_offset)
                    .map_err(|_| "selected block end is too large")?;
                contents.extend_from_slice(&block_contents[start..end]);
            }
            file_offset = block_end;
        }

        if file_offset != file_entry.size || contents.len() != selected_capacity {
            return Err(format!("decoded size mismatch for {path}").into());
        }
        if read_full_file
            && file_entry.size != 0
            && file_entry
                .checksum
                .is_some_and(|expected| expected != checksum.digest())
        {
            return Err(format!("file checksum mismatch for {path}").into());
        }
        files.insert(path.as_str().to_owned(), contents);
    }

    Ok(files)
}

/// Extracts exactly one file into `output` in a single pass over the archive.
/// Only the data blocks of that file are read and decoded.
pub fn extract_single_file(
    archive: &str,
    path: &str,
    output: &Path,
    passwords: &PasswordSet,
) -> Result<u64, Box<dyn std::error::Error>> {
    let wanted = FilterSet::from_patterns(&[escape_glob(path)], &[], &[], &[], &[] as &[PathBuf])?;
    let (toc, sources, _) =
        load_extraction_state_impl(archive, None, passwords, None, Some(&wanted), None)?;
    let archive_path = ArchivePath::new(path)?;
    let Some(TocEntry::File(file_entry)) = toc
        .iter()
        .find_map(|(entry_path, entry)| (*entry_path == archive_path).then_some(entry))
    else {
        return Err(format!("{path} is not a file in the archive").into());
    };
    let source = sources
        .get(&archive_path)
        .ok_or_else(|| format!("missing extraction source for {path}"))?;
    if !matches!(source.kind, ExtractedKind::File) || source.entry != *file_entry {
        return Err(format!("extraction source mismatch for {path}").into());
    }
    let mut archive_file = File::open(archive)?;
    let mut writer = io::BufWriter::new(File::create(output)?);
    let mut checksum = Xxh64::new(0);
    let mut written = 0_u64;
    for block in &source.blocks {
        let contents = decode_extraction_block(&mut archive_file, block)
            .map_err(ExtractionFailure::from_send_sync)?;
        if contents.len() != block.uncompressed_size {
            return Err("archive block changed after extraction validation".into());
        }
        checksum.update(&contents);
        writer.write_all(&contents)?;
        written += contents.len() as u64;
    }
    writer.flush()?;
    if written != file_entry.size {
        return Err(format!("decoded size mismatch for {path}").into());
    }
    if file_entry.size != 0
        && file_entry
            .checksum
            .is_some_and(|expected| expected != checksum.digest())
    {
        return Err(format!("file checksum mismatch for {path}").into());
    }
    Ok(written)
}

fn escape_glob(path: &str) -> String {
    let mut escaped = String::with_capacity(path.len());
    for character in path.chars() {
        if matches!(character, '*' | '?' | '[' | '{') {
            escaped.push('[');
            escaped.push(character);
            escaped.push(']');
        } else {
            escaped.push(character);
        }
    }
    escaped
}

pub fn extract_archive_with_options(
    archive: &str,
    destination: &Path,
    dry_run: bool,
    through: Option<&str>,
    password: Option<&[u8]>,
    collision_policy: CollisionPolicy,
    existing_policy: ExistingPolicy,
) -> Result<ExtractionPlan, Box<dyn std::error::Error>> {
    extract_archive_with_progress(
        archive,
        destination,
        ExtractionOptions {
            dry_run,
            through,
            collision_policy,
            existing_policy,
            restore_metadata: false,
            restore_symlinks: false,
            restore_devices: false,
            strip_top_level: false,
        },
        password,
        None,
    )
}

#[derive(Clone)]
pub struct ExtractionPlan {
    pub toc: Toc,
    pub directories: Vec<PathBuf>,
    pub files: Vec<(PathBuf, FileEntry, ExtractedKind)>,
}

pub fn extract_archive_with_progress(
    archive: &str,
    destination: &Path,
    options: ExtractionOptions<'_>,
    password: Option<&[u8]>,
    progress: Option<&dyn Progress>,
) -> Result<ExtractionPlan, Box<dyn std::error::Error>> {
    extract_archive_with_progress_and_filters(
        archive,
        destination,
        options,
        password,
        progress,
        &FilterSet::default(),
    )
}

pub fn extract_archive_with_progress_and_filters(
    archive: &str,
    destination: &Path,
    options: ExtractionOptions<'_>,
    password: Option<&[u8]>,
    progress: Option<&dyn Progress>,
    filters: &FilterSet,
) -> Result<ExtractionPlan, Box<dyn std::error::Error>> {
    let passwords = PasswordSet::new(password.map(<[u8]>::to_vec));
    extract_archive_with_passwords_progress_and_filters(
        archive,
        destination,
        options,
        &passwords,
        progress,
        filters,
    )
}

pub fn extract_archive_with_passwords_progress_and_filters(
    archive: &str,
    destination: &Path,
    options: ExtractionOptions<'_>,
    passwords: &PasswordSet,
    progress: Option<&dyn Progress>,
    filters: &FilterSet,
) -> Result<ExtractionPlan, Box<dyn std::error::Error>> {
    let mut dest_buf = destination.canonicalize().unwrap_or_else(|_| {
        if destination.is_relative() {
            std::env::current_dir()
                .map(|cwd| cwd.join(destination))
                .unwrap_or_else(|_| destination.to_path_buf())
        } else {
            destination.to_path_buf()
        }
    });
    if let Ok(stripped) = dest_buf.strip_prefix(r"\\?\") {
        dest_buf = stripped.to_path_buf();
    }
    let destination = dest_buf.as_path();
    let ExtractionOptions {
        dry_run,
        through,
        collision_policy,
        existing_policy,
        restore_metadata,
        restore_symlinks,
        restore_devices,
        strip_top_level,
    } = options;
    let progress = ProgressReporter::new(progress);
    let archive_size = File::open(archive)?.metadata()?.len().saturating_sub(4);
    progress.start_phase("Reading archive headers", archive_size.max(1));
    let full_toc =
        load_toc_through_with_passwords_and_progress(archive, through, passwords, Some(&progress))?;
    let toc = filters.filter_toc(&full_toc)?;
    progress.reset_phase("Planning extraction", archive_size.max(1));
    let summary = summarize_toc(&toc, restore_symlinks, restore_devices)?;
    progress.extraction_summary(&summary);
    // The loader reuses the TOC built above instead of rebuilding it.
    let (_, extraction_sources, entry_metadata) = if dry_run {
        load_extraction_state_impl(
            archive,
            through,
            passwords,
            None,
            Some(filters),
            Some(full_toc),
        )?
    } else {
        progress.reset_phase("Preparing extraction", archive_size.max(1));
        progress.set_message("Validating data frames");
        load_extraction_state_impl(
            archive,
            through,
            passwords,
            Some(&progress),
            Some(filters),
            Some(full_toc),
        )?
    };
    validate_interactive_policies(
        progress.0.is_some_and(Progress::is_interactive),
        collision_policy,
        existing_policy,
    )?;
    let mut planned_files: Vec<(PathBuf, ExtractedSource)> = Vec::new();
    let mut planned_directories = Vec::new();
    let mut collision_indices = BTreeMap::<String, usize>::new();
    let mut reserved_paths: std::collections::BTreeSet<String> = toc
        .iter()
        .map(|(path, _)| path.as_str().to_lowercase())
        .collect();

    for (path, entry) in toc.iter() {
        let relative = if strip_top_level {
            match path.as_str().split_once('/') {
                Some((_, rest)) => rest,
                None if matches!(entry, TocEntry::Directory) => continue,
                None => path.as_str(),
            }
        } else {
            path.as_str()
        };
        let target = relative
            .split('/')
            .fold(destination.to_path_buf(), |target, component| {
                target.join(component)
            });
        match entry {
            TocEntry::Directory => {
                planned_directories.push((target, entry_metadata.get(path).cloned()))
            }
            TocEntry::File(file_entry) => {
                let source = extraction_sources
                    .get(path)
                    .ok_or_else(|| format!("missing extraction source for {path}"))?;
                if !matches!(source.kind, ExtractedKind::File) || source.entry != *file_entry {
                    return Err(format!("extraction source mismatch for {path}").into());
                }
                let mut source = source.clone();
                source.metadata = entry_metadata.get(path).cloned();
                let collision_key = relative.to_lowercase();
                if let Some(&index) = collision_indices.get(&collision_key) {
                    match collision_policy {
                        CollisionPolicy::Error => {
                            return Err(format!("destination path collision: {path}").into());
                        }
                        CollisionPolicy::First => continue,
                        CollisionPolicy::Latest => {
                            planned_files[index] = (target, source.clone());
                            continue;
                        }
                        CollisionPolicy::Rename => {
                            let renamed = next_renamed_path(&target, &mut reserved_paths);
                            planned_files.push((renamed, source.clone()));
                            continue;
                        }
                        CollisionPolicy::Ask => {
                            if progress.0.unwrap().confirm(&format!(
                                "Replace colliding extraction entry with `{path}`? [y/N] "
                            ))? {
                                planned_files[index] = (target, source.clone());
                            }
                            continue;
                        }
                    }
                }
                collision_indices.insert(collision_key, planned_files.len());
                planned_files.push((target, source));
            }
            TocEntry::Symlink {
                target: link_target,
            } => {
                if restore_symlinks {
                    planned_files.push((
                        target,
                        ExtractedSource::symlink(
                            link_target.clone(),
                            entry_metadata.get(path).cloned(),
                        ),
                    ));
                }
            }
            TocEntry::NamedPipe => planned_files.push((
                target,
                ExtractedSource::named_pipe(entry_metadata.get(path).cloned()),
            )),
            TocEntry::Device { kind, major, minor } if restore_devices => planned_files.push((
                target,
                ExtractedSource::device(*kind, *major, *minor, entry_metadata.get(path).cloned()),
            )),
            TocEntry::Device { .. } => {}
        }
    }

    let mut accepted_directories = Vec::new();
    for (target, metadata) in &planned_directories {
        if target.exists() && !target.is_dir() {
            return Err(
                format!("destination path is not a directory: {}", target.display()).into(),
            );
        }
        accepted_directories.push((target.clone(), metadata.clone()));
    }
    let mut accepted_files = Vec::new();
    for (target, source) in planned_files {
        if !matches!(source.kind, ExtractedKind::File) {
            validate_special_target(&target, destination, &source.kind)?;
            let exists = std::fs::symlink_metadata(&target).is_ok();
            if exists {
                match existing_policy {
                    ExistingPolicy::Overwrite => {}
                    ExistingPolicy::Skip => continue,
                    ExistingPolicy::Error => {
                        return Err(format!(
                            "destination path already exists: {}",
                            target.display()
                        )
                        .into());
                    }
                    ExistingPolicy::Ask => {
                        if !progress
                            .0
                            .unwrap()
                            .confirm(&format!("Replace `{}`? [y/N] ", target.display()))?
                        {
                            continue;
                        }
                    }
                }
            }
            accepted_files.push((target, source));
            continue;
        }
        if existing_policy != ExistingPolicy::Overwrite && target.exists() {
            match existing_policy {
                ExistingPolicy::Overwrite => unreachable!(),
                ExistingPolicy::Skip => continue,
                ExistingPolicy::Error => {
                    return Err(
                        format!("destination file already exists: {}", target.display()).into(),
                    );
                }
                ExistingPolicy::Ask => {
                    if !progress
                        .0
                        .unwrap()
                        .confirm(&format!("Overwrite `{}`? [y/N] ", target.display()))?
                    {
                        continue;
                    }
                }
            }
        }
        accepted_files.push((target, source));
    }

    let plan = ExtractionPlan {
        toc: toc.clone(),
        directories: accepted_directories
            .iter()
            .map(|(path, _)| path.clone())
            .collect(),
        files: accepted_files
            .iter()
            .map(|(path, source)| (path.clone(), source.entry.clone(), source.kind.clone()))
            .collect(),
    };
    if dry_run {
        return Ok(plan);
    }

    let total_progress = accepted_files
        .iter()
        .try_fold(accepted_files.len() as u64, |total, (_, source)| {
            let bytes = if matches!(source.kind, ExtractedKind::File) {
                source.entry.size
            } else {
                0
            };
            total.checked_add(bytes)
        })
        .ok_or("extraction progress size overflow")?;
    progress.reset_phase("Extracting files", total_progress.max(1));
    let directory_paths = accepted_directories
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    validate_destination_paths(
        destination,
        &directory_paths,
        &accepted_files,
        existing_policy,
    )?;
    create_destination_directory(destination)?;
    let mut all_dirs = std::collections::BTreeSet::new();
    for (directory, _) in &accepted_directories {
        all_dirs.insert(directory.clone());
    }
    for (target, _) in &accepted_files {
        if let Some(parent) = target.parent() {
            all_dirs.insert(parent.to_path_buf());
        }
    }
    for directory in &all_dirs {
        ensure_directory_no_follow(destination, directory)?;
    }

    let mut regular_files = Vec::new();
    for (target, source) in accepted_files {
        if matches!(source.kind, ExtractedKind::File) {
            regular_files.push((target, source));
            continue;
        }
        progress.set_message(format!("Extracting {}", target.display()));
        if let Some(parent) = target.parent() {
            ensure_directory_no_follow(destination, parent)?;
        }
        create_special_entry(destination, &target, &source.kind, existing_policy)?;
        apply_special_metadata(
            &target,
            source.metadata.as_ref(),
            restore_metadata,
            progress.0,
        )?;
        progress.inc(1);
    }
    regular_files.sort_by_key(|(_, source)| source.blocks.first().map(|b| b.offset).unwrap_or(0));

    #[cfg(target_os = "wasi")]
    for (target, source) in regular_files {
        let mut archive_file = File::open(archive)?;
        extract_regular_file(
            &mut archive_file,
            destination,
            &target,
            &source,
            existing_policy,
            restore_metadata,
            progress.0,
        )
        .map_err(extraction_worker_error)?;
    }

    #[cfg(not(target_os = "wasi"))]
    {
        let workers = compression_worker_count(CREATE_BLOCK_SIZE);
        let parallel_files = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .thread_name(|index| format!("zarc-extract-{index}"))
            .build()?;
        parallel_files
            .install(|| {
                regular_files.into_par_iter().try_for_each_init(
                    || File::open(archive),
                    |archive_file, (target, source)| {
                        let archive_file = match archive_file {
                            Ok(archive_file) => archive_file,
                            Err(error) => {
                                return Err(Box::new(ExtractionFailure::wrap(io::Error::new(
                                    error.kind(),
                                    error.to_string(),
                                )))
                                    as Box<dyn std::error::Error + Send + Sync>);
                            }
                        };
                        extract_regular_file(
                            archive_file,
                            destination,
                            &target,
                            &source,
                            existing_policy,
                            restore_metadata,
                            progress.0,
                        )
                    },
                )
            })
            .map_err(extraction_worker_error)?;
    }
    if restore_metadata {
        restore_directory_metadata(destination, &accepted_directories, progress.0)?;
    }
    Ok(plan)
}

fn summarize_toc(
    toc: &Toc,
    restore_symlinks: bool,
    restore_devices: bool,
) -> Result<InputSummary, Box<dyn std::error::Error>> {
    let mut summary = InputSummary::default();
    for (_, entry) in toc.iter() {
        match entry {
            TocEntry::Directory => summary.directories += 1,
            TocEntry::File(file) => {
                summary.files += 1;
                summary.total_bytes = summary
                    .total_bytes
                    .checked_add(file.size)
                    .ok_or("extraction size estimate overflow")?;
                if file.size == 0 {
                    summary.empty_files += 1;
                } else if file.size <= 8 {
                    summary.tiny_files += 1;
                } else if file.size < 8 * 1024 * 1024 {
                    summary.small_files += 1;
                } else {
                    summary.big_files += 1;
                }
            }
            TocEntry::Symlink { .. } if restore_symlinks => summary.symlinks += 1,
            TocEntry::NamedPipe => summary.named_pipes += 1,
            TocEntry::Device { .. } if restore_devices => summary.devices += 1,
            TocEntry::Symlink { .. } | TocEntry::Device { .. } => {}
        }
    }
    Ok(summary)
}

#[derive(Debug)]
struct ExtractionFailure {
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl ExtractionFailure {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }
    fn wrap(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self {
            message: error.to_string(),
            source: Some(Box::new(error)),
        }
    }

    fn from_send_sync(error: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Self {
            message: error.to_string(),
            source: Some(error),
        }
    }

    fn from_error(error: Box<dyn std::error::Error>) -> Self {
        if let Some(io_error) = error.downcast_ref::<io::Error>() {
            return Self::wrap(io::Error::new(io_error.kind(), error.to_string()));
        }
        Self::new(error.to_string())
    }
}

impl std::fmt::Display for ExtractionFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for ExtractionFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|error| error as &(dyn std::error::Error + 'static))
    }
}

fn extraction_worker_error(
    error: Box<dyn std::error::Error + Send + Sync>,
) -> Box<dyn std::error::Error> {
    Box::new(ExtractionFailure::from_send_sync(error))
}

fn extract_regular_file(
    archive: &mut File,
    destination: &Path,
    target: &Path,
    source: &ExtractedSource,
    existing_policy: ExistingPolicy,
    restore_metadata: bool,
    callback: Option<&dyn Progress>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let progress = ProgressReporter::new(callback);
    let mut output = match create_extraction_file(destination, target, existing_policy) {
        Ok(output) => output,
        Err(error)
            if existing_policy == ExistingPolicy::Skip
                && error.kind() == io::ErrorKind::AlreadyExists =>
        {
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    let mut written = 0_u64;
    let mut checksum = Xxh64::new(0);
    for block in &source.blocks {
        let contents =
            decode_extraction_block(archive, block).map_err(ExtractionFailure::from_send_sync)?;
        if contents.len() != block.uncompressed_size {
            return Err(Box::new(ExtractionFailure::new(
                "archive block changed after extraction validation",
            )));
        }
        output.write_all(&contents)?;
        checksum.update(&contents);
        progress.inc(contents.len() as u64);
        written = written
            .checked_add(contents.len() as u64)
            .ok_or_else(|| ExtractionFailure::new("extracted file size overflow"))?;
    }
    if written != source.entry.size {
        return Err(Box::new(ExtractionFailure::new(format!(
            "decoded size mismatch: expected {} bytes, got {written}",
            source.entry.size
        ))));
    }
    if written != 0
        && source
            .entry
            .checksum
            .is_some_and(|expected| expected != checksum.digest())
    {
        return Err(Box::new(ExtractionFailure::new(format!(
            "file checksum mismatch for {}",
            target.display()
        ))));
    }
    if restore_metadata {
        apply_metadata(&output, source.metadata.as_ref(), target, callback)
            .map_err(ExtractionFailure::from_error)?;
    }
    drop(output);
    progress.inc(1);
    Ok(())
}

fn decode_extraction_block(
    archive_file: &mut File,
    block: &ExtractedBlock,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    archive_file.seek(SeekFrom::Start(block.offset))?;
    let mut raw = vec![0u8; block.encoded_size as usize];
    archive_file.read_exact(&mut raw)?;
    let frame = BlockFrame::read_from(
        &mut raw.as_slice(),
        CommitValidationLimits::default().max_frame_payload_size,
    )?;

    if frame.block_index != block.block_index {
        return Err(Box::new(ExtractionFailure::new(
            "archive block index changed after extraction validation",
        )));
    }
    let contents = if frame.flags & crate::BLOCK_FLAG_ENCRYPTED != 0 {
        let algorithm = block
            .algorithm
            .ok_or_else(|| ExtractionFailure::new("encrypted block has no content algorithm"))?;
        let key = block
            .dek
            .as_ref()
            .ok_or_else(|| ExtractionFailure::new("encrypted block has not been unlocked"))?;
        frame.uncompressed_encrypted(
            &block.commit_header,
            key,
            algorithm,
            CommitValidationLimits::default().max_uncompressed_block_size,
        )?
    } else {
        frame.uncompressed_at_most(CommitValidationLimits::default().max_uncompressed_block_size)?
    };
    Ok(contents)
}

fn validate_extraction_block_batch(
    batch: Vec<(u64, BlockFrame)>,
    commit_header: &CommitHeader,
    algorithm: Option<ContentAead>,
    dek: Option<&Zeroizing<[u8; DEK_SIZE]>>,
    limits: CommitValidationLimits,
) -> Result<HashMap<u32, ExtractedBlock>, Box<dyn std::error::Error>> {
    #[cfg(target_os = "wasi")]
    let validated = batch.into_iter().map(|(offset, frame)| {
        validate_extraction_frame(offset, frame, commit_header, algorithm, dek, limits)
    });
    #[cfg(not(target_os = "wasi"))]
    let validated = batch.into_par_iter().map(|(offset, frame)| {
        validate_extraction_frame(offset, frame, commit_header, algorithm, dek, limits)
    });
    validated
        .collect::<Result<HashMap<_, _>, ExtractionFailure>>()
        .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
}

fn validate_extraction_frame(
    offset: u64,
    frame: BlockFrame,
    commit_header: &CommitHeader,
    algorithm: Option<ContentAead>,
    dek: Option<&Zeroizing<[u8; DEK_SIZE]>>,
    limits: CommitValidationLimits,
) -> Result<(u32, ExtractedBlock), ExtractionFailure> {
    let uncompressed_size = if frame.flags & crate::BLOCK_FLAG_ENCRYPTED != 0 {
        let (Some(key), Some(algorithm)) = (dek, algorithm) else {
            return Err(ExtractionFailure::new(
                "cannot extract data-encrypted blocks without a password",
            ));
        };
        frame
            .uncompressed_encrypted(
                commit_header,
                key,
                algorithm,
                limits.max_uncompressed_block_size,
            )
            .map_err(ExtractionFailure::wrap)?
            .len()
    } else if frame.flags & crate::BLOCK_FLAG_UNCOMPRESSED != 0 {
        if frame.payload.len() > limits.max_uncompressed_block_size {
            return Err(ExtractionFailure::wrap(crate::FramingError::InvalidSize));
        }
        frame.payload.len()
    } else if let Ok(Some(content_size)) = zstd::zstd_safe::get_frame_content_size(&frame.payload) {
        let content_size = usize::try_from(content_size)
            .map_err(|_| ExtractionFailure::wrap(crate::FramingError::InvalidSize))?;
        if content_size > limits.max_uncompressed_block_size {
            return Err(ExtractionFailure::wrap(crate::FramingError::InvalidSize));
        }
        content_size
    } else {
        frame
            .uncompressed_at_most(limits.max_uncompressed_block_size)
            .map_err(ExtractionFailure::wrap)?
            .len()
    };
    Ok((
        frame.block_index,
        ExtractedBlock {
            offset,
            block_index: frame.block_index,
            encoded_size: u64::try_from(frame.encoded_size().map_err(ExtractionFailure::wrap)?)
                .map_err(ExtractionFailure::wrap)?,
            uncompressed_size,
            commit_header: commit_header.clone(),
            algorithm,
            dek: dek.map(|key| Zeroizing::new(**key)),
        },
    ))
}

/// Buffered reader whose relative seeks keep the buffer, so skipping small
/// block payloads does not cost a syscall per block. It tracks the logical
/// position itself because querying it would require a syscall.
struct RelativeSeekReader<R> {
    inner: BufReader<R>,
    position: u64,
}

impl<R: Read + Seek> RelativeSeekReader<R> {
    fn new(mut reader: R) -> io::Result<Self> {
        let position = reader.stream_position()?;
        Ok(Self {
            inner: BufReader::with_capacity(128 * 1024, reader),
            position,
        })
    }
}

impl<R: Read> Read for RelativeSeekReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buf)?;
        self.position += read as u64;
        Ok(read)
    }
}

impl<R: Read + Seek> Seek for RelativeSeekReader<R> {
    fn seek(&mut self, target: SeekFrom) -> io::Result<u64> {
        match target {
            SeekFrom::Current(offset) => {
                self.inner.seek_relative(offset)?;
                self.position = self
                    .position
                    .checked_add_signed(offset)
                    .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
            }
            other => self.position = self.inner.seek(other)?,
        }
        Ok(self.position)
    }

    fn stream_position(&mut self) -> io::Result<u64> {
        Ok(self.position)
    }
}

pub fn load_extraction_state(
    archive: &str,
    through: Option<&str>,
    password: Option<&[u8]>,
) -> Result<LoadedExtractionState, Box<dyn std::error::Error>> {
    load_extraction_state_with_progress(archive, through, password, None)
}

pub fn load_extraction_state_with_progress(
    archive: &str,
    through: Option<&str>,
    password: Option<&[u8]>,
    progress: Option<&ProgressReporter<'_>>,
) -> Result<LoadedExtractionState, Box<dyn std::error::Error>> {
    let passwords = PasswordSet::new(password.map(<[u8]>::to_vec));
    load_extraction_state_with_passwords_progress(archive, through, &passwords, progress)
}

pub fn load_extraction_state_with_passwords(
    archive: &str,
    through: Option<&str>,
    passwords: &PasswordSet,
) -> Result<LoadedExtractionState, Box<dyn std::error::Error>> {
    load_extraction_state_with_passwords_progress(archive, through, passwords, None)
}

pub fn load_extraction_state_with_passwords_progress(
    archive: &str,
    through: Option<&str>,
    passwords: &PasswordSet,
    progress: Option<&ProgressReporter<'_>>,
) -> Result<LoadedExtractionState, Box<dyn std::error::Error>> {
    load_extraction_state_impl(archive, through, passwords, progress, None, None)
}

/// Blocks that only belong to files rejected by `filters` are not read or validated.
/// A `known_toc` for the same archive and `through` is returned as is instead of being rebuilt.
fn load_extraction_state_impl(
    archive: &str,
    through: Option<&str>,
    passwords: &PasswordSet,
    progress: Option<&ProgressReporter<'_>>,
    filters: Option<&FilterSet>,
    known_toc: Option<Toc>,
) -> Result<LoadedExtractionState, Box<dyn std::error::Error>> {
    let mut file = File::open(archive)?;
    let requested_id = through.map(parse_commit_id).transpose()?;
    let rebuild_toc = known_toc.is_none();
    let mut toc = known_toc.unwrap_or_default();
    let mut file_contents = BTreeMap::<ArchivePath, ExtractedSource>::new();
    let mut archived_special_entries = BTreeMap::<ArchivePath, ExtractedSource>::new();
    let mut archived_metadata = BTreeMap::<ArchivePath, EntryMetadata>::new();
    let mut header_bytes_read = 0_u64;
    let mut progressed_header_bytes = 0_u64;
    let mut total_blocks = 0_u64;
    let mut matches = 0_u32;
    let mut stopped_at_through = false;
    let limits = CommitValidationLimits::default();

    let mut header_offset = read_first_header_offset(&mut file)?;
    while header_offset != 0 {
        let (commit_header, data_start, next_header_offset) =
            read_commit_at_offset(&mut file, header_offset, limits)?;
        let matched_this_commit = requested_id == Some(commit_header.commit_id);
        if matched_this_commit {
            matches += 1;
            if matches > 1 {
                return Err("--through commit ID is ambiguous".into());
            }
        }
        let _commit_end = header_offset
            .checked_add(u64::from(commit_header.header_frame_size))
            .and_then(|size| size.checked_add(HEADER_OFFSET_SIZE))
            .ok_or("commit end overflow")?;
        if stopped_at_through {
            header_offset = next_header_offset;
            continue;
        }
        file.seek(SeekFrom::Start(header_offset))?;
        let header_start = header_offset;
        let (validated, dek) = read_header_with_passwords(&mut file, limits, passwords)?;
        header_bytes_read = header_bytes_read
            .checked_add(file.stream_position()?.saturating_sub(header_start))
            .ok_or("extraction progress size overflow")?;
        let header = validated
            .logical_header
            .as_ref()
            .ok_or("cannot extract a header-encrypted commit; use --password")?;
        if validated.encryption.is_data_encrypted() && dek.is_none() {
            return Err("cannot extract a data-encrypted commit without a password".into());
        }
        let mut block_indices = std::collections::HashSet::new();
        let mut nonces = std::collections::HashSet::new();
        if validated.encryption.is_header_encrypted()
            && let Some(nonce) = extraction_content_nonce(&validated.encryption)
        {
            nonces.insert(nonce);
        }
        let algorithm = encryption_algorithm(&validated.encryption);
        file.seek(SeekFrom::Start(data_start))?;
        let mut remaining = commit_header.data_section_size;
        let mut block_offset = data_start;
        let mut blocks = HashMap::new();
        let mut pending_blocks = Vec::new();
        let workers = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .max(1);
        let wanted_blocks: Option<std::collections::HashSet<u32>> = filters.map(|filters| {
            header
                .changes
                .iter()
                .filter_map(|change| match change {
                    TocChange::PutFile { path, entry }
                        if filters.includes(path.as_str(), false) =>
                    {
                        Some(entry.block_indices.iter().copied())
                    }
                    _ => None,
                })
                .flatten()
                .collect()
        });
        let mut data = RelativeSeekReader::new(&mut file)?;
        while remaining > 0 {
            let remaining_before = remaining;
            let offset = block_offset;
            let (block_index, flags, nonce, encoded_size, frame) = match &wanted_blocks {
                None => {
                    let frame = BlockFrame::read_from(&mut data, limits.max_frame_payload_size)?;
                    let encoded_size = u64::try_from(frame.encoded_size()?)?;
                    (
                        frame.block_index,
                        frame.flags,
                        frame.nonce,
                        encoded_size,
                        Some(frame),
                    )
                }
                Some(wanted) => {
                    let metadata =
                        BlockFrame::read_metadata_from(&mut data, limits.max_frame_payload_size)?;
                    let frame = if wanted.contains(&metadata.block_index) {
                        data.seek(SeekFrom::Start(offset))?;
                        Some(BlockFrame::read_from(
                            &mut data,
                            limits.max_frame_payload_size,
                        )?)
                    } else {
                        None
                    };
                    (
                        metadata.block_index,
                        metadata.flags,
                        metadata.nonce,
                        metadata.encoded_size,
                        frame,
                    )
                }
            };
            if encoded_size == 0
                || encoded_size > remaining_before
                || !block_indices.insert(block_index)
                || (flags & crate::BLOCK_FLAG_ENCRYPTED != 0)
                    != validated.encryption.is_data_encrypted()
            {
                return Err(crate::FramingError::InvalidSize.into());
            }
            if let Some(nonce) = nonce
                && !nonces.insert(nonce)
            {
                return Err(crate::FramingError::InvalidDescriptor.into());
            }
            block_offset = block_offset
                .checked_add(encoded_size)
                .ok_or("archive offset overflow")?;
            remaining -= encoded_size;
            if frame.is_none() {
                blocks.insert(
                    block_index,
                    ExtractedBlock {
                        offset,
                        block_index,
                        encoded_size,
                        uncompressed_size: 0,
                        commit_header: commit_header.clone(),
                        algorithm,
                        dek: dek.as_ref().map(|key| Zeroizing::new(**key)),
                    },
                );
            }
            if let Some(frame) = frame {
                pending_blocks.push((offset, frame));
                if pending_blocks.len() >= workers {
                    blocks.extend(validate_extraction_block_batch(
                        std::mem::take(&mut pending_blocks),
                        &commit_header,
                        algorithm,
                        dek.as_ref(),
                        limits,
                    )?);
                }
            }
            if let Some(progress) = progress {
                progress.inc(encoded_size);
            }
        }
        if !pending_blocks.is_empty() {
            blocks.extend(validate_extraction_block_batch(
                pending_blocks,
                &commit_header,
                algorithm,
                dek.as_ref(),
                limits,
            )?);
        }
        if let Some(progress) = progress {
            progress.inc(header_bytes_read.saturating_sub(progressed_header_bytes));
        }
        progressed_header_bytes = header_bytes_read;
        total_blocks = total_blocks
            .checked_add(block_indices.len() as u64)
            .ok_or("archive block count overflow")?;
        if total_blocks > limits.max_blocks {
            return Err("archive exceeds the extraction block limit".into());
        }

        if rebuild_toc {
            toc.apply_commit_with_available_blocks(&header.changes, &block_indices)?;
        } else {
            Toc::check_available_blocks(&header.changes, &block_indices)?;
        }
        {
            for change in &header.changes {
                if let TocChange::PutFile { path, entry } = change {
                    if filters.is_some_and(|filters| !filters.includes(path.as_str(), false)) {
                        continue;
                    }
                    let decoded_size =
                        entry.block_indices.iter().try_fold(0_u64, |size, index| {
                            size.checked_add(
                                blocks
                                    .get(index)
                                    .ok_or("missing decoded block")?
                                    .uncompressed_size as u64,
                            )
                            .ok_or("decoded file size overflow")
                        })?;
                    if decoded_size != entry.size {
                        return Err(format!(
                            "decoded size mismatch for {path}: expected {} bytes, got {decoded_size}",
                            entry.size
                        ).into());
                    }
                }
            }
        }
        for change in &header.changes {
            match change {
                TocChange::PutFile { path, .. }
                | TocChange::PutDirectory { path }
                | TocChange::PutSymlink { path, .. }
                | TocChange::PutNamedPipe { path }
                | TocChange::PutDevice { path, .. }
                | TocChange::RemoveFile { path } => {
                    archived_metadata.remove(path);
                    archived_special_entries.remove(path);
                    file_contents.remove(path);
                }
                TocChange::RemoveDirectory { path } => {
                    archived_metadata.retain(|existing, _| {
                        existing != path
                            && !existing
                                .as_str()
                                .starts_with(&format!("{}/", path.as_str()))
                    });
                    archived_special_entries.retain(|existing, _| {
                        existing != path
                            && !existing
                                .as_str()
                                .starts_with(&format!("{}/", path.as_str()))
                    });
                    file_contents.retain(|existing, _| {
                        existing != path
                            && !existing
                                .as_str()
                                .starts_with(&format!("{}/", path.as_str()))
                    });
                }
            }
        }
        for metadata in &header.entry_metadata {
            archived_metadata.insert(metadata.path.clone(), metadata.clone());
        }
        for change in &header.changes {
            match change {
                TocChange::PutFile { path, entry } => {
                    let mut source_blocks = Vec::with_capacity(entry.block_indices.len());
                    for index in &entry.block_indices {
                        let block = blocks
                            .get(index)
                            .ok_or_else(|| format!("TOC references unavailable block {index}"))?
                            .clone();
                        source_blocks.push(block);
                    }
                    let archived_metadata = header
                        .entry_metadata
                        .iter()
                        .find(|metadata| metadata.path == *path)
                        .cloned();
                    file_contents.insert(
                        path.clone(),
                        ExtractedSource {
                            entry: entry.clone(),
                            blocks: source_blocks,
                            metadata: archived_metadata,
                            kind: ExtractedKind::File,
                        },
                    );
                }
                TocChange::RemoveFile { path } => {
                    file_contents.remove(path);
                }
                TocChange::RemoveDirectory { path } => {
                    let prefix = format!("{}/", path.as_str());
                    file_contents.retain(|existing, _| {
                        existing != path && !existing.as_str().starts_with(&prefix)
                    });
                }
                TocChange::PutDirectory { .. } => {}
                TocChange::PutSymlink { path, target } => {
                    archived_special_entries.insert(
                        path.clone(),
                        ExtractedSource::symlink(
                            target.clone(),
                            archived_metadata.get(path).cloned(),
                        ),
                    );
                }
                TocChange::PutNamedPipe { path } => {
                    archived_special_entries.insert(
                        path.clone(),
                        ExtractedSource::named_pipe(archived_metadata.get(path).cloned()),
                    );
                }
                TocChange::PutDevice {
                    path,
                    kind,
                    major,
                    minor,
                } => {
                    archived_special_entries.insert(
                        path.clone(),
                        ExtractedSource::device(
                            *kind,
                            *major,
                            *minor,
                            archived_metadata.get(path).cloned(),
                        ),
                    );
                }
            }
        }
        for metadata in &header.entry_metadata {
            archived_metadata.insert(metadata.path.clone(), metadata.clone());
        }
        header_offset = next_header_offset;
        if matched_this_commit {
            stopped_at_through = true;
        }
    }

    if requested_id.is_some() && matches == 0 {
        return Err("--through commit ID was not found".into());
    }
    for (path, metadata) in &archived_metadata {
        if let Some(source) = file_contents.get_mut(path) {
            source.metadata = Some(metadata.clone());
        }
        if let Some(source) = archived_special_entries.get_mut(path) {
            source.metadata = Some(metadata.clone());
        }
    }
    file_contents.extend(archived_special_entries);
    Ok((toc, file_contents, archived_metadata))
}

#[cfg(unix)]
fn restore_directory_metadata(
    destination: &Path,
    directories: &[(PathBuf, Option<EntryMetadata>)],
    progress: Option<&dyn Progress>,
) -> Result<(), Box<dyn std::error::Error>> {
    for (directory, metadata) in directories.iter().rev() {
        let Some(metadata) = metadata else {
            continue;
        };
        let parent = open_parent_directory(destination, directory, false)?;
        let name = CString::new(
            directory
                .file_name()
                .ok_or("directory path has no leaf")?
                .as_bytes(),
        )
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid directory name"))?;
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error().into());
        }
        let directory_handle = unsafe { File::from_raw_fd(fd) };
        apply_metadata(&directory_handle, Some(metadata), directory, progress)?;
    }
    Ok(())
}

#[cfg(windows)]
fn restore_directory_metadata(
    _destination: &Path,
    directories: &[(PathBuf, Option<EntryMetadata>)],
    progress: Option<&dyn Progress>,
) -> Result<(), Box<dyn std::error::Error>> {
    for (directory, metadata) in directories.iter().rev() {
        let Some(metadata) = metadata else {
            continue;
        };
        if metadata.platform != platform_identifier() {
            if let Some(progress) = progress {
                progress.warning(&format!(
                    "warning: skipping metadata for {} from platform {}",
                    directory.display(),
                    metadata.platform
                ));
            }
        } else if let Some(readonly) = metadata.permissions {
            let mut permissions = std::fs::metadata(directory)?.permissions();
            permissions.set_readonly(readonly != 0);
            std::fs::set_permissions(directory, permissions)?;
        }
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn restore_directory_metadata(
    _destination: &Path,
    _directories: &[(PathBuf, Option<EntryMetadata>)],
    _progress: Option<&dyn Progress>,
) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[cfg(unix)]
fn apply_metadata(
    file: &File,
    metadata: Option<&EntryMetadata>,
    target: &Path,
    progress: Option<&dyn Progress>,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(metadata) = metadata else {
        return Ok(());
    };
    if metadata.platform != platform_identifier() {
        if let Some(progress) = progress {
            progress.warning(&format!(
                "warning: skipping metadata for {} from platform {}",
                target.display(),
                metadata.platform
            ));
        }
        return Ok(());
    }
    let mut ownership_error = None;
    if metadata.owner.is_some() || metadata.group.is_some() {
        let owner = metadata.owner.unwrap_or(u32::MAX);
        let group = metadata.group.unwrap_or(u32::MAX);
        if unsafe { libc::fchown(file.as_raw_fd(), owner, group) } < 0 {
            ownership_error = Some(io::Error::last_os_error());
        }
    }
    if let Some(permissions) = metadata.permissions
        && unsafe { libc::fchmod(file.as_raw_fd(), permissions as libc::mode_t) } < 0
    {
        return Err(io::Error::last_os_error().into());
    }
    if let Some(error) = ownership_error
        && let Some(progress) = progress
    {
        progress.warning(&format!(
            "warning: could not restore owner/group for {}: {error}",
            target.display()
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn apply_metadata(
    _file: &File,
    metadata: Option<&EntryMetadata>,
    target: &Path,
    progress: Option<&dyn Progress>,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(metadata) = metadata else {
        return Ok(());
    };
    if metadata.platform != platform_identifier() {
        if let Some(progress) = progress {
            progress.warning(&format!(
                "warning: skipping metadata for {} from platform {}",
                target.display(),
                metadata.platform
            ));
        }
        return Ok(());
    }
    if let Some(readonly) = metadata.permissions {
        let mut permissions = std::fs::metadata(target)?.permissions();
        permissions.set_readonly(readonly != 0);
        std::fs::set_permissions(target, permissions)?;
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn apply_metadata(
    _file: &File,
    metadata: Option<&EntryMetadata>,
    target: &Path,
    progress: Option<&dyn Progress>,
) -> Result<(), Box<dyn std::error::Error>> {
    if metadata.is_some()
        && let Some(progress) = progress
    {
        progress.warning(&format!(
            "warning: metadata restoration unsupported for {}",
            target.display()
        ));
    }
    Ok(())
}

pub fn validate_interactive_policies(
    is_terminal: bool,
    collision_policy: CollisionPolicy,
    existing_policy: ExistingPolicy,
) -> Result<(), Box<dyn std::error::Error>> {
    if !is_terminal
        && (collision_policy == CollisionPolicy::Ask || existing_policy == ExistingPolicy::Ask)
    {
        return Err("the `ask` policy requires an interactive terminal".into());
    }
    Ok(())
}

fn next_renamed_path(
    path: &Path,
    reserved_paths: &mut std::collections::BTreeSet<String>,
) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new("."));
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let extension = path.extension().map(|ext| ext.to_string_lossy());
    for index in 1_u64.. {
        let name = match &extension {
            Some(extension) => format!("{stem} ({index}).{extension}"),
            None => format!("{stem} ({index})"),
        };
        let candidate = parent.join(name);
        let key = candidate.to_string_lossy().to_lowercase();
        if !candidate.exists() && reserved_paths.insert(key) {
            return candidate;
        }
    }
    unreachable!()
}

fn create_destination_directory(destination: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        if let Ok(metadata) = std::fs::symlink_metadata(destination)
            && metadata.file_type().is_symlink()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("refusing destination symlink: {}", destination.display()),
            ));
        }
    }
    std::fs::create_dir_all(destination)
}

#[cfg(unix)]
fn open_destination_root(destination: &Path) -> io::Result<File> {
    let path = CString::new(destination.as_os_str().as_bytes()).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "destination path contains NUL")
    })?;
    let fd = unsafe {
        libc::open(
            path.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn open_directory_at(parent: &File, name: &std::ffi::OsStr) -> io::Result<File> {
    let name = CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path component contains NUL"))?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn ensure_directory_at(parent: &File, name: &std::ffi::OsStr) -> io::Result<File> {
    let name = CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path component contains NUL"))?;
    let result = unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o777) };
    if result < 0 {
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::AlreadyExists {
            return Err(error);
        }
    }
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

#[cfg(unix)]
fn open_parent_directory(destination: &Path, target: &Path, create: bool) -> io::Result<File> {
    let relative = target.strip_prefix(destination).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "extraction target escaped destination",
        )
    })?;
    let mut directory = open_destination_root(destination)?;
    let components = relative.components().collect::<Vec<_>>();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        let std::path::Component::Normal(name) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid extraction path",
            ));
        };
        directory = if create {
            ensure_directory_at(&directory, name)?
        } else {
            open_directory_at(&directory, name)?
        };
    }
    Ok(directory)
}

#[cfg(unix)]
fn ensure_directory_no_follow(destination: &Path, target: &Path) -> io::Result<()> {
    let relative = target.strip_prefix(destination).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "extraction target escaped destination",
        )
    })?;
    let mut directory = open_destination_root(destination)?;
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid extraction path",
            ));
        };
        directory = ensure_directory_at(&directory, name)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_directory_no_follow(_destination: &Path, target: &Path) -> io::Result<()> {
    std::fs::create_dir_all(target)
}

#[cfg(unix)]
fn create_special_entry(
    destination: &Path,
    target: &Path,
    kind: &ExtractedKind,
    policy: ExistingPolicy,
) -> Result<(), Box<dyn std::error::Error>> {
    let parent = open_parent_directory(destination, target, true)?;
    let name = CString::new(
        target
            .file_name()
            .ok_or("special file has no leaf name")?
            .as_bytes(),
    )?;
    let exists = unsafe {
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        if libc::fstatat(
            parent.as_raw_fd(),
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        ) == 0
        {
            true
        } else {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::NotFound {
                false
            } else {
                return Err(error.into());
            }
        }
    };
    if exists {
        if matches!(policy, ExistingPolicy::Error | ExistingPolicy::Skip) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "special output already exists",
            )
            .into());
        }
        let remove_directory = unsafe {
            let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
            if libc::fstatat(
                parent.as_raw_fd(),
                name.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            ) < 0
            {
                return Err(io::Error::last_os_error().into());
            }
            let stat = stat.assume_init();
            stat.st_mode & libc::S_IFMT == libc::S_IFDIR
        };
        if unsafe {
            libc::unlinkat(
                parent.as_raw_fd(),
                name.as_ptr(),
                if remove_directory {
                    libc::AT_REMOVEDIR
                } else {
                    0
                },
            )
        } < 0
        {
            return Err(io::Error::last_os_error().into());
        }
    }
    match kind {
        ExtractedKind::File => unreachable!(),
        ExtractedKind::Symlink(link_target) => {
            validate_link_target(link_target, destination, target)?;
            let link_target = CString::new(link_target.as_bytes())?;
            if unsafe { libc::symlinkat(link_target.as_ptr(), parent.as_raw_fd(), name.as_ptr()) }
                < 0
            {
                return Err(io::Error::last_os_error().into());
            }
        }
        ExtractedKind::NamedPipe => {
            if unsafe { libc::mkfifoat(parent.as_raw_fd(), name.as_ptr(), 0o666) } < 0 {
                return Err(io::Error::last_os_error().into());
            }
        }
        ExtractedKind::Device { kind, major, minor } => {
            let mode = match kind {
                DeviceKind::Character => libc::S_IFCHR,
                DeviceKind::Block => libc::S_IFBLK,
            } | 0o600;
            let device = {
                #[allow(clippy::useless_conversion)]
                libc::makedev((*major).try_into().unwrap(), (*minor).try_into().unwrap())
            };
            if unsafe { libc::mknodat(parent.as_raw_fd(), name.as_ptr(), mode, device) } < 0 {
                return Err(io::Error::last_os_error().into());
            }
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn create_special_entry(
    _destination: &Path,
    _target: &Path,
    kind: &ExtractedKind,
    _policy: ExistingPolicy,
) -> Result<(), Box<dyn std::error::Error>> {
    match kind {
        ExtractedKind::Symlink(_) => {
            Err("symbolic-link extraction is unsupported on this platform".into())
        }
        ExtractedKind::NamedPipe => {
            Err("named-pipe extraction is unsupported on this platform".into())
        }
        ExtractedKind::Device { .. } => {
            Err("device extraction is unsupported on this platform".into())
        }
        ExtractedKind::File => unreachable!(),
    }
}

fn validate_special_target(
    target: &Path,
    destination: &Path,
    kind: &ExtractedKind,
) -> Result<(), Box<dyn std::error::Error>> {
    match kind {
        ExtractedKind::Symlink(link_target) => {
            validate_link_target(link_target, destination, target)?
        }
        ExtractedKind::File | ExtractedKind::NamedPipe | ExtractedKind::Device { .. } => {}
    }
    Ok(())
}

fn apply_special_metadata(
    target: &Path,
    metadata: Option<&EntryMetadata>,
    enabled: bool,
    progress: Option<&dyn Progress>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !enabled {
        return Ok(());
    }
    let Some(metadata) = metadata else {
        return Ok(());
    };
    if metadata.platform != platform_identifier() {
        if let Some(progress) = progress {
            progress.warning(&format!(
                "warning: skipping metadata for {} from platform {}",
                target.display(),
                metadata.platform
            ));
        }
        return Ok(());
    }
    #[cfg(unix)]
    {
        let mut ownership_error = None;
        if metadata.owner.is_some() || metadata.group.is_some() {
            let c_path = CString::new(target.as_os_str().as_bytes())?;
            let owner = metadata.owner.unwrap_or(u32::MAX);
            let group = metadata.group.unwrap_or(u32::MAX);
            if unsafe { libc::lchown(c_path.as_ptr(), owner, group) } < 0 {
                ownership_error = Some(io::Error::last_os_error());
            }
        }
        if let Some(error) = ownership_error
            && let Some(progress) = progress
        {
            progress.warning(&format!(
                "warning: could not restore owner/group for {}: {error}",
                target.display()
            ));
        }
        if let Some(mode) = metadata.permissions
            && !std::fs::symlink_metadata(target)?.file_type().is_symlink()
        {
            let parent = target.parent().ok_or("special path has no parent")?;
            let parent_handle = open_parent_directory(parent, target, false)?;
            let name = CString::new(
                target
                    .file_name()
                    .ok_or("special path has no leaf")?
                    .as_bytes(),
            )?;
            let flags = 0;
            if unsafe {
                libc::fchmodat(
                    parent_handle.as_raw_fd(),
                    name.as_ptr(),
                    mode as libc::mode_t,
                    flags,
                )
            } < 0
            {
                return Err(io::Error::last_os_error().into());
            }
        }
    }
    #[cfg(windows)]
    if let Some(readonly) = metadata.permissions {
        let mut permissions = std::fs::symlink_metadata(target)?.permissions();
        permissions.set_readonly(readonly != 0);
        std::fs::set_permissions(target, permissions)?;
    }
    Ok(())
}

#[cfg(unix)]
pub fn create_extraction_file(
    destination: &Path,
    target: &Path,
    policy: ExistingPolicy,
) -> io::Result<File> {
    let parent = open_parent_directory(destination, target, true)?;
    let name = target
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing output filename"))?;
    let name = CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "filename contains NUL"))?;
    let mut flags = libc::O_WRONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK;
    flags |= match policy {
        ExistingPolicy::Overwrite | ExistingPolicy::Ask => libc::O_CREAT | libc::O_TRUNC,
        ExistingPolicy::Skip | ExistingPolicy::Error => libc::O_CREAT | libc::O_EXCL,
    };
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags, 0o666) };
    if fd < 0 {
        let error = io::Error::last_os_error();
        if policy == ExistingPolicy::Skip && error.kind() == io::ErrorKind::AlreadyExists {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "skip existing output",
            ));
        }
        return Err(error);
    }
    let output = unsafe { File::from_raw_fd(fd) };
    let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe { libc::fstat(output.as_raw_fd(), metadata.as_mut_ptr()) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let metadata = unsafe { metadata.assume_init() };
    if metadata.st_mode & libc::S_IFMT != libc::S_IFREG {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "extraction target is not a regular file",
        ));
    }
    Ok(output)
}

#[cfg(windows)]
pub fn create_extraction_file(
    _destination: &Path,
    target: &Path,
    policy: ExistingPolicy,
) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    let mut options = OpenOptions::new();
    options.write(true);
    match policy {
        ExistingPolicy::Overwrite | ExistingPolicy::Ask => {
            options.create(true).truncate(false);
        }
        ExistingPolicy::Skip | ExistingPolicy::Error => {
            options.create_new(true);
        }
    }
    options.custom_flags(0x0800_0000); // FILE_FLAG_SEQUENTIAL_SCAN
    options.access_mode(0x4000_0000); // GENERIC_WRITE only
    options.open(target)
}

#[cfg(not(any(unix, windows)))]
pub fn create_extraction_file(
    _destination: &Path,
    target: &Path,
    policy: ExistingPolicy,
) -> io::Result<File> {
    match policy {
        ExistingPolicy::Overwrite | ExistingPolicy::Ask => File::create(target),
        ExistingPolicy::Skip | ExistingPolicy::Error => {
            OpenOptions::new().write(true).create_new(true).open(target)
        }
    }
}

fn validate_destination_paths(
    destination: &Path,
    directories: &[PathBuf],
    files: &[(PathBuf, ExtractedSource)],
    existing_policy: ExistingPolicy,
) -> Result<(), Box<dyn std::error::Error>> {
    if destination.exists() && !destination.is_dir() {
        return Err(format!("destination is not a directory: {}", destination.display()).into());
    }
    let mut unique_dirs = std::collections::BTreeSet::new();
    for d in directories {
        unique_dirs.insert(d.as_path());
    }
    for (target, _) in files {
        if let Some(parent) = target.parent() {
            unique_dirs.insert(parent);
        }
    }
    let mut checked_dirs = std::collections::HashSet::new();
    for target in unique_dirs {
        reject_symlink_components_cached(destination, target, &mut checked_dirs)?;
        if let Ok(metadata) = std::fs::symlink_metadata(target)
            && !metadata.file_type().is_dir()
        {
            return Err(format!("destination is not a directory: {}", target.display()).into());
        }
    }
    for (target, source) in files {
        if existing_policy == ExistingPolicy::Overwrite
            && matches!(source.kind, ExtractedKind::File)
        {
            continue;
        }
        if let Ok(metadata) = std::fs::symlink_metadata(target) {
            let file_type = metadata.file_type();
            let compatible = match source.kind {
                ExtractedKind::File => file_type.is_file(),
                ExtractedKind::Symlink(_) => file_type.is_symlink(),
                ExtractedKind::NamedPipe => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::FileTypeExt;
                        file_type.is_fifo()
                    }
                    #[cfg(not(unix))]
                    {
                        false
                    }
                }
                ExtractedKind::Device { .. } => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::FileTypeExt;
                        file_type.is_char_device() || file_type.is_block_device()
                    }
                    #[cfg(not(unix))]
                    {
                        false
                    }
                }
            };
            if !compatible {
                return Err(format!(
                    "destination type does not match archive entry: {}",
                    target.display()
                )
                .into());
            }
        }
    }
    Ok(())
}

fn reject_symlink_components_cached(
    destination: &Path,
    target: &Path,
    checked_dirs: &mut std::collections::HashSet<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let relative = target
        .strip_prefix(destination)
        .map_err(|_| "extraction target escaped the destination")?;
    let mut current = destination.to_path_buf();
    for component in relative.components() {
        current.push(component);
        if checked_dirs.contains(&current) {
            continue;
        }
        if let Ok(metadata) = std::fs::symlink_metadata(&current)
            && metadata.file_type().is_symlink()
        {
            return Err(format!(
                "refusing to follow destination symlink: {}",
                current.display()
            )
            .into());
        }
        checked_dirs.insert(current.clone());
    }
    Ok(())
}
