use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyModule};
use std::path::PathBuf;
use std::sync::Mutex;
use zarlib::operations::{
    self, AddInputOptions, CheckReport, CollisionPolicy, CommitInfo, CreateFilterOptions,
    CreateInputOptions, EncodingOptions, ExistingPolicy, ExtractedKind, ExtractionOptions,
    FilterSet, Progress,
};
use zarlib::{DeviceKind, Toc, TocEntry};

struct WarningCollector {
    warnings: Mutex<Vec<String>>,
}

impl WarningCollector {
    fn new() -> Self {
        Self {
            warnings: Mutex::new(Vec::new()),
        }
    }

    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.warnings.lock().expect("warning lock poisoned"))
    }
}

impl Progress for WarningCollector {
    fn message(&self, _message: &str) {}

    fn advance(&self, _amount: u64) {}

    fn warning(&self, message: &str) {
        self.warnings
            .lock()
            .expect("warning lock poisoned")
            .push(message.to_owned());
    }
}

fn runtime_error(error: impl std::fmt::Display) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
}

fn value_error(error: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn archive_string(path: PathBuf) -> PyResult<String> {
    path.into_os_string()
        .into_string()
        .map_err(|_| value_error("archive path is not valid UTF-8"))
}

fn encoding_options(block_size: usize, compression_level: i32) -> PyResult<EncodingOptions> {
    if block_size != 0 && !(64 * 1024..=32 * 1024 * 1024).contains(&block_size) {
        return Err(value_error(
            "block_size must be 0 (automatic) or between 64 KiB and 32 MiB",
        ));
    }
    if !(-7..=22).contains(&compression_level) {
        return Err(value_error("compression_level must be between -7 and 22"));
    }
    Ok(EncodingOptions {
        block_size,
        compression_level,
    })
}

fn filters(
    include_files: Option<Vec<String>>,
    exclude_files: Option<Vec<String>>,
    include_folders: Option<Vec<String>>,
    exclude_folders: Option<Vec<String>>,
) -> PyResult<FilterSet> {
    FilterSet::from_patterns(
        &include_files.unwrap_or_default(),
        &exclude_files.unwrap_or_default(),
        &include_folders.unwrap_or_default(),
        &exclude_folders.unwrap_or_default(),
        &[] as &[PathBuf],
    )
    .map_err(value_error)
}

fn collision_policy(policy: &str) -> PyResult<CollisionPolicy> {
    match policy {
        "error" => Ok(CollisionPolicy::Error),
        "latest" => Ok(CollisionPolicy::Latest),
        "first" => Ok(CollisionPolicy::First),
        "rename" => Ok(CollisionPolicy::Rename),
        "ask" => Ok(CollisionPolicy::Ask),
        _ => Err(value_error(
            "collision must be one of: error, latest, first, rename, ask",
        )),
    }
}

fn existing_policy(policy: &str) -> PyResult<ExistingPolicy> {
    match policy {
        "overwrite" => Ok(ExistingPolicy::Overwrite),
        "skip" => Ok(ExistingPolicy::Skip),
        "error" => Ok(ExistingPolicy::Error),
        "ask" => Ok(ExistingPolicy::Ask),
        _ => Err(value_error(
            "existing must be one of: overwrite, skip, error, ask",
        )),
    }
}

fn commit_id(id: &[u8; 4]) -> String {
    id.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[pyfunction]
#[pyo3(signature = (
    archive,
    paths,
    *,
    strict = false,
    overwrite = false,
    save_metadata = false,
    include_special_files = false,
    verbose = false,
    block_size = 8388608,
    compression_level = 3,
    password = None,
    include_files = None,
    exclude_files = None,
    include_folders = None,
    exclude_folders = None
))]
#[allow(clippy::too_many_arguments)]
fn create_archive(
    py: Python<'_>,
    archive: PathBuf,
    paths: Vec<PathBuf>,
    strict: bool,
    overwrite: bool,
    save_metadata: bool,
    include_special_files: bool,
    verbose: bool,
    block_size: usize,
    compression_level: i32,
    password: Option<Vec<u8>>,
    include_files: Option<Vec<String>>,
    exclude_files: Option<Vec<String>>,
    include_folders: Option<Vec<String>>,
    exclude_folders: Option<Vec<String>>,
) -> PyResult<Bound<'_, PyDict>> {
    let archive = archive_string(archive)?;
    let options = encoding_options(block_size, compression_level)?;
    let filters = filters(
        include_files,
        exclude_files,
        include_folders,
        exclude_folders,
    )?;
    let report = py
        .detach(move || {
            operations::create_archive_with_password_progress_and_filters(
                &archive,
                &paths,
                strict,
                options,
                password.as_deref(),
                None,
                CreateFilterOptions {
                    input: CreateInputOptions {
                        save_metadata,
                        include_special_files,
                        overwrite_existing: overwrite,
                        verbose,
                    },
                    filters: &filters,
                },
            )
            .map_err(|error| error.to_string())
        })
        .map_err(runtime_error)?;
    let result = PyDict::new(py);
    result.set_item("block_size", report.block_size)?;
    result.set_item("warnings", report.warnings)?;
    Ok(result)
}

#[pyfunction]
#[pyo3(signature = (
    archive,
    paths,
    *,
    strict = false,
    save_metadata = false,
    include_special_files = false,
    verbose = false,
    block_size = 8388608,
    compression_level = 3,
    include_files = None,
    exclude_files = None,
    include_folders = None,
    exclude_folders = None
))]
#[allow(clippy::too_many_arguments)]
fn add_archive(
    py: Python<'_>,
    archive: PathBuf,
    paths: Vec<PathBuf>,
    strict: bool,
    save_metadata: bool,
    include_special_files: bool,
    verbose: bool,
    block_size: usize,
    compression_level: i32,
    include_files: Option<Vec<String>>,
    exclude_files: Option<Vec<String>>,
    include_folders: Option<Vec<String>>,
    exclude_folders: Option<Vec<String>>,
) -> PyResult<Bound<'_, PyDict>> {
    let archive = archive_string(archive)?;
    let options = encoding_options(block_size, compression_level)?;
    let filters = filters(
        include_files,
        exclude_files,
        include_folders,
        exclude_folders,
    )?;
    let warnings = WarningCollector::new();
    py.detach(|| {
        operations::add_archive_with_encoding_progress_and_options_and_filters(
            &archive,
            &paths,
            options,
            Some(&warnings),
            AddInputOptions {
                save_metadata,
                include_special_files,
                strict,
                verbose,
            },
            &filters,
        )
        .map_err(|error| error.to_string())
    })
    .map_err(runtime_error)?;
    let result = PyDict::new(py);
    result.set_item("warnings", warnings.take())?;
    Ok(result)
}

#[pyfunction]
fn remove_archive(py: Python<'_>, archive: PathBuf, paths: Vec<PathBuf>) -> PyResult<()> {
    let archive = archive_string(archive)?;
    py.detach(|| operations::remove_archive(&archive, &paths).map_err(|e| e.to_string()))
        .map_err(runtime_error)
}

#[pyfunction]
#[pyo3(signature = (
    archive,
    from_id,
    through_id,
    *,
    block_size = 8388608,
    compression_level = 3
))]
fn join_archive(
    py: Python<'_>,
    archive: PathBuf,
    from_id: String,
    through_id: String,
    block_size: usize,
    compression_level: i32,
) -> PyResult<Bound<'_, PyDict>> {
    let archive = archive_string(archive)?;
    let options = encoding_options(block_size, compression_level)?;
    let warnings = WarningCollector::new();
    py.detach(|| {
        operations::join_archive_with_encoding_progress(
            &archive,
            &from_id,
            &through_id,
            options,
            Some(&warnings),
        )
        .map_err(|error| error.to_string())
    })
    .map_err(runtime_error)?;
    let result = PyDict::new(py);
    result.set_item("warnings", warnings.take())?;
    Ok(result)
}

#[pyfunction]
#[pyo3(signature = (archive, *, recover = false, password = None))]
fn check_archive(
    py: Python<'_>,
    archive: PathBuf,
    recover: bool,
    password: Option<Vec<u8>>,
) -> PyResult<Bound<'_, PyDict>> {
    let archive = archive_string(archive)?;
    let report = py
        .detach(|| {
            operations::check_archive_with_recovery(&archive, recover, password.as_deref())
                .map_err(|error| error.to_string())
        })
        .map_err(runtime_error)?;
    check_report_dict(py, report)
}

#[pyfunction]
#[pyo3(signature = (archive, *, verbose = false))]
fn info_archive(py: Python<'_>, archive: PathBuf, verbose: bool) -> PyResult<Bound<'_, PyList>> {
    let archive = archive_string(archive)?;
    let commits = py
        .detach(|| operations::info_archive(&archive, verbose).map_err(|error| error.to_string()))
        .map_err(runtime_error)?;
    let output = PyList::empty(py);
    for commit in commits {
        output.append(commit_info_dict(py, commit)?)?;
    }
    Ok(output)
}

#[pyfunction]
#[pyo3(signature = (archive, *, through = None, password = None))]
fn list_archive(
    py: Python<'_>,
    archive: PathBuf,
    through: Option<String>,
    password: Option<Vec<u8>>,
) -> PyResult<Bound<'_, PyList>> {
    let archive = archive_string(archive)?;
    let toc = py
        .detach(|| {
            operations::list_archive(&archive, through.as_deref(), password.as_deref())
                .map_err(|error| error.to_string())
        })
        .map_err(runtime_error)?;
    toc_dicts(py, toc)
}

#[pyfunction]
#[pyo3(signature = (
    archive,
    destination,
    *,
    dry_run = false,
    through = None,
    password = None,
    collision = "rename",
    existing = "error",
    restore_metadata = false,
    restore_symlinks = false,
    restore_devices = false,
    include_files = None,
    exclude_files = None,
    include_folders = None,
    exclude_folders = None
))]
#[allow(clippy::too_many_arguments)]
fn extract_archive<'py>(
    py: Python<'py>,
    archive: PathBuf,
    destination: PathBuf,
    dry_run: bool,
    through: Option<String>,
    password: Option<Vec<u8>>,
    collision: &str,
    existing: &str,
    restore_metadata: bool,
    restore_symlinks: bool,
    restore_devices: bool,
    include_files: Option<Vec<String>>,
    exclude_files: Option<Vec<String>>,
    include_folders: Option<Vec<String>>,
    exclude_folders: Option<Vec<String>>,
) -> PyResult<Bound<'py, PyDict>> {
    let archive = archive_string(archive)?;
    let collision_policy = collision_policy(collision)?;
    let existing_policy = existing_policy(existing)?;
    let filters = filters(
        include_files,
        exclude_files,
        include_folders,
        exclude_folders,
    )?;
    let plan = py
        .detach(|| {
            operations::extract_archive_with_progress_and_filters(
                &archive,
                &destination,
                ExtractionOptions {
                    dry_run,
                    through: through.as_deref(),
                    collision_policy,
                    existing_policy,
                    restore_metadata,
                    restore_symlinks,
                    restore_devices,
                },
                password.as_deref(),
                None,
                &filters,
            )
            .map_err(|error| error.to_string())
        })
        .map_err(runtime_error)?;
    let result = PyDict::new(py);
    result.set_item(
        "directories",
        plan.directories
            .iter()
            .map(|path| display_path(path))
            .collect::<Vec<_>>(),
    )?;
    let files = PyList::empty(py);
    for (path, entry, kind) in plan.files {
        let file = PyDict::new(py);
        file.set_item("path", display_path(&path))?;
        file.set_item("size", entry.size)?;
        file.set_item("block_indices", entry.block_indices)?;
        match kind {
            ExtractedKind::File => file.set_item("type", "file")?,
            ExtractedKind::Symlink(target) => {
                file.set_item("type", "symlink")?;
                file.set_item("target", target)?;
            }
            ExtractedKind::NamedPipe => file.set_item("type", "named_pipe")?,
            ExtractedKind::Device { kind, major, minor } => {
                file.set_item("type", "device")?;
                file.set_item(
                    "device_kind",
                    match kind {
                        DeviceKind::Character => "character",
                        DeviceKind::Block => "block",
                    },
                )?;
                file.set_item("major", major)?;
                file.set_item("minor", minor)?;
            }
        }
        files.append(file)?;
    }
    result.set_item("files", files)?;
    result.set_item("dry_run", dry_run)?;
    Ok(result)
}

#[pyfunction]
#[pyo3(signature = (
    archive,
    *,
    through = None,
    password = None,
    size_limit = None,
    offset = 0,
    include_files = None,
    exclude_files = None,
    include_folders = None,
    exclude_folders = None
))]
#[allow(clippy::too_many_arguments)]
fn extract_archive_to_memory(
    py: Python<'_>,
    archive: PathBuf,
    through: Option<String>,
    password: Option<Vec<u8>>,
    size_limit: Option<u64>,
    offset: u64,
    include_files: Option<Vec<String>>,
    exclude_files: Option<Vec<String>>,
    include_folders: Option<Vec<String>>,
    exclude_folders: Option<Vec<String>>,
) -> PyResult<Bound<'_, PyDict>> {
    let archive = archive_string(archive)?;
    let filters = filters(
        include_files,
        exclude_files,
        include_folders,
        exclude_folders,
    )?;
    let files = py
        .detach(|| {
            operations::extract_archive_to_memory_with_filters(
                &archive,
                through.as_deref(),
                password.as_deref(),
                size_limit,
                offset,
                &filters,
            )
            .map_err(|error| error.to_string())
        })
        .map_err(runtime_error)?;
    let result = PyDict::new(py);
    for (path, contents) in files {
        result.set_item(path, PyBytes::new(py, &contents))?;
    }
    Ok(result)
}

fn display_path(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

fn check_report_dict(py: Python<'_>, report: CheckReport) -> PyResult<Bound<'_, PyDict>> {
    let result = PyDict::new(py);
    result.set_item("commits", report.commits)?;
    result.set_item("checked_blocks", report.checked_blocks)?;
    result.set_item("encrypted_blocks", report.encrypted_blocks)?;
    result.set_item("recoveries", report.recoveries)?;
    result.set_item("logical_state_readable", report.logical_state_readable)?;
    result.set_item("recovery_messages", report.recovery_messages)?;
    Ok(result)
}

fn commit_info_dict(py: Python<'_>, commit: CommitInfo) -> PyResult<Bound<'_, PyDict>> {
    let result = PyDict::new(py);
    result.set_item("id", commit_id(&commit.id))?;
    result.set_item("size", commit.size)?;
    result.set_item("header_size", commit.header_size)?;
    result.set_item("data_size", commit.data_size)?;
    result.set_item("uncompressed_size", commit.uncompressed_size)?;
    result.set_item("ratio_percent", commit.ratio_percent)?;
    result.set_item("header_encrypted", commit.header_encrypted)?;
    if let Some(header) = commit.header_compression {
        let header_dict = PyDict::new(py);
        header_dict.set_item("compressed_size", header.compressed_size)?;
        header_dict.set_item("uncompressed_size", header.uncompressed_size)?;
        header_dict.set_item("ratio_percent", header.ratio_percent)?;
        result.set_item("header_compression", header_dict)?;
    } else {
        result.set_item("header_compression", py.None())?;
    }
    if let Some(details) = commit.details {
        let details_dict = PyDict::new(py);
        let groups = PyDict::new(py);
        for (name, group) in details.groups {
            let group_dict = PyDict::new(py);
            group_dict.set_item("files", group.files)?;
            group_dict.set_item("compressed_size", group.compressed_size)?;
            group_dict.set_item("uncompressed_size", group.uncompressed_size)?;
            group_dict.set_item("has_encrypted_blocks", group.has_encrypted_blocks)?;
            groups.set_item(name, group_dict)?;
        }
        details_dict.set_item("groups", groups)?;
        details_dict.set_item("deduplicated_files", details.deduplicated_files)?;
        details_dict.set_item(
            "deduplicated_uncompressed_size",
            details.deduplicated_uncompressed_size,
        )?;
        details_dict.set_item("symlinks", details.symlinks)?;
        details_dict.set_item("named_pipes", details.named_pipes)?;
        details_dict.set_item("devices", details.devices)?;
        result.set_item("details", details_dict)?;
    } else {
        result.set_item("details", py.None())?;
    }
    Ok(result)
}

fn toc_dicts(py: Python<'_>, toc: Toc) -> PyResult<Bound<'_, PyList>> {
    let output = PyList::empty(py);
    for (path, entry) in toc.iter() {
        let record = PyDict::new(py);
        record.set_item("path", path.as_str())?;
        match entry {
            TocEntry::File(file) => {
                record.set_item("type", "file")?;
                record.set_item("size", file.size)?;
                record.set_item("block_indices", file.block_indices.clone())?;
                record.set_item("checksum", file.checksum)?;
            }
            TocEntry::Directory => record.set_item("type", "directory")?,
            TocEntry::Symlink { target } => {
                record.set_item("type", "symlink")?;
                record.set_item("target", target)?;
            }
            TocEntry::NamedPipe => record.set_item("type", "named_pipe")?,
            TocEntry::Device { kind, major, minor } => {
                record.set_item("type", "device")?;
                record.set_item(
                    "device_kind",
                    match kind {
                        DeviceKind::Character => "character",
                        DeviceKind::Block => "block",
                    },
                )?;
                record.set_item("major", major)?;
                record.set_item("minor", minor)?;
            }
        }
        output.append(record)?;
    }
    Ok(output)
}

#[pymodule(gil_used = false)]
fn _pyzarlib(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(create_archive, module)?)?;
    module.add_function(wrap_pyfunction!(add_archive, module)?)?;
    module.add_function(wrap_pyfunction!(remove_archive, module)?)?;
    module.add_function(wrap_pyfunction!(join_archive, module)?)?;
    module.add_function(wrap_pyfunction!(check_archive, module)?)?;
    module.add_function(wrap_pyfunction!(info_archive, module)?)?;
    module.add_function(wrap_pyfunction!(list_archive, module)?)?;
    module.add_function(wrap_pyfunction!(extract_archive, module)?)?;
    module.add_function(wrap_pyfunction!(extract_archive_to_memory, module)?)?;
    Ok(())
}
