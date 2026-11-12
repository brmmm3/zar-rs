use std::io::{self, IsTerminal, Seek, Write};
use std::process::ExitCode;
mod args;
use args::{Cli, Command};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(test)]
use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
};
use zarlib::operations::*;
#[cfg(test)]
use zarlib::*;
use zarlib::{ArchivePath, CommitValidationLimits, ContentAead, EncryptionScope, TocChange};

#[cfg(unix)]
struct TerminalEchoGuard {
    fd: libc::c_int,
    original: libc::termios,
}

#[cfg(unix)]
impl Drop for TerminalEchoGuard {
    fn drop(&mut self) {
        // Best-effort restoration also runs when reading the password fails.
        unsafe {
            libc::tcsetattr(self.fd, libc::TCSAFLUSH, &self.original);
        }
    }
}

#[cfg(unix)]
fn prompt_password(prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
    if !io::stdin().is_terminal() {
        return Err("password entry requires an interactive terminal".into());
    }
    let fd = io::stdin().as_raw_fd();
    let mut original = std::mem::MaybeUninit::<libc::termios>::uninit();
    if unsafe { libc::tcgetattr(fd, original.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error().into());
    }
    let original = unsafe { original.assume_init() };
    let mut hidden = original;
    hidden.c_lflag &= !libc::ECHO;
    if unsafe { libc::tcsetattr(fd, libc::TCSAFLUSH, &hidden) } != 0 {
        return Err(io::Error::last_os_error().into());
    }
    let _echo_guard = TerminalEchoGuard { fd, original };
    eprint!("{prompt}");
    io::stderr().flush()?;
    let mut password = String::new();
    io::stdin().read_line(&mut password)?;
    eprintln!();
    while password.ends_with(['\n', '\r']) {
        password.pop();
    }
    if password.is_empty() {
        return Err("password must not be empty".into());
    }
    Ok(password)
}

#[cfg(not(unix))]
fn prompt_password(_prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
    Err("password entry without terminal echo is not supported on this platform".into())
}

fn prompt_password_set(
    default_prompt: bool,
    commit_ids: &[String],
) -> Result<PasswordSet, Box<dyn std::error::Error>> {
    let default = default_prompt
        .then(|| prompt_password("Default archive password: "))
        .transpose()?
        .map(String::into_bytes);
    let mut passwords = PasswordSet::new(default);
    let mut seen = std::collections::BTreeSet::new();
    for value in commit_ids {
        let id = parse_commit_id(value)?;
        if !seen.insert(id) {
            return Err(format!("password for commit {value} was specified more than once").into());
        }
        let password = prompt_password(&format!("Password for commit {value}: "))?;
        passwords.insert_for_commit(id, password.into_bytes());
    }
    Ok(passwords)
}

struct ReplacedFile {
    path: ArchivePath,
    previous_commit: [u8; 4],
    replacing_commit: [u8; 4],
}

#[derive(Default)]
struct ListDiagnostics {
    replaced_files: Vec<ReplacedFile>,
    skipped_commits: Vec<[u8; 4]>,
}

fn list_diagnostics(
    archive: &str,
    to: Option<&str>,
    passwords: &PasswordSet,
) -> Result<ListDiagnostics, Box<dyn std::error::Error>> {
    let requested_id = to.map(parse_commit_id).transpose()?;
    let mut file = std::fs::File::open(archive)?;
    let mut header_offset = zarlib::ContainerHeader::read_from(&mut file)?.first_header_offset;
    let limits = CommitValidationLimits::default();
    let mut matches = 0_u32;
    let mut stopped_at_to = false;
    let mut active_files = std::collections::BTreeMap::<ArchivePath, [u8; 4]>::new();
    let mut diagnostics = ListDiagnostics::default();

    while header_offset != 0 {
        let (commit_header, _, next_header_offset) =
            read_commit_at_offset(&mut file, header_offset, limits)?;
        if requested_id == Some(commit_header.commit_id) {
            matches += 1;
            if matches > 1 {
                return Err("--to commit ID is ambiguous".into());
            }
        }
        if !stopped_at_to {
            file.seek(std::io::SeekFrom::Start(header_offset))?;
            let (validated, _) = read_header_with_passwords(&mut file, limits, passwords)?;
            if let Some(header) = validated.logical_header {
                for change in header.changes {
                    match change {
                        TocChange::PutFile { path, .. } => {
                            if let Some(previous_commit) =
                                active_files.insert(path.clone(), validated.commit_header.commit_id)
                                && previous_commit != validated.commit_header.commit_id
                            {
                                diagnostics.replaced_files.push(ReplacedFile {
                                    path,
                                    previous_commit,
                                    replacing_commit: validated.commit_header.commit_id,
                                });
                            }
                        }
                        TocChange::PutDirectory { path }
                        | TocChange::PutSymlink { path, .. }
                        | TocChange::PutNamedPipe { path }
                        | TocChange::PutDevice { path, .. }
                        | TocChange::RemoveFile { path } => {
                            active_files.remove(&path);
                        }
                        TocChange::RemoveDirectory { path } => {
                            active_files.retain(|existing, _| {
                                existing != &path
                                    && !existing
                                        .as_str()
                                        .strip_prefix(path.as_str())
                                        .is_some_and(|suffix| suffix.starts_with('/'))
                            });
                        }
                    }
                }
            } else if validated.encryption.is_header_encrypted() {
                diagnostics
                    .skipped_commits
                    .push(validated.commit_header.commit_id);
            } else {
                return Err("commit header is missing its logical header".into());
            }
        }
        if requested_id == Some(commit_header.commit_id) {
            stopped_at_to = true;
        }
        header_offset = next_header_offset;
    }

    if requested_id.is_some() && matches == 0 {
        return Err("--to commit ID was not found".into());
    }
    Ok(diagnostics)
}

fn render_list_diagnostics(diagnostics: &ListDiagnostics) -> String {
    let mut output = String::new();
    if !diagnostics.replaced_files.is_empty() {
        output.push_str("Files replaced by newer commits:\n");
        for replaced in &diagnostics.replaced_files {
            output.push_str(&format!(
                "  {} (commit {} replaced by commit {})",
                replaced.path,
                format_commit_id(&replaced.previous_commit),
                format_commit_id(&replaced.replacing_commit)
            ));
            output.push('\n');
        }
    }
    if !diagnostics.skipped_commits.is_empty() {
        output.push_str("Ignored commits (password not provided):\n");
        for commit_id in &diagnostics.skipped_commits {
            output.push_str(&format!("  {}\n", format_commit_id(commit_id)));
        }
    }
    output
}

fn parse_content_aead(value: &str) -> Result<ContentAead, Box<dyn std::error::Error>> {
    match value {
        "aes-256-gcm" => Ok(ContentAead::Aes256Gcm),
        "chacha20-poly1305" => Ok(ContentAead::ChaCha20Poly1305),
        _ => Err(format!("unsupported AEAD algorithm: {value}").into()),
    }
}

fn parse_encryption_scope(value: &str) -> Result<EncryptionScope, Box<dyn std::error::Error>> {
    match value {
        "data" => Ok(EncryptionScope::Data),
        "header" => Ok(EncryptionScope::Header),
        "header-and-data" => Ok(EncryptionScope::HeaderAndData),
        _ => Err(format!("unsupported encryption scope: {value}").into()),
    }
}

fn main() -> ExitCode {
    match args::parse_from_env() {
        Ok(cli) => match execute(cli) {
            Ok(code) => ExitCode::from(code),
            Err(error) => {
                eprintln!("zarc: {error}");
                ExitCode::from(classify_exit_code(error.as_ref()))
            }
        },
        Err(error) => {
            let code = error.exit_code() as u8;
            let is_help = matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            );
            let _ = error.print();
            if is_help {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(code)
            }
        }
    }
}

fn error_chain_contains<T: std::error::Error + 'static>(
    error: &(dyn std::error::Error + 'static),
) -> bool {
    let mut current = Some(error);
    while let Some(error) = current {
        if error.downcast_ref::<T>().is_some() {
            return true;
        }
        current = error.source();
    }
    false
}

fn classify_exit_code(error: &(dyn std::error::Error + 'static)) -> u8 {
    if error
        .downcast_ref::<zarlib::operations::StrictModeWarning>()
        .is_some()
    {
        return 6;
    }
    if error_chain_contains::<io::Error>(error) {
        return 3;
    }
    if error_chain_contains::<zarlib::FramingError>(error) {
        return 4;
    }
    if error.downcast_ref::<clap::Error>().is_some() {
        return 2;
    }
    let message = error.to_string();
    if message.starts_with("usage:")
        || message.starts_with("unsupported ")
        || message.starts_with("invalid ")
        || message.starts_with("duplicate option")
        || message.contains(" requires ")
    {
        2
    } else {
        4
    }
}

#[cfg(test)]
fn run(args: impl Iterator<Item = String>) -> Result<u8, Box<dyn std::error::Error>> {
    use clap::Parser;
    let cli = Cli::try_parse_from(std::iter::once("zarc".to_owned()).chain(args))?;
    execute(cli)
}

struct CliProgress {
    enabled: bool,
    bar: std::sync::Mutex<Option<ProgressBar>>,
    total_bytes: std::sync::atomic::AtomicU64,
}

impl CliProgress {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            bar: std::sync::Mutex::new(enabled.then(|| Self::make_bar(false))),
            total_bytes: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn make_bar(labelled: bool) -> ProgressBar {
        let progress = ProgressBar::with_draw_target(Some(0), ProgressDrawTarget::hidden());
        progress.set_style(
            ProgressStyle::with_template(
                if labelled {
                    "{prefix:<22} [{elapsed_precise}] [{bar:40.cyan}] {binary_bytes}/{binary_total_bytes} ({binary_bytes_per_sec}, {eta})"
                } else {
                    "[{elapsed_precise}] [{bar:40.cyan}] {binary_bytes}/{binary_total_bytes} ({binary_bytes_per_sec}, {eta})"
                },
            )
            .expect("static progress template is valid")
            .progress_chars("##>-"),
        );
        progress
    }

    fn bar(&self) -> Option<ProgressBar> {
        self.bar.lock().ok().and_then(|bar| bar.clone())
    }

    /// Finishes the current phase bar (keeping it visible) and starts a new one.
    fn next_phase(&self, message: &str, total: u64) -> Option<ProgressBar> {
        let mut guard = self.bar.lock().ok()?;
        let previous = guard.take()?;
        if previous.position() > 0 {
            previous.set_position(previous.length().unwrap_or(0));
            previous.finish();
        } else {
            previous.finish_and_clear();
        }
        let progress = Self::make_bar(true);
        progress.set_prefix(message.to_owned());
        progress.set_length(total);
        if message != "Planning extraction" {
            progress.set_draw_target(ProgressDrawTarget::stderr_with_hz(5));
            progress.enable_steady_tick(std::time::Duration::from_millis(200));
        }
        *guard = Some(progress.clone());
        Some(progress)
    }

    fn total_bytes(&self) -> u64 {
        self.total_bytes.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn print_summary(&self, action: &str, summary: &InputSummary) {
        if self.enabled {
            for line in format_summary_lines(action, summary) {
                eprintln!("{line}");
            }
        }
    }
}

fn parse_overwrite_decision(answer: &str) -> OverwriteDecision {
    match answer.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => OverwriteDecision::Yes,
        "a" | "always" => OverwriteDecision::Always,
        "n" | "no" => OverwriteDecision::No,
        "never" => OverwriteDecision::Never,
        _ => OverwriteDecision::No,
    }
}

fn format_summary_lines(action: &str, summary: &InputSummary) -> Vec<String> {
    let mut lines = Vec::new();
    if summary.files > 0 || summary.directories > 0 {
        lines.push(format!(
            "{action}: {} file(s) and {} dir(s) with {} total",
            summary.files,
            summary.directories,
            display_size(summary.total_bytes)
        ));
    }
    if summary.empty_files > 0
        || summary.tiny_files > 0
        || summary.small_files > 0
        || summary.big_files > 0
    {
        lines.push(format!(
            "File Groups: {} empty, {} tiny, {} small, {} big",
            summary.empty_files, summary.tiny_files, summary.small_files, summary.big_files,
        ));
    }
    if summary.symlinks > 0 || summary.named_pipes > 0 || summary.devices > 0 {
        lines.push(format!(
            "Special Files: {} symlinks, {} pipes, {} devices",
            summary.symlinks, summary.named_pipes, summary.devices
        ));
    }
    lines
}
impl Progress for CliProgress {
    fn message(&self, message: &str) {
        if let Some(progress) = self.bar() {
            progress.set_message(message.to_owned());
        }
    }
    fn add_summary(&self, summary: &InputSummary) {
        self.total_bytes
            .store(summary.total_bytes, std::sync::atomic::Ordering::Relaxed);
        self.print_summary("Add", summary);
    }
    fn extract_summary(&self, summary: &InputSummary) {
        self.print_summary("Extract", summary);
    }
    fn set_total(&self, total: u64) {
        self.total_bytes
            .store(total, std::sync::atomic::Ordering::Relaxed);
        if let Some(progress) = self.bar() {
            progress.set_length(total);
            if total > 0 {
                progress.set_draw_target(ProgressDrawTarget::stderr_with_hz(5));
                progress.enable_steady_tick(std::time::Duration::from_millis(200));
            }
        }
    }
    fn start_phase(&self, message: &str, total: u64) {
        self.total_bytes
            .store(total, std::sync::atomic::Ordering::Relaxed);
        self.next_phase(message, total);
    }
    fn reset_phase(&self, message: &str, total: u64) {
        self.next_phase(message, total);
    }
    fn advance(&self, amount: u64) {
        if amount == 0 {
            return;
        }
        if let Some(progress) = self.bar() {
            progress.inc(amount);
        }
    }
    fn warning(&self, message: &str) {
        eprintln!("{message}");
    }
    fn is_interactive(&self) -> bool {
        io::stdin().is_terminal()
    }
    fn confirm_overwrite(
        &self,
        prompt: &str,
    ) -> Result<OverwriteDecision, Box<dyn std::error::Error>> {
        eprint!("{prompt}");
        io::stderr().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        Ok(parse_overwrite_decision(&answer))
    }
    fn confirm(&self, prompt: &str) -> Result<bool, Box<dyn std::error::Error>> {
        eprint!("{prompt}");
        io::stderr().flush()?;
        let mut answer = String::new();
        io::stdin().read_line(&mut answer)?;
        Ok(matches!(
            answer.trim().to_ascii_lowercase().as_str(),
            "y" | "yes"
        ))
    }
}
impl Drop for CliProgress {
    fn drop(&mut self) {
        if let Some(progress) = self.bar() {
            progress.finish_and_clear();
        }
    }
}

fn format_commit_id(bytes: &[u8; 4]) -> String {
    format!("{:08x}", u32::from_le_bytes(*bytes))
}

struct InfoJsonCommit {
    id: String,
    size: u64,
    header_size: u32,
    data_size: u64,
    header_encrypted: bool,
    data_encrypted: bool,
    encryption_algorithm: Option<&'static str>,
    header_changes: Option<HeaderChangeStats>,
    created_unix_millis: Option<i64>,
    user: Option<String>,
}

fn render_info_json(commits: &[InfoJsonCommit]) -> String {
    let entries = commits
        .iter()
        .map(|commit| {
            format!(
                "{{\"id\":\"{}\",\"size\":{},\"header_size\":{},\"data_size\":{},\"header_encrypted\":{},\"data_encrypted\":{},\"encryption_algorithm\":{},\"header_changes\":{},\"created_at\":{},\"created_unix_millis\":{},\"user\":{}}}",
                commit.id,
                commit.size,
                commit.header_size,
                commit.data_size,
                commit.header_encrypted,
                commit.data_encrypted,
                commit.encryption_algorithm.map_or_else(
                    || "null".to_owned(),
                    |value| format!("\"{value}\"")
                ),
                render_header_changes_json(commit.header_changes.as_ref()),
                commit.created_unix_millis.map_or_else(
                    || "null".to_owned(),
                    |value| render_json_string(&format_unix_millis(value))
                ),
                commit.created_unix_millis.map_or_else(
                    || "null".to_owned(),
                    |value| value.to_string()
                ),
                commit.user.as_deref().map_or_else(
                    || "null".to_owned(),
                    render_json_string
                )
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"commit_count\":{},\"commits\":[{entries}]}}",
        commits.len()
    )
}

fn render_json_string(value: &str) -> String {
    let mut rendered = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => rendered.push_str("\\\""),
            '\\' => rendered.push_str("\\\\"),
            '\u{08}' => rendered.push_str("\\b"),
            '\u{0c}' => rendered.push_str("\\f"),
            '\n' => rendered.push_str("\\n"),
            '\r' => rendered.push_str("\\r"),
            '\t' => rendered.push_str("\\t"),
            character if character.is_control() => {
                rendered.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => rendered.push(character),
        }
    }
    rendered.push('"');
    rendered
}

fn render_header_changes_json(changes: Option<&HeaderChangeStats>) -> String {
    let Some(changes) = changes else {
        return "null".to_owned();
    };
    format!(
        "{{\"files_added_or_updated\":{},\"directories_added_or_updated\":{},\"symlinks_added_or_updated\":{},\"named_pipes_added_or_updated\":{},\"devices_added_or_updated\":{},\"files_removed\":{},\"directories_removed\":{}}}",
        changes.files_added_or_updated,
        changes.directories_added_or_updated,
        changes.symlinks_added_or_updated,
        changes.named_pipes_added_or_updated,
        changes.devices_added_or_updated,
        changes.files_removed,
        changes.directories_removed
    )
}

fn render_header_changes(changes: Option<&HeaderChangeStats>) -> String {
    let Some(changes) = changes else {
        return "  header content changes: unavailable (encrypted header)\n".to_owned();
    };
    let rows = [
        ("files added or updated", changes.files_added_or_updated),
        (
            "directories added or updated",
            changes.directories_added_or_updated,
        ),
        (
            "symlinks added or updated",
            changes.symlinks_added_or_updated,
        ),
        (
            "named pipes added or updated",
            changes.named_pipes_added_or_updated,
        ),
        ("devices added or updated", changes.devices_added_or_updated),
        ("file entries removed", changes.files_removed),
        ("directories removed", changes.directories_removed),
    ];
    let mut output = String::from("  header content changes:\n");
    let mut has_changes = false;
    for (label, count) in rows {
        if count == 0 {
            continue;
        }
        has_changes = true;
        output.push_str(&format!("    {label}: {count}\n"));
    }
    if !has_changes {
        output.push_str("    content changes: none\n");
    }
    output
}

fn format_commit_summary(commit: &CommitInfo) -> String {
    let scope = match (commit.header_encrypted, commit.data_encrypted) {
        (false, false) => "none",
        (false, true) => "data",
        (true, false) => "header",
        (true, true) => "header-and-data",
    };
    let algorithm = commit
        .encryption_algorithm
        .map_or("none", |algorithm| match algorithm {
            ContentAead::Aes256Gcm => "aes-256-gcm",
            ContentAead::ChaCha20Poly1305 => "chacha20-poly1305",
        });
    format!(
        "commit {} size={} header={} data={} uncompressed={} ratio={} encryption={} algorithm={}",
        format_commit_id(&commit.id),
        display_size(commit.size),
        display_size(u64::from(commit.header_size)),
        display_size(commit.data_size),
        commit
            .uncompressed_size
            .map(display_size)
            .unwrap_or_else(|| "unavailable (encrypted header)".to_owned()),
        commit
            .ratio_percent
            .map(|ratio| format!("{ratio:.2}%"))
            .unwrap_or_else(|| "n/a".to_owned()),
        scope,
        algorithm,
    )
}

fn render_commit_metadata(commit: &CommitInfo) -> String {
    let mut output = String::new();
    if let Some(created) = commit.created_unix_millis {
        output.push_str(&format!("  created: {}\n", format_unix_millis(created)));
    }
    if let Some(user) = &commit.user {
        output.push_str(&format!("  user: {user}\n"));
    }
    output
}

fn render_block_summaries(blocks: &[BlockInfo]) -> String {
    let mut output = format!("  blocks: {}\n", blocks.len());
    for block in blocks {
        output.push_str(&format!(
            "    block {:08x} files={} compressed={} ratio={} groups={}\n",
            block.index,
            block.file_count,
            display_size(block.compressed_size),
            block
                .ratio_percent
                .map(|ratio| format!("{ratio:.2}%"))
                .unwrap_or_else(|| "n/a".to_owned()),
            if block.file_groups.is_empty() {
                "unavailable".to_owned()
            } else {
                block.file_groups.join(", ")
            }
        ));
    }
    output
}

fn display_size(size: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = size as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{size} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

fn format_extraction_plan(plan: &ExtractionPlan) -> Vec<String> {
    let mut lines = vec!["final TOC:".to_owned()];
    for (path, entry) in plan.toc.iter() {
        match entry {
            zarlib::TocEntry::Directory => lines.push(format!("  {path}/")),
            zarlib::TocEntry::File(file) => lines.push(format!("  {path}\t{} bytes", file.size)),
            zarlib::TocEntry::Symlink { target } => lines.push(format!("  {path} -> {target}")),
            zarlib::TocEntry::NamedPipe => lines.push(format!("  {path} [named pipe]")),
            zarlib::TocEntry::Device { kind, major, minor } => {
                lines.push(format!("  {path} [{kind:?} device {major}:{minor}]"));
            }
        }
    }
    lines.extend(
        plan.directories
            .iter()
            .map(|path| format!("create directory {}", path.display())),
    );
    lines.extend(plan.files.iter().map(|(path, entry, kind)| match kind {
        ExtractedKind::File => format!("write {} ({} bytes)", path.display(), entry.size),
        ExtractedKind::Symlink(target) => {
            format!("create symlink {} -> {target}", path.display())
        }
        ExtractedKind::NamedPipe => format!("create named pipe {}", path.display()),
        ExtractedKind::Device { kind, major, minor } => format!(
            "create {kind:?} device {} ({major}:{minor})",
            path.display()
        ),
    }));
    lines
}

fn render_commit_stats(stats: &CommitFileStats) -> String {
    let mut output = String::from("  file groups:\n");
    for name in [
        "empty files",
        "tiny files",
        "small files",
        "big files",
        "files (block size unknown)",
    ] {
        let Some(group) = stats.groups.get(name) else {
            continue;
        };
        let ratio = if group.uncompressed_size == 0 {
            "n/a".to_owned()
        } else {
            format!(
                "{:.2}%",
                group.compressed_size as f64 * 100.0 / group.uncompressed_size as f64
            )
        };
        let suffix = if group.has_encrypted_blocks {
            " (some encrypted sizes unavailable)"
        } else {
            ""
        };
        output.push_str(&format!(
            "    {name}: {} file(s), compressed {}, uncompressed {}, ratio {ratio}{suffix}\n",
            group.files,
            display_size(group.compressed_size),
            display_size(group.uncompressed_size),
        ));
    }
    if stats.deduplicated_files > 0 {
        output.push_str(&format!(
            "  deduplicated files: {}, saved uncompressed space: {}\n",
            stats.deduplicated_files,
            display_size(stats.deduplicated_uncompressed_size)
        ));
    }
    if stats.symlinks > 0 || stats.named_pipes > 0 || stats.devices > 0 {
        output.push_str(&format!(
            "  special entries: {} symlinks, {} pipes, {} devices\n",
            stats.symlinks, stats.named_pipes, stats.devices
        ));
    }
    let metadata = &stats.entry_metadata;
    let saved = [
        ("owner", metadata.owner),
        ("group", metadata.group),
        ("permissions", metadata.permissions),
        ("modification time", metadata.modification_times),
        ("creation time", metadata.creation_times),
        ("access time", metadata.access_times),
    ]
    .iter()
    .filter(|(_, count)| *count > 0)
    .map(|(name, count)| format!("{name} ({count})"))
    .collect::<Vec<_>>();
    output.push_str(&format!(
        "  saved metadata: {}\n",
        if saved.is_empty() {
            "none".to_owned()
        } else {
            saved.join(", ")
        }
    ));
    output
}

fn parse_cli_collision_policy(value: &str) -> Result<CollisionPolicy, Box<dyn std::error::Error>> {
    match value {
        "error" => Ok(CollisionPolicy::Error),
        "latest" => Ok(CollisionPolicy::Latest),
        "first" => Ok(CollisionPolicy::First),
        "rename" => Ok(CollisionPolicy::Rename),
        "ask" => Ok(CollisionPolicy::Ask),
        _ => Err(format!("invalid collision policy `{value}`").into()),
    }
}

fn parse_cli_existing_policy(value: &str) -> Result<ExistingPolicy, Box<dyn std::error::Error>> {
    match value {
        "overwrite" => Ok(ExistingPolicy::Overwrite),
        "skip" => Ok(ExistingPolicy::Skip),
        "error" => Ok(ExistingPolicy::Error),
        "ask" => Ok(ExistingPolicy::Ask),
        _ => Err(format!("invalid existing-file policy `{value}`").into()),
    }
}

fn parse_cli_block_size(value: &str) -> Result<usize, Box<dyn std::error::Error>> {
    if value == "auto" {
        return Ok(0);
    }
    let (digits, multiplier) = match value.as_bytes().last().copied() {
        Some(b'k' | b'K') => (&value[..value.len() - 1], 1024_usize),
        Some(b'm' | b'M') => (&value[..value.len() - 1], 1024 * 1024),
        _ => (value, 1),
    };
    let size = digits
        .parse::<usize>()
        .map_err(|_| format!("invalid block size `{value}`"))?
        .checked_mul(multiplier)
        .ok_or_else(|| format!("invalid block size `{value}`"))?;
    if !(MIN_CREATE_BLOCK_SIZE..=MAX_CREATE_BLOCK_SIZE).contains(&size) {
        return Err(format!(
            "invalid block size: choose 64 KiB to 32 MiB or `auto` (got `{value}`)"
        )
        .into());
    }
    Ok(size)
}

fn parse_cli_compression_level(value: &str) -> Result<i32, Box<dyn std::error::Error>> {
    let level = value
        .parse::<i32>()
        .map_err(|_| format!("invalid compression level `{value}`"))?;
    if !(-7..=22).contains(&level) {
        return Err("invalid compression level: choose a value from -7 to 22".into());
    }
    Ok(level)
}

fn cli_encoding_options(
    block_size: Option<&str>,
    compression_level: Option<&str>,
) -> Result<EncodingOptions, Box<dyn std::error::Error>> {
    let mut options = EncodingOptions::default();
    if let Some(block_size) = block_size {
        options.block_size = parse_cli_block_size(block_size)?;
    }
    if let Some(level) = compression_level {
        options.compression_level = parse_cli_compression_level(level)?;
    }
    Ok(options)
}

fn zar_output_filename(filename: &str) -> Option<String> {
    let lowercase = filename.to_ascii_lowercase();
    let suffix = [
        ".tar.gz", ".tgz", ".zip", ".7z", ".rar", ".iso", ".tar", ".gz",
    ]
    .into_iter()
    .find(|suffix| lowercase.ends_with(suffix))?;
    Some(format!(
        "{}.zar",
        &filename[..filename.len() - suffix.len()]
    ))
}

fn conversion_sources(
    input_folder: &std::path::Path,
    filters: &FilterSet,
) -> Result<Vec<(std::path::PathBuf, String)>, Box<dyn std::error::Error>> {
    let mut entries = std::fs::read_dir(input_folder)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    let mut sources = Vec::new();
    for entry in entries {
        if !entry.file_type()?.is_file() {
            continue;
        }
        let filename = entry.file_name();
        let Some(filename) = filename.to_str() else {
            continue;
        };
        let source = entry.path();
        if !is_supported_import_archive(&source) || !filters.includes(filename, false) {
            continue;
        }
        let output_filename = zar_output_filename(filename)
            .ok_or_else(|| format!("cannot determine ZAR output name for {filename:?}"))?;
        sources.push((source, output_filename));
    }
    Ok(sources)
}

fn convert_archive_folder(
    input_folder: &std::path::Path,
    output_folder: Option<&std::path::Path>,
    filter: &str,
    overwrite: bool,
    block_size: Option<&str>,
    compression_level: Option<&str>,
) -> Result<u8, Box<dyn std::error::Error>> {
    let input_folder = input_folder.canonicalize()?;
    if !input_folder.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            format!("input path is not a directory: {}", input_folder.display()),
        )
        .into());
    }

    let include_patterns = [filter.to_owned()];
    let empty_patterns: [String; 0] = [];
    let empty_filelists: [std::path::PathBuf; 0] = [];
    let filters = FilterSet::from_patterns(
        &include_patterns,
        &empty_patterns,
        &empty_patterns,
        &empty_patterns,
        &empty_filelists,
    )?;

    let sources = conversion_sources(&input_folder, &filters)?;

    if sources.is_empty() {
        eprintln!(
            "no supported archives matched fnmatch filter `{filter}` in {}",
            input_folder.display()
        );
        return Ok(1);
    }

    let output_folder = match output_folder {
        Some(path) => {
            std::fs::create_dir_all(path)?;
            path.canonicalize()?
        }
        None => input_folder.clone(),
    };
    if output_folder.to_str().is_none() {
        return Err("output folder path must be valid UTF-8".into());
    }

    let mut output_counts = std::collections::HashMap::new();
    for (_, filename) in &sources {
        *output_counts
            .entry(filename.to_lowercase())
            .or_insert(0usize) += 1;
    }

    let options = cli_encoding_options(block_size, compression_level)?;
    let total = sources.len();
    let mut converted = 0;
    let mut failures = 0;
    let mut warnings = false;
    for (source, filename) in sources {
        let destination = output_folder.join(&filename);
        if output_counts[&filename.to_lowercase()] > 1 {
            eprintln!(
                "cannot convert {}: multiple selected archives map to {}",
                source.display(),
                destination.display()
            );
            failures += 1;
            continue;
        }
        let archive = destination
            .to_str()
            .ok_or("output archive path must be valid UTF-8")?;
        let paths = [source.clone()];
        match create_archive_with_password_progress_and_special_files(
            archive,
            &paths,
            false,
            options,
            None,
            None,
            CreateInputOptions {
                overwrite_existing: overwrite,
                ..CreateInputOptions::default()
            },
        ) {
            Ok(report) => {
                converted += 1;
                for warning in report.warnings {
                    eprintln!("{}: {warning}", source.display());
                    warnings = true;
                }
            }
            Err(error) => {
                eprintln!("cannot convert {}: {error}", source.display());
                failures += 1;
            }
        }
    }

    println!("Converted {converted} of {total} matching archive(s).");
    if failures > 0 {
        eprintln!("{failures} archive(s) failed to convert.");
        Ok(1)
    } else {
        Ok(u8::from(warnings))
    }
}

fn format_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs_f64();
    if secs < 60.0 {
        format!("{secs:.2}s")
    } else {
        let mins = (secs / 60.0) as u64;
        let rem = secs % 60.0;
        format!("{mins}m {rem:.2}s")
    }
}

fn format_compression_ratio(compressed: u64, uncompressed: u64) -> String {
    if uncompressed == 0 {
        "n/a".to_owned()
    } else {
        format!("{:.2}%", compressed as f64 * 100.0 / uncompressed as f64)
    }
}

fn execute(cli: Cli) -> Result<u8, Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    match cli.command {
        Command::Create {
            archive,
            paths,
            strict,
            verbose,
            metadata,
            modification_time,
            creation_time,
            access_time,
            user,
            special_files,
            include_files,
            exclude_files,
            include_folders,
            exclude_folders,
            filelist,
            overwrite,
            no_progress,
            password: encrypt,
            aead,
            encryption_scope,
            block_size,
            compression_level,
        } => {
            let options =
                cli_encoding_options(block_size.as_deref(), compression_level.as_deref())?;
            let filters = FilterSet::from_patterns(
                &include_files,
                &exclude_files,
                &include_folders,
                &exclude_folders,
                &filelist,
            )?;
            let progress_callback = CliProgress::new(!no_progress);
            let password = encrypt
                .then(|| prompt_password("Archive password: "))
                .transpose()?;
            let aead = parse_content_aead(&aead)?;
            let encryption_scope = parse_encryption_scope(&encryption_scope)?;
            let report = create_archive_with_password_progress_and_filters_and_encryption(
                &archive,
                &paths,
                strict,
                options,
                password
                    .as_ref()
                    .map(|value| (value.as_bytes(), aead, encryption_scope)),
                Some(&progress_callback as &dyn Progress),
                CreateFilterOptions {
                    input: CreateInputOptions {
                        save_metadata: metadata,
                        save_modification_time: modification_time,
                        save_creation_time: creation_time,
                        save_access_time: access_time,
                        save_user: user,
                        include_special_files: special_files,
                        overwrite_existing: overwrite,
                        verbose,
                    },
                    filters: &filters,
                },
            )?;
            let has_warnings = !report.warnings.is_empty();
            for warning in report.warnings {
                eprintln!("{warning}");
            }
            if let Some(block_size) = block_size
                && block_size == "auto"
            {
                eprintln!("using automatic block size: {} bytes", report.block_size);
            }
            let elapsed = start.elapsed();
            let total_bytes = progress_callback.total_bytes();
            let speed =
                (total_bytes as f64 / (1024.0 * 1024.0)) / elapsed.as_secs_f64().max(0.000_001);
            let final_size = std::fs::metadata(&archive).map(|m| m.len()).unwrap_or(0);
            let ratio = format_compression_ratio(final_size, total_bytes);
            println!(
                "Archive size: {}, compression ratio: {}",
                display_size(final_size),
                ratio
            );
            if no_progress {
                println!("Time: {}", format_duration(elapsed));
            } else {
                println!("Time: {} ({:.2} MB/s)", format_duration(elapsed), speed);
            }
            Ok(u8::from(has_warnings))
        }
        Command::Add {
            archive,
            paths,
            no_progress,
            metadata,
            modification_time,
            creation_time,
            access_time,
            user,
            special_files,
            password: encrypt,
            aead,
            encryption_scope,
            password_for,
            strict,
            verbose,
            include_files,
            exclude_files,
            include_folders,
            exclude_folders,
            filelist,
            block_size,
            compression_level,
        } => {
            let progress_callback = CliProgress::new(!no_progress);
            let filters = FilterSet::from_patterns(
                &include_files,
                &exclude_files,
                &include_folders,
                &exclude_folders,
                &filelist,
            )?;
            let old_size = std::fs::metadata(&archive).map(|m| m.len()).unwrap_or(0);
            let new_password = encrypt
                .then(|| prompt_password("New commit password: "))
                .transpose()?;
            let mut passwords = PasswordSet::new(
                new_password
                    .as_ref()
                    .map(|password| password.as_bytes().to_vec()),
            );
            let mut seen_password_ids = std::collections::BTreeSet::new();
            for id in &password_for {
                let parsed = parse_commit_id(id)?;
                if !seen_password_ids.insert(parsed) {
                    return Err(
                        format!("password for commit {id} was specified more than once").into(),
                    );
                }
                let specific = prompt_password(&format!("Password for commit {id}: "))?;
                passwords.insert_for_commit(parsed, specific.into_bytes());
            }
            let encryption: Option<(&[u8], ContentAead, EncryptionScope)> =
                if let Some(password) = &new_password {
                    Some((
                        password.as_bytes(),
                        parse_content_aead(&aead)?,
                        parse_encryption_scope(&encryption_scope)?,
                    ))
                } else {
                    None
                };
            add_archive_with_encoding_progress_and_filters_and_encryption(
                &archive,
                &paths,
                cli_encoding_options(block_size.as_deref(), compression_level.as_deref())?,
                Some(&progress_callback as &dyn Progress),
                AddInputOptions {
                    save_metadata: metadata,
                    save_modification_time: modification_time,
                    save_creation_time: creation_time,
                    save_access_time: access_time,
                    save_user: user,
                    include_special_files: special_files,
                    strict,
                    verbose,
                },
                &filters,
                encryption,
                &passwords,
            )?;
            let elapsed = start.elapsed();
            let total_bytes = progress_callback.total_bytes();
            let speed =
                (total_bytes as f64 / (1024.0 * 1024.0)) / elapsed.as_secs_f64().max(0.000_001);
            let final_size = std::fs::metadata(&archive).map(|m| m.len()).unwrap_or(0);
            let added_bytes = final_size.saturating_sub(old_size);
            let ratio = format_compression_ratio(added_bytes, total_bytes);
            println!(
                "Archive size: {}, compression ratio: {}",
                display_size(final_size),
                ratio
            );
            if no_progress {
                println!("Time: {}", format_duration(elapsed));
            } else {
                println!("Time: {} ({:.2} MB/s)", format_duration(elapsed), speed);
            }
            Ok(0)
        }
        Command::Convert {
            input_folder,
            output_folder,
            filter,
            overwrite,
            block_size,
            compression_level,
        } => convert_archive_folder(
            &input_folder,
            output_folder.as_deref(),
            &filter,
            overwrite,
            block_size.as_deref(),
            compression_level.as_deref(),
        ),
        Command::Remove {
            archive,
            paths,
            no_progress,
        } => {
            let progress_callback = CliProgress::new(!no_progress);
            remove_archive_with_progress(
                &archive,
                &paths,
                Some(&progress_callback as &dyn Progress),
            )?;
            let elapsed = start.elapsed();
            println!("Time: {}", format_duration(elapsed));
            Ok(0)
        }
        Command::RemoveCommits {
            archive,
            commits,
            output,
        } => {
            let result = remove_archive_commits(&archive, &commits, output.as_deref())?;
            if !result.replay_validated {
                eprintln!(
                    "Warning: retained encrypted headers prevented validation of the remaining history."
                );
            }
            println!("Removed {} commit(s)", commits.len());
            Ok(0)
        }
        Command::Meta {
            archive,
            set,
            remove,
        } => {
            if set.is_empty() && remove.is_empty() {
                for (key, value) in read_custom_metadata(&archive)? {
                    println!("{key}={}", format_metadata_value(&value));
                }
            } else {
                let set = set
                    .iter()
                    .map(|item| {
                        item.split_once('=')
                            .map(|(key, value)| (key.to_owned(), parse_metadata_value(value)))
                            .ok_or_else(|| format!("expected KEY=VALUE, got `{item}`"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                update_custom_metadata(&archive, &set, &remove)?;
            }
            Ok(0)
        }
        Command::Join {
            archive,
            from,
            to,
            no_progress,
            block_size,
            compression_level,
        } => {
            let progress_callback = CliProgress::new(!no_progress);
            join_archive_with_encoding_progress(
                &archive,
                &from,
                &to,
                cli_encoding_options(block_size.as_deref(), compression_level.as_deref())?,
                Some(&progress_callback as &dyn Progress),
            )?;
            let elapsed = start.elapsed();
            println!("Time: {}", format_duration(elapsed));
            Ok(0)
        }
        Command::Encrypt {
            archive,
            from,
            to,
            output,
            password,
            password_for,
            aead,
            encryption_scope,
            strict,
        } => {
            parse_commit_id(&from)?;
            parse_commit_id(&to)?;
            let aead = parse_content_aead(&aead)?;
            let encryption_scope = parse_encryption_scope(&encryption_scope)?;
            let new_password = prompt_password("New encryption password: ")?;
            let passwords = prompt_password_set(password, &password_for)?;
            encrypt_archive_commits(
                &archive,
                CommitRangeOptions {
                    from_id: &from,
                    to_id: &to,
                    output: output.as_deref(),
                    passwords: &passwords,
                    strict,
                },
                new_password.as_bytes(),
                aead,
                encryption_scope,
            )?;
            println!("Commit range transformation completed.");
            Ok(0)
        }
        Command::Decrypt {
            archive,
            from,
            to,
            output,
            password,
            password_for,
            strict,
        } => {
            parse_commit_id(&from)?;
            parse_commit_id(&to)?;
            let passwords = prompt_password_set(password, &password_for)?;
            decrypt_archive_commits(
                &archive,
                CommitRangeOptions {
                    from_id: &from,
                    to_id: &to,
                    output: output.as_deref(),
                    passwords: &passwords,
                    strict,
                },
            )?;
            println!("Commit range transformation completed.");
            Ok(0)
        }
        Command::Check {
            archive,
            recover,
            password,
            password_for,
            no_progress,
        } => {
            let passwords = prompt_password_set(password, &password_for)?;
            let progress_callback = CliProgress::new(!no_progress);
            let report = check_archive_with_recovery_and_passwords_progress(
                &archive,
                recover,
                &passwords,
                Some(&progress_callback as &dyn Progress),
            )?;
            println!(
                "valid framing: {} commit(s); content-checked {} unencrypted block(s)",
                report.commits, report.checked_blocks
            );
            for message in report.recovery_messages {
                eprintln!("{message}");
            }
            if report.recoveries > 0 {
                println!(
                    "recovery scan found {} damaged framing gap(s)",
                    report.recoveries
                );
            }
            if report.encrypted_blocks > 0 {
                println!(
                    "content not checked: {} encrypted block(s)",
                    report.encrypted_blocks
                );
            }
            if !report.logical_state_readable {
                println!("logical TOC consistency not fully checked: encrypted header present");
            }
            let elapsed = start.elapsed();
            println!("Time: {}", format_duration(elapsed));
            Ok(u8::from(report.recoveries > 0))
        }
        Command::Info {
            archive,
            verbose,
            json,
            meta,
        } => {
            if meta {
                for (key, value) in read_custom_metadata(&archive)? {
                    println!("{key}={}", format_metadata_value(&value));
                }
                return Ok(0);
            }
            let commits = if verbose >= 2 {
                info_archive_with_blocks(&archive, true)?
            } else {
                info_archive(&archive, verbose > 0)?
                    .into_iter()
                    .map(|commit| CommitInfoWithBlocks {
                        commit,
                        blocks: Vec::new(),
                    })
                    .collect()
            };
            if json {
                let json_commits = commits
                    .iter()
                    .map(|info| {
                        let commit = &info.commit;
                        InfoJsonCommit {
                            id: format_commit_id(&commit.id),
                            size: commit.size,
                            header_size: commit.header_size,
                            data_size: commit.data_size,
                            header_encrypted: commit.header_encrypted,
                            data_encrypted: commit.data_encrypted,
                            encryption_algorithm: commit.encryption_algorithm.map(|algorithm| {
                                match algorithm {
                                    ContentAead::Aes256Gcm => "aes-256-gcm",
                                    ContentAead::ChaCha20Poly1305 => "chacha20-poly1305",
                                }
                            }),
                            header_changes: commit.header_changes.clone(),
                            created_unix_millis: commit.created_unix_millis,
                            user: commit.user.clone(),
                        }
                    })
                    .collect::<Vec<_>>();
                println!("{}", render_info_json(&json_commits));
            } else {
                for info in &commits {
                    let commit = &info.commit;
                    println!("{}", format_commit_summary(commit));
                    print!("{}", render_commit_metadata(commit));
                    print!("{}", render_header_changes(commit.header_changes.as_ref()));
                    if let Some(header) = &commit.header_compression {
                        println!(
                            "  header: compressed {}, uncompressed {}, ratio {}",
                            display_size(header.compressed_size),
                            header
                                .uncompressed_size
                                .map(display_size)
                                .unwrap_or_else(|| "unavailable (encrypted)".to_owned()),
                            header
                                .ratio_percent
                                .map(|ratio| format!("{ratio:.2}%"))
                                .unwrap_or_else(|| "n/a".to_owned())
                        );
                    }
                    if let Some(details) = &commit.details {
                        print!("{}", render_commit_stats(details));
                    } else if commit.header_encrypted {
                        println!(
                            "  detailed file statistics unavailable: commit header is encrypted"
                        );
                    }
                    if verbose >= 2 {
                        print!("{}", render_block_summaries(&info.blocks));
                    }
                }
                println!("{} commit(s)", commits.len());
            }
            let elapsed = start.elapsed();
            if json {
                eprintln!("Time: {}", format_duration(elapsed));
            } else {
                println!("Time: {}", format_duration(elapsed));
            }
            Ok(0)
        }
        Command::List {
            archive,
            to,
            password,
            password_for,
            verbose,
            include_files,
            exclude_files,
            include_folders,
            exclude_folders,
            filelist,
        } => {
            let passwords = prompt_password_set(password, &password_for)?;
            let toc = list_archive_with_passwords(&archive, to.as_deref(), &passwords)?;
            let filters = FilterSet::from_patterns(
                &include_files,
                &exclude_files,
                &include_folders,
                &exclude_folders,
                &filelist,
            )?;
            for (archive_path, entry) in toc.iter() {
                if !filters.includes(
                    archive_path.as_str(),
                    matches!(entry, zarlib::TocEntry::Directory),
                ) {
                    continue;
                }
                match entry {
                    zarlib::TocEntry::Directory => println!("{}/", archive_path),
                    zarlib::TocEntry::File(file) => {
                        println!("{}\t{} bytes", archive_path, file.size)
                    }
                    zarlib::TocEntry::Symlink { target } => {
                        println!("{} -> {}", archive_path, target)
                    }
                    zarlib::TocEntry::NamedPipe => println!("{} [named pipe]", archive_path),
                    zarlib::TocEntry::Device { kind, major, minor } => {
                        println!("{} [{kind:?} device {major}:{minor}]", archive_path)
                    }
                }
            }
            if verbose {
                print!(
                    "{}",
                    render_list_diagnostics(&list_diagnostics(
                        &archive,
                        to.as_deref(),
                        &passwords,
                    )?)
                );
            }
            let elapsed = start.elapsed();
            println!("Time: {}", format_duration(elapsed));
            Ok(0)
        }
        Command::Extract {
            archive,
            output: destination,
            to,
            collision,
            existing,
            dry_run,
            metadata,
            password,
            password_for,
            no_progress,
            symlinks,
            devices,
            strip_top_level,
            include_files,
            exclude_files,
            include_folders,
            exclude_folders,
            filelist,
        } => {
            let collision = parse_cli_collision_policy(&collision)?;
            let existing = parse_cli_existing_policy(&existing)?;
            let passwords = prompt_password_set(password, &password_for)?;
            let progress_callback = CliProgress::new(!no_progress);
            let filters = FilterSet::from_patterns(
                &include_files,
                &exclude_files,
                &include_folders,
                &exclude_folders,
                &filelist,
            )?;
            let plan = extract_archive_with_passwords_progress_and_filters(
                &archive,
                &destination,
                ExtractionOptions {
                    dry_run,
                    to: to.as_deref(),
                    collision_policy: collision,
                    existing_policy: existing,
                    restore_metadata: metadata,
                    restore_symlinks: symlinks,
                    restore_devices: devices,
                    strip_top_level,
                },
                &passwords,
                Some(&progress_callback as &dyn Progress),
                &filters,
            )?;
            if dry_run {
                for line in format_extraction_plan(&plan) {
                    println!("{line}");
                }
            }
            let elapsed = start.elapsed();
            println!("Time: {}", format_duration(elapsed));
            Ok(0)
        }
    }
}

#[cfg(test)]
mod tests;
