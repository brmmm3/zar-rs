use super::archive_input::{
    ImportArchiveKind, extract_7z, extract_gzip_file, extract_iso, extract_rar, extract_tar,
    extract_zip, import_archive_kind,
};
use super::common::*;
use super::filters::FilterSet;
use super::pipeline::{TreeEntryKind, filter_tree_entries, scan_tree};
use crate::{
    ArchivePath, BlockFrame, CommitHeader, CommitValidationLimits, ContentAead, DEK_SIZE,
    DeviceKind, EntryMetadata, FileEntry, HEADER_OFFSET_SIZE, Toc, TocChange, TocEntry,
    ValidatedHeader,
};
#[cfg(not(target_os = "wasi"))]
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};
#[cfg(unix)]
use std::ffi::CString;
use std::fs::File;
#[cfg(not(unix))]
use std::fs::OpenOptions;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::ops::Range;
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
type DecodedBlockCache = Option<(u64, Zeroizing<Vec<u8>>)>;

/// Validated extraction state for an archive that has not changed.
pub struct PreparedExtraction {
    archive: PathBuf,
    size: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
    to: Option<String>,
    state: LoadedExtractionState,
    range_cache: std::sync::Mutex<DecodedBlockCache>,
}

impl std::fmt::Debug for PreparedExtraction {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedExtraction")
            .field("to", &self.to)
            .finish_non_exhaustive()
    }
}

impl PreparedExtraction {
    /// Validates an archive through the requested commit.
    pub fn new(
        archive: &str,
        to: Option<&str>,
        passwords: &PasswordSet,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::new_with_progress(archive, to, passwords, None)
    }

    /// Validates an archive, checking for cancellation during preparation.
    pub fn new_with_progress(
        archive: &str,
        to: Option<&str>,
        passwords: &PasswordSet,
        progress: Option<&dyn Progress>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::prepare(archive, to, passwords, progress, false)
    }

    /// Validates an archive and rejects any commit whose header is locked.
    pub fn new_complete(
        archive: &str,
        to: Option<&str>,
        passwords: &PasswordSet,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::prepare(archive, to, passwords, None, true)
    }

    fn prepare(
        archive: &str,
        to: Option<&str>,
        passwords: &PasswordSet,
        progress: Option<&dyn Progress>,
        require_complete: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let progress = ProgressReporter::new(progress);
        progress.check_cancelled()?;
        let metadata = std::fs::metadata(archive)?;
        let size = metadata.len();
        let modified = metadata.modified()?;
        #[cfg(unix)]
        let identity = {
            use std::os::unix::fs::MetadataExt;
            (
                metadata.dev(),
                metadata.ino(),
                metadata.ctime(),
                metadata.ctime_nsec(),
            )
        };
        let state = if require_complete {
            let headers =
                load_extraction_headers(&mut File::open(archive)?, to, passwords, Some(&progress))?;
            if headers
                .commits
                .iter()
                .any(|commit| commit.validated.logical_header.is_none())
            {
                return Err("cannot prepare an archive with locked commit headers".into());
            }
            load_extraction_state_impl(
                archive,
                to,
                passwords,
                Some(&progress),
                None,
                Some(headers),
                false,
            )?
        } else {
            load_extraction_state_with_passwords_progress(archive, to, passwords, Some(&progress))?
        };
        let prepared = Self {
            archive: PathBuf::from(archive),
            size,
            modified,
            #[cfg(unix)]
            identity,
            to: to.map(str::to_owned),
            state,
            range_cache: std::sync::Mutex::new(None),
        };
        prepared.check_unchanged()?;
        Ok(prepared)
    }

    fn check_unchanged(&self) -> Result<(), Box<dyn std::error::Error>> {
        let metadata = std::fs::metadata(&self.archive)?;
        self.check_metadata(&metadata)
    }

    fn check_metadata(
        &self,
        metadata: &std::fs::Metadata,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if metadata.len() != self.size || metadata.modified()? != self.modified {
            return Err("archive changed since validation".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if (
                metadata.dev(),
                metadata.ino(),
                metadata.ctime(),
                metadata.ctime_nsec(),
            ) != self.identity
            {
                return Err("archive changed since validation".into());
            }
        }
        Ok(())
    }

    /// Returns the validated table of contents of the archive.
    pub fn toc(&self) -> &Toc {
        &self.state.0
    }

    /// Returns the commit limit used when preparing the archive.
    pub fn to(&self) -> Option<&str> {
        self.to.as_deref()
    }

    /// Reads a bounded range of one regular file, validating each decoded block.
    ///
    /// The archive must not change after preparation. A partial read validates
    /// its blocks, but cannot verify the checksum of the complete file.
    pub fn read_file_range(
        &self,
        path: &str,
        offset: u64,
        size_limit: u64,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        self.check_unchanged()?;
        let archive_path = ArchivePath::new(path)?;
        let Some(TocEntry::File(entry)) = self.state.0.get(&archive_path) else {
            return Err(format!("{path} is not a file in the archive").into());
        };
        let source = self
            .state
            .1
            .get(&archive_path)
            .ok_or_else(|| format!("missing extraction source for {path}"))?;
        if !matches!(source.kind, ExtractedKind::File) || source.entry != *entry {
            return Err(format!("extraction source mismatch for {path}").into());
        }
        let mut file = File::open(&self.archive)?;
        self.check_metadata(&file.metadata()?)?;
        let mut cache = self
            .range_cache
            .lock()
            .map_err(|_| "range cache is poisoned")?;
        let result = read_file_range_from_state(
            &mut file,
            path,
            entry,
            source,
            offset,
            Some(size_limit),
            Some(&mut cache),
        )?;
        self.check_metadata(&file.metadata()?)?;
        self.check_unchanged()?;
        Ok(result)
    }

    /// Extracts entries without rebuilding the validated state.
    pub fn extract(
        &self,
        destination: &Path,
        options: ExtractionOptions<'_>,
        progress: Option<&dyn Progress>,
        filters: &FilterSet,
    ) -> Result<ExtractionPlan, Box<dyn std::error::Error>> {
        self.extract_with_block_validation(destination, options, progress, filters, true)
    }

    /// Extracts entries, optionally rechecking block checksums when reading output data.
    pub fn extract_with_block_validation(
        &self,
        destination: &Path,
        options: ExtractionOptions<'_>,
        progress: Option<&dyn Progress>,
        filters: &FilterSet,
        validate_blocks: bool,
    ) -> Result<ExtractionPlan, Box<dyn std::error::Error>> {
        if options.to != self.to.as_deref() {
            return Err("prepared archive commit does not match extraction options".into());
        }
        self.check_unchanged()?;
        let archive = self.archive.to_str().ok_or("invalid archive path")?;
        extract_archive_impl(
            archive,
            destination,
            options,
            progress,
            filters,
            ExtractionPreparation::Prepared(&self.state),
            validate_blocks,
        )
    }

    /// Extracts one file without rebuilding the validated state.
    pub fn extract_single_file(
        &self,
        path: &str,
        output: &Path,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        self.extract_single_file_with_block_validation(path, output, true)
    }

    /// Extracts one file, optionally rechecking block checksums while reading it.
    pub fn extract_single_file_with_block_validation(
        &self,
        path: &str,
        output: &Path,
        validate_blocks: bool,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        self.check_unchanged()?;
        let archive = self.archive.to_str().ok_or("invalid archive path")?;
        extract_single_file_from_state(
            archive,
            path,
            output,
            &self.state.0,
            &self.state.1,
            validate_blocks,
        )
    }
}

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
    pub to: Option<&'a str>,
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
            to: None,
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
    to: Option<&str>,
    password: Option<&[u8]>,
    size_limit: Option<u64>,
    offset: u64,
) -> Result<HashMap<String, Vec<u8>>, Box<dyn std::error::Error>> {
    extract_archive_to_memory_with_filters(
        archive,
        to,
        password,
        size_limit,
        offset,
        &FilterSet::default(),
    )
}

pub fn extract_archive_to_memory_with_filters(
    archive: &str,
    to: Option<&str>,
    password: Option<&[u8]>,
    size_limit: Option<u64>,
    offset: u64,
    filters: &FilterSet,
) -> Result<HashMap<String, Vec<u8>>, Box<dyn std::error::Error>> {
    let passwords = PasswordSet::new(password.map(<[u8]>::to_vec));
    extract_archive_to_memory_with_passwords_and_filters(
        archive, to, &passwords, size_limit, offset, filters,
    )
}

pub fn extract_archive_to_memory_with_passwords(
    archive: &str,
    to: Option<&str>,
    passwords: &PasswordSet,
    size_limit: Option<u64>,
    offset: u64,
) -> Result<HashMap<String, Vec<u8>>, Box<dyn std::error::Error>> {
    extract_archive_to_memory_with_passwords_and_filters(
        archive,
        to,
        passwords,
        size_limit,
        offset,
        &FilterSet::default(),
    )
}

pub fn extract_archive_to_memory_with_passwords_and_filters(
    archive: &str,
    to: Option<&str>,
    passwords: &PasswordSet,
    size_limit: Option<u64>,
    offset: u64,
    filters: &FilterSet,
) -> Result<HashMap<String, Vec<u8>>, Box<dyn std::error::Error>> {
    let (toc, extraction_sources, _) =
        load_extraction_state_with_passwords(archive, to, passwords)?;
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

        let contents = read_file_range_from_state(
            &mut archive_file,
            path.as_str(),
            file_entry,
            source,
            offset,
            size_limit,
            None,
        )?;
        files.insert(path.as_str().to_owned(), contents);
    }

    Ok(files)
}

fn read_file_range_from_state(
    archive_file: &mut File,
    path: &str,
    file_entry: &FileEntry,
    source: &ExtractedSource,
    offset: u64,
    size_limit: Option<u64>,
    mut cache: Option<&mut DecodedBlockCache>,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
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
        let block_range =
            entry_block_range(&source.entry, block.block_index, block.uncompressed_size)?;
        let block_file_size = u64::try_from(block_range.len())?;
        let block_end = file_offset
            .checked_add(block_file_size)
            .ok_or("decoded file size overflow")?;
        if file_offset < selected_end && block_end > offset {
            let uncached;
            let block_contents = if let Some(cache) = cache.as_deref_mut() {
                if cache
                    .as_ref()
                    .is_none_or(|(offset, _)| *offset != block.offset)
                {
                    *cache = Some((
                        block.offset,
                        Zeroizing::new(
                            decode_extraction_block(archive_file, block)
                                .map_err(ExtractionFailure::from_send_sync)?,
                        ),
                    ));
                }
                &**cache.as_ref().expect("decoded block is cached").1
            } else {
                uncached = decode_extraction_block(archive_file, block)
                    .map_err(ExtractionFailure::from_send_sync)?;
                &uncached
            };
            if block_contents.len() != block.uncompressed_size {
                return Err("archive block changed after extraction validation".into());
            }
            let file_contents = &block_contents[block_range.clone()];
            if read_full_file {
                checksum.update(file_contents);
            }
            let start = block_range.start
                + usize::try_from(offset.saturating_sub(file_offset))
                    .map_err(|_| "selected block offset is too large")?;
            let end = block_range.start
                + usize::try_from(selected_end.min(block_end) - file_offset)
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
    Ok(contents)
}

/// Extracts exactly one file into `output` in a single pass over the archive.
/// Only the data blocks of that file are read and decoded.
pub fn extract_single_file(
    archive: &str,
    path: &str,
    output: &Path,
    passwords: &PasswordSet,
) -> Result<u64, Box<dyn std::error::Error>> {
    extract_single_file_through(archive, path, output, passwords, None)
}

/// Extracts one file from the archive state through the selected commit.
pub fn extract_single_file_through(
    archive: &str,
    path: &str,
    output: &Path,
    passwords: &PasswordSet,
    to: Option<&str>,
) -> Result<u64, Box<dyn std::error::Error>> {
    extract_single_file_through_with_block_validation(archive, path, output, passwords, to, true)
}

/// Extracts one file, optionally rechecking block checksums while reading it.
pub fn extract_single_file_through_with_block_validation(
    archive: &str,
    path: &str,
    output: &Path,
    passwords: &PasswordSet,
    to: Option<&str>,
    validate_blocks: bool,
) -> Result<u64, Box<dyn std::error::Error>> {
    let wanted = FilterSet::from_patterns(&[escape_glob(path)], &[], &[], &[], &[] as &[PathBuf])?;
    let (toc, sources, _) =
        load_extraction_state_impl(archive, to, passwords, None, Some(&wanted), None, false)?;
    extract_single_file_from_state(archive, path, output, &toc, &sources, validate_blocks)
}

fn extract_single_file_from_state(
    archive: &str,
    path: &str,
    output: &Path,
    toc: &Toc,
    sources: &ExtractionSources,
    validate_blocks: bool,
) -> Result<u64, Box<dyn std::error::Error>> {
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
    let written = write_single_file_from_state(
        &mut archive_file,
        path,
        &mut writer,
        file_entry,
        source,
        validate_blocks,
        true,
    )?;
    Ok(written)
}

fn write_single_file_from_state(
    archive_file: &mut (impl Read + Seek),
    path: &str,
    writer: &mut (impl Write + ?Sized),
    file_entry: &FileEntry,
    source: &ExtractedSource,
    validate_blocks: bool,
    flush_output: bool,
) -> Result<u64, Box<dyn std::error::Error>> {
    let mut checksum = Xxh64::new(0);
    let mut written = 0_u64;
    for block in &source.blocks {
        let contents =
            decode_extraction_block_with_validation(archive_file, block, validate_blocks)
                .map_err(ExtractionFailure::from_send_sync)?;
        if contents.len() != block.uncompressed_size {
            return Err("archive block changed after extraction validation".into());
        }
        let range = entry_block_range(&source.entry, block.block_index, contents.len())?;
        let file_contents = &contents[range];
        checksum.update(file_contents);
        writer.write_all(file_contents)?;
        written += file_contents.len() as u64;
    }
    if flush_output {
        writer.flush()?;
    }
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

/// Extracts one file from a seekable ZAR archive into a writer.
///
/// The reader must contain the archive starting at offset zero. Locked headers
/// through the selected commit are rejected rather than returning a partial
/// archive view. The destination may contain partial data if an I/O or
/// integrity error occurs. Callers must flush the writer themselves and must
/// not modify the archive during the call.
pub fn extract_single_file_to_writer(
    archive: &mut (impl Read + Seek),
    path: &str,
    output: &mut (impl Write + ?Sized),
    passwords: &PasswordSet,
    to: Option<&str>,
) -> Result<u64, Box<dyn std::error::Error>> {
    let archive_path = ArchivePath::new(path)?;
    let wanted = FilterSet::from_patterns(&[escape_glob(path)], &[], &[], &[], &[] as &[PathBuf])?;
    archive.seek(SeekFrom::Start(0))?;
    let headers = load_extraction_headers(archive, to, passwords, None)?;
    if headers
        .commits
        .iter()
        .any(|commit| commit.validated.logical_header.is_none())
    {
        return Err("cannot stream a file with locked commit headers".into());
    }
    let (toc, sources, _) = load_extraction_state_from_reader(
        archive,
        to,
        passwords,
        None,
        Some(&wanted),
        Some(headers),
        true,
    )?;
    let Some(TocEntry::File(entry)) = toc.get(&archive_path) else {
        return Err(format!("{path} is not a file in the archive").into());
    };
    let source = sources
        .get(&archive_path)
        .ok_or_else(|| format!("missing extraction source for {path}"))?;
    if !matches!(source.kind, ExtractedKind::File) || source.entry != *entry {
        return Err(format!("extraction source mismatch for {path}").into());
    }
    write_single_file_from_state(archive, path, output, entry, source, true, false)
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
    to: Option<&str>,
    password: Option<&[u8]>,
    collision_policy: CollisionPolicy,
    existing_policy: ExistingPolicy,
) -> Result<ExtractionPlan, Box<dyn std::error::Error>> {
    extract_archive_with_progress(
        archive,
        destination,
        ExtractionOptions {
            dry_run,
            to,
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
    if let Some(kind) = import_archive_kind(Path::new(archive)) {
        let passwords = PasswordSet::new(password.map(<[u8]>::to_vec));
        return extract_external_archive(
            Path::new(archive),
            destination,
            options,
            &passwords,
            progress,
            filters,
            kind,
        );
    }
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

fn extract_external_archive(
    archive: &Path,
    destination: &Path,
    options: ExtractionOptions<'_>,
    _passwords: &PasswordSet,
    callback: Option<&dyn Progress>,
    filters: &FilterSet,
    kind: ImportArchiveKind,
) -> Result<ExtractionPlan, Box<dyn std::error::Error>> {
    if !archive.is_file() {
        return Err(format!("input archive is not a file: {}", archive.display()).into());
    }
    let progress = ProgressReporter::new(callback);
    progress.check_cancelled()?;
    #[cfg(not(target_os = "wasi"))]
    let temporary = tempfile::tempdir()?;
    #[cfg(target_os = "wasi")]
    let temporary = {
        let parent = destination
            .ancestors()
            .map(|path| {
                if path.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    path
                }
            })
            .find(|path| path.is_dir())
            .ok_or("no accessible directory for temporary extraction files")?;
        tempfile::Builder::new()
            .prefix(".zar-extract-")
            .tempdir_in(parent)?
    };
    let temporary_root = temporary.path().canonicalize()?;
    match kind {
        ImportArchiveKind::Zip => extract_zip(archive, &temporary_root, &progress)?,
        ImportArchiveKind::SevenZip => extract_7z(archive, &temporary_root, &progress)?,
        ImportArchiveKind::Rar => extract_rar(archive, &temporary_root, &progress)?,
        ImportArchiveKind::Iso => extract_iso(archive, &temporary_root, &progress)?,
        ImportArchiveKind::Tar => extract_tar(File::open(archive)?, &temporary_root, &progress)?,
        ImportArchiveKind::TarGz => extract_tar(
            flate2::read::MultiGzDecoder::new(File::open(archive)?),
            &temporary_root,
            &progress,
        )?,
        ImportArchiveKind::Gzip => extract_gzip_file(archive, &temporary_root, archive, &progress)?,
    }

    progress.check_cancelled()?;
    let entries = filter_tree_entries(scan_tree(&temporary_root, "")?, filters);
    progress.check_cancelled()?;
    let mut toc = Toc::default();
    let mut planned_directories: Vec<(PathBuf, Option<EntryMetadata>)> = Vec::new();
    let mut regular_files = Vec::new();
    let mut specials = Vec::new();
    for entry in entries {
        progress.check_cancelled()?;
        let source_archive_path = ArchivePath::new(entry.archive_path.clone())?;
        let relative = if options.strip_top_level {
            match source_archive_path.as_str().split_once('/') {
                Some((_, rest)) => rest,
                None if matches!(entry.kind, TreeEntryKind::Directory) => continue,
                None => source_archive_path.as_str(),
            }
        } else {
            source_archive_path.as_str()
        };
        if relative.is_empty() {
            continue;
        }
        let path = ArchivePath::new(relative.to_owned())?;
        let target = relative
            .split('/')
            .fold(destination.to_path_buf(), |path, component| {
                path.join(component)
            });
        match entry.kind {
            TreeEntryKind::Directory => {
                toc.apply_commit(&[crate::TocChange::PutDirectory { path: path.clone() }])?;
                planned_directories.push((target.clone(), None));
            }
            TreeEntryKind::File => {
                toc.apply_commit(&[crate::TocChange::PutFile {
                    path: path.clone(),
                    entry: FileEntry::new(
                        entry.size,
                        if entry.size == 0 { Vec::new() } else { vec![1] },
                    )?,
                }])?;
                regular_files.push((path.clone(), target.clone(), entry.source_path, entry.size));
            }
            TreeEntryKind::Symlink {
                target: link_target,
            } if options.restore_symlinks => {
                toc.apply_commit(&[crate::TocChange::PutSymlink {
                    path: path.clone(),
                    target: link_target.clone(),
                }])?;
                specials.push((target.clone(), ExtractedSource::symlink(link_target, None)));
            }
            TreeEntryKind::NamedPipe if options.restore_devices => {
                toc.apply_commit(&[crate::TocChange::PutNamedPipe { path: path.clone() }])?;
                specials.push((target.clone(), ExtractedSource::named_pipe(None)));
            }
            TreeEntryKind::Device { kind, major, minor } if options.restore_devices => {
                toc.apply_commit(&[crate::TocChange::PutDevice {
                    path: path.clone(),
                    kind,
                    major,
                    minor,
                }])?;
                specials.push((
                    target.clone(),
                    ExtractedSource::device(kind, major, minor, None),
                ));
            }
            _ => continue,
        }
    }
    let summary = InputSummary {
        files: regular_files.len() as u64,
        directories: planned_directories.len() as u64,
        total_bytes: regular_files
            .iter()
            .try_fold(0_u64, |total, (_, _, _, size)| total.checked_add(*size))
            .ok_or("archive size overflow")?,
        ..InputSummary::default()
    };
    progress.extraction_summary(&summary);
    let mut accepted_directories = Vec::new();
    for (target, metadata) in planned_directories {
        progress.check_cancelled()?;
        if target.exists() && !target.is_dir() {
            return Err(
                format!("destination path is not a directory: {}", target.display()).into(),
            );
        }
        accepted_directories.push((target, metadata));
    }
    let mut accepted_files: Vec<(PathBuf, ExtractedSource)> = Vec::new();
    let mut skipped_targets = std::collections::HashSet::new();
    let mut overwrite_all = None;
    for (_, target, _source_path, size) in &regular_files {
        progress.check_cancelled()?;
        if options.existing_policy != ExistingPolicy::Overwrite && target.exists() {
            match options.existing_policy {
                ExistingPolicy::Skip => {
                    skipped_targets.insert(target.clone());
                    continue;
                }
                ExistingPolicy::Error => {
                    return Err(
                        format!("destination file already exists: {}", target.display()).into(),
                    );
                }
                ExistingPolicy::Ask => {
                    let decision = match overwrite_all {
                        Some(decision) => decision,
                        None => progress
                            .0
                            .ok_or("interactive confirmation is unavailable")?
                            .confirm_overwrite(&format!(
                                "Overwrite `{}`? [y/N/Always/Never] ",
                                target.display()
                            ))?,
                    };
                    match decision {
                        OverwriteDecision::Yes => {}
                        OverwriteDecision::No => {
                            skipped_targets.insert(target.clone());
                            continue;
                        }
                        OverwriteDecision::Always => overwrite_all = Some(OverwriteDecision::Yes),
                        OverwriteDecision::Never => {
                            overwrite_all = Some(OverwriteDecision::No);
                            skipped_targets.insert(target.clone());
                            continue;
                        }
                    }
                }
                ExistingPolicy::Overwrite => unreachable!(),
            }
        }
        accepted_files.push((
            target.clone(),
            ExtractedSource {
                entry: FileEntry::new(*size, if *size == 0 { Vec::new() } else { vec![1] })?,
                blocks: Vec::new(),
                metadata: None,
                kind: ExtractedKind::File,
            },
        ));
    }
    for (target, source) in &specials {
        progress.check_cancelled()?;
        validate_special_target(target, destination, &source.kind)?;
        accepted_files.push((target.clone(), source.clone()));
    }
    let plan = ExtractionPlan {
        toc,
        directories: accepted_directories
            .iter()
            .map(|(path, _)| path.clone())
            .collect(),
        files: accepted_files
            .iter()
            .map(|(path, source)| (path.clone(), source.entry.clone(), source.kind.clone()))
            .collect(),
    };
    if options.dry_run {
        progress.check_cancelled()?;
        return Ok(plan);
    }
    let selected_total = accepted_files
        .iter()
        .filter(|(_, source)| matches!(source.kind, ExtractedKind::File))
        .try_fold(accepted_files.len() as u64, |total, (_, source)| {
            total.checked_add(source.entry.size)
        })
        .ok_or("extraction progress size overflow")?;
    progress.reset_phase("Extracting files", selected_total.max(1));
    progress.check_cancelled()?;
    let directory_paths = accepted_directories
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    validate_destination_paths(
        destination,
        &directory_paths,
        &accepted_files,
        options.existing_policy,
    )?;
    create_destination_directory(destination)?;
    let mut all_dirs = std::collections::BTreeSet::new();
    for directory in &directory_paths {
        all_dirs.insert(directory.clone());
    }
    for (target, _) in &accepted_files {
        if let Some(parent) = target.parent() {
            all_dirs.insert(parent.to_path_buf());
        }
    }
    for directory in &all_dirs {
        progress.check_cancelled()?;
        ensure_directory_no_follow(destination, directory)?;
    }
    for (target, source) in &specials {
        progress.check_cancelled()?;
        if accepted_files
            .iter()
            .any(|(accepted, _)| accepted == target)
        {
            if let Some(parent) = target.parent() {
                ensure_directory_no_follow(destination, parent)?;
            }
            create_special_entry(destination, target, &source.kind, options.existing_policy)?;
        }
    }
    let copy_file = |(_, target, source_path, size): &(ArchivePath, PathBuf, PathBuf, u64)|
     -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut input = BufReader::new(File::open(source_path)?);
        let mut output =
            match create_extraction_file(destination, target, options.existing_policy) {
                Ok(output) => output,
                Err(error)
                    if options.existing_policy == ExistingPolicy::Skip
                        && error.kind() == io::ErrorKind::AlreadyExists =>
                {
                    return Ok(());
                }
                Err(error) => return Err(error.into()),
            };
        let copied = copy_interruptibly(&mut input, &mut output, &progress)?;
        if copied != *size {
            return Err(format!("extracted size mismatch for {}", target.display()).into());
        }
        progress.inc(copied);
        progress.inc(1);
        Ok(())
    };
    let is_selected = |(_, target, _, _): &&(ArchivePath, PathBuf, PathBuf, u64)| {
        !skipped_targets.contains(target)
            && accepted_files.iter().any(|(accepted, source)| {
                accepted == target && matches!(source.kind, ExtractedKind::File)
            })
    };
    #[cfg(target_os = "wasi")]
    regular_files
        .iter()
        .filter(is_selected)
        .try_for_each(copy_file)
        .map_err(extraction_worker_error)?;
    #[cfg(not(target_os = "wasi"))]
    {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(compression_worker_count(CREATE_BLOCK_SIZE))
            .thread_name(|index| format!("zarc-extract-{index}"))
            .build()?;
        pool.install(|| {
            regular_files
                .par_iter()
                .filter(is_selected)
                .try_for_each(copy_file)
        })
        .map_err(extraction_worker_error)?;
    }
    Ok(plan)
}

/// Extracts entries whose commits are visible with `passwords`. Commits with
/// locked headers are skipped, so the resulting plan may be a partial view.
pub fn extract_archive_with_passwords_progress_and_filters(
    archive: &str,
    destination: &Path,
    options: ExtractionOptions<'_>,
    passwords: &PasswordSet,
    progress: Option<&dyn Progress>,
    filters: &FilterSet,
) -> Result<ExtractionPlan, Box<dyn std::error::Error>> {
    extract_archive_with_passwords_progress_and_filters_and_frame_validation(
        archive,
        destination,
        options,
        passwords,
        progress,
        filters,
        false,
    )
}

/// Extracts selected entries, optionally skipping unselected frame checks when an index is available.
/// Selected frames and the block index are always validated; without an index all frames are scanned.
pub fn extract_archive_with_passwords_progress_and_filters_and_frame_validation(
    archive: &str,
    destination: &Path,
    options: ExtractionOptions<'_>,
    passwords: &PasswordSet,
    progress: Option<&dyn Progress>,
    filters: &FilterSet,
    skip_unselected_frame_validation: bool,
) -> Result<ExtractionPlan, Box<dyn std::error::Error>> {
    extract_archive_with_validation_options(
        archive,
        destination,
        options,
        passwords,
        progress,
        filters,
        ExtractionFrameValidation {
            skip_unselected: skip_unselected_frame_validation,
            validate_on_read: true,
        },
    )
}

/// Controls frame validation during extraction.
#[derive(Debug, Clone, Copy)]
pub struct ExtractionFrameValidation {
    /// Skip validation of unselected frames where an index permits it.
    pub skip_unselected: bool,
    /// Recheck frame checksums when reading data for output.
    pub validate_on_read: bool,
}

impl Default for ExtractionFrameValidation {
    fn default() -> Self {
        Self {
            skip_unselected: false,
            validate_on_read: true,
        }
    }
}

/// Extracts selected entries with configurable frame checks.
pub fn extract_archive_with_validation_options(
    archive: &str,
    destination: &Path,
    options: ExtractionOptions<'_>,
    passwords: &PasswordSet,
    progress: Option<&dyn Progress>,
    filters: &FilterSet,
    validation: ExtractionFrameValidation,
) -> Result<ExtractionPlan, Box<dyn std::error::Error>> {
    extract_archive_impl(
        archive,
        destination,
        options,
        progress,
        filters,
        ExtractionPreparation::Validate {
            passwords,
            skip_unselected_frame_validation: validation.skip_unselected,
        },
        validation.validate_on_read,
    )
}

enum ExtractionPreparation<'a> {
    Validate {
        passwords: &'a PasswordSet,
        skip_unselected_frame_validation: bool,
    },
    Prepared(&'a LoadedExtractionState),
}

fn extract_archive_impl(
    archive: &str,
    destination: &Path,
    options: ExtractionOptions<'_>,
    progress: Option<&dyn Progress>,
    filters: &FilterSet,
    preparation: ExtractionPreparation<'_>,
    validate_blocks: bool,
) -> Result<ExtractionPlan, Box<dyn std::error::Error>> {
    let default_passwords = PasswordSet::default();
    let (passwords, skip_unselected_frame_validation, prepared) = match preparation {
        ExtractionPreparation::Validate {
            passwords,
            skip_unselected_frame_validation,
        } => (passwords, skip_unselected_frame_validation, None),
        ExtractionPreparation::Prepared(state) => (&default_passwords, false, Some(state)),
    };
    if let Some(kind) = import_archive_kind(Path::new(archive)) {
        return extract_external_archive(
            Path::new(archive),
            destination,
            options,
            passwords,
            progress,
            filters,
            kind,
        );
    }
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
        to,
        collision_policy,
        existing_policy,
        restore_metadata,
        restore_symlinks,
        restore_devices,
        strip_top_level,
    } = options;
    let progress = ProgressReporter::new(progress);
    progress.check_cancelled()?;
    let archive_size = File::open(archive)?.metadata()?.len().saturating_sub(4);
    progress.start_phase("Reading archive headers", archive_size.max(1));
    let headers = if prepared.is_none() {
        Some(load_extraction_headers(
            &mut File::open(archive)?,
            to,
            passwords,
            Some(&progress),
        )?)
    } else {
        None
    };
    let full_toc = prepared
        .map(|state| &state.0)
        .or(headers.as_ref().map(|headers| &headers.toc))
        .unwrap();
    progress.check_cancelled()?;
    let toc = filters.filter_toc(full_toc)?;
    progress.reset_phase("Planning extraction", archive_size.max(1));
    let summary = summarize_toc(&toc, restore_symlinks, restore_devices)?;
    progress.extraction_summary(&summary);
    let loaded;
    let (_, extraction_sources, entry_metadata) = if let Some(state) = prepared {
        state
    } else if dry_run {
        loaded = load_extraction_state_impl(
            archive,
            to,
            passwords,
            None,
            Some(filters),
            headers,
            skip_unselected_frame_validation,
        )?;
        &loaded
    } else {
        progress.reset_phase("Preparing extraction", archive_size.max(1));
        progress.set_message(if skip_unselected_frame_validation {
            "Preparing selected data frames"
        } else {
            "Validating data frames"
        });
        loaded = load_extraction_state_impl(
            archive,
            to,
            passwords,
            Some(&progress),
            Some(filters),
            headers,
            skip_unselected_frame_validation,
        )?;
        &loaded
    };
    progress.check_cancelled()?;
    validate_interactive_policies(
        progress.0.is_some_and(Progress::is_interactive),
        collision_policy,
        existing_policy,
    )?;
    let mut planned_files: Vec<(PathBuf, ExtractedSource)> = Vec::new();
    let mut planned_directories = Vec::new();
    let mut collision_indices = BTreeMap::<String, usize>::new();
    let mut replace_all_collisions = None;
    let mut reserved_paths: std::collections::BTreeSet<String> = toc
        .iter()
        .map(|(path, _)| path.as_str().to_lowercase())
        .collect();

    for (path, entry) in toc.iter() {
        progress.check_cancelled()?;
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
                            let decision = match replace_all_collisions {
                                Some(decision) => decision,
                                None => progress.0.unwrap().confirm_overwrite(&format!(
                                    "Replace colliding extraction entry with `{path}`? [y/N/Always/Never] "
                                ))?,
                            };
                            let replace = match decision {
                                OverwriteDecision::Yes => true,
                                OverwriteDecision::No => false,
                                OverwriteDecision::Always => {
                                    replace_all_collisions = Some(OverwriteDecision::Yes);
                                    true
                                }
                                OverwriteDecision::Never => {
                                    replace_all_collisions = Some(OverwriteDecision::No);
                                    false
                                }
                            };
                            if replace {
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
        progress.check_cancelled()?;
        if target.exists() && !target.is_dir() {
            return Err(
                format!("destination path is not a directory: {}", target.display()).into(),
            );
        }
        accepted_directories.push((target.clone(), metadata.clone()));
    }
    let mut accepted_files = Vec::new();
    let mut overwrite_all = None;
    for (target, source) in planned_files {
        progress.check_cancelled()?;
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
                        let decision = if overwrite_all == Some(OverwriteDecision::No) {
                            OverwriteDecision::No
                        } else if overwrite_all == Some(OverwriteDecision::Yes) {
                            OverwriteDecision::Yes
                        } else {
                            progress.0.unwrap().confirm_overwrite(&format!(
                                "Replace `{}`? [y/N/Always/Never] ",
                                target.display()
                            ))?
                        };
                        match decision {
                            OverwriteDecision::Yes => {}
                            OverwriteDecision::No => continue,
                            OverwriteDecision::Always => {
                                overwrite_all = Some(OverwriteDecision::Yes)
                            }
                            OverwriteDecision::Never => {
                                overwrite_all = Some(OverwriteDecision::No);
                                continue;
                            }
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
                    let decision = match overwrite_all {
                        Some(decision) => decision,
                        None => progress.0.unwrap().confirm_overwrite(&format!(
                            "Overwrite `{}`? [y/N/Always/Never] ",
                            target.display()
                        ))?,
                    };
                    match decision {
                        OverwriteDecision::Yes => {}
                        OverwriteDecision::No => continue,
                        OverwriteDecision::Always => overwrite_all = Some(OverwriteDecision::Yes),
                        OverwriteDecision::Never => {
                            overwrite_all = Some(OverwriteDecision::No);
                            continue;
                        }
                    }
                }
            }
        }
        accepted_files.push((target, source));
    }

    let plan = ExtractionPlan {
        toc,
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
        progress.check_cancelled()?;
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
    progress.check_cancelled()?;
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
        progress.check_cancelled()?;
        ensure_directory_no_follow(destination, directory)?;
    }

    let mut regular_files = Vec::new();
    for (target, source) in accepted_files {
        progress.check_cancelled()?;
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
    regular_files.sort_by_key(|(_, source)| {
        (
            source.blocks.first().map(|b| b.offset).unwrap_or(0),
            source.blocks.len() != 1,
        )
    });
    let groups: Vec<_> = regular_files
        .chunk_by(|(_, left), (_, right)| {
            left.blocks.len() == 1
                && right.blocks.len() == 1
                && left.blocks.first().map(|block| block.offset)
                    == right.blocks.first().map(|block| block.offset)
        })
        .collect();

    #[cfg(target_os = "wasi")]
    for group in groups {
        let mut reader = ExtractionReader::open(archive, validate_blocks)?;
        for (target, source) in group {
            extract_regular_file(
                &mut reader,
                destination,
                target,
                source,
                existing_policy,
                restore_metadata,
                progress.0,
            )
            .map_err(extraction_worker_error)?;
        }
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
                groups.into_par_iter().try_for_each_init(
                    || ExtractionReader::open(archive, validate_blocks),
                    |reader, group| {
                        let reader = match reader {
                            Ok(reader) => reader,
                            Err(error) => {
                                return Err(Box::new(ExtractionFailure::wrap(io::Error::new(
                                    error.kind(),
                                    error.to_string(),
                                )))
                                    as Box<dyn std::error::Error + Send + Sync>);
                            }
                        };
                        reader.block_cache = None;
                        for (target, source) in group {
                            extract_regular_file(
                                reader,
                                destination,
                                target,
                                source,
                                existing_policy,
                                restore_metadata,
                                progress.0,
                            )?;
                        }
                        Ok(())
                    },
                )
            })
            .map_err(extraction_worker_error)?;
    }
    if restore_metadata {
        progress.check_cancelled()?;
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

struct ExtractionReader {
    archive: File,
    block_cache: Option<(u64, Vec<u8>)>,
    validate_blocks: bool,
}

impl ExtractionReader {
    fn open(archive: &str, validate_blocks: bool) -> io::Result<Self> {
        Ok(Self {
            archive: File::open(archive)?,
            block_cache: None,
            validate_blocks,
        })
    }
}

fn extract_regular_file(
    reader: &mut ExtractionReader,
    destination: &Path,
    target: &Path,
    source: &ExtractedSource,
    existing_policy: ExistingPolicy,
    restore_metadata: bool,
    callback: Option<&dyn Progress>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let progress = ProgressReporter::new(callback);
    progress.check_cancelled()?;
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
        progress.check_cancelled()?;
        let uncached;
        let contents = if source.blocks.len() == 1 {
            cached_extraction_block(&mut reader.block_cache, block.offset, || {
                decode_extraction_block_with_validation(
                    &mut reader.archive,
                    block,
                    reader.validate_blocks,
                )
            })?
        } else {
            uncached = decode_extraction_block_with_validation(
                &mut reader.archive,
                block,
                reader.validate_blocks,
            )?;
            &uncached
        };
        if contents.len() != block.uncompressed_size {
            return Err(Box::new(ExtractionFailure::new(
                "archive block changed after extraction validation",
            )));
        }
        let range = entry_block_range(&source.entry, block.block_index, contents.len())
            .map_err(|error| ExtractionFailure::new(error.to_string()))?;
        let file_contents = &contents[range];
        output.write_all(file_contents)?;
        checksum.update(file_contents);
        progress.inc(file_contents.len() as u64);
        written = written
            .checked_add(file_contents.len() as u64)
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
    output.set_len(written)?;
    if restore_metadata {
        apply_metadata(&output, source.metadata.as_ref(), target, callback)
            .map_err(ExtractionFailure::from_error)?;
    }
    drop(output);
    progress.inc(1);
    Ok(())
}

fn cached_extraction_block(
    cache: &mut Option<(u64, Vec<u8>)>,
    offset: u64,
    decode: impl FnOnce() -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>>,
) -> Result<&[u8], Box<dyn std::error::Error + Send + Sync>> {
    if cache
        .as_ref()
        .is_none_or(|(cached_offset, _)| *cached_offset != offset)
    {
        *cache = Some((offset, decode()?));
    }
    Ok(&cache.as_ref().expect("decoded block is cached").1)
}

fn entry_block_range(
    entry: &FileEntry,
    block_index: u32,
    block_size: usize,
) -> Result<Range<usize>, io::Error> {
    let Some(offset) = entry.offset_in_block else {
        return Ok(0..block_size);
    };
    if entry.block_indices.as_slice() != [block_index] {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "packed file must reference exactly one block",
        ));
    }
    let start = usize::try_from(offset)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "file block offset overflow"))?;
    let size = usize::try_from(entry.size)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "file size overflow"))?;
    let end = start
        .checked_add(size)
        .filter(|&end| end <= block_size)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "packed file range exceeds its data block",
            )
        })?;
    Ok(start..end)
}

fn decode_extraction_block(
    archive_file: &mut File,
    block: &ExtractedBlock,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    decode_extraction_block_with_validation(archive_file, block, true)
}

fn decode_extraction_block_with_validation(
    archive_file: &mut (impl Read + Seek),
    block: &ExtractedBlock,
    validate_blocks: bool,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    archive_file.seek(SeekFrom::Start(block.offset))?;
    let mut raw = vec![0u8; block.encoded_size as usize];
    archive_file.read_exact(&mut raw)?;
    let frame = BlockFrame::read_with_checksum_validation(
        &mut raw.as_slice(),
        CommitValidationLimits::default().max_frame_payload_size,
        validate_blocks,
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
        frame.uncompressed_encrypted_with_checksum_validation(
            &block.commit_header,
            key,
            algorithm,
            CommitValidationLimits::default().max_uncompressed_block_size,
            false,
        )?
    } else {
        frame.uncompressed_with_checksum_validation(
            CommitValidationLimits::default().max_uncompressed_block_size,
            false,
        )?
    };
    Ok(contents)
}

fn validate_extraction_block_batch(
    batch: Vec<(u64, crate::frames::BlockFrameMetadata, Vec<u8>)>,
    commit_header: &CommitHeader,
    algorithm: Option<ContentAead>,
    dek: Option<&Zeroizing<[u8; DEK_SIZE]>>,
    limits: CommitValidationLimits,
) -> Result<HashMap<u32, ExtractedBlock>, Box<dyn std::error::Error>> {
    #[cfg(target_os = "wasi")]
    let validated = batch.into_iter().map(|(offset, metadata, bytes)| {
        validate_encoded_extraction_frame(
            offset,
            metadata,
            bytes,
            commit_header,
            algorithm,
            dek,
            limits,
        )
    });
    #[cfg(not(target_os = "wasi"))]
    let validated = batch.into_par_iter().map(|(offset, metadata, bytes)| {
        validate_encoded_extraction_frame(
            offset,
            metadata,
            bytes,
            commit_header,
            algorithm,
            dek,
            limits,
        )
    });
    validated
        .collect::<Result<HashMap<_, _>, ExtractionFailure>>()
        .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)
}

fn validate_encoded_extraction_frame(
    offset: u64,
    metadata: crate::frames::BlockFrameMetadata,
    bytes: Vec<u8>,
    commit_header: &CommitHeader,
    algorithm: Option<ContentAead>,
    dek: Option<&Zeroizing<[u8; DEK_SIZE]>>,
    limits: CommitValidationLimits,
) -> Result<(u32, ExtractedBlock), ExtractionFailure> {
    let mut input = bytes.as_slice();
    let frame = BlockFrame::read_from(&mut input, limits.max_frame_payload_size)
        .map_err(ExtractionFailure::wrap)?;
    if !input.is_empty()
        || frame.block_index != metadata.block_index
        || frame.flags != metadata.flags
        || frame.nonce != metadata.nonce
    {
        return Err(ExtractionFailure::wrap(crate::FramingError::InvalidSize));
    }
    validate_extraction_frame(offset, frame, commit_header, algorithm, dek, limits)
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
    to: Option<&str>,
    password: Option<&[u8]>,
) -> Result<LoadedExtractionState, Box<dyn std::error::Error>> {
    load_extraction_state_with_progress(archive, to, password, None)
}

pub fn load_extraction_state_with_progress(
    archive: &str,
    to: Option<&str>,
    password: Option<&[u8]>,
    progress: Option<&ProgressReporter<'_>>,
) -> Result<LoadedExtractionState, Box<dyn std::error::Error>> {
    let passwords = PasswordSet::new(password.map(<[u8]>::to_vec));
    load_extraction_state_with_passwords_progress(archive, to, &passwords, progress)
}

pub fn load_extraction_state_with_passwords(
    archive: &str,
    to: Option<&str>,
    passwords: &PasswordSet,
) -> Result<LoadedExtractionState, Box<dyn std::error::Error>> {
    load_extraction_state_with_passwords_progress(archive, to, passwords, None)
}

pub fn load_extraction_state_with_passwords_progress(
    archive: &str,
    to: Option<&str>,
    passwords: &PasswordSet,
    progress: Option<&ProgressReporter<'_>>,
) -> Result<LoadedExtractionState, Box<dyn std::error::Error>> {
    load_extraction_state_impl(archive, to, passwords, progress, None, None, false)
}

struct ExtractionCommit {
    validated: ValidatedHeader,
    dek: Option<Zeroizing<[u8; DEK_SIZE]>>,
    data_start: u64,
    header_size: u64,
}

struct ExtractionHeaders {
    toc: Toc,
    commits: Vec<ExtractionCommit>,
}

fn load_extraction_headers(
    file: &mut (impl Read + Seek),
    to: Option<&str>,
    passwords: &PasswordSet,
    progress: Option<&ProgressReporter<'_>>,
) -> Result<ExtractionHeaders, Box<dyn std::error::Error>> {
    let requested_id = to.map(parse_commit_id).transpose()?;
    let mut toc = Toc::default();
    let mut commits = Vec::new();
    let limits = CommitValidationLimits::default();
    let mut offset = read_first_header_offset_from_reader(file)?;
    let mut offsets = HashSet::new();
    let mut matched_to = false;
    let mut stopped_at_to = false;
    while offset != 0 {
        if !offsets.insert(offset) {
            return Err("archive commit chain contains a cycle".into());
        }
        if let Some(progress) = progress {
            progress.check_cancelled()?;
        }
        if stopped_at_to {
            let (commit, _, next) = read_commit_at_offset_from_reader(file, offset, limits)?;
            if requested_id == Some(commit.commit_id) {
                return Err("--to commit ID is ambiguous".into());
            }
            offset = next;
            continue;
        }
        file.seek(SeekFrom::Start(offset))?;
        let (validated, dek) = read_header_with_passwords_from_reader(file, limits, passwords)?;
        let header_size = file.stream_position()?.saturating_sub(offset);
        let commit = &validated.commit_header;
        let _commit_end = offset
            .checked_add(u64::from(commit.header_frame_size))
            .and_then(|size| size.checked_add(HEADER_OFFSET_SIZE))
            .ok_or("commit end overflow")?;
        let data_start = offset
            .checked_sub(commit.data_section_size)
            .ok_or("data section offset underflow")?;
        let next_offset_pos = data_start
            .checked_sub(HEADER_OFFSET_SIZE)
            .ok_or("next_header_offset position underflow")?;
        file.seek(SeekFrom::Start(next_offset_pos))?;
        let mut next_bytes = [0; 8];
        file.read_exact(&mut next_bytes)?;
        if let Some(progress) = progress {
            progress.inc(header_size);
            progress.set_message(format!(
                "Reading header for commit {:08x}",
                u32::from_le_bytes(commit.commit_id)
            ));
        }
        if let Some(header) = validated.logical_header.as_ref() {
            toc.apply_archive_commit(commit.commit_id, &header.changes)?;
        } else if !validated.encryption.is_header_encrypted() {
            return Err("commit header is missing its logical header".into());
        }
        if requested_id == Some(commit.commit_id) {
            matched_to = true;
            stopped_at_to = true;
        }
        commits.push(ExtractionCommit {
            validated,
            dek,
            data_start,
            header_size,
        });
        offset = u64::from_le_bytes(next_bytes);
    }
    if requested_id.is_some() && !matched_to {
        return Err("--to commit ID was not found".into());
    }
    Ok(ExtractionHeaders { toc, commits })
}

fn referenced_blocks(
    commits: &[ExtractionCommit],
    filters: Option<&FilterSet>,
) -> HashMap<[u8; 4], HashSet<u32>> {
    let mut references = HashMap::<[u8; 4], HashSet<u32>>::new();
    for commit in commits {
        if let Some(header) = commit.validated.logical_header.as_ref() {
            for change in &header.changes {
                if let TocChange::PutFile { path, entry } = change
                    && let Some(source) = entry.source_commit
                    && filters.is_none_or(|filters| filters.includes(path.as_str(), false))
                {
                    references
                        .entry(source)
                        .or_default()
                        .extend(entry.block_indices.iter().copied());
                }
            }
        }
    }
    references
}

/// Blocks that only belong to files rejected by `filters` are not read or validated.
/// Cached headers and their TOC are reused without rereading the archive headers.
fn load_extraction_state_impl(
    archive: &str,
    to: Option<&str>,
    passwords: &PasswordSet,
    progress: Option<&ProgressReporter<'_>>,
    filters: Option<&FilterSet>,
    headers: Option<ExtractionHeaders>,
    skip_unselected_frame_validation: bool,
) -> Result<LoadedExtractionState, Box<dyn std::error::Error>> {
    let mut file = File::open(archive)?;
    load_extraction_state_from_reader(
        &mut file,
        to,
        passwords,
        progress,
        filters,
        headers,
        skip_unselected_frame_validation,
    )
}

fn load_extraction_state_from_reader(
    file: &mut (impl Read + Seek),
    to: Option<&str>,
    passwords: &PasswordSet,
    progress: Option<&ProgressReporter<'_>>,
    filters: Option<&FilterSet>,
    headers: Option<ExtractionHeaders>,
    skip_unselected_frame_validation: bool,
) -> Result<LoadedExtractionState, Box<dyn std::error::Error>> {
    let use_block_index =
        filters.is_some() && (headers.is_none() || skip_unselected_frame_validation);
    let ExtractionHeaders { toc, commits } = match headers {
        Some(headers) => headers,
        None => load_extraction_headers(file, to, passwords, progress)?,
    };
    let references = referenced_blocks(&commits, filters);
    let mut historical_blocks = HashMap::<[u8; 4], HashMap<u32, ExtractedBlock>>::new();
    let mut file_contents = BTreeMap::<ArchivePath, ExtractedSource>::new();
    let mut archived_special_entries = BTreeMap::<ArchivePath, ExtractedSource>::new();
    let mut archived_metadata = BTreeMap::<ArchivePath, EntryMetadata>::new();
    let mut header_bytes_read = 0_u64;
    let mut progressed_header_bytes = 0_u64;
    let mut total_blocks = 0_u64;
    let limits = CommitValidationLimits::default();

    for ExtractionCommit {
        validated,
        dek,
        data_start,
        header_size,
    } in commits
    {
        if let Some(progress) = progress {
            progress.check_cancelled()?;
        }
        let commit_header = &validated.commit_header;
        header_bytes_read = header_bytes_read
            .checked_add(header_size)
            .ok_or("extraction progress size overflow")?;
        let Some(header) = validated.logical_header.as_ref() else {
            if !validated.encryption.is_header_encrypted() {
                return Err("commit header is missing its logical header".into());
            }
            if let Some(progress) = progress {
                if let Some(callback) = progress.0 {
                    callback.warning(&format!(
                        "skipping header-encrypted commit {:08x} without its password",
                        u32::from_le_bytes(commit_header.commit_id)
                    ));
                }
                progress.inc(header_bytes_read.saturating_sub(progressed_header_bytes));
                progress.inc(commit_header.data_section_size);
            }
            progressed_header_bytes = header_bytes_read;
            continue;
        };
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
        let workers = available_compression_workers();
        let wanted_blocks: Option<HashSet<u32>> = filters.map(|filters| {
            let mut wanted: HashSet<u32> = header
                .changes
                .iter()
                .filter_map(|change| match change {
                    TocChange::PutFile { path, entry }
                        if entry.source_commit.is_none()
                            && filters.includes(path.as_str(), false) =>
                    {
                        Some(entry.block_indices.iter().copied())
                    }
                    _ => None,
                })
                .flatten()
                .collect();
            if let Some(referenced) = references.get(&commit_header.commit_id) {
                wanted.extend(referenced);
            }
            wanted
        });
        if let (true, Some(index), Some(wanted)) = (
            use_block_index,
            header.block_index.as_ref(),
            wanted_blocks.as_ref(),
        ) {
            if index.len() as u64 > limits.max_blocks.saturating_sub(total_blocks) {
                return Err("archive exceeds the extraction block limit".into());
            }
            let mut expected_offset = 0_u64;
            for entry in index {
                if let Some(progress) = progress {
                    progress.check_cancelled()?;
                }
                if entry.offset != expected_offset
                    || entry.encoded_size < 20
                    || !block_indices.insert(entry.index)
                {
                    return Err(crate::FramingError::InvalidSize.into());
                }
                expected_offset = expected_offset
                    .checked_add(entry.encoded_size)
                    .filter(|&end| end <= commit_header.data_section_size)
                    .ok_or(crate::FramingError::InvalidSize)?;
                if wanted.contains(&entry.index) {
                    let offset = data_start
                        .checked_add(entry.offset)
                        .ok_or(crate::FramingError::InvalidSize)?;
                    file.seek(SeekFrom::Start(offset))?;
                    let size = usize::try_from(entry.encoded_size)?;
                    if size
                        > limits
                            .max_frame_payload_size
                            .saturating_add(20 + crate::frames::BLOCK_NONCE_SIZE)
                    {
                        return Err(crate::FramingError::InvalidSize.into());
                    }
                    let mut bytes = vec![0; size];
                    file.read_exact(&mut bytes)?;
                    let metadata = BlockFrame::read_metadata_from(
                        &mut io::Cursor::new(&bytes),
                        limits.max_frame_payload_size,
                    )?;
                    if metadata.encoded_size != entry.encoded_size
                        || metadata.block_index != entry.index
                        || (metadata.flags & crate::BLOCK_FLAG_ENCRYPTED != 0)
                            != validated.encryption.is_data_encrypted()
                    {
                        return Err(crate::FramingError::InvalidSize.into());
                    }
                    if let Some(nonce) = metadata.nonce
                        && !nonces.insert(nonce)
                    {
                        return Err(crate::FramingError::InvalidDescriptor.into());
                    }
                    pending_blocks.push((offset, metadata, bytes));
                    if pending_blocks.len() >= workers {
                        blocks.extend(validate_extraction_block_batch(
                            std::mem::take(&mut pending_blocks),
                            commit_header,
                            algorithm,
                            dek.as_ref(),
                            limits,
                        )?);
                    }
                }
            }
            if expected_offset != commit_header.data_section_size {
                return Err(crate::FramingError::InvalidSize.into());
            }
        } else {
            let mut data = RelativeSeekReader::new(&mut *file)?;
            while remaining > 0 {
                if let Some(progress) = progress {
                    progress.check_cancelled()?;
                }
                let remaining_before = remaining;
                let offset = block_offset;
                let metadata =
                    BlockFrame::read_metadata_from(&mut data, limits.max_frame_payload_size)?;
                let block_index = metadata.block_index;
                let flags = metadata.flags;
                let nonce = metadata.nonce;
                let encoded_size = metadata.encoded_size;
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
                if wanted_blocks
                    .as_ref()
                    .is_none_or(|wanted| wanted.contains(&block_index))
                {
                    data.seek(SeekFrom::Start(offset))?;
                    let mut bytes = vec![0; usize::try_from(encoded_size)?];
                    data.read_exact(&mut bytes)?;
                    pending_blocks.push((offset, metadata, bytes));
                    if pending_blocks.len() >= workers {
                        blocks.extend(validate_extraction_block_batch(
                            std::mem::take(&mut pending_blocks),
                            commit_header,
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
        }
        if !pending_blocks.is_empty() {
            blocks.extend(validate_extraction_block_batch(
                pending_blocks,
                commit_header,
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

        Toc::check_available_blocks(&header.changes, &block_indices)?;
        {
            for change in &header.changes {
                if let TocChange::PutFile { path, entry } = change {
                    if filters.is_some_and(|filters| !filters.includes(path.as_str(), false)) {
                        continue;
                    }
                    let source_blocks = match entry.source_commit {
                        Some(source) => historical_blocks
                            .get(&source)
                            .ok_or("missing source commit blocks")?,
                        None => &blocks,
                    };
                    let decoded_size = if let Some(offset) = entry.offset_in_block {
                        if entry.block_indices.len() != 1 {
                            None
                        } else {
                            source_blocks
                                .get(&entry.block_indices[0])
                                .and_then(|block| {
                                    u64::from(offset)
                                        .checked_add(entry.size)
                                        .filter(|&end| end <= block.uncompressed_size as u64)
                                        .map(|_| entry.size)
                                })
                        }
                    } else {
                        Some(entry.block_indices.iter().try_fold(0_u64, |size, index| {
                            size.checked_add(
                                source_blocks
                                    .get(index)
                                    .ok_or("missing decoded block")?
                                    .uncompressed_size as u64,
                            )
                            .ok_or("decoded file size overflow")
                        })?)
                    };
                    if decoded_size != Some(entry.size) {
                        return Err(format!(
                            "decoded size mismatch for {path}: expected {} bytes, got {decoded_size:?}",
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
                    if filters.is_some_and(|filters| !filters.includes(path.as_str(), false)) {
                        continue;
                    }
                    let mut source_blocks = Vec::with_capacity(entry.block_indices.len());
                    let available = match entry.source_commit {
                        Some(source) => historical_blocks
                            .get(&source)
                            .ok_or("missing source commit blocks")?,
                        None => &blocks,
                    };
                    for index in &entry.block_indices {
                        let block = available
                            .get(index)
                            .ok_or_else(|| format!("TOC references unavailable block {index}"))?
                            .clone();
                        source_blocks.push(block);
                    }
                    let archived_metadata = archived_metadata.get(path).cloned();
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
        if let Some(wanted) = references.get(&commit_header.commit_id) {
            historical_blocks.insert(
                commit_header.commit_id,
                blocks
                    .into_iter()
                    .filter(|(index, _)| wanted.contains(index))
                    .collect(),
            );
        }
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
            options.create(true).truncate(true);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::fs::OpenOptions;

    #[cfg(unix)]
    #[test]
    fn foreign_metadata_is_skipped_with_warnings() {
        use std::os::unix::fs::PermissionsExt;

        #[derive(Default)]
        struct WarningProgress(std::sync::Mutex<Vec<String>>);
        impl Progress for WarningProgress {
            fn message(&self, _: &str) {}
            fn advance(&self, _: u64) {}
            fn warning(&self, message: &str) {
                self.0.lock().unwrap().push(message.to_owned());
            }
        }

        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("file");
        std::fs::write(&target, b"content").unwrap();
        let original_mode = std::fs::metadata(&target).unwrap().permissions().mode();
        let file = File::open(&target).unwrap();
        let metadata = EntryMetadata {
            path: ArchivePath::new("file").unwrap(),
            platform: "foreign-platform".to_owned(),
            owner: None,
            group: None,
            permissions: Some(0),
            modified_unix_millis: None,
            created_unix_millis: None,
            accessed_unix_millis: None,
        };
        let progress = WarningProgress::default();
        apply_metadata(&file, Some(&metadata), &target, Some(&progress)).unwrap();
        apply_special_metadata(&target, Some(&metadata), true, Some(&progress)).unwrap();
        let warnings = progress.0.lock().unwrap();
        assert_eq!(warnings.len(), 2);
        assert!(
            warnings
                .iter()
                .all(|warning| warning.contains("foreign-platform"))
        );
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode(),
            original_mode
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"content");
    }

    #[cfg(unix)]
    #[test]
    fn special_entry_creation_handles_existing_directories_and_rejects_escape() {
        use std::os::unix::fs::FileTypeExt;

        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path();
        let target = destination.join("existing");
        std::fs::create_dir(&target).unwrap();
        let error = create_special_entry(
            destination,
            &target,
            &ExtractedKind::NamedPipe,
            ExistingPolicy::Error,
        )
        .unwrap_err();
        assert_eq!(
            error.downcast_ref::<io::Error>().unwrap().kind(),
            io::ErrorKind::AlreadyExists
        );
        create_special_entry(
            destination,
            &target,
            &ExtractedKind::NamedPipe,
            ExistingPolicy::Overwrite,
        )
        .unwrap();
        assert!(
            std::fs::symlink_metadata(&target)
                .unwrap()
                .file_type()
                .is_fifo()
        );
        let error = create_special_entry(
            destination,
            &destination.join("escape"),
            &ExtractedKind::Symlink("../outside".into()),
            ExistingPolicy::Error,
        )
        .unwrap_err();
        assert!(error.to_string().contains("outside") || error.to_string().contains("escape"));
        assert!(!destination.join("escape").exists());
    }

    #[test]
    fn cached_headers_preserve_references_and_still_validate_selected_data() {
        for to in [None, Some("00000001")] {
            for corrupt_data in [false, true] {
                let directory = tempfile::tempdir().unwrap();
                let input = directory.path().join("input.txt");
                let archive = directory.path().join("archive.zar");
                let contents = b"contents from the original commit";
                std::fs::write(&input, contents).unwrap();
                let archive_name = archive.to_str().unwrap();
                crate::operations::create_archive(archive_name, &[input]).unwrap();
                crate::operations::rename_archive_entry(
                    archive_name,
                    &ArchivePath::new("input.txt").unwrap(),
                    &ArchivePath::new("renamed.txt").unwrap(),
                )
                .unwrap();
                let path = if to.is_some() {
                    "input.txt"
                } else {
                    "renamed.txt"
                };
                let filters =
                    FilterSet::from_patterns(&[path.to_owned()], &[], &[], &[], &[] as &[PathBuf])
                        .unwrap();
                let passwords = PasswordSet::default();
                let mut file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&archive)
                    .unwrap();
                let headers = load_extraction_headers(&mut file, to, &passwords, None).unwrap();
                let expected_toc = headers.toc.clone();
                let data_start = headers.commits[0].data_start;
                let header_offset =
                    data_start + headers.commits[0].validated.commit_header.data_section_size;
                // Preparation must use its validated snapshot, not reread this header.
                file.seek(SeekFrom::Start(header_offset)).unwrap();
                file.write_all(&[0]).unwrap();
                if corrupt_data {
                    file.seek(SeekFrom::Start(data_start + 12)).unwrap();
                    let mut byte = [0];
                    file.read_exact(&mut byte).unwrap();
                    file.seek(SeekFrom::Start(data_start + 12)).unwrap();
                    file.write_all(&[byte[0] ^ 1]).unwrap();
                }
                drop(file);
                let state = load_extraction_state_impl(
                    archive_name,
                    to,
                    &passwords,
                    None,
                    Some(&filters),
                    Some(headers),
                    false,
                );
                if corrupt_data {
                    assert!(state.is_err());
                } else {
                    let (toc, sources, _) = state.unwrap();
                    assert_eq!(toc, expected_toc);
                    assert_eq!(sources.len(), 1);
                    let entry = &sources[&ArchivePath::new(path).unwrap()].entry;
                    assert_eq!(entry.source_commit.is_some(), to.is_none());
                    let output = directory.path().join("output.txt");
                    extract_single_file_from_state(
                        archive_name,
                        path,
                        &output,
                        &toc,
                        &sources,
                        true,
                    )
                    .unwrap();
                    assert_eq!(std::fs::read(output).unwrap(), contents);
                }
            }
        }
    }

    #[test]
    fn prepared_extraction_honors_cancellation() {
        struct Cancelled;
        impl Progress for Cancelled {
            fn message(&self, _: &str) {}
            fn advance(&self, _: u64) {}
            fn is_cancelled(&self) -> bool {
                true
            }
        }
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("archive.zar");
        std::fs::write(&archive, b"archive").unwrap();
        let result = PreparedExtraction::new_with_progress(
            archive.to_str().unwrap(),
            None,
            &PasswordSet::default(),
            Some(&Cancelled),
        );
        assert!(result.unwrap_err().to_string().contains("cancelled"));
    }

    #[test]
    fn prepared_extraction_rejects_archive_changes() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input.txt");
        let archive = directory.path().join("archive.zar");
        std::fs::write(&input, b"original").unwrap();
        super::super::create_archive(archive.to_str().unwrap(), &[input]).unwrap();
        let prepared =
            PreparedExtraction::new(archive.to_str().unwrap(), None, &PasswordSet::default())
                .unwrap();
        let output = directory.path().join("single.txt");
        prepared.extract_single_file("input.txt", &output).unwrap();
        assert_eq!(std::fs::read(output).unwrap(), b"original");

        std::fs::write(&archive, b"changed archive").unwrap();
        assert!(
            prepared
                .extract_single_file("input.txt", &directory.path().join("changed.txt"))
                .is_err()
        );

        let destination = directory.path().join("extracted");
        let options = ExtractionOptions {
            dry_run: false,
            to: None,
            collision_policy: CollisionPolicy::Error,
            existing_policy: ExistingPolicy::Error,
            restore_metadata: false,
            restore_symlinks: false,
            restore_devices: false,
            strip_top_level: false,
        };
        assert!(
            prepared
                .extract(&destination, options, None, &FilterSet::default())
                .is_err()
        );
    }

    #[test]
    fn prepared_extraction_rejects_modification_time_change() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input.txt");
        let archive = directory.path().join("archive.zar");
        std::fs::write(&input, b"original").unwrap();
        super::super::create_archive(archive.to_str().unwrap(), &[input]).unwrap();
        let prepared =
            PreparedExtraction::new(archive.to_str().unwrap(), None, &PasswordSet::default())
                .unwrap();
        let modified = std::fs::metadata(&archive).unwrap().modified().unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&archive)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(modified + std::time::Duration::from_secs(2)),
            )
            .unwrap();
        assert!(
            prepared
                .extract_single_file("input.txt", &directory.path().join("output.txt"))
                .is_err()
        );
    }

    #[test]
    fn special_targets_reject_links_outside_extraction_root() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("destination");
        let target = destination.join("nested/link");
        for safe in ["file", "./file", "../sibling"] {
            let source = ExtractedSource::symlink(safe.to_owned(), None);
            validate_special_target(&target, &destination, &source.kind).unwrap();
        }
        for unsafe_target in ["../../escape", "/absolute"] {
            let source = ExtractedSource::symlink(unsafe_target.to_owned(), None);
            assert!(validate_special_target(&target, &destination, &source.kind).is_err());
        }
        assert!(
            validate_special_target(
                &directory.path().join("outside/link"),
                &destination,
                &ExtractedSource::symlink("file".to_owned(), None).kind,
            )
            .is_err()
        );
        for source in [
            ExtractedSource::named_pipe(None),
            ExtractedSource::device(DeviceKind::Character, 1, 2, None),
        ] {
            validate_special_target(&target, &destination, &source.kind).unwrap();
        }
    }

    #[test]
    fn block_validation_enforces_size_limits_and_encryption_keys() {
        let header = CommitHeader::new(0, [1; 4], 0, 64).unwrap();
        let limits = CommitValidationLimits::default();
        let small_limits = CommitValidationLimits {
            max_uncompressed_block_size: 4,
            ..limits
        };
        let stored = BlockFrame::from_stored(1, b"plain text").unwrap();
        assert!(
            validate_extraction_frame(0, stored.clone(), &header, None, None, small_limits)
                .is_err()
        );
        let (_, block) = validate_extraction_frame(0, stored, &header, None, None, limits).unwrap();
        assert_eq!(block.uncompressed_size, 10);

        let contents = vec![b'x'; 1024];
        let compressed = BlockFrame::from_uncompressed(2, &contents, 3).unwrap();
        assert_eq!(compressed.flags & crate::BLOCK_FLAG_UNCOMPRESSED, 0);
        assert!(
            validate_extraction_frame(0, compressed.clone(), &header, None, None, small_limits)
                .is_err()
        );
        let (_, block) =
            validate_extraction_frame(0, compressed, &header, None, None, limits).unwrap();
        assert_eq!(block.uncompressed_size, contents.len());

        let key = Zeroizing::new([7; DEK_SIZE]);
        let algorithm = ContentAead::Aes256Gcm;
        let encrypted =
            BlockFrame::from_uncompressed_encrypted(3, &contents, 3, &header, &key, algorithm)
                .unwrap();
        assert!(
            validate_extraction_frame(0, encrypted.clone(), &header, None, None, limits).is_err()
        );
        let wrong_key = Zeroizing::new([8; DEK_SIZE]);
        assert!(
            validate_extraction_frame(
                0,
                encrypted.clone(),
                &header,
                Some(algorithm),
                Some(&wrong_key),
                limits
            )
            .is_err()
        );
        let (_, block) = validate_extraction_frame(
            0,
            encrypted.clone(),
            &header,
            Some(algorithm),
            Some(&key),
            limits,
        )
        .unwrap();
        let mut file = tempfile::tempfile().unwrap();
        encrypted.write_to(&mut file).unwrap();
        assert_eq!(
            decode_extraction_block(&mut file, &block).unwrap(),
            contents
        );
        let mut missing_algorithm = block.clone();
        missing_algorithm.algorithm = None;
        assert!(
            decode_extraction_block(&mut file, &missing_algorithm)
                .unwrap_err()
                .to_string()
                .contains("no content algorithm")
        );
        let mut missing_key = block;
        missing_key.dek = None;
        assert!(
            decode_extraction_block(&mut file, &missing_key)
                .unwrap_err()
                .to_string()
                .contains("not been unlocked")
        );
    }

    #[cfg(not(unix))]
    #[test]
    fn unsupported_special_entries_are_not_created() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("special");
        for kind in [
            ExtractedKind::Symlink("file".to_owned()),
            ExtractedKind::NamedPipe,
            ExtractedKind::Device {
                kind: DeviceKind::Block,
                major: 1,
                minor: 2,
            },
        ] {
            let error =
                create_special_entry(directory.path(), &target, &kind, ExistingPolicy::Error)
                    .unwrap_err();
            assert!(error.to_string().contains("unsupported"));
            assert!(!target.exists());
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_metadata_skips_foreign_platform_and_restores_readonly() {
        #[derive(Default)]
        struct WarningProgress(std::sync::Mutex<Vec<String>>);

        impl Progress for WarningProgress {
            fn message(&self, _: &str) {}

            fn advance(&self, _: u64) {}

            fn warning(&self, message: &str) {
                self.0.lock().unwrap().push(message.to_owned());
            }
        }

        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("output");
        std::fs::write(&target, b"content").unwrap();
        let file = File::open(&target).unwrap();
        let mut metadata = EntryMetadata {
            path: ArchivePath::new("output").unwrap(),
            platform: "foreign-platform".to_owned(),
            owner: None,
            group: None,
            permissions: Some(1),
            modified_unix_millis: None,
            created_unix_millis: None,
            accessed_unix_millis: None,
        };
        let progress = WarningProgress::default();
        apply_metadata(&file, Some(&metadata), &target, Some(&progress)).unwrap();
        apply_special_metadata(&target, Some(&metadata), true, Some(&progress)).unwrap();
        restore_directory_metadata(
            directory.path(),
            &[
                (directory.path().to_path_buf(), Some(metadata.clone())),
                (target.clone(), None),
            ],
            Some(&progress),
        )
        .unwrap();
        let warnings = progress.0.lock().unwrap();
        assert_eq!(warnings.len(), 3);
        assert!(
            warnings
                .iter()
                .all(|warning| warning.contains("foreign-platform"))
        );
        drop(warnings);
        assert!(!std::fs::metadata(&target).unwrap().permissions().readonly());
        assert!(
            !std::fs::metadata(directory.path())
                .unwrap()
                .permissions()
                .readonly()
        );

        metadata.platform = platform_identifier().to_owned();
        apply_special_metadata(&target, Some(&metadata), false, None).unwrap();
        apply_special_metadata(&target, None, true, None).unwrap();
        assert!(!std::fs::metadata(&target).unwrap().permissions().readonly());
        apply_special_metadata(&target, Some(&metadata), true, None).unwrap();
        assert!(std::fs::metadata(&target).unwrap().permissions().readonly());
        metadata.permissions = Some(0);
        apply_metadata(&file, Some(&metadata), &target, None).unwrap();
        assert!(!std::fs::metadata(&target).unwrap().permissions().readonly());
    }

    #[test]
    fn regular_extraction_rejects_changed_sources_and_skips_existing_files() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("sample.bin");
        let archive = directory.path().join("sample.zar");
        std::fs::write(&input, b"sample file contents").unwrap();
        crate::operations::create_archive(archive.to_str().unwrap(), &[input]).unwrap();
        let archive_path = archive.to_str().unwrap();
        let (_, sources, _) = load_extraction_state(archive_path, None, None).unwrap();
        let source = &sources[&ArchivePath::new("sample.bin").unwrap()];
        assert!(!source.blocks.is_empty());
        assert!(source.entry.checksum.is_some());
        let output = directory.path().join("output.bin");
        let mut reader = ExtractionReader::open(archive_path, true).unwrap();

        let mut changed = source.clone();
        changed.blocks[0].uncompressed_size += 1;
        let error = extract_regular_file(
            &mut reader,
            directory.path(),
            &output,
            &changed,
            ExistingPolicy::Overwrite,
            false,
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("block changed"));

        reader.block_cache = None;
        let mut changed = source.clone();
        changed.blocks[0].block_index += 1;
        let error = extract_regular_file(
            &mut reader,
            directory.path(),
            &output,
            &changed,
            ExistingPolicy::Overwrite,
            false,
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("block index changed"));

        let mut changed = source.clone();
        changed.entry.checksum = changed.entry.checksum.map(|checksum| checksum ^ 1);
        reader.validate_blocks = false;
        let error = extract_regular_file(
            &mut reader,
            directory.path(),
            &output,
            &changed,
            ExistingPolicy::Overwrite,
            false,
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("checksum mismatch"));
        reader.validate_blocks = true;

        let mut changed = source.clone();
        changed.blocks.clear();
        let error = extract_regular_file(
            &mut reader,
            directory.path(),
            &output,
            &changed,
            ExistingPolicy::Overwrite,
            false,
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("decoded size mismatch"));

        reader.block_cache = None;
        let mut changed = source.clone();
        changed.entry.offset_in_block = Some(u32::MAX);
        let error = extract_regular_file(
            &mut reader,
            directory.path(),
            &output,
            &changed,
            ExistingPolicy::Overwrite,
            false,
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("packed file range"));

        std::fs::write(&output, b"keep existing").unwrap();
        extract_regular_file(
            &mut reader,
            directory.path(),
            &output,
            source,
            ExistingPolicy::Skip,
            false,
            None,
        )
        .unwrap();
        assert_eq!(std::fs::read(output).unwrap(), b"keep existing");
    }

    #[test]
    fn packed_block_cache_decodes_once_per_offset() {
        let mut cache = None;
        let calls = Cell::new(0);
        let decode = || {
            calls.set(calls.get() + 1);
            Ok(vec![calls.get()])
        };
        assert_eq!(
            cached_extraction_block(&mut cache, 10, decode).unwrap(),
            [1]
        );
        assert_eq!(
            cached_extraction_block(&mut cache, 10, decode).unwrap(),
            [1]
        );
        assert_eq!(
            cached_extraction_block(&mut cache, 20, decode).unwrap(),
            [2]
        );
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn extraction_option_controls_output_block_checksum() {
        struct ChangeChecksum {
            archive: PathBuf,
            offset: u64,
        }

        impl Progress for ChangeChecksum {
            fn message(&self, _: &str) {}
            fn advance(&self, _: u64) {}
            fn reset_phase(&self, message: &str, _: u64) {
                if message == "Extracting files" {
                    let mut file = OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(&self.archive)
                        .unwrap();
                    file.seek(SeekFrom::Start(self.offset + 12)).unwrap();
                    let mut byte = [0];
                    file.read_exact(&mut byte).unwrap();
                    file.seek(SeekFrom::Start(self.offset + 12)).unwrap();
                    file.write_all(&[byte[0] ^ 1]).unwrap();
                }
            }
        }

        for prepared in [false, true] {
            for validate in [false, true] {
                let directory = tempfile::tempdir().unwrap();
                let input = directory.path().join("input.txt");
                let archive = directory.path().join("archive.zar");
                std::fs::write(&input, b"sample contents").unwrap();
                crate::operations::create_archive(archive.to_str().unwrap(), &[input]).unwrap();
                let archive_path = archive.to_str().unwrap();
                let (_, sources, _) = load_extraction_state(archive_path, None, None).unwrap();
                let offset = sources[&ArchivePath::new("input.txt").unwrap()].blocks[0].offset;
                let state = prepared.then(|| {
                    PreparedExtraction::new(archive_path, None, &PasswordSet::default()).unwrap()
                });
                let progress = ChangeChecksum {
                    archive: archive.clone(),
                    offset,
                };
                let destination = directory.path().join("output");
                let options = ExtractionOptions {
                    dry_run: false,
                    to: None,
                    collision_policy: CollisionPolicy::Error,
                    existing_policy: ExistingPolicy::Error,
                    restore_metadata: false,
                    restore_symlinks: false,
                    restore_devices: false,
                    strip_top_level: false,
                };
                let filters = FilterSet::default();
                let result = if let Some(state) = state {
                    state.extract_with_block_validation(
                        &destination,
                        options,
                        Some(&progress),
                        &filters,
                        validate,
                    )
                } else {
                    extract_archive_with_validation_options(
                        archive_path,
                        &destination,
                        options,
                        &PasswordSet::default(),
                        Some(&progress),
                        &filters,
                        ExtractionFrameValidation {
                            skip_unselected: false,
                            validate_on_read: validate,
                        },
                    )
                };
                if validate {
                    assert!(result.is_err());
                } else {
                    result.unwrap();
                    assert_eq!(
                        std::fs::read(destination.join("input.txt")).unwrap(),
                        b"sample contents"
                    );
                }
            }
        }
    }

    #[test]
    fn single_block_references_reuse_decoded_data() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input.bin");
        let archive = directory.path().join("archive.zar");
        let contents = vec![b'x'; 65_536];
        std::fs::write(&input, &contents).unwrap();
        crate::operations::create_archive_with_encoding(
            archive.to_str().unwrap(),
            &[input],
            false,
            EncodingOptions {
                block_size: 65_536,
                compression_level: 1,
            },
        )
        .unwrap();
        let (_, sources, _) = load_extraction_state(archive.to_str().unwrap(), None, None).unwrap();
        let source = &sources[&ArchivePath::new("input.bin").unwrap()];
        assert_eq!(source.blocks.len(), 1);
        assert!(source.entry.offset_in_block.is_none());
        let mut reader = ExtractionReader::open(archive.to_str().unwrap(), true).unwrap();
        let first = directory.path().join("first.bin");
        extract_regular_file(
            &mut reader,
            directory.path(),
            &first,
            source,
            ExistingPolicy::Error,
            false,
            None,
        )
        .unwrap();
        let mut writer = OpenOptions::new().write(true).open(&archive).unwrap();
        writer
            .seek(SeekFrom::Start(source.blocks[0].offset))
            .unwrap();
        writer.write_all(&[0]).unwrap();
        drop(writer);
        let second = directory.path().join("second.bin");
        extract_regular_file(
            &mut reader,
            directory.path(),
            &second,
            source,
            ExistingPolicy::Error,
            false,
            None,
        )
        .unwrap();
        assert_eq!(std::fs::read(first).unwrap(), contents);
        assert_eq!(std::fs::read(second).unwrap(), contents);
    }

    #[test]
    fn extraction_reuses_single_block_for_multiple_files() {
        use std::sync::atomic::{AtomicBool, Ordering};

        struct CorruptAfterFirstRead {
            archive: PathBuf,
            offset: u64,
            extracting: AtomicBool,
            corrupted: AtomicBool,
        }

        impl Progress for CorruptAfterFirstRead {
            fn message(&self, _: &str) {}

            fn reset_phase(&self, message: &str, _: u64) {
                self.extracting
                    .store(message == "Extracting files", Ordering::SeqCst);
            }

            fn advance(&self, amount: u64) {
                if self.extracting.load(Ordering::SeqCst)
                    && amount == 3
                    && !self.corrupted.swap(true, Ordering::SeqCst)
                {
                    let mut file = OpenOptions::new().write(true).open(&self.archive).unwrap();
                    file.seek(SeekFrom::Start(self.offset)).unwrap();
                    file.write_all(&[0]).unwrap();
                }
            }
        }

        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("archive.zar");
        let inputs = ["first", "second", "third"].map(|name| {
            let path = directory.path().join(name);
            std::fs::write(&path, b"abc").unwrap();
            path
        });
        crate::operations::create_archive(archive.to_str().unwrap(), &inputs).unwrap();
        let (_, sources, _) = load_extraction_state(archive.to_str().unwrap(), None, None).unwrap();
        let blocks: Vec<_> = sources
            .values()
            .map(|source| {
                assert!(source.entry.offset_in_block.is_none());
                assert_eq!(source.blocks.len(), 1);
                source.blocks[0].offset
            })
            .collect();
        assert_eq!(blocks.len(), 3);
        assert!(blocks.iter().all(|offset| *offset == blocks[0]));
        let progress = CorruptAfterFirstRead {
            archive: archive.clone(),
            offset: blocks[0],
            extracting: AtomicBool::new(false),
            corrupted: AtomicBool::new(false),
        };
        let destination = directory.path().join("output");
        extract_archive_with_progress(
            archive.to_str().unwrap(),
            &destination,
            ExtractionOptions {
                dry_run: false,
                to: None,
                collision_policy: CollisionPolicy::Error,
                existing_policy: ExistingPolicy::Error,
                restore_metadata: false,
                restore_symlinks: false,
                restore_devices: false,
                strip_top_level: false,
            },
            None,
            Some(&progress),
        )
        .unwrap();
        assert!(progress.corrupted.load(Ordering::SeqCst));
        for name in ["first", "second", "third"] {
            assert_eq!(std::fs::read(destination.join(name)).unwrap(), b"abc");
        }
    }
}
