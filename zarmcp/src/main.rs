use base64::{Engine, engine::general_purpose::STANDARD};
use clap::Parser;
use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ContentBlock},
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use zarlib::{ArchivePath, ContentAead, EncryptionScope, operations::PasswordSet};
use zeroize::Zeroizing;

const MAX_READ_BYTES: u64 = 1024 * 1024;
const READ_ONLY_TOOLS: &[&str] = &["info_archive", "list_archive", "check_archive", "read_file"];

#[derive(Parser)]
#[command(about = "Serve ZAR archive tools over MCP stdio")]
struct Options {
    /// Only expose archive inspection and in-memory reading tools.
    #[arg(long)]
    read_only: bool,
    /// Only expose the named tools (comma-separated; may be repeated).
    #[arg(long, value_delimiter = ',')]
    allow_tools: Option<Vec<String>>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ArchiveRequest {
    /// Path to the ZAR archive.
    archive: String,
    /// Default password for encrypted commits.
    password: Option<String>,
    /// Passwords keyed by eight-digit commit ID, overriding the default.
    passwords: Option<BTreeMap<String, String>>,
    /// Select the archive state through this commit ID; omit for latest.
    to: Option<String>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct InfoRequest {
    /// Path to the ZAR archive.
    archive: String,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CheckRequest {
    /// Path to the ZAR archive.
    archive: String,
    /// Default password for encrypted commits.
    password: Option<String>,
    /// Passwords keyed by eight-digit commit ID, overriding the default.
    passwords: Option<BTreeMap<String, String>>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CreateRequest {
    /// Path for the new archive; an existing file is not overwritten.
    archive: String,
    /// Files and directories to archive. Supported archive inputs are imported.
    paths: Vec<String>,
    /// Encrypt the new commit with this password.
    password: Option<String>,
    /// Content cipher: aes-256-gcm or chacha20-poly1305.
    aead: Option<String>,
    /// Encryption scope: data, header, or header-and-data.
    encryption_scope: Option<String>,
    /// Reject unsupported inputs instead of skipping them.
    strict: Option<bool>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AddRequest {
    /// Existing archive to update.
    archive: String,
    /// Files and directories to add or replace.
    paths: Vec<String>,
    /// Password for the new commit and default for existing encrypted commits.
    password: Option<String>,
    /// Existing commit passwords keyed by eight-digit commit ID.
    passwords: Option<BTreeMap<String, String>>,
    /// Content cipher: aes-256-gcm or chacha20-poly1305.
    aead: Option<String>,
    /// Encryption scope: data, header, or header-and-data.
    encryption_scope: Option<String>,
    /// Reject unsupported inputs instead of skipping them.
    strict: Option<bool>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RemoveRequest {
    /// Existing archive to update.
    archive: String,
    /// Archive-relative paths to remove; earlier history is retained.
    paths: Vec<String>,
    /// Default password for existing encrypted commits.
    password: Option<String>,
    /// Existing commit passwords keyed by eight-digit commit ID.
    passwords: Option<BTreeMap<String, String>>,
    /// Password for the new removal commit; defaults to password.
    removal_password: Option<String>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ExtractRequest {
    /// Archive to extract.
    archive: String,
    /// Destination directory. Existing files are not overwritten.
    destination: String,
    /// Default password for encrypted commits.
    password: Option<String>,
    /// Passwords keyed by eight-digit commit ID.
    passwords: Option<BTreeMap<String, String>>,
    /// Select the archive state through this commit ID; omit for latest.
    to: Option<String>,
    /// Preview the extraction without writing files.
    dry_run: Option<bool>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReadRequest {
    /// Archive to read.
    archive: String,
    /// Exact archive-relative path of one regular file.
    path: String,
    /// Default password for encrypted commits.
    password: Option<String>,
    /// Passwords keyed by eight-digit commit ID.
    passwords: Option<BTreeMap<String, String>>,
    /// Select the archive state through this commit ID; omit for latest.
    to: Option<String>,
    /// Byte offset within the file; defaults to zero.
    offset: Option<u64>,
    /// Maximum bytes to return, from 1 through 1048576; defaults to 1048576.
    size_limit: Option<u64>,
}

#[derive(Clone)]
struct ZarServer {
    operation_lock: Arc<Mutex<()>>,
    tool_router: ToolRouter<Self>,
}

fn request_passwords(request: &Value) -> Result<PasswordSet, String> {
    let default = request.get("password").and_then(Value::as_str);
    let mut passwords = PasswordSet::new(default.map(|value| value.as_bytes().to_vec()));
    if let Some(entries) = request.get("passwords").and_then(Value::as_object) {
        for (id, password) in entries {
            let id = zarlib::operations::parse_commit_id(id).map_err(|error| error.to_string())?;
            let password = password.as_str().ok_or("password values must be strings")?;
            passwords.insert_for_commit(id, password.as_bytes().to_vec());
        }
    }
    Ok(passwords)
}

fn verify_credentials(request: &Value) -> Result<bool, String> {
    let archive = request["archive"].as_str().ok_or("archive is required")?;
    let credentials = request_passwords(request)?;
    let commits = zarlib::operations::info_archive(archive, false).map_err(|e| e.to_string())?;
    let mut encrypted = false;
    for commit in commits {
        if commit.header_encrypted || commit.data_encrypted {
            encrypted = true;
            if credentials.password_for(&commit.id).is_none() {
                return Err(format!(
                    "password required for encrypted commit {:08x}",
                    u32::from_le_bytes(commit.id)
                ));
            }
        }
    }
    Ok(encrypted)
}

fn execute_json(operation: &str, request: &Value) -> Result<Value, String> {
    let serialized = Zeroizing::new(serde_json::to_string(request).map_err(|e| e.to_string())?);
    let response = czarlib::execute_json(operation, &serialized)?;
    serde_json::from_str(&response).map_err(|e| format!("invalid archive API response: {e}"))
}

fn execute(operation: &str, request: Value) -> Result<Value, String> {
    match operation {
        "create_archive" => {
            if request["paths"].as_array().is_none_or(Vec::is_empty) {
                return Err("paths must contain at least one input".into());
            }
            execute_json(operation, &request)
        }
        "info_archive" => execute_json(operation, &request),
        "add_archive" => {
            if request["paths"].as_array().is_none_or(Vec::is_empty) {
                return Err("paths must contain at least one input".into());
            }
            if verify_credentials(&request)? && request["password"].as_str().is_none() {
                return Err(
                    "password is required to encrypt the new commit of an encrypted archive".into(),
                );
            }
            execute_json(operation, &request)
        }
        "remove_archive" => {
            let encrypted = verify_credentials(&request)?;
            let removal_password = request["removal_password"]
                .as_str()
                .or_else(|| request["password"].as_str());
            if encrypted && removal_password.is_none() {
                return Err("removal_password is required for an encrypted archive".into());
            }
            let paths = request["paths"].as_array().ok_or("paths are required")?;
            if paths.is_empty() {
                return Err("paths must contain at least one archive path".into());
            }
            let paths = paths
                .iter()
                .map(|path| {
                    ArchivePath::new(path.as_str().ok_or("invalid path")?)
                        .map_err(|e| e.to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            let passwords = request_passwords(&request)?;
            let encryption = removal_password.map(|password| {
                (
                    password.as_bytes(),
                    ContentAead::Aes256Gcm,
                    EncryptionScope::HeaderAndData,
                )
            });
            zarlib::operations::remove_archive_entries_with_progress_and_encryption(
                request["archive"].as_str().ok_or("archive is required")?,
                &paths,
                None,
                encryption,
                &passwords,
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({"requested_paths": paths.len()}))
        }
        "list_archive" | "check_archive" | "extract_archive" => {
            verify_credentials(&request)?;
            execute_json(operation, &request)
        }
        "read_file" => {
            verify_credentials(&request)?;
            let path = request["path"].as_str().ok_or("path is required")?;
            ArchivePath::new(path).map_err(|e| e.to_string())?;
            let limit = request["size_limit"].as_u64().unwrap_or(MAX_READ_BYTES);
            if !(1..=MAX_READ_BYTES).contains(&limit) {
                return Err("size_limit must be between 1 and 1048576".into());
            }
            let offset = request["offset"].as_u64().unwrap_or(0);
            let mut query = request.clone();
            let query = query.as_object_mut().ok_or("invalid request")?;
            query.insert("include_files".into(), json!([globset::escape(path)]));
            query.insert("size_limit".into(), json!(limit));
            query.insert("offset".into(), json!(offset));
            let files = execute_json("extract_archive_to_memory", &Value::Object(query.clone()))?;
            let bytes = files[path]
                .as_array()
                .ok_or_else(|| format!("regular file not found: {path}"))?
                .iter()
                .map(|byte| {
                    byte.as_u64()
                        .and_then(|byte| u8::try_from(byte).ok())
                        .ok_or("invalid file data returned by archive API")
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(json!({
                "path": path,
                "offset": offset,
                "bytes_returned": bytes.len(),
                "encoding": "base64",
                "data": STANDARD.encode(bytes),
            }))
        }
        _ => Err("unknown archive operation".into()),
    }
}

impl ZarServer {
    fn new(options: Options) -> Result<Self, String> {
        let mut tool_router = Self::tool_router();
        let tools = tool_router.list_all();
        if let Some(allowed) = &options.allow_tools {
            for name in allowed {
                if !tools.iter().any(|tool| tool.name == name.as_str()) {
                    return Err(format!("unknown tool in --allow-tools: {name}"));
                }
                if options.read_only && !READ_ONLY_TOOLS.contains(&name.as_str()) {
                    return Err(format!("tool {name} is not permitted with --read-only"));
                }
            }
        }
        for tool in tools {
            let name = tool.name.as_ref();
            if (options.read_only && !READ_ONLY_TOOLS.contains(&name))
                || options
                    .allow_tools
                    .as_ref()
                    .is_some_and(|allowed| !allowed.iter().any(|allowed| allowed == name))
            {
                tool_router.disable_route(tool.name);
            }
        }
        Ok(Self {
            operation_lock: Arc::new(Mutex::new(())),
            tool_router,
        })
    }

    async fn run<T: Serialize + Send + 'static>(
        &self,
        operation: &'static str,
        request: T,
    ) -> CallToolResult {
        let operation_lock = Arc::clone(&self.operation_lock);
        match tokio::task::spawn_blocking(move || {
            let _guard = operation_lock
                .lock()
                .map_err(|_| "archive operation lock poisoned")?;
            let mut request = serde_json::to_value(request).map_err(|e| e.to_string())?;
            request
                .as_object_mut()
                .ok_or("invalid request")?
                .retain(|_, value| !value.is_null());
            execute(operation, request)
        })
        .await
        {
            Ok(Ok(value)) => CallToolResult::success(vec![ContentBlock::text(value.to_string())]),
            Ok(Err(error)) => CallToolResult::error(vec![ContentBlock::text(error)]),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(format!(
                "archive operation failed: {error}"
            ))]),
        }
    }
}

#[tool_router]
impl ZarServer {
    #[tool(
        description = "Create a new ZAR archive from local inputs; does not overwrite existing archives.",
        annotations(destructive_hint = false)
    )]
    async fn create_archive(
        &self,
        Parameters(request): Parameters<CreateRequest>,
    ) -> CallToolResult {
        self.run("create_archive", request).await
    }

    #[tool(
        description = "Append files or directories as a new commit to a ZAR archive.",
        annotations(destructive_hint = true)
    )]
    async fn add_archive(&self, Parameters(request): Parameters<AddRequest>) -> CallToolResult {
        self.run("add_archive", request).await
    }

    #[tool(
        description = "Remove archive-relative paths by appending a commit; previous history remains.",
        annotations(destructive_hint = true)
    )]
    async fn remove_archive(
        &self,
        Parameters(request): Parameters<RemoveRequest>,
    ) -> CallToolResult {
        self.run("remove_archive", request).await
    }

    #[tool(
        description = "Inspect archive commit IDs and encryption information.",
        annotations(read_only_hint = true)
    )]
    async fn info_archive(&self, Parameters(request): Parameters<InfoRequest>) -> CallToolResult {
        self.run("info_archive", request).await
    }

    #[tool(
        description = "List archive entries at the latest or a selected historical commit.",
        annotations(read_only_hint = true)
    )]
    async fn list_archive(
        &self,
        Parameters(request): Parameters<ArchiveRequest>,
    ) -> CallToolResult {
        self.run("list_archive", request).await
    }

    #[tool(
        description = "Verify archive framing, checksums and encrypted content.",
        annotations(read_only_hint = true)
    )]
    async fn check_archive(&self, Parameters(request): Parameters<CheckRequest>) -> CallToolResult {
        self.run("check_archive", request).await
    }

    #[tool(
        description = "Extract an archive state to a local directory, without overwriting existing files.",
        annotations(destructive_hint = true)
    )]
    async fn extract_archive(
        &self,
        Parameters(request): Parameters<ExtractRequest>,
    ) -> CallToolResult {
        self.run("extract_archive", request).await
    }

    #[tool(
        description = "Read up to 1 MiB from one archive file, returning base64 encoded bytes.",
        annotations(read_only_hint = true)
    )]
    async fn read_file(&self, Parameters(request): Parameters<ReadRequest>) -> CallToolResult {
        self.run("read_file", request).await
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ZarServer {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = Options::parse();
    let server = ZarServer::new(options)?;
    server
        .serve(rmcp::transport::stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}
