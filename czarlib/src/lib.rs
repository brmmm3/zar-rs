use serde_json::{Map, Value, json};
use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Mutex;
use zarlib::operations::{
    self, AddInputOptions, CollisionPolicy, CreateFilterOptions, CreateInputOptions,
    EncodingOptions, ExistingPolicy, ExtractionOptions, FilterSet, Progress,
};
use zarlib::{ContentAead, EncryptionScope};

struct WarningCollector(Mutex<Vec<String>>);

impl WarningCollector {
    fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }

    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock().expect("warning lock poisoned"))
    }
}

impl Progress for WarningCollector {
    fn message(&self, _message: &str) {}

    fn advance(&self, _amount: u64) {}

    fn warning(&self, message: &str) {
        self.0
            .lock()
            .expect("warning lock poisoned")
            .push(message.to_owned());
    }
}

fn required_string<'a>(request: &'a Value, key: &str) -> Result<&'a str, String> {
    request
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("'{key}' must be a string"))
}

fn optional_string<'a>(request: &'a Value, key: &str) -> Result<Option<&'a str>, String> {
    match request.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        _ => Err(format!("'{key}' must be a string or null")),
    }
}

fn bool_option(request: &Value, key: &str, default: bool) -> Result<bool, String> {
    request.get(key).map_or(Ok(default), |value| {
        value
            .as_bool()
            .ok_or_else(|| format!("'{key}' must be a boolean"))
    })
}

fn integer_option<T>(request: &Value, key: &str, default: T) -> Result<T, String>
where
    T: TryFrom<u64> + Copy,
{
    match request.get(key) {
        None => Ok(default),
        Some(value) => value
            .as_u64()
            .and_then(|number| T::try_from(number).ok())
            .ok_or_else(|| format!("'{key}' must be a non-negative integer in range")),
    }
}

fn paths(request: &Value) -> Result<Vec<PathBuf>, String> {
    request
        .get("paths")
        .and_then(Value::as_array)
        .ok_or_else(|| "'paths' must be an array of strings".to_owned())?
        .iter()
        .map(|path| {
            path.as_str()
                .map(PathBuf::from)
                .ok_or_else(|| "'paths' must contain only strings".to_owned())
        })
        .collect()
}

fn string_list(request: &Value, key: &str) -> Result<Vec<String>, String> {
    match request.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("'{key}' must contain only strings"))
            })
            .collect(),
        _ => Err(format!("'{key}' must be an array of strings")),
    }
}

fn filters(request: &Value) -> Result<FilterSet, String> {
    let include_files = string_list(request, "include_files")?;
    let exclude_files = string_list(request, "exclude_files")?;
    let include_folders = string_list(request, "include_folders")?;
    let exclude_folders = string_list(request, "exclude_folders")?;
    FilterSet::from_patterns(
        &include_files,
        &exclude_files,
        &include_folders,
        &exclude_folders,
        &[] as &[PathBuf],
    )
    .map_err(|error| error.to_string())
}

fn encoding_options(request: &Value) -> Result<EncodingOptions, String> {
    let block_size = integer_option(request, "block_size", 8 * 1024 * 1024_usize)?;
    if block_size != 0 && !(64 * 1024..=32 * 1024 * 1024).contains(&block_size) {
        return Err("'block_size' must be 0 (automatic) or between 64 KiB and 32 MiB".to_owned());
    }
    let compression_level = request
        .get("compression_level")
        .map_or(Ok(3_i32), |value| {
            value
                .as_i64()
                .and_then(|level| i32::try_from(level).ok())
                .ok_or_else(|| "'compression_level' must be an integer".to_owned())
        })?;
    if !(-7..=22).contains(&compression_level) {
        return Err("'compression_level' must be between -7 and 22".to_owned());
    }
    Ok(EncodingOptions {
        block_size,
        compression_level,
    })
}

fn password(request: &Value) -> Result<Option<Vec<u8>>, String> {
    optional_string(request, "password")
        .map(|password| password.map(|value| value.as_bytes().to_vec()))
}

fn passwords(request: &Value) -> Result<operations::PasswordSet, String> {
    let mut default = password(request)?;
    let mut entries_by_commit = Vec::new();
    if let Some(entries) = request.get("passwords") {
        let entries = entries
            .as_object()
            .ok_or("'passwords' must be an object keyed by commit ID")?;
        for (commit_id, value) in entries {
            let value = value
                .as_str()
                .ok_or("password values must be strings")?
                .as_bytes()
                .to_vec();
            if commit_id == "default" {
                if default.is_some() {
                    return Err("specify the default password only once".to_owned());
                }
                default = Some(value);
            } else {
                let id =
                    operations::parse_commit_id(commit_id).map_err(|error| error.to_string())?;
                entries_by_commit.push((id, value));
            }
        }
    }
    let mut result = operations::PasswordSet::new(default);
    for (id, value) in entries_by_commit {
        result.insert_for_commit(id, value);
    }
    Ok(result)
}

fn content_aead(request: &Value) -> Result<ContentAead, String> {
    match request
        .get("aead")
        .and_then(Value::as_str)
        .unwrap_or("aes-256-gcm")
    {
        "aes-256-gcm" => Ok(ContentAead::Aes256Gcm),
        "chacha20-poly1305" => Ok(ContentAead::ChaCha20Poly1305),
        _ => Err("'aead' must be 'aes-256-gcm' or 'chacha20-poly1305'".to_owned()),
    }
}

fn encryption_scope(request: &Value) -> Result<EncryptionScope, String> {
    match request
        .get("encryption_scope")
        .and_then(Value::as_str)
        .unwrap_or("header-and-data")
    {
        "data" => Ok(EncryptionScope::Data),
        "header" => Ok(EncryptionScope::Header),
        "header-and-data" => Ok(EncryptionScope::HeaderAndData),
        _ => Err("'encryption_scope' must be 'data', 'header', or 'header-and-data'".to_owned()),
    }
}

fn create(request: &Value) -> Result<Value, String> {
    let archive = required_string(request, "archive")?;
    let input_paths = paths(request)?;
    let options = encoding_options(request)?;
    let filters = filters(request)?;
    let aead = content_aead(request)?;
    let scope = encryption_scope(request)?;
    let report = operations::create_archive_with_password_progress_and_filters_and_encryption(
        archive,
        &input_paths,
        bool_option(request, "strict", false)?,
        options,
        password(request)?
            .as_deref()
            .map(|password| (password, aead, scope)),
        None,
        CreateFilterOptions {
            input: CreateInputOptions {
                save_metadata: bool_option(request, "save_metadata", false)?,
                include_special_files: bool_option(request, "include_special_files", false)?,
                overwrite_existing: bool_option(request, "overwrite", false)?,
                verbose: bool_option(request, "verbose", false)?,
            },
            filters: &filters,
        },
    )
    .map_err(|error| error.to_string())?;
    Ok(json!({
        "block_size": report.block_size,
        "warnings": report.warnings,
        "aead": match aead {
            ContentAead::Aes256Gcm => "aes-256-gcm",
            ContentAead::ChaCha20Poly1305 => "chacha20-poly1305",
        },
    }))
}

fn add(request: &Value) -> Result<Value, String> {
    let archive = required_string(request, "archive")?;
    let input_paths = paths(request)?;
    let options = encoding_options(request)?;
    let filters = filters(request)?;
    let warnings = WarningCollector::new();
    let password = password(request)?;
    let aead = content_aead(request)?;
    let scope = encryption_scope(request)?;
    let passwords = passwords(request)?;
    operations::add_archive_with_encoding_progress_and_filters_and_encryption(
        archive,
        &input_paths,
        options,
        Some(&warnings),
        AddInputOptions {
            save_metadata: bool_option(request, "save_metadata", false)?,
            include_special_files: bool_option(request, "include_special_files", false)?,
            strict: bool_option(request, "strict", false)?,
            verbose: bool_option(request, "verbose", false)?,
        },
        &filters,
        password.as_deref().map(|password| (password, aead, scope)),
        &passwords,
    )
    .map_err(|error| error.to_string())?;
    Ok(json!({ "warnings": warnings.take() }))
}

fn remove(request: &Value) -> Result<Value, String> {
    operations::remove_archive(required_string(request, "archive")?, &paths(request)?)
        .map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

fn join(request: &Value) -> Result<Value, String> {
    let warnings = WarningCollector::new();
    operations::join_archive_with_encoding_progress(
        required_string(request, "archive")?,
        required_string(request, "from_id")?,
        required_string(request, "through_id")?,
        encoding_options(request)?,
        Some(&warnings),
    )
    .map_err(|error| error.to_string())?;
    Ok(json!({ "warnings": warnings.take() }))
}

fn check(request: &Value) -> Result<Value, String> {
    let passwords = passwords(request)?;
    let report = operations::check_archive_with_recovery_and_passwords(
        required_string(request, "archive")?,
        bool_option(request, "recover", false)?,
        &passwords,
    )
    .map_err(|error| error.to_string())?;
    Ok(json!({
        "commits": report.commits,
        "checked_blocks": report.checked_blocks,
        "encrypted_blocks": report.encrypted_blocks,
        "recoveries": report.recoveries,
        "logical_state_readable": report.logical_state_readable,
        "recovery_messages": report.recovery_messages,
    }))
}

fn info(request: &Value) -> Result<Value, String> {
    let commits = operations::info_archive(
        required_string(request, "archive")?,
        bool_option(request, "verbose", false)?,
    )
    .map_err(|error| error.to_string())?;
    let result = commits
        .into_iter()
        .map(|commit| {
            let details = commit.details.map(|details| {
                let groups = details
                    .groups
                    .into_iter()
                    .map(|(name, group)| {
                        (
                            name.to_owned(),
                            json!({
                                "files": group.files,
                                "compressed_size": group.compressed_size,
                                "uncompressed_size": group.uncompressed_size,
                                "has_encrypted_blocks": group.has_encrypted_blocks,
                            }),
                        )
                    })
                    .collect::<Map<_, _>>();
                json!({
                    "groups": groups,
                    "deduplicated_files": details.deduplicated_files,
                    "deduplicated_uncompressed_size": details.deduplicated_uncompressed_size,
                    "symlinks": details.symlinks,
                    "named_pipes": details.named_pipes,
                    "devices": details.devices,
                })
            });
            let header_compression = commit.header_compression.map(|header| {
                json!({
                    "compressed_size": header.compressed_size,
                    "uncompressed_size": header.uncompressed_size,
                    "ratio_percent": header.ratio_percent,
                })
            });
            let id = format!("{:08x}", u32::from_le_bytes(commit.id));
            json!({
                "id": id,
                "size": commit.size,
                "header_size": commit.header_size,
                "data_size": commit.data_size,
                "uncompressed_size": commit.uncompressed_size,
                "ratio_percent": commit.ratio_percent,
                "header_encrypted": commit.header_encrypted,
                "data_encrypted": commit.data_encrypted,
                "encryption_algorithm": commit.encryption_algorithm.map(|algorithm| match algorithm {
                    ContentAead::Aes256Gcm => "aes-256-gcm",
                    ContentAead::ChaCha20Poly1305 => "chacha20-poly1305",
                }),
                "header_compression": header_compression,
                "details": details,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!(result))
}

fn list(request: &Value) -> Result<Value, String> {
    let passwords = passwords(request)?;
    let toc = operations::list_archive_with_passwords(
        required_string(request, "archive")?,
        optional_string(request, "through")?,
        &passwords,
    )
    .map_err(|error| error.to_string())?;
    let entries = toc
        .iter()
        .map(|(path, entry)| match entry {
            zarlib::TocEntry::File(file) => json!({
                "path": path.as_str(),
                "type": "file",
                "size": file.size,
                "block_indices": file.block_indices,
                "checksum": file.checksum,
            }),
            zarlib::TocEntry::Directory => json!({
                "path": path.as_str(),
                "type": "directory",
            }),
            zarlib::TocEntry::Symlink { target } => json!({
                "path": path.as_str(),
                "type": "symlink",
                "target": target,
            }),
            zarlib::TocEntry::NamedPipe => json!({
                "path": path.as_str(),
                "type": "named_pipe",
            }),
            zarlib::TocEntry::Device { kind, major, minor } => json!({
                "path": path.as_str(),
                "type": "device",
                "device_kind": match kind {
                    zarlib::DeviceKind::Character => "character",
                    zarlib::DeviceKind::Block => "block",
                },
                "major": major,
                "minor": minor,
            }),
        })
        .collect::<Vec<_>>();
    Ok(json!(entries))
}

fn collision_policy(request: &Value) -> Result<CollisionPolicy, String> {
    match request
        .get("collision")
        .and_then(Value::as_str)
        .unwrap_or("rename")
    {
        "error" => Ok(CollisionPolicy::Error),
        "latest" => Ok(CollisionPolicy::Latest),
        "first" => Ok(CollisionPolicy::First),
        "rename" => Ok(CollisionPolicy::Rename),
        "ask" => Ok(CollisionPolicy::Ask),
        _ => Err("'collision' must be error, latest, first, rename, or ask".to_owned()),
    }
}

fn existing_policy(request: &Value) -> Result<ExistingPolicy, String> {
    match request
        .get("existing")
        .and_then(Value::as_str)
        .unwrap_or("error")
    {
        "overwrite" => Ok(ExistingPolicy::Overwrite),
        "skip" => Ok(ExistingPolicy::Skip),
        "error" => Ok(ExistingPolicy::Error),
        "ask" => Ok(ExistingPolicy::Ask),
        _ => Err("'existing' must be overwrite, skip, error, or ask".to_owned()),
    }
}

fn extract(request: &Value) -> Result<Value, String> {
    let archive = required_string(request, "archive")?;
    let destination = PathBuf::from(required_string(request, "destination")?);
    let filters = filters(request)?;
    let passwords = passwords(request)?;
    let dry_run = bool_option(request, "dry_run", false)?;
    let plan = operations::extract_archive_with_passwords_progress_and_filters(
        archive,
        &destination,
        ExtractionOptions {
            dry_run,
            through: optional_string(request, "through")?,
            collision_policy: collision_policy(request)?,
            existing_policy: existing_policy(request)?,
            restore_metadata: bool_option(request, "restore_metadata", false)?,
            restore_symlinks: bool_option(request, "restore_symlinks", false)?,
            restore_devices: bool_option(request, "restore_devices", false)?,
        },
        &passwords,
        None,
        &filters,
    )
    .map_err(|error| error.to_string())?;
    let files = plan
        .files
        .into_iter()
        .map(|(path, entry, kind)| {
            let mut item = Map::new();
            item.insert("path".to_owned(), json!(path.to_string_lossy()));
            item.insert("size".to_owned(), json!(entry.size));
            item.insert("block_indices".to_owned(), json!(entry.block_indices));
            match kind {
                operations::ExtractedKind::File => {
                    item.insert("type".to_owned(), json!("file"));
                }
                operations::ExtractedKind::Symlink(target) => {
                    item.insert("type".to_owned(), json!("symlink"));
                    item.insert("target".to_owned(), json!(target));
                }
                operations::ExtractedKind::NamedPipe => {
                    item.insert("type".to_owned(), json!("named_pipe"));
                }
                operations::ExtractedKind::Device { kind, major, minor } => {
                    item.insert("type".to_owned(), json!("device"));
                    item.insert(
                        "device_kind".to_owned(),
                        json!(match kind {
                            zarlib::DeviceKind::Character => "character",
                            zarlib::DeviceKind::Block => "block",
                        }),
                    );
                    item.insert("major".to_owned(), json!(major));
                    item.insert("minor".to_owned(), json!(minor));
                }
            }
            Value::Object(item)
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "directories": plan.directories.iter().map(|path| path.to_string_lossy().to_string()).collect::<Vec<_>>(),
        "files": files,
        "dry_run": dry_run,
    }))
}

fn extract_to_memory(request: &Value) -> Result<Value, String> {
    let filters = filters(request)?;
    let passwords = passwords(request)?;
    let files =
        operations::extract_archive_to_memory_with_passwords_and_filters(
            required_string(request, "archive")?,
            optional_string(request, "through")?,
            &passwords,
            match request.get("size_limit") {
                None | Some(Value::Null) => None,
                Some(value) => Some(value.as_u64().ok_or_else(|| {
                    "'size_limit' must be a non-negative integer or null".to_owned()
                })?),
            },
            integer_option(request, "offset", 0_u64)?,
            &filters,
        )
        .map_err(|error| error.to_string())?;
    let contents = files
        .into_iter()
        .map(|(path, bytes)| (path, json!(bytes)))
        .collect::<Map<_, _>>();
    Ok(json!(contents))
}

fn execute(operation: &str, request: &Value) -> Result<Value, String> {
    match operation {
        "create_archive" => create(request),
        "add_archive" => add(request),
        "remove_archive" => remove(request),
        "join_archive" => join(request),
        "check_archive" => check(request),
        "info_archive" => info(request),
        "list_archive" => list(request),
        "extract_archive" => extract(request),
        "extract_archive_to_memory" => extract_to_memory(request),
        _ => Err(format!("unknown operation: {operation}")),
    }
}

fn write_string(output: *mut *mut c_char, value: String) -> Result<(), ()> {
    let value = CString::new(value).map_err(|_| ())?;
    unsafe {
        *output = value.into_raw();
    }
    Ok(())
}

/// Execute a pyzarlib-compatible operation using JSON request and response data.
///
/// The function returns 0 on success and writes an allocated JSON response to
/// `response_json`. On failure it returns 1 and writes an allocated message to
/// `error_message`. Release returned strings with [`czarlib_string_free`].
///
/// # Safety
/// `operation`, `request_json`, `response_json`, and `error_message` must point
/// to valid C strings / writable pointer slots. The output pointers must not
/// alias each other.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn czarlib_execute_json(
    operation: *const c_char,
    request_json: *const c_char,
    response_json: *mut *mut c_char,
    error_message: *mut *mut c_char,
) -> i32 {
    if operation.is_null()
        || request_json.is_null()
        || response_json.is_null()
        || error_message.is_null()
        || response_json == error_message
    {
        return 2;
    }
    unsafe {
        *response_json = std::ptr::null_mut();
        *error_message = std::ptr::null_mut();
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let operation = unsafe { CStr::from_ptr(operation) }
            .to_str()
            .map_err(|_| "operation name is not valid UTF-8".to_owned())?;
        let request_json = unsafe { CStr::from_ptr(request_json) }
            .to_str()
            .map_err(|_| "request JSON is not valid UTF-8".to_owned())?;
        let request: Value = serde_json::from_str(request_json)
            .map_err(|error| format!("invalid request JSON: {error}"))?;
        if !request.is_object() {
            return Err("request JSON must be an object".to_owned());
        }
        execute(operation, &request).and_then(|response| {
            serde_json::to_string(&response).map_err(|error| error.to_string())
        })
    }));
    match result {
        Ok(Ok(response)) => {
            if write_string(response_json, response).is_ok() {
                0
            } else {
                2
            }
        }
        Ok(Err(error)) => {
            if write_string(error_message, error).is_ok() {
                1
            } else {
                2
            }
        }
        Err(_) => {
            let _ = write_string(error_message, "internal Rust panic".to_owned());
            3
        }
    }
}

/// Release a string returned by [`czarlib_execute_json`].
///
/// # Safety
/// `value` must be null or a string previously returned by
/// [`czarlib_execute_json`] and not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn czarlib_string_free(value: *mut c_char) {
    if !value.is_null() {
        unsafe {
            drop(CString::from_raw(value));
        }
    }
}
