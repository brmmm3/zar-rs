use eframe::egui;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use zarlib::operations::{
    self, AddInputOptions, ArchiveEntryDetails, CheckReport, CollisionPolicy, CommitInfo,
    CreateFilterOptions, CreateInputOptions, EncodingOptions, ExistingPolicy, ExtractionOptions,
    FilterSet, PasswordSet, Progress,
};
use zarlib::{ArchivePath, ContentAead, EncryptionScope, Toc, TocEntry};

fn left_aligned_cell(
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    widget: impl egui::Widget,
) -> egui::Response {
    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        egui::Layout::left_to_right(egui::Align::Center),
        |cell_ui| {
            cell_ui.set_min_size(egui::vec2(width, height));
            cell_ui.add(widget)
        },
    )
    .inner
}

struct HeaderLayout {
    offset: f32,
    trim: f32,
    gap: f32,
    pad: f32,
}

// Draws a header row with clickable sort labels and draggable column borders.
fn render_column_header(
    ui: &mut egui::Ui,
    id: egui::Id,
    columns: &[(&str, Option<SortBy>)],
    widths: &mut [f32],
    sort: (SortBy, bool),
    layout: &HeaderLayout,
) -> Option<SortBy> {
    let height = ui.spacing().interact_size.y;
    let total =
        layout.offset + widths.iter().sum::<f32>() + layout.gap * widths.len() as f32 - layout.trim;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(total, height), egui::Sense::hover());
    let font = egui::TextStyle::Body.resolve(ui.style());
    let text_color = ui.visuals().strong_text_color();
    let mut clicked = None;
    let mut x = rect.left() + layout.offset;
    for (index, (label, sortable)) in columns.iter().enumerate() {
        let width = if index == 0 {
            widths[index] - layout.trim
        } else {
            widths[index]
        };
        let cell = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(width, height));
        let text = (*label).to_owned();
        let mut active = false;
        if let Some(column_sort) = sortable {
            active = *column_sort == sort.0;
            let response = ui.interact(cell, id.with(("sort", index)), egui::Sense::click());
            if response.hovered() {
                ui.painter()
                    .rect_filled(cell, 2.0, ui.visuals().widgets.hovered.bg_fill);
            }
            if response.clicked() {
                clicked = Some(*column_sort);
            }
        }
        let galley = ui.painter().layout_no_wrap(text, font.clone(), text_color);
        let text_width = galley.size().x;
        let clipped = ui.painter().with_clip_rect(cell);
        clipped.galley(
            egui::pos2(
                cell.left() + layout.pad,
                cell.center().y - galley.size().y / 2.0,
            ),
            galley,
            text_color,
        );
        if active {
            let center = egui::pos2(
                cell.left() + layout.pad + text_width + 10.0,
                cell.center().y,
            );
            let points = if sort.1 {
                vec![
                    egui::pos2(center.x - 4.0, center.y - 2.5),
                    egui::pos2(center.x + 4.0, center.y - 2.5),
                    egui::pos2(center.x, center.y + 3.0),
                ]
            } else {
                vec![
                    egui::pos2(center.x - 4.0, center.y + 2.5),
                    egui::pos2(center.x + 4.0, center.y + 2.5),
                    egui::pos2(center.x, center.y - 3.0),
                ]
            };
            clipped.add(egui::Shape::convex_polygon(
                points,
                text_color,
                egui::Stroke::NONE,
            ));
        }
        let border_x = cell.right() + layout.gap / 2.0;
        let handle = egui::Rect::from_min_max(
            egui::pos2(border_x - 3.0, rect.top()),
            egui::pos2(border_x + 3.0, rect.bottom()),
        );
        let response = ui.interact(handle, id.with(("resize", index)), egui::Sense::drag());
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
        }
        if response.dragged() {
            widths[index] = (widths[index] + response.drag_delta().x).max(40.0 + layout.trim);
        }
        ui.painter().vline(
            border_x,
            rect.y_range(),
            ui.visuals().widgets.noninteractive.bg_stroke,
        );
        x += width + layout.gap;
    }
    clicked
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OperationKind {
    Create,
    Add,
    Remove,
    Join,
    Metadata,
    Check,
    List,
    Extract,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum ArchiveView {
    #[default]
    Normal,
    Compact,
    Details,
    Structured,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SortBy {
    #[default]
    Name,
    Date,
    CompressedSize,
    UncompressedSize,
    Ratio,
    Type,
}

#[derive(Clone)]
struct StructuredRow {
    path: String,
    data: EntryData,
    depth: usize,
    has_children: bool,
    expanded: bool,
    child_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextAction {
    Create,
    Add,
    CreateNamed,
    Open,
    ExtractHere,
    ExtractTo,
    ExtractToFolder,
    Test,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) enum UiIcon {
    Application,
    OpenArchive,
    CreateArchive,
    Quit,
    SelectAll,
    ClearSelection,
    Refresh,
    ListContents,
    TestArchive,
    Join,
    About,
    Add,
    AddFolder,
    Extract,
    Info,
    Remove,
    Browse,
    ClearInputs,
    Confirm,
    Cancel,
    AddFilterList,
    Folder,
    Drive,
    Symlink,
    Image,
    Audio,
    Video,
    Archive,
    ZarArchive,
    Pdf,
    Code,
    Python,
    Java,
    Script,
    JavaScript,
    Text,
    Word,
    Spreadsheet,
    Presentation,
    Generic,
    SpecialData,
}

fn icon_bytes(icon: UiIcon) -> &'static [u8] {
    match icon {
        UiIcon::Application => include_bytes!("../assets/zarg-icon.png"),
        UiIcon::OpenArchive => include_bytes!("../assets/icons/actions/document-open.png"),
        UiIcon::CreateArchive => include_bytes!("../assets/icons/actions/document-new.png"),
        UiIcon::Quit => include_bytes!("../assets/icons/actions/application-exit.png"),
        UiIcon::SelectAll => include_bytes!("../assets/icons/actions/edit-select-all.png"),
        UiIcon::ClearSelection | UiIcon::ClearInputs => {
            include_bytes!("../assets/icons/actions/edit-clear.png")
        }
        UiIcon::Refresh => include_bytes!("../assets/icons/actions/view-refresh.png"),
        UiIcon::ListContents => {
            include_bytes!("../assets/icons/actions/document-preview-archive.png")
        }
        UiIcon::TestArchive => include_bytes!("../assets/icons/actions/dialog-ok-apply.png"),
        UiIcon::Join => include_bytes!("../assets/icons/actions/archive-insert.png"),
        UiIcon::About => include_bytes!("../assets/icons/actions/help-about.png"),
        UiIcon::Add => include_bytes!("../assets/icons/actions/archive-insert.png"),
        UiIcon::AddFolder => {
            include_bytes!("../assets/icons/actions/archive-insert-directory.png")
        }
        UiIcon::Extract => include_bytes!("../assets/icons/actions/archive-extract.png"),
        UiIcon::Info => include_bytes!("../assets/icons/actions/document-properties.png"),
        UiIcon::Remove => include_bytes!("../assets/icons/actions/archive-remove.png"),
        UiIcon::Browse => include_bytes!("../assets/icons/actions/document-open-folder.png"),
        UiIcon::Confirm => include_bytes!("../assets/icons/actions/dialog-ok.png"),
        UiIcon::Cancel => include_bytes!("../assets/icons/actions/dialog-cancel.png"),
        UiIcon::AddFilterList => include_bytes!("../assets/icons/actions/list-add.png"),
        UiIcon::Folder => include_bytes!("../assets/icons/places/folder.png"),
        UiIcon::Drive => include_bytes!("../assets/icons/devices/drive-harddisk.png"),
        UiIcon::Symlink => include_bytes!("../assets/icons/mimetypes/emblem-symbolic-link.png"),
        UiIcon::Image => include_bytes!("../assets/icons/mimetypes/image-x-generic.png"),
        UiIcon::Audio => include_bytes!("../assets/icons/mimetypes/audio-x-generic.png"),
        UiIcon::Video => include_bytes!("../assets/icons/mimetypes/video-x-generic.png"),
        UiIcon::Archive => include_bytes!("../assets/icons/mimetypes/application-x-archive.png"),
        UiIcon::ZarArchive => include_bytes!("../assets/application-x-zar.png"),
        UiIcon::Pdf => include_bytes!("../assets/icons/mimetypes/application-pdf.png"),
        UiIcon::Code => include_bytes!("../assets/icons/mimetypes/text-x-csrc.png"),
        UiIcon::Python => include_bytes!("../assets/icons/mimetypes/text-x-python.png"),
        UiIcon::Java => include_bytes!("../assets/icons/mimetypes/text-x-java.png"),
        UiIcon::Script => include_bytes!("../assets/icons/mimetypes/text-x-script.png"),
        UiIcon::JavaScript => {
            include_bytes!("../assets/icons/mimetypes/application-x-javascript.png")
        }
        UiIcon::Text => include_bytes!("../assets/icons/mimetypes/text-plain.png"),
        UiIcon::Word => include_bytes!("../assets/icons/mimetypes/application-vnd.ms-word.png"),
        UiIcon::Spreadsheet => {
            include_bytes!("../assets/icons/mimetypes/application-vnd.ms-excel.png")
        }
        UiIcon::Presentation => {
            include_bytes!("../assets/icons/mimetypes/application-vnd.ms-powerpoint.png")
        }
        UiIcon::Generic => include_bytes!("../assets/icons/mimetypes/unknown.png"),
        UiIcon::SpecialData => {
            include_bytes!("../assets/icons/mimetypes/application-octet-stream.png")
        }
    }
}

pub(crate) fn icon_texture(
    context: &egui::Context,
    cache: &RefCell<HashMap<UiIcon, egui::TextureHandle>>,
    icon: UiIcon,
) -> egui::TextureHandle {
    if let Some(texture) = cache.borrow().get(&icon) {
        return texture.clone();
    }

    let image = image::load_from_memory(icon_bytes(icon))
        .expect("bundled KDE icon must be a valid image")
        .into_rgba8();
    let [width, height] = [image.width() as usize, image.height() as usize];
    let texture = context.load_texture(
        format!("zarg-icon-{icon:?}"),
        egui::ColorImage::from_rgba_unmultiplied([width, height], image.as_raw()),
        egui::TextureOptions::LINEAR,
    );
    cache.borrow_mut().insert(icon, texture.clone());
    texture
}

fn icon_button(
    ui: &mut egui::Ui,
    cache: &RefCell<HashMap<UiIcon, egui::TextureHandle>>,
    icon: UiIcon,
    label: impl Into<egui::WidgetText>,
    enabled: bool,
) -> egui::Response {
    let texture = icon_texture(ui.ctx(), cache, icon);
    ui.add_enabled(
        enabled,
        egui::Button::image_and_text(
            egui::Image::new(&texture).fit_to_exact_size(egui::vec2(16.0, 16.0)),
            label,
        ),
    )
}

fn icon_menu_button<R>(
    ui: &mut egui::Ui,
    texture: egui::TextureHandle,
    label: impl Into<egui::WidgetText>,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    ui.menu_image_text_button(
        egui::Image::new(&texture).fit_to_exact_size(egui::vec2(16.0, 16.0)),
        label,
        add_contents,
    )
}

pub(crate) fn file_icon(path: &str) -> UiIcon {
    let extension = path.rsplit_once('.').map(|(_, extension)| extension);
    match extension.map(str::to_ascii_lowercase).as_deref() {
        Some("png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "tif" | "tiff") => {
            UiIcon::Image
        }
        Some("mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac") => UiIcon::Audio,
        Some("mp4" | "mkv" | "mov" | "avi" | "webm" | "mpeg" | "mpg") => UiIcon::Video,
        Some("zar") => UiIcon::ZarArchive,
        Some("zip" | "7z" | "rar" | "tar" | "gz" | "bz2" | "xz") => UiIcon::Archive,
        Some("pdf") => UiIcon::Pdf,
        Some("py") => UiIcon::Python,
        Some("java" | "kt") => UiIcon::Java,
        Some("sh" | "bash" | "zsh") => UiIcon::Script,
        Some("js" | "jsx" | "mjs" | "cjs") => UiIcon::JavaScript,
        Some("doc" | "docx" | "odt") => UiIcon::Word,
        Some("xls" | "xlsx" | "ods" | "csv") => UiIcon::Spreadsheet,
        Some("ppt" | "pptx" | "odp") => UiIcon::Presentation,
        Some(
            "rs" | "c" | "h" | "cpp" | "hpp" | "ts" | "tsx" | "go" | "cs" | "toml" | "yaml" | "yml"
            | "json" | "xml" | "html" | "css",
        ) => UiIcon::Code,
        Some("md" | "txt" | "log" | "rtf") => UiIcon::Text,
        _ => UiIcon::Generic,
    }
}

impl OperationKind {
    fn title(self) -> &'static str {
        match self {
            Self::Create => "Create archive",
            Self::Add => "Add to archive",
            Self::Remove => "Remove entries",
            Self::Join => "Join commits",
            Self::Metadata => "Custom metadata",
            Self::Check => "Test archive",
            Self::List => "List archive",
            Self::Extract => "Extract",
        }
    }

    fn dialog_size_key(self) -> &'static str {
        match self {
            Self::Create => "operation-create",
            Self::Add => "operation-add",
            Self::Remove => "operation-remove",
            Self::Join => "operation-join",
            Self::Metadata => "operation-metadata",
            Self::Check => "operation-check",
            Self::List => "operation-list",
            Self::Extract => "operation-extract",
        }
    }

    fn needs_archive(self) -> bool {
        self != Self::Create
    }
}

#[derive(Default)]
struct OperationForm {
    kind: Option<OperationKind>,
    archive: String,
    input_paths: Vec<PathBuf>,
    selected_input_paths: BTreeSet<PathBuf>,
    input_selection_anchor: Option<PathBuf>,
    destination: String,
    from_id: String,
    through_id: String,
    join_output: String,
    // (key, original value, edited value, remove) of existing custom metadata.
    meta_items: Vec<(String, String, String, bool)>,
    meta_new: Vec<(String, String)>,
    block_size: String,
    compression_level: String,
    include_files: String,
    exclude_files: String,
    include_folders: String,
    exclude_folders: String,
    filelists: Vec<PathBuf>,
    password: String,
    password_confirm: String,
    existing_password: String,
    existing_password_is_set: bool,
    show_password: bool,
    aead: usize,
    encryption_scope: usize,
    strict: bool,
    metadata: bool,
    dates: bool,
    special_files: bool,
    overwrite: bool,
    recover: bool,
    verbose: bool,
    dry_run: bool,
    restore_symlinks: bool,
    restore_devices: bool,
    strip_top_level: bool,
    password_is_set: bool,
    collision: usize,
    existing: usize,
    through: String,
    commit_ids: Vec<String>,
    remove_text: String,
}

impl OperationForm {
    fn new(kind: OperationKind, archive: &str) -> Self {
        Self {
            kind: Some(kind),
            archive: archive.to_owned(),
            destination: ".".to_owned(),
            block_size: "8M".to_owned(),
            compression_level: "3".to_owned(),
            ..Self::default()
        }
    }

    fn filters(&self) -> Result<FilterSet, Box<dyn std::error::Error>> {
        FilterSet::from_patterns(
            &lines(&self.include_files),
            &lines(&self.exclude_files),
            &lines(&self.include_folders),
            &lines(&self.exclude_folders),
            &self.filelists,
        )
    }

    fn encoding(&self) -> Result<EncodingOptions, Box<dyn std::error::Error>> {
        let block_size = parse_block_size(&self.block_size)?;
        let compression_level: i32 = self
            .compression_level
            .trim()
            .parse()
            .map_err(|_| "compression level must be an integer from -7 through 22")?;
        if !(-7..=22).contains(&compression_level) {
            return Err("compression level must be from -7 through 22".into());
        }
        Ok(EncodingOptions {
            block_size,
            compression_level,
        })
    }
}

enum UiEvent {
    Message(String),
    Advance(u64),
    Total(u64),
    Warning(String),
    Confirm(String),
    Loaded(
        Result<
            (
                Toc,
                std::collections::BTreeMap<ArchivePath, ArchiveEntryDetails>,
            ),
            String,
        >,
    ),
    CommitIds(Vec<String>),
    Finished(Result<String, String>),
}

struct ArchiveInfoView {
    archive: String,
    commits: Vec<CommitInfo>,
    metadata: Vec<(String, String)>,
}

struct BenchmarkSettings {
    block_size: String,
    compression_level: i32,
    owner_permissions: bool,
    dates: bool,
    special_files: bool,
}

impl Default for BenchmarkSettings {
    fn default() -> Self {
        Self {
            block_size: "8M".to_owned(),
            compression_level: 3,
            owner_permissions: false,
            dates: false,
            special_files: true,
        }
    }
}

#[derive(Clone)]
struct BenchmarkResult {
    files: u64,
    uncompressed: u64,
    archive_size: u64,
    create_seconds: f64,
    extract_seconds: f64,
}

#[derive(Default)]
struct BenchmarkState {
    open: bool,
    settings: BenchmarkSettings,
    receiver: Option<Receiver<Result<BenchmarkResult, String>>>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    result: Option<Result<BenchmarkResult, String>>,
}

/// Removes the benchmark's scratch folder when dropped.
struct ScratchDir(PathBuf);

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn directory_stats(path: &std::path::Path) -> (u64, u64) {
    let mut files = 0;
    let mut bytes = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                files += 1;
                bytes += meta.len();
            }
        }
    }
    (files, bytes)
}

/// Unpacks the archive to a scratch folder, then times creating and extracting a
/// new archive from it with the given settings. Nothing outside the scratch folder is written.
fn run_benchmark(
    archive: &str,
    password: Option<Vec<u8>>,
    settings: &BenchmarkSettings,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<BenchmarkResult, String> {
    let check_cancel = || {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            Err("Benchmark cancelled.".to_owned())
        } else {
            Ok(())
        }
    };
    let error = |error: Box<dyn std::error::Error>| error.to_string();
    let encoding = EncodingOptions {
        block_size: parse_block_size(&settings.block_size).map_err(error)?,
        compression_level: settings.compression_level,
    };
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_nanos());
    let scratch =
        ScratchDir(std::env::temp_dir().join(format!("zarg-bench-{}-{nanos}", std::process::id())));
    let source = scratch.0.join("source");
    let restored = scratch.0.join("restored");
    std::fs::create_dir_all(&source).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&restored).map_err(|e| e.to_string())?;
    let extraction = |restore_metadata| ExtractionOptions {
        dry_run: false,
        through: None,
        collision_policy: CollisionPolicy::Rename,
        existing_policy: ExistingPolicy::Overwrite,
        restore_metadata,
        restore_symlinks: settings.special_files,
        restore_devices: false,
        strip_top_level: false,
    };
    operations::extract_archive_with_progress(
        archive,
        &source,
        extraction(false),
        password.as_deref(),
        None,
    )
    .map_err(error)?;
    check_cancel()?;
    let (files, uncompressed) = directory_stats(&source);
    let inputs: Vec<PathBuf> = std::fs::read_dir(&source)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    if inputs.is_empty() {
        return Err("The archive has no entries to benchmark.".to_owned());
    }
    let benchmark_archive = scratch.0.join("bench.zar");
    let benchmark_archive = benchmark_archive.to_string_lossy().into_owned();

    check_cancel()?;
    let started = std::time::Instant::now();
    operations::create_archive_with_password_progress_and_filters(
        &benchmark_archive,
        &inputs,
        false,
        encoding,
        None,
        None,
        CreateFilterOptions {
            input: CreateInputOptions {
                save_metadata: settings.owner_permissions,
                save_dates: settings.dates,
                include_special_files: settings.special_files,
                overwrite_existing: false,
                verbose: false,
            },
            filters: &FilterSet::default(),
        },
    )
    .map_err(error)?;
    let create_seconds = started.elapsed().as_secs_f64();
    let archive_size = std::fs::metadata(&benchmark_archive)
        .map_err(|e| e.to_string())?
        .len();

    check_cancel()?;
    let started = std::time::Instant::now();
    operations::extract_archive_with_progress(
        &benchmark_archive,
        &restored,
        extraction(settings.owner_permissions || settings.dates),
        None,
        None,
    )
    .map_err(error)?;
    let extract_seconds = started.elapsed().as_secs_f64();
    check_cancel()?;
    Ok(BenchmarkResult {
        files,
        uncompressed,
        archive_size,
        create_seconds,
        extract_seconds,
    })
}

fn throughput(bytes: u64, seconds: f64) -> String {
    if seconds <= 0.0 {
        return "n/a".to_owned();
    }
    format!("{}/s", display_size((bytes as f64 / seconds) as u64))
}

type OperationJob = Box<dyn FnOnce() -> Result<String, String> + Send>;
type OperationBuildResult = Result<OperationJob, Box<dyn std::error::Error>>;

struct UiProgress {
    events: Sender<UiEvent>,
    answers: Arc<Mutex<Receiver<bool>>>,
}

impl Progress for UiProgress {
    fn message(&self, message: &str) {
        let _ = self.events.send(UiEvent::Message(message.to_owned()));
    }

    fn advance(&self, amount: u64) {
        let _ = self.events.send(UiEvent::Advance(amount));
    }

    fn set_total(&self, total: u64) {
        let _ = self.events.send(UiEvent::Total(total));
    }

    fn warning(&self, warning: &str) {
        let _ = self.events.send(UiEvent::Warning(warning.to_owned()));
    }

    fn confirm(&self, prompt: &str) -> Result<bool, Box<dyn std::error::Error>> {
        self.events.send(UiEvent::Confirm(prompt.to_owned()))?;
        self.answers
            .lock()
            .map_err(|_| "confirmation channel poisoned")?
            .recv()
            .map_err(|_| "confirmation dialog closed".into())
    }
}

struct WorkerChannels {
    events: Receiver<UiEvent>,
    answers: Sender<bool>,
}

pub struct ZargApp {
    icons: RefCell<HashMap<UiIcon, egui::TextureHandle>>,
    archive: String,
    location: String,
    entries: Vec<(String, EntryData)>,
    entry_details: std::collections::BTreeMap<String, ArchiveEntryDetails>,
    visible_entry_indices: Vec<usize>,
    selected: BTreeSet<String>,
    selection_anchor: Option<String>,
    status: String,
    operation: OperationForm,
    worker: Option<WorkerChannels>,
    busy: bool,
    progress: f32,
    progress_total: u64,
    progress_done: u64,
    progress_known: bool,
    progress_message: String,
    confirmation: Option<String>,
    report: Option<String>,
    warnings: Vec<String>,
    filter_text: String,
    filter_regex: bool,
    filter_match_case: bool,
    filter_invalid: bool,
    archive_view: ArchiveView,
    sort_by: SortBy,
    sort_reversed: bool,
    pending_open: Option<String>,
    recent_archives: Vec<String>,
    recent_check: Option<Receiver<Vec<String>>>,
    info_loading: Option<Receiver<Result<ArchiveInfoView, String>>>,
    info_view: Option<ArchiveInfoView>,
    benchmark: BenchmarkState,
    pending_recent_archive: Option<String>,
    window_geometry: Option<crate::settings::WindowGeometry>,
    dialog_sizes: HashMap<String, [f32; 2]>,
    archive_password: Option<String>,
    column_widths: HashMap<String, f32>,
    has_entry_dates: bool,
    has_entry_owner: bool,
    has_entry_permissions: bool,
    expanded_tree_dirs: BTreeSet<String>,
    structured_rows_cache: Vec<StructuredRow>,
    structured_rows_dirty: bool,
    show_about: bool,
    opening_archive: bool,
    commit_ids: Vec<String>,
    file_dialog: crate::file_dialog::FileDialog,
}

impl Default for ZargApp {
    fn default() -> Self {
        Self {
            icons: RefCell::new(HashMap::new()),
            archive: String::new(),
            location: String::new(),
            entries: Vec::new(),
            entry_details: std::collections::BTreeMap::new(),
            visible_entry_indices: Vec::new(),
            selected: BTreeSet::new(),
            selection_anchor: None,
            status: String::new(),
            operation: OperationForm::default(),
            worker: None,
            busy: false,
            progress: 0.0,
            progress_total: 0,
            progress_done: 0,
            progress_known: false,
            progress_message: String::new(),
            confirmation: None,
            report: None,
            warnings: Vec::new(),
            filter_text: String::new(),
            filter_regex: false,
            filter_match_case: false,
            filter_invalid: false,
            archive_view: ArchiveView::default(),
            sort_by: SortBy::default(),
            sort_reversed: false,
            pending_open: None,
            recent_archives: Vec::new(),
            recent_check: None,
            info_loading: None,
            info_view: None,
            benchmark: BenchmarkState::default(),
            pending_recent_archive: None,
            window_geometry: None,
            dialog_sizes: HashMap::new(),
            archive_password: None,
            column_widths: HashMap::new(),
            has_entry_dates: false,
            has_entry_owner: false,
            has_entry_permissions: false,
            expanded_tree_dirs: BTreeSet::new(),
            structured_rows_cache: Vec::new(),
            structured_rows_dirty: false,
            show_about: false,
            opening_archive: false,
            commit_ids: Vec::new(),
            file_dialog: crate::file_dialog::FileDialog::new(),
        }
    }
}

#[derive(Clone)]
enum EntryData {
    Directory,
    File(u64),
    Symlink(String),
    NamedPipe,
    Device(String),
}

impl ZargApp {
    pub fn from_context_action(action: ContextAction, paths: Vec<PathBuf>) -> Result<Self, String> {
        let mut app = Self::default();
        match action {
            ContextAction::Create | ContextAction::Add | ContextAction::CreateNamed => {
                if paths.is_empty() {
                    return Err("Select one or more files or folders first.".to_owned());
                }
                if let Some(path) = paths.iter().find(|path| !path.exists()) {
                    return Err(format!("Selected path does not exist: {}", path.display()));
                }
                if action == ContextAction::CreateNamed {
                    let archive = named_archive_path(&paths[0])?;
                    let mut form =
                        OperationForm::new(OperationKind::Create, &archive.to_string_lossy());
                    form.input_paths = paths;
                    app.start_operation(form);
                } else {
                    let archive = if action == ContextAction::Create {
                        named_archive_path(&paths[0])?
                            .to_string_lossy()
                            .into_owned()
                    } else {
                        String::new()
                    };
                    app.operation = OperationForm::new(OperationKind::Create, &archive);
                    app.operation.input_paths = paths;
                    if action == ContextAction::Add {
                        app.status = "Choose a new archive destination.".to_owned();
                    }
                }
            }
            ContextAction::Open
            | ContextAction::ExtractHere
            | ContextAction::ExtractTo
            | ContextAction::ExtractToFolder
            | ContextAction::Test => {
                let archive = paths
                    .first()
                    .filter(|path| path.is_file())
                    .ok_or_else(|| "Select a valid archive file first.".to_owned())?;
                let archive = archive.to_string_lossy().into_owned();
                match action {
                    ContextAction::Open => app.open_archive(PathBuf::from(&archive), None),
                    ContextAction::ExtractHere => {
                        let mut form = OperationForm::new(OperationKind::Extract, &archive);
                        form.destination = PathBuf::from(&archive)
                            .parent()
                            .unwrap_or_else(|| std::path::Path::new("."))
                            .to_string_lossy()
                            .into_owned();
                        app.start_operation(form);
                    }
                    ContextAction::ExtractTo => {
                        app.operation = OperationForm::new(OperationKind::Extract, &archive);
                        app.operation.destination = PathBuf::from(&archive)
                            .parent()
                            .unwrap_or_else(|| std::path::Path::new("."))
                            .to_string_lossy()
                            .into_owned();
                    }
                    ContextAction::ExtractToFolder => {
                        let destination =
                            archive_folder_destination(PathBuf::from(&archive).as_path())?;
                        let mut form = OperationForm::new(OperationKind::Extract, &archive);
                        form.destination = destination.to_string_lossy().into_owned();
                        app.start_operation(form);
                    }
                    ContextAction::Test => {
                        app.start_operation(OperationForm::new(OperationKind::Check, &archive));
                    }
                    ContextAction::Create | ContextAction::Add | ContextAction::CreateNamed => {
                        unreachable!()
                    }
                }
            }
        }
        Ok(app)
    }

    fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("ZAR archive", &["zar"])
            .pick_file()
        {
            self.open_archive(path, None);
        }
    }

    fn open_archive(&mut self, path: PathBuf, password: Option<String>) {
        let archive = path.to_string_lossy().into_owned();
        self.commit_ids.clear();
        let opened_archive = archive.clone();
        self.pending_recent_archive = Some(archive.clone());
        self.archive.clone_from(&archive);
        self.location.clone_from(&archive);
        self.selected.clear();
        self.selection_anchor = None;
        self.report = None;
        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();
        drop(answer_rx);
        let password = password.unwrap_or_default();
        self.archive_password = (!password.is_empty()).then(|| password.clone());
        self.opening_archive = true;
        self.begin_worker(events_rx, answer_tx, "Reading archive");
        let event_sender = events_tx.clone();
        std::thread::spawn(move || {
            if let Ok(commits) = operations::info_archive(&archive, false) {
                let ids = commits
                    .iter()
                    .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                    .collect();
                let _ = event_sender.send(UiEvent::CommitIds(ids));
            }
            let result = operations::list_archive_with_details(
                &archive,
                None,
                (!password.is_empty()).then_some(password.as_bytes()),
            )
            .map_err(|error| error.to_string());
            match result {
                Ok(toc) => {
                    let count = toc.0.len();
                    let _ = events_tx.send(UiEvent::Loaded(Ok(toc)));
                    finish_with_events(events_tx, Ok(format!("{count} entries loaded")));
                }
                Err(error) => {
                    let _ = events_tx.send(UiEvent::Loaded(Err(error.clone())));
                    finish_with_events(events_tx, Err(error));
                }
            }
        });
        self.status = format!("Opened {opened_archive}");
    }

    fn begin_worker(&mut self, events: Receiver<UiEvent>, answers: Sender<bool>, message: &str) {
        self.busy = true;
        self.progress = 0.0;
        self.progress_done = 0;
        self.progress_total = 0;
        self.progress_known = false;
        self.progress_message = message.to_owned();
        self.worker = Some(WorkerChannels { events, answers });
    }

    fn start_operation(&mut self, form: OperationForm) {
        let Some(kind) = form.kind else {
            return;
        };
        if (kind.needs_archive() || kind == OperationKind::Create) && form.archive.trim().is_empty()
        {
            self.status = "Choose an archive first.".to_owned();
            return;
        }
        if matches!(kind, OperationKind::Create | OperationKind::Add) && form.password_is_set {
            let missing = password_requirements(&form.password);
            if !missing.is_empty() {
                self.status = format!("Encryption password is missing: {}.", missing.join(", "));
                return;
            }
            if form.password != form.password_confirm {
                self.status = "The encryption passwords do not match.".to_owned();
                return;
            }
        }
        if kind == OperationKind::Add
            && form.existing_password_is_set
            && form.existing_password.is_empty()
        {
            self.status = "Enter the password for existing commits.".to_owned();
            return;
        }

        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();
        let progress = UiProgress {
            events: events_tx.clone(),
            answers: Arc::new(Mutex::new(answer_rx)),
        };
        let archive = form.archive.clone();
        let refresh_password = (kind == OperationKind::Add && form.existing_password_is_set)
            .then(|| form.existing_password.clone())
            .or_else(|| form.password_is_set.then(|| form.password.clone()));
        if let Some(password) = &refresh_password {
            self.archive_password = Some(password.clone());
        }
        let refresh_through = (kind == OperationKind::List && !form.through.trim().is_empty())
            .then(|| form.through.clone());
        let result = match build_job(kind, form, events_tx.clone(), progress) {
            Ok(job) => job,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        self.begin_worker(events_rx, answer_tx, kind.title());
        self.report = None;
        self.warnings.clear();
        std::thread::spawn(move || {
            let mut outcome = result();
            if outcome.is_ok() {
                match operations::list_archive_with_details(
                    &archive,
                    refresh_through.as_deref(),
                    refresh_password.as_deref().map(str::as_bytes),
                ) {
                    Ok(toc) => {
                        let _ = events_tx.send(UiEvent::Loaded(Ok(toc)));
                    }
                    Err(error) => {
                        outcome = Err(format!(
                            "Operation completed, but archive refresh failed: {error}"
                        ));
                    }
                }
            }
            finish_with_events(events_tx, outcome);
        });
        self.operation = OperationForm::default();
        self.status = format!("{} in progress", kind.title());
    }

    fn open_entry_externally(&mut self, path: String) {
        let is_file = self
            .entries
            .iter()
            .any(|(entry, data)| *entry == path && matches!(data, EntryData::File(_)));
        if !is_file || self.busy {
            return;
        }
        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, _answer_rx) = mpsc::channel();
        self.begin_worker(events_rx, answer_tx, "Extracting file");
        self.status = format!("Opening {path}");
        self.warnings.clear();
        let archive = self.archive.clone();
        let password = self.archive_password.clone();
        std::thread::spawn(move || {
            let outcome = extract_and_open(&archive, &path, password.as_deref());
            finish_with_events(events_tx, outcome);
        });
    }

    fn poll_worker(&mut self, context: &egui::Context) {
        let Some(worker) = &self.worker else {
            return;
        };
        let (events, disconnected): (Vec<UiEvent>, bool) = {
            let mut events = Vec::new();
            let disconnected = loop {
                match worker.events.try_recv() {
                    Ok(event) => events.push(event),
                    Err(mpsc::TryRecvError::Empty) => break false,
                    Err(mpsc::TryRecvError::Disconnected) => break true,
                }
            };
            (events, disconnected)
        };
        for event in events {
            match event {
                UiEvent::Message(message) => self.progress_message = message,
                UiEvent::Advance(amount) => {
                    self.progress_done = self.progress_done.saturating_add(amount);
                    self.progress = if self.progress_total == 0 {
                        0.0
                    } else {
                        (self.progress_done as f32 / self.progress_total as f32).min(1.0)
                    };
                }
                UiEvent::Total(total) => {
                    self.progress_known = total > 0;
                    self.progress_total = total;
                    self.progress_done = 0;
                    self.progress = 0.0;
                }
                UiEvent::Warning(warning) => {
                    self.warnings.push(warning);
                }
                UiEvent::Confirm(prompt) => self.confirmation = Some(prompt),
                UiEvent::CommitIds(ids) => self.commit_ids = ids,
                UiEvent::Loaded(result) => match result {
                    Ok((toc, details)) => {
                        self.opening_archive = false;
                        if let Some(archive) = self.pending_recent_archive.take() {
                            remember_recent_archive(&mut self.recent_archives, &archive);
                        }
                        self.set_entries(toc, details);
                    }
                    Err(error) => {
                        self.status = error.clone();
                        if self.opening_archive && error.contains("password") {
                            self.operation = OperationForm::new(OperationKind::List, &self.archive);
                            self.operation.commit_ids.clone_from(&self.commit_ids);
                            self.operation.through =
                                self.commit_ids.last().cloned().unwrap_or_default();
                            self.operation.password_is_set = true;
                        } else {
                            self.pending_recent_archive = None;
                        }
                        self.opening_archive = false;
                    }
                },
                UiEvent::Finished(result) => {
                    self.busy = false;
                    self.worker = None;
                    match result {
                        Ok(summary) => {
                            self.status = summary.clone();
                        }
                        Err(error) => {
                            self.status = error.clone();
                            self.report = Some(error);
                        }
                    }
                    context.request_repaint();
                    return;
                }
            }
        }
        if disconnected {
            self.busy = false;
            self.worker = None;
            self.status = "Operation worker stopped unexpectedly.".to_owned();
            self.report = Some(self.status.clone());
        }
        context.request_repaint_after(std::time::Duration::from_millis(100));
    }

    fn set_entries(
        &mut self,
        toc: Toc,
        details: std::collections::BTreeMap<ArchivePath, ArchiveEntryDetails>,
    ) {
        self.entry_details = details
            .into_iter()
            .map(|(path, details)| (path.as_str().to_owned(), details))
            .collect();
        self.entries = toc
            .iter()
            .map(|(path, entry)| {
                let path_string = path.as_str().to_owned();
                let data = match entry {
                    TocEntry::Directory => EntryData::Directory,
                    TocEntry::File(file) => EntryData::File(file.size),
                    TocEntry::Symlink { target } => EntryData::Symlink(target.clone()),
                    TocEntry::NamedPipe => EntryData::NamedPipe,
                    TocEntry::Device { kind, major, minor } => {
                        EntryData::Device(format!("{kind:?} {major}:{minor}"))
                    }
                };
                (path_string, data)
            })
            .collect();
        self.has_entry_owner = self
            .entry_details
            .values()
            .any(|details| details.owner.is_some());
        self.has_entry_permissions = self
            .entry_details
            .values()
            .any(|details| details.permissions.is_some());
        self.has_entry_dates = self
            .entry_details
            .values()
            .any(|details| details.modified_unix_millis.is_some());
        self.entries.sort_by(|left, right| left.0.cmp(&right.0));
        self.rebuild_visible_entry_indices();
        self.structured_rows_dirty = true;
    }

    fn rebuild_visible_entry_indices(&mut self) {
        let matcher =
            EntryFilter::new(&self.filter_text, self.filter_regex, self.filter_match_case);
        self.filter_invalid = matcher.is_err();
        self.visible_entry_indices =
            filtered_entry_indices(&self.entries, &matcher.unwrap_or(EntryFilter::All));
        sort_entry_indices(
            &mut self.visible_entry_indices,
            &self.entries,
            &self.entry_details,
            self.sort_by,
        );
        if self.sort_reversed {
            self.visible_entry_indices.reverse();
        }
    }

    fn apply_sort_choice(&mut self, sort_by: SortBy) {
        if self.sort_by == sort_by {
            self.sort_reversed = !self.sort_reversed;
        } else {
            self.sort_by = sort_by;
            self.sort_reversed = false;
        }
        self.rebuild_visible_entry_indices();
        self.structured_rows_dirty = true;
    }

    fn column_widths_for(&self, view: &str, labels: &[&str], defaults: &[f32]) -> Vec<f32> {
        labels
            .iter()
            .zip(defaults)
            .map(|(label, default)| {
                self.column_widths
                    .get(&format!("{view}:{label}"))
                    .copied()
                    .unwrap_or(*default)
            })
            .collect()
    }

    fn store_column_widths(&mut self, view: &str, labels: &[&str], widths: &[f32]) {
        for (label, width) in labels.iter().zip(widths) {
            self.column_widths.insert(format!("{view}:{label}"), *width);
        }
    }

    fn choose_input_paths(&mut self) {
        self.file_dialog.open_for_selection();
    }

    fn choose_archive(&mut self, save: bool) {
        let dialog = rfd::FileDialog::new().add_filter("ZAR archive", &["zar"]);
        let path = if save {
            dialog.save_file()
        } else {
            dialog.pick_file()
        };
        if let Some(path) = path {
            self.operation.archive = path.to_string_lossy().into_owned();
        }
    }

    fn render_menus(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            icon_menu_button(
                ui,
                icon_texture(ui.ctx(), &self.icons, UiIcon::OpenArchive),
                "File",
                |ui| {
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::OpenArchive,
                        "Open archive...",
                        true,
                    )
                    .clicked()
                    {
                        self.open_dialog();
                        ui.close();
                    }
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::CreateArchive,
                        "Create archive...",
                        true,
                    )
                    .clicked()
                    {
                        self.show_operation(OperationKind::Create);
                        ui.close();
                    }
                    ui.separator();
                    ui.menu_button("Recent archives", |ui| {
                        if self.recent_archives.is_empty() {
                            ui.label("No recent archives");
                        }
                        for archive in self.recent_archives.clone() {
                            if icon_button(
                                ui,
                                &self.icons,
                                UiIcon::OpenArchive,
                                &archive,
                                !self.busy,
                            )
                            .clicked()
                            {
                                self.open_archive(PathBuf::from(&archive), None);
                                ui.close();
                            }
                        }
                    });
                    ui.separator();
                    if icon_button(ui, &self.icons, UiIcon::Quit, "Quit", !self.busy).clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                },
            );
            icon_menu_button(
                ui,
                icon_texture(ui.ctx(), &self.icons, UiIcon::SelectAll),
                "Edit",
                |ui| {
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::SelectAll,
                        "Select all",
                        !self.selected.is_empty() && !self.busy,
                    )
                    .clicked()
                    {
                        self.selected
                            .extend(self.entries.iter().map(|(path, _)| path.clone()));
                        self.selection_anchor = self.entries.first().map(|(path, _)| path.clone());
                        ui.close();
                    }
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::SelectAll,
                        "Invert selection",
                        !self.entries.is_empty() && !self.busy,
                    )
                    .clicked()
                    {
                        let visible = self
                            .visible_entry_indices
                            .iter()
                            .map(|index| self.entries[*index].0.clone())
                            .collect::<Vec<_>>();
                        self.selected = invert_selection(&self.selected, &visible);
                        self.selection_anchor = None;
                        ui.close();
                    }
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::ClearSelection,
                        "Clear selection",
                        true,
                    )
                    .clicked()
                    {
                        self.selected.clear();
                        self.selection_anchor = None;
                        ui.close();
                    }
                },
            );
            icon_menu_button(
                ui,
                icon_texture(ui.ctx(), &self.icons, UiIcon::Refresh),
                "View",
                |ui| {
                    for (view, label) in [
                        (ArchiveView::Normal, "Normal"),
                        (ArchiveView::Compact, "Compact"),
                        (ArchiveView::Details, "Details"),
                        (ArchiveView::Structured, "Structured"),
                    ] {
                        if ui
                            .selectable_value(&mut self.archive_view, view, label)
                            .clicked()
                        {
                            ui.close();
                        }
                    }
                    ui.separator();
                    ui.label("Sort by");
                    for (sort_by, label) in [
                        (SortBy::Name, "Name"),
                        (SortBy::Date, "Date"),
                        (SortBy::CompressedSize, "Compressed size"),
                        (SortBy::UncompressedSize, "Uncompressed size"),
                        (SortBy::Ratio, "Ratio"),
                        (SortBy::Type, "Type"),
                    ] {
                        if ui
                            .selectable_label(self.sort_by == sort_by, label)
                            .clicked()
                        {
                            self.sort_reversed = false;
                            self.sort_by = sort_by;
                            self.rebuild_visible_entry_indices();
                            self.structured_rows_dirty = true;
                            ui.close();
                        }
                    }
                    ui.separator();
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::Refresh,
                        "Refresh",
                        !self.archive.is_empty(),
                    )
                    .clicked()
                    {
                        self.open_archive(PathBuf::from(&self.archive), None);
                        ui.close();
                    }
                },
            );
            icon_menu_button(
                ui,
                icon_texture(ui.ctx(), &self.icons, UiIcon::TestArchive),
                "Tools",
                |ui| {
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::TestArchive,
                        "Test archive",
                        !self.busy,
                    )
                    .clicked()
                    {
                        self.show_operation(OperationKind::Check);
                        ui.close();
                    }
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::Info,
                        "Custom metadata...",
                        !self.busy,
                    )
                    .clicked()
                    {
                        self.show_operation(OperationKind::Metadata);
                        ui.close();
                    }
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::TestArchive,
                        "Benchmark...",
                        !self.archive.is_empty(),
                    )
                    .clicked()
                    {
                        self.benchmark.open = true;
                        ui.close();
                    }
                    if icon_button(ui, &self.icons, UiIcon::Join, "Join commits...", !self.busy)
                        .clicked()
                    {
                        self.show_operation(OperationKind::Join);
                        ui.close();
                    }
                },
            );
            icon_menu_button(
                ui,
                icon_texture(ui.ctx(), &self.icons, UiIcon::About),
                "Help",
                |ui| {
                    if ui.button("Online Help").clicked() {
                        ui.ctx()
                            .open_url(egui::OpenUrl::new_tab("https://github.com/brmmm3/zar-rs"));
                        ui.close();
                    }
                    if icon_button(ui, &self.icons, UiIcon::About, "About zarg", true).clicked() {
                        self.show_about = true;
                        ui.close();
                    }
                },
            );
        });
    }

    fn show_operation(&mut self, kind: OperationKind) {
        let archive = if kind == OperationKind::Create {
            String::new()
        } else {
            self.archive.clone()
        };
        self.operation = OperationForm::new(kind, &archive);
        if matches!(kind, OperationKind::List | OperationKind::Extract) {
            self.operation.commit_ids.clone_from(&self.commit_ids);
            self.operation.through = self.commit_ids.last().cloned().unwrap_or_default();
        }
        if kind == OperationKind::Metadata {
            match operations::read_custom_metadata(&archive) {
                Ok(items) => {
                    self.operation.meta_items = items
                        .iter()
                        .map(|(key, value)| {
                            let text = operations::format_metadata_value(value);
                            (key.clone(), text.clone(), text, false)
                        })
                        .collect();
                }
                Err(error) => self.status = error.to_string(),
            }
            self.operation.meta_new.push(Default::default());
        }
        if kind == OperationKind::Join {
            self.operation.commit_ids.clone_from(&self.commit_ids);
            self.operation.from_id = self.commit_ids.first().cloned().unwrap_or_default();
            self.operation.through_id = self.commit_ids.last().cloned().unwrap_or_default();
        }
    }

    fn render_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if icon_button(ui, &self.icons, UiIcon::CreateArchive, "Create", !self.busy).clicked() {
                self.show_operation(OperationKind::Create);
            }
            for (label, icon, kind) in [
                ("Add", UiIcon::Add, OperationKind::Add),
                ("Extract", UiIcon::Extract, OperationKind::Extract),
                ("Test", UiIcon::TestArchive, OperationKind::Check),
            ] {
                if icon_button(
                    ui,
                    &self.icons,
                    icon,
                    label,
                    !self.busy && !self.archive.is_empty(),
                )
                .clicked()
                {
                    self.show_operation(kind);
                }
            }
            if icon_button(
                ui,
                &self.icons,
                UiIcon::Info,
                "Info",
                !self.busy && !self.archive.is_empty(),
            )
            .clicked()
            {
                self.show_archive_info();
            }
            if icon_button(
                ui,
                &self.icons,
                UiIcon::Remove,
                "Remove",
                !self.busy && !self.archive.is_empty() && !self.selected.is_empty(),
            )
            .clicked()
            {
                let mut form = OperationForm::new(OperationKind::Remove, &self.archive);
                form.remove_text = self.selected.iter().cloned().collect::<Vec<_>>().join("\n");
                self.start_operation(form);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let case_toggle = ui
                    .selectable_label(self.filter_match_case, "Aa")
                    .on_hover_text("Match upper and lower case");
                let mut changed = case_toggle.clicked();
                if changed {
                    self.filter_match_case = !self.filter_match_case;
                }
                let regex_toggle = ui.selectable_label(self.filter_regex, ".*").on_hover_text(
                    "Use a regular expression instead of a wildcard (fnmatch) pattern",
                );
                if regex_toggle.clicked() {
                    self.filter_regex = !self.filter_regex;
                    changed = true;
                }
                let text_color = self
                    .filter_invalid
                    .then_some(egui::Color32::from_rgb(230, 80, 80));
                let edit = egui::TextEdit::singleline(&mut self.filter_text)
                    .hint_text("Filter: *.txt or text")
                    .desired_width(ui.available_width().min(260.0))
                    .text_color_opt(text_color);
                changed |= ui.add(edit).changed();
                if changed {
                    self.rebuild_visible_entry_indices();
                    self.structured_rows_dirty = true;
                }
            });
        });
    }

    fn dialog_size(&self, key: &str, default: [f32; 2]) -> [f32; 2] {
        self.dialog_sizes.get(key).copied().unwrap_or(default)
    }

    fn record_dialog_size(&mut self, key: &str, size: Option<[f32; 2]>) {
        if let Some(size) = size.filter(|size| {
            size.iter()
                .all(|dimension| dimension.is_finite() && *dimension >= 100.0)
        }) {
            self.dialog_sizes.insert(key.to_owned(), size);
        }
    }

    fn viewport_inner_size(context: &egui::Context) -> Option<[f32; 2]> {
        context.input(|input| {
            input
                .viewport()
                .inner_rect
                .map(|rect| [rect.width(), rect.height()])
        })
    }

    fn centered_dialog_viewport_builder(
        context: &egui::Context,
        title: &str,
        inner_size: [f32; 2],
        resizable: bool,
    ) -> egui::ViewportBuilder {
        let builder = egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size(inner_size)
            .with_resizable(resizable);
        context
            .input(|input| input.viewport().outer_rect)
            .map(|parent_rect| {
                let size = egui::vec2(inner_size[0], inner_size[1]);
                builder
                    .clone()
                    .with_position(parent_rect.center() - size * 0.5)
            })
            .unwrap_or(builder)
    }

    fn render_operation_dialog(&mut self, context: &egui::Context) {
        let Some(kind) = self.operation.kind else {
            return;
        };
        if kind == OperationKind::Remove {
            return;
        }
        let viewport_id =
            egui::ViewportId::from_hash_of(format!("operation-dialog:{}", kind.title()));
        let mut open = true;
        let default_size = if kind == OperationKind::Create {
            [568.0, 330.0]
        } else if kind == OperationKind::Add {
            [568.0, 360.0]
        } else if kind == OperationKind::Check {
            [440.0, 105.0]
        } else if kind == OperationKind::Metadata {
            [440.0, 180.0]
        } else if kind == OperationKind::Join {
            [440.0, 130.0]
        } else if kind == OperationKind::Extract {
            [560.0, 310.0]
        } else {
            [500.0, 500.0]
        };
        let size_key = kind.dialog_size_key();
        let size = self.dialog_size(size_key, default_size);
        let mut actual_size = None;
        context.show_viewport_immediate(
            viewport_id,
            Self::centered_dialog_viewport_builder(context, kind.title(), size, true),
            |ui, _class| {
                actual_size = Self::viewport_inner_size(ui.ctx());
                if ui.ctx().input(|input| input.viewport().close_requested()) {
                    open = false;
                }
                // Reserve room for the archive controls, option groups, and action buttons.
                let input_list_height = if kind == OperationKind::Create {
                    (ui.available_height() - 152.0).clamp(180.0, 400.0)
                } else {
                    180.0
                };
                let fields_scroll = egui::ScrollArea::vertical().max_height(560.0);
                let fields_scroll = if kind == OperationKind::Create {
                    fields_scroll.auto_shrink([false, false])
                } else {
                    fields_scroll
                };
                fields_scroll.show(ui, |ui| {
                    self.render_operation_fields(ui, kind, input_list_height);
                });
                if !open || self.operation.kind.is_none() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            },
        );
        self.record_dialog_size(size_key, actual_size);
        if !open && self.operation.kind == Some(kind) {
            self.operation = OperationForm::default();
        }
    }

    fn render_operation_fields(
        &mut self,
        ui: &mut egui::Ui,
        kind: OperationKind,
        input_list_height: f32,
    ) {
        if kind.needs_archive() || kind == OperationKind::Create {
            ui.horizontal(|ui| {
                ui.label("Archive:");
                if matches!(
                    kind,
                    OperationKind::Create
                        | OperationKind::Add
                        | OperationKind::Check
                        | OperationKind::Extract
                        | OperationKind::Join
                ) {
                    ui.allocate_ui_with_layout(
                        ui.available_size(),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if icon_button(ui, &self.icons, UiIcon::Browse, "Browse...", true)
                                .clicked()
                            {
                                self.choose_archive(kind == OperationKind::Create);
                            }
                            ui.add(
                                egui::TextEdit::singleline(&mut self.operation.archive)
                                    .desired_width(ui.available_width()),
                            );
                        },
                    );
                } else {
                    ui.text_edit_singleline(&mut self.operation.archive);
                    if icon_button(ui, &self.icons, UiIcon::Browse, "Browse...", true).clicked() {
                        self.choose_archive(kind == OperationKind::Create);
                    }
                }
            });
        }

        match kind {
            OperationKind::Create | OperationKind::Add => {
                ui.horizontal(|ui| {
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::AddFolder,
                        "Add folders/files",
                        true,
                    )
                    .clicked()
                    {
                        self.choose_input_paths();
                    }
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::Remove,
                        "Remove items",
                        !self.operation.selected_input_paths.is_empty(),
                    )
                    .clicked()
                    {
                        self.operation
                            .input_paths
                            .retain(|path| !self.operation.selected_input_paths.contains(path));
                        self.operation.selected_input_paths.clear();
                        self.operation.input_selection_anchor = None;
                    }
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::ClearInputs,
                        "Clear list",
                        !self.operation.input_paths.is_empty(),
                    )
                    .clicked()
                    {
                        self.operation.input_paths.clear();
                        self.operation.selected_input_paths.clear();
                        self.operation.input_selection_anchor = None;
                    }
                });
                egui::ScrollArea::vertical()
                    .id_salt("create-add-input-paths")
                    .max_height(input_list_height)
                    .min_scrolled_height(input_list_height)
                    .show(ui, |ui| {
                        ui.set_min_height(input_list_height);
                        ui.set_min_width(ui.available_width());
                        for path in &self.operation.input_paths {
                            let path_text = path.display().to_string();
                            let icon_kind = if path.is_dir() {
                                UiIcon::Folder
                            } else {
                                file_icon(&path_text)
                            };
                            let icon = icon_texture(ui.ctx(), &self.icons, icon_kind);
                            ui.horizontal(|ui| {
                                let response = ui.add(
                                    egui::Image::new(&icon)
                                        .fit_to_exact_size(egui::vec2(16.0, 16.0)),
                                );
                                let selected = self.operation.selected_input_paths.contains(path);
                                let response =
                                    response.union(ui.selectable_label(selected, path_text));
                                if response.clicked() {
                                    let modifiers = ui.input(|input| input.modifiers);
                                    let selection = extended_selection(
                                        &self.operation.input_paths,
                                        &self.operation.selected_input_paths,
                                        &self.operation.input_selection_anchor,
                                        path,
                                        modifiers,
                                    );
                                    self.operation.selected_input_paths = selection.selected;
                                    self.operation.input_selection_anchor = selection.anchor;
                                }
                            });
                        }
                    });
                ui.collapsing("Compression and archive options", |ui| {
                    ui.label("Format: ZAR (Zstandard-compressed blocks)");
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_label("Block size")
                            .selected_text(format!(
                                "{} MB",
                                self.operation.block_size.trim_end_matches(['M', 'm'])
                            ))
                            .show_ui(ui, |ui| {
                                for size in ["1M", "2M", "4M", "8M", "16M"] {
                                    ui.selectable_value(
                                        &mut self.operation.block_size,
                                        size.to_owned(),
                                        format!("{} MB", size.trim_end_matches('M')),
                                    );
                                }
                            });
                        egui::ComboBox::from_label("Compression level")
                            .selected_text(&self.operation.compression_level)
                            .show_ui(ui, |ui| {
                                for level in -7..=22 {
                                    let level = level.to_string();
                                    ui.selectable_value(
                                        &mut self.operation.compression_level,
                                        level.clone(),
                                        level,
                                    );
                                }
                            });
                        ui.label("-7 (fastest) to 22 (smallest)");
                    });
                    ui.checkbox(
                        &mut self.operation.metadata,
                        "Preserve platform-specific metadata",
                    );
                    ui.checkbox(&mut self.operation.dates, "Save modification dates");
                    ui.checkbox(
                        &mut self.operation.special_files,
                        "Include named pipes and device nodes",
                    );
                    ui.checkbox(&mut self.operation.strict, "Strict input validation");
                    ui.checkbox(&mut self.operation.verbose, "Show symbolic-link warnings");
                    if kind == OperationKind::Create {
                        ui.checkbox(
                            &mut self.operation.overwrite,
                            "Replace archive if it already exists",
                        );
                    } else {
                        ui.label(
                            "Update mode: add or replace files; earlier commits are retained.",
                        );
                    }
                });
                if kind == OperationKind::Add {
                    ui.collapsing("Unlock existing archive", |ui| {
                        ui.checkbox(
                            &mut self.operation.existing_password_is_set,
                            "Existing commits require a password",
                        );
                        if self.operation.existing_password_is_set {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.operation.existing_password)
                                    .password(!self.operation.show_password)
                                    .hint_text("Password for existing archive commits"),
                            );
                        }
                    });
                }
                ui.collapsing("Encryption", |ui| {
                    ui.checkbox(
                        &mut self.operation.password_is_set,
                        "Encrypt this archive / new commit",
                    );
                    if self.operation.password_is_set {
                        egui::ComboBox::from_label("Encryption algorithm")
                            .selected_text(
                                ["AES-256-GCM", "ChaCha20-Poly1305"][self.operation.aead.min(1)],
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.operation.aead, 0, "AES-256-GCM");
                                ui.selectable_value(
                                    &mut self.operation.aead,
                                    1,
                                    "ChaCha20-Poly1305",
                                );
                            });
                        egui::ComboBox::from_label("Encryption scope")
                            .selected_text(
                                ["Header and data", "Data only", "Header only"]
                                    [self.operation.encryption_scope.min(2)],
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.operation.encryption_scope,
                                    0,
                                    "Header and data",
                                );
                                ui.selectable_value(
                                    &mut self.operation.encryption_scope,
                                    1,
                                    "Data only",
                                );
                                ui.selectable_value(
                                    &mut self.operation.encryption_scope,
                                    2,
                                    "Header only",
                                );
                            });
                        ui.add(
                            egui::TextEdit::singleline(&mut self.operation.password)
                                .password(!self.operation.show_password)
                                .hint_text("Enter password"),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.operation.password_confirm)
                                .password(!self.operation.show_password)
                                .hint_text("Re-enter password"),
                        );
                        ui.checkbox(&mut self.operation.show_password, "Show password");
                        let missing = password_requirements(&self.operation.password);
                        if missing.is_empty() {
                            ui.colored_label(
                                egui::Color32::GREEN,
                                "Password meets the minimum requirements.",
                            );
                        } else {
                            ui.label("Password must have:");
                            for requirement in missing {
                                ui.colored_label(egui::Color32::YELLOW, format!("• {requirement}"));
                            }
                        }
                        if !self.operation.password_confirm.is_empty()
                            && self.operation.password != self.operation.password_confirm
                        {
                            ui.colored_label(egui::Color32::RED, "Passwords do not match.");
                        }
                    }
                });
                self.render_filter_fields(ui);
            }
            OperationKind::Remove => return,
            OperationKind::Join => {
                ui.horizontal(|ui| {
                    let form = &mut self.operation;
                    let position = |ids: &[String], id: &str| ids.iter().position(|c| c == id);
                    egui::ComboBox::from_label("From commit")
                        .selected_text(&form.from_id)
                        .show_ui(ui, |ui| {
                            for id in &form.commit_ids {
                                ui.selectable_value(&mut form.from_id, id.clone(), id);
                            }
                        });
                    egui::ComboBox::from_label("Through")
                        .selected_text(&form.through_id)
                        .show_ui(ui, |ui| {
                            let min = position(&form.commit_ids, &form.from_id).unwrap_or(0);
                            for id in form.commit_ids.iter().skip(min) {
                                ui.selectable_value(&mut form.through_id, id.clone(), id);
                            }
                        });
                    // Keep "through" from pointing before "from".
                    if let (Some(from), Some(through)) = (
                        position(&form.commit_ids, &form.from_id),
                        position(&form.commit_ids, &form.through_id),
                    ) && through < from
                    {
                        form.through_id = form.from_id.clone();
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Write to:");
                    ui.allocate_ui_with_layout(
                        ui.available_size(),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if icon_button(ui, &self.icons, UiIcon::Browse, "Browse...", true)
                                .clicked()
                                && let Some(path) = rfd::FileDialog::new().save_file()
                            {
                                self.operation.join_output = path.to_string_lossy().into_owned();
                            }
                            ui.add(
                                egui::TextEdit::singleline(&mut self.operation.join_output)
                                    .desired_width(ui.available_width())
                                    .hint_text("Optional; empty replaces the current archive"),
                            );
                        },
                    );
                });
                ui.horizontal(|ui| {
                    egui::ComboBox::from_label("Block size")
                        .selected_text(format!(
                            "{} MB",
                            self.operation.block_size.trim_end_matches(['M', 'm'])
                        ))
                        .show_ui(ui, |ui| {
                            for size in ["1M", "2M", "4M", "8M", "16M"] {
                                ui.selectable_value(
                                    &mut self.operation.block_size,
                                    size.to_owned(),
                                    format!("{} MB", size.trim_end_matches('M')),
                                );
                            }
                        });
                    egui::ComboBox::from_label("Compression level")
                        .selected_text(&self.operation.compression_level)
                        .show_ui(ui, |ui| {
                            for level in -7..=22 {
                                let level = level.to_string();
                                ui.selectable_value(
                                    &mut self.operation.compression_level,
                                    level.clone(),
                                    level,
                                );
                            }
                        });
                });
            }
            OperationKind::Metadata => {
                ui.label("Existing entries (edit a value or press Remove):");
                if self.operation.meta_items.is_empty() {
                    ui.weak("No custom metadata.");
                }
                let total = ui.available_width();
                let gap = ui.spacing().item_spacing.x;
                let key_width = (total - gap) / 4.0;
                for (key, _, value, remove) in &mut self.operation.meta_items {
                    ui.horizontal(|ui| {
                        ui.add_sized([key_width, 20.0], egui::Label::new(key.as_str()).truncate());
                        let remove_width = 80.0;
                        let value_width = (total - key_width - remove_width - gap * 2.0).max(40.0);
                        ui.add_enabled(
                            !*remove,
                            egui::TextEdit::singleline(value).desired_width(value_width),
                        );
                        let label = if *remove { "Undo" } else { "Remove" };
                        if ui
                            .add_sized([remove_width, 20.0], egui::Button::new(label))
                            .clicked()
                        {
                            *remove = !*remove;
                        }
                    });
                }
                ui.separator();
                ui.label("New entries:");
                let total = ui.available_width();
                let gap = ui.spacing().item_spacing.x;
                let key_width = (total - gap) / 4.0;
                let remove_width = 80.0;
                let value_width = (total - key_width - remove_width - gap * 2.0).max(40.0);
                let mut delete_row = None;
                for (index, (key, value)) in self.operation.meta_new.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(key)
                                .hint_text("key")
                                .desired_width(key_width),
                        );
                        ui.add(
                            egui::TextEdit::singleline(value)
                                .hint_text("value")
                                .desired_width(value_width),
                        );
                        if ui
                            .add_sized([remove_width, 20.0], egui::Button::new("Remove"))
                            .clicked()
                        {
                            delete_row = Some(index);
                        }
                    });
                }
                if let Some(index) = delete_row {
                    self.operation.meta_new.remove(index);
                }
                if ui.button("Add another entry").clicked() {
                    self.operation.meta_new.push(Default::default());
                }
                ui.weak("Keys starting with \"zar.\" are reserved. Numbers are stored as numbers.");
            }
            OperationKind::Check => {
                ui.checkbox(&mut self.operation.recover, "Recover past damaged framing");
                ui.checkbox(
                    &mut self.operation.password_is_set,
                    "Unlock encrypted archive",
                );
                if self.operation.password_is_set {
                    ui.add(egui::TextEdit::singleline(&mut self.operation.password).password(true));
                }
            }
            OperationKind::List => {
                egui::ComboBox::from_label("Last commit")
                    .selected_text(selected_commit_label(
                        &self.operation.through,
                        &self.operation.commit_ids,
                    ))
                    .show_ui(ui, |ui| {
                        for (index, commit_id) in self.operation.commit_ids.iter().enumerate() {
                            let label = if index + 1 == self.operation.commit_ids.len() {
                                format!("Last commit ({commit_id})")
                            } else {
                                commit_id.clone()
                            };
                            ui.selectable_value(
                                &mut self.operation.through,
                                commit_id.clone(),
                                label,
                            );
                        }
                    });
                ui.checkbox(
                    &mut self.operation.password_is_set,
                    "Unlock encrypted headers",
                );
                if self.operation.password_is_set {
                    ui.add(egui::TextEdit::singleline(&mut self.operation.password).password(true));
                }
            }
            OperationKind::Extract => {
                ui.horizontal(|ui| {
                    ui.label("Destination:");
                    ui.allocate_ui_with_layout(
                        ui.available_size(),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if icon_button(ui, &self.icons, UiIcon::Browse, "Browse...", true)
                                .clicked()
                                && let Some(path) = rfd::FileDialog::new().pick_folder()
                            {
                                self.operation.destination = path.to_string_lossy().into_owned();
                            }
                            ui.add(
                                egui::TextEdit::singleline(&mut self.operation.destination)
                                    .desired_width(ui.available_width()),
                            );
                        },
                    );
                });
                egui::ComboBox::from_label("Last commit")
                    .selected_text(selected_commit_label(
                        &self.operation.through,
                        &self.operation.commit_ids,
                    ))
                    .show_ui(ui, |ui| {
                        for (index, commit_id) in self.operation.commit_ids.iter().enumerate() {
                            let label = if index + 1 == self.operation.commit_ids.len() {
                                format!("Last commit ({commit_id})")
                            } else {
                                commit_id.clone()
                            };
                            ui.selectable_value(
                                &mut self.operation.through,
                                commit_id.clone(),
                                label,
                            );
                        }
                    });
                egui::ComboBox::from_label("Case collision policy")
                    .selected_text(
                        ["Error", "Latest", "First", "Rename", "Ask"][self.operation.collision],
                    )
                    .show_ui(ui, |ui| {
                        for (index, label) in ["Error", "Latest", "First", "Rename", "Ask"]
                            .into_iter()
                            .enumerate()
                        {
                            ui.selectable_value(&mut self.operation.collision, index, label);
                        }
                    });
                egui::ComboBox::from_label("Existing file policy")
                    .selected_text(["Overwrite", "Skip", "Error", "Ask"][self.operation.existing])
                    .show_ui(ui, |ui| {
                        for (index, label) in ["Overwrite", "Skip", "Error", "Ask"]
                            .into_iter()
                            .enumerate()
                        {
                            ui.selectable_value(&mut self.operation.existing, index, label);
                        }
                    });
                ui.checkbox(&mut self.operation.dry_run, "Dry run");
                ui.checkbox(&mut self.operation.metadata, "Restore metadata");
                ui.checkbox(
                    &mut self.operation.restore_symlinks,
                    "Restore symbolic links",
                );
                ui.checkbox(&mut self.operation.restore_devices, "Restore device nodes");
                ui.checkbox(
                    &mut self.operation.strip_top_level,
                    "Skip top-level folder (extract its contents directly)",
                );
                ui.checkbox(
                    &mut self.operation.password_is_set,
                    "Unlock encrypted archive",
                );
                if self.operation.password_is_set {
                    ui.add(egui::TextEdit::singleline(&mut self.operation.password).password(true));
                }
                self.render_filter_fields(ui);
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            let (icon, label) = match kind {
                OperationKind::Create => (UiIcon::CreateArchive, "Create"),
                OperationKind::Add => (UiIcon::Add, "Add"),
                OperationKind::Remove => return,
                OperationKind::Join => (UiIcon::Join, "Join"),
                OperationKind::Metadata => (UiIcon::Info, "Apply"),
                OperationKind::Check => (UiIcon::TestArchive, "Test"),
                OperationKind::List => (UiIcon::ListContents, "List"),
                OperationKind::Extract => (UiIcon::Extract, "Extract"),
            };
            let can_start = match kind {
                OperationKind::Create => {
                    !self.operation.archive.trim().is_empty()
                        && !self.operation.input_paths.is_empty()
                        && (!self.operation.password_is_set
                            || (password_requirements(&self.operation.password).is_empty()
                                && self.operation.password == self.operation.password_confirm))
                }
                OperationKind::Extract => {
                    PathBuf::from(self.operation.archive.trim()).is_file()
                        && valid_destination_path(&self.operation.destination)
                }
                OperationKind::Check => PathBuf::from(self.operation.archive.trim()).is_file(),
                _ => true,
            };
            if icon_button(ui, &self.icons, icon, label, can_start).clicked() {
                let form = std::mem::take(&mut self.operation);
                self.start_operation(form);
            }
            if icon_button(ui, &self.icons, UiIcon::Cancel, "Cancel", true).clicked() {
                self.operation = OperationForm::default();
            }
        });
    }

    fn render_filter_fields(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Include / exclude filters", |ui| {
            ui.label("File and folder rules use fnmatch syntax; one pattern per line.");
            ui.horizontal(|ui| {
                ui.label("Include files");
                ui.label("Exclude files");
            });
            ui.columns(2, |columns| {
                columns[0].add(
                    egui::TextEdit::multiline(&mut self.operation.include_files)
                        .desired_rows(2)
                        .hint_text("*.rs"),
                );
                columns[1].add(
                    egui::TextEdit::multiline(&mut self.operation.exclude_files)
                        .desired_rows(2)
                        .hint_text("*.tmp"),
                );
            });
            ui.columns(2, |columns| {
                columns[0].add(
                    egui::TextEdit::multiline(&mut self.operation.include_folders)
                        .desired_rows(2)
                        .hint_text("src/**"),
                );
                columns[1].add(
                    egui::TextEdit::multiline(&mut self.operation.exclude_folders)
                        .desired_rows(2)
                        .hint_text("target/**"),
                );
            });
            ui.horizontal(|ui| {
                if icon_button(
                    ui,
                    &self.icons,
                    UiIcon::AddFilterList,
                    "Add filter list...",
                    true,
                )
                .clicked()
                    && let Some(paths) = rfd::FileDialog::new()
                        .add_filter("Text file", &["txt", "list"])
                        .pick_files()
                {
                    self.operation.filelists.extend(paths);
                }
                for path in &self.operation.filelists {
                    ui.label(path.file_name().unwrap_or_default().to_string_lossy());
                }
            });
        });
    }

    fn render_icon_archive_list(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        if self.archive_view == ArchiveView::Compact {
            self.render_compact_archive_list(ui, context);
            return;
        }

        let available_width = ui.available_width();
        let item_width = 112.0;
        let item_gap = ui.spacing().item_spacing.x;
        let column_count = ((available_width + item_gap) / (item_width + item_gap))
            .floor()
            .max(1.0) as usize;
        let cell_width = ((available_width - item_gap * (column_count.saturating_sub(1) as f32))
            / column_count as f32)
            .max(item_width);
        let row_height = 104.0;
        let row_count = self.visible_entry_indices.len().div_ceil(column_count);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row_height, row_count, |ui, row_range| {
                for row in row_range {
                    ui.horizontal(|ui| {
                        for column in 0..column_count {
                            let visible_index = row * column_count + column;
                            if visible_index >= self.visible_entry_indices.len() {
                                break;
                            }
                            let entry_index = self.visible_entry_indices[visible_index];
                            let (path, data) = self.entries[entry_index].clone();
                            let icon = match &data {
                                EntryData::Directory => UiIcon::Folder,
                                EntryData::File(_) => file_icon(&path),
                                EntryData::Symlink(_) => UiIcon::Symlink,
                                EntryData::NamedPipe | EntryData::Device(_) => UiIcon::SpecialData,
                            };
                            let (rect, response) = ui.allocate_exact_size(
                                egui::vec2(cell_width, row_height),
                                egui::Sense::click(),
                            );
                            let selected = self.selected.contains(&path);
                            if selected {
                                ui.painter()
                                    .rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
                            } else if response.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    2.0,
                                    ui.visuals().widgets.hovered.bg_fill,
                                );
                            }
                            let inner = rect.shrink(4.0);
                            let layout = egui::Layout::top_down(egui::Align::Center);
                            ui.scope_builder(
                                egui::UiBuilder::new().max_rect(inner).layout(layout),
                                |cell_ui| {
                                    let texture = icon_texture(context, &self.icons, icon);
                                    cell_ui.add_space(3.0);
                                    cell_ui.add(
                                        egui::Image::new(&texture)
                                            .fit_to_exact_size(egui::vec2(48.0, 48.0)),
                                    );
                                    cell_ui.add_sized(
                                        [inner.width(), 36.0],
                                        egui::Label::new(&path).truncate(),
                                    );
                                },
                            );
                            let response = response.on_hover_text(match data {
                                EntryData::Symlink(target) => format!("{path} → {target}"),
                                _ => path.clone(),
                            });
                            if response.double_clicked() {
                                self.pending_open = Some(path.clone());
                            }
                            if response.clicked() {
                                let modifiers = ui.input(|input| input.modifiers);
                                let visible_paths = self
                                    .visible_entry_indices
                                    .iter()
                                    .map(|index| self.entries[*index].0.clone())
                                    .collect::<Vec<_>>();
                                let selection = extended_selection(
                                    &visible_paths,
                                    &self.selected,
                                    &self.selection_anchor,
                                    &path,
                                    modifiers,
                                );
                                self.selected = selection.selected;
                                self.selection_anchor = selection.anchor;
                            }
                        }
                    });
                }
            });
    }

    fn render_compact_archive_list(&mut self, ui: &mut egui::Ui, context: &egui::Context) {
        let available_width = ui.available_width();
        let item_gap = ui.spacing().item_spacing.x;
        let min_cell_width = 300.0;
        let column_count = ((available_width + item_gap) / (min_cell_width + item_gap))
            .floor()
            .max(1.0) as usize;
        let cell_width = ((available_width - item_gap * (column_count - 1) as f32)
            / column_count as f32)
            .max(min_cell_width);
        let row_height = ui.spacing().interact_size.y.max(24.0);
        let row_count = self.visible_entry_indices.len().div_ceil(column_count);
        let text_font = egui::TextStyle::Body.resolve(ui.style());
        let list_id = ui.scope_id();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, row_height, row_count, |ui, row_range| {
                for row in row_range {
                    let (_, row_rect) =
                        ui.allocate_space(egui::vec2(ui.available_width(), row_height));
                    for column in 0..column_count {
                        let visible_index = row * column_count + column;
                        if visible_index >= self.visible_entry_indices.len() {
                            break;
                        }

                        let entry_index = self.visible_entry_indices[visible_index];
                        let (path, data) = &self.entries[entry_index];
                        let cell_rect = egui::Rect::from_min_size(
                            egui::pos2(
                                row_rect.left() + column as f32 * (cell_width + item_gap),
                                row_rect.top(),
                            ),
                            egui::vec2(cell_width, row_height),
                        );
                        let response = ui.interact(
                            cell_rect,
                            list_id.with(("compact-entry", visible_index)),
                            egui::Sense::click(),
                        );
                        let painter = ui.painter().with_clip_rect(cell_rect);
                        if self.selected.contains(path) {
                            painter.rect_filled(cell_rect, 2.0, ui.visuals().selection.bg_fill);
                        } else if response.hovered() {
                            painter.rect_filled(
                                cell_rect,
                                2.0,
                                ui.visuals().widgets.hovered.bg_fill,
                            );
                        }

                        let icon = match data {
                            EntryData::Directory => UiIcon::Folder,
                            EntryData::File(_) => file_icon(path),
                            EntryData::Symlink(_) => UiIcon::Symlink,
                            EntryData::NamedPipe | EntryData::Device(_) => UiIcon::SpecialData,
                        };
                        let texture = icon_texture(context, &self.icons, icon);
                        let icon_size = 16.0;
                        let icon_rect = egui::Rect::from_center_size(
                            egui::pos2(
                                cell_rect.left() + 4.0 + icon_size / 2.0,
                                cell_rect.center().y,
                            ),
                            egui::vec2(icon_size, icon_size),
                        );
                        painter.image(
                            texture.id(),
                            icon_rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                        painter.text(
                            egui::pos2(icon_rect.right() + 6.0, cell_rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            path,
                            text_font.clone(),
                            if self.selected.contains(path) {
                                ui.visuals().selection.stroke.color
                            } else {
                                ui.visuals().text_color()
                            },
                        );

                        let hover_text = match data {
                            EntryData::Symlink(target) => format!("{path} → {target}"),
                            _ => path.clone(),
                        };
                        let response = response.on_hover_text(hover_text);
                        if response.double_clicked() {
                            self.pending_open = Some(path.clone());
                        }
                        if response.clicked() {
                            let modifiers = ui.input(|input| input.modifiers);
                            let visible_paths = self
                                .visible_entry_indices
                                .iter()
                                .map(|index| self.entries[*index].0.clone())
                                .collect::<Vec<_>>();
                            let selection = extended_selection(
                                &visible_paths,
                                &self.selected,
                                &self.selection_anchor,
                                path,
                                modifiers,
                            );
                            self.selected = selection.selected;
                            self.selection_anchor = selection.anchor;
                        }
                    }
                }
            });
    }

    fn render_structured_archive_list(&mut self, ui: &mut egui::Ui) {
        if self.structured_rows_dirty {
            self.structured_rows_cache = structured_rows(
                &self.entries,
                &self.visible_entry_indices,
                &self.expanded_tree_dirs,
                !self.filter_text.trim().is_empty(),
                &self.entry_details,
                self.sort_by,
                self.sort_reversed,
            );
            self.structured_rows_dirty = false;
        }
        let viewport_width = ui.available_width();
        egui::ScrollArea::horizontal()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut labels = vec!["Name", "Size"];
                let mut defaults = vec![390.0, 120.0];
                let mut sorts = vec![Some(SortBy::Name), Some(SortBy::UncompressedSize)];
                if self.has_entry_dates {
                    labels.push("Modified");
                    defaults.push(140.0);
                    sorts.push(Some(SortBy::Date));
                }
                if self.has_entry_owner {
                    labels.push("Owner");
                    defaults.push(110.0);
                    sorts.push(None);
                }
                if self.has_entry_permissions {
                    labels.push("Permissions");
                    defaults.push(110.0);
                    sorts.push(None);
                }
                let mut widths = self.column_widths_for("structured", &labels, &defaults);
                let content_width = widths.iter().sum::<f32>().max(viewport_width);
                let header_height = ui.spacing().interact_size.y;
                let row_height = header_height.max(28.0);
                let font = egui::TextStyle::Body.resolve(ui.style());
                let text_color = ui.visuals().text_color();
                let list_id = ui.scope_id();

                let columns = labels
                    .iter()
                    .copied()
                    .zip(sorts.iter().copied())
                    .collect::<Vec<_>>();
                let clicked_sort = render_column_header(
                    ui,
                    list_id.with("header"),
                    &columns,
                    &mut widths,
                    (self.sort_by, self.sort_reversed),
                    &HeaderLayout {
                        offset: 0.0,
                        trim: 0.0,
                        gap: 0.0,
                        pad: 8.0,
                    },
                );
                self.store_column_widths("structured", &labels, &widths);
                if let Some(sort_by) = clicked_sort {
                    self.apply_sort_choice(sort_by);
                }
                let width_of = |label: &str| {
                    labels
                        .iter()
                        .position(|candidate| *candidate == label)
                        .map_or(0.0, |index| widths[index])
                };
                let name_width = width_of("Name");
                let size_width = width_of("Size");
                let modified_width = width_of("Modified");
                let owner_width = width_of("Owner");
                let permissions_width = width_of("Permissions");
                ui.separator();
                let rows = &self.structured_rows_cache;

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show_rows(ui, row_height, rows.len(), |ui, row_range| {
                        for row_index in row_range {
                            let row = rows[row_index].clone();
                            let (_, row_rect) =
                                ui.allocate_space(egui::vec2(content_width, row_height));
                            let response = ui.interact(
                                row_rect,
                                list_id.with(("structured-entry", row_index)),
                                egui::Sense::click(),
                            );
                            let painter = ui.painter().with_clip_rect(row_rect);
                            if self.selected.contains(&row.path) {
                                painter.rect_filled(row_rect, 0.0, ui.visuals().selection.bg_fill);
                            } else if response.hovered() {
                                painter.rect_filled(
                                    row_rect,
                                    0.0,
                                    ui.visuals().widgets.hovered.bg_fill,
                                );
                            }

                            let depth_width = row.depth as f32 * 20.0;
                            let disclosure_rect = egui::Rect::from_min_size(
                                egui::pos2(row_rect.left() + depth_width, row_rect.top()),
                                egui::vec2(18.0, row_height),
                            );
                            if row.has_children {
                                let center = disclosure_rect.center();
                                let points = if row.expanded {
                                    vec![
                                        egui::pos2(center.x - 4.0, center.y - 2.0),
                                        egui::pos2(center.x + 4.0, center.y - 2.0),
                                        egui::pos2(center.x, center.y + 3.0),
                                    ]
                                } else {
                                    vec![
                                        egui::pos2(center.x - 2.0, center.y - 4.0),
                                        egui::pos2(center.x - 2.0, center.y + 4.0),
                                        egui::pos2(center.x + 3.0, center.y),
                                    ]
                                };
                                painter.add(egui::Shape::convex_polygon(
                                    points,
                                    text_color,
                                    egui::Stroke::NONE,
                                ));
                            }

                            let icon = match row.data {
                                EntryData::Directory => UiIcon::Folder,
                                EntryData::File(_) => file_icon(&row.path),
                                EntryData::Symlink(_) => UiIcon::Symlink,
                                EntryData::NamedPipe | EntryData::Device(_) => UiIcon::SpecialData,
                            };
                            let texture = icon_texture(ui.ctx(), &self.icons, icon);
                            let icon_size = 22.0;
                            let icon_rect = egui::Rect::from_center_size(
                                egui::pos2(
                                    disclosure_rect.right() + 2.0 + icon_size / 2.0,
                                    row_rect.center().y,
                                ),
                                egui::vec2(icon_size, icon_size),
                            );
                            painter.image(
                                texture.id(),
                                icon_rect,
                                egui::Rect::from_min_max(
                                    egui::pos2(0.0, 0.0),
                                    egui::pos2(1.0, 1.0),
                                ),
                                egui::Color32::WHITE,
                            );
                            let name_rect = egui::Rect::from_min_max(
                                egui::pos2(icon_rect.right() + 6.0, row_rect.top()),
                                egui::pos2(row_rect.left() + name_width, row_rect.bottom()),
                            );
                            ui.painter().with_clip_rect(name_rect).text(
                                egui::pos2(name_rect.left(), row_rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                row.path.rsplit('/').next().unwrap_or(&row.path),
                                font.clone(),
                                if self.selected.contains(&row.path) {
                                    ui.visuals().selection.stroke.color
                                } else {
                                    text_color
                                },
                            );

                            let size = match row.data {
                                EntryData::Directory => {
                                    let unit = if row.child_count == 1 {
                                        "item"
                                    } else {
                                        "items"
                                    };
                                    format!("{} {unit}", row.child_count)
                                }
                                EntryData::File(size) => display_size(size),
                                _ => "—".to_owned(),
                            };
                            painter.text(
                                egui::pos2(row_rect.left() + name_width + 8.0, row_rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                size,
                                font.clone(),
                                text_color,
                            );
                            if modified_width > 0.0 {
                                painter.text(
                                    egui::pos2(
                                        row_rect.left() + name_width + size_width + 8.0,
                                        row_rect.center().y,
                                    ),
                                    egui::Align2::LEFT_CENTER,
                                    format_modified(self.entry_details.get(&row.path)),
                                    font.clone(),
                                    text_color,
                                );
                            }
                            let mut column_left =
                                row_rect.left() + name_width + size_width + modified_width;
                            for (width, text) in [
                                (
                                    owner_width,
                                    (owner_width > 0.0)
                                        .then(|| format_owner(self.entry_details.get(&row.path))),
                                ),
                                (
                                    permissions_width,
                                    (permissions_width > 0.0).then(|| {
                                        format_permissions(self.entry_details.get(&row.path))
                                    }),
                                ),
                            ] {
                                if let Some(text) = text {
                                    painter.text(
                                        egui::pos2(column_left + 8.0, row_rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        text,
                                        font.clone(),
                                        text_color,
                                    );
                                }
                                column_left += width;
                            }

                            let response = response.on_hover_text(&row.path);
                            if response.double_clicked() && !row.has_children {
                                self.pending_open = Some(row.path.clone());
                            }
                            if response.clicked() {
                                let clicked_path = row.path.clone();
                                let toggle = row.has_children
                                    && response
                                        .interact_pointer_pos()
                                        .is_some_and(|position| disclosure_rect.contains(position));
                                if toggle {
                                    if row.expanded {
                                        self.expanded_tree_dirs.remove(&clicked_path);
                                    } else {
                                        self.expanded_tree_dirs.insert(clicked_path);
                                    }
                                    self.structured_rows_dirty = true;
                                } else {
                                    let visible_paths = rows
                                        .iter()
                                        .map(|visible_row| visible_row.path.clone())
                                        .collect::<Vec<_>>();
                                    let modifiers = ui.input(|input| input.modifiers);
                                    let selection = extended_selection(
                                        &visible_paths,
                                        &self.selected,
                                        &self.selection_anchor,
                                        &row.path,
                                        modifiers,
                                    );
                                    self.selected = selection.selected;
                                    self.selection_anchor = selection.anchor;
                                }
                            }
                        }
                    });
            });
    }
}

impl ArchiveView {
    fn as_setting(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Compact => "compact",
            Self::Details => "details",
            Self::Structured => "structured",
        }
    }

    fn from_setting(value: &str) -> Option<Self> {
        [Self::Normal, Self::Compact, Self::Details, Self::Structured]
            .into_iter()
            .find(|view| view.as_setting() == value)
    }
}

impl SortBy {
    fn as_setting(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Date => "date",
            Self::CompressedSize => "compressed_size",
            Self::UncompressedSize => "uncompressed_size",
            Self::Ratio => "ratio",
            Self::Type => "type",
        }
    }

    fn from_setting(value: &str) -> Option<Self> {
        [
            Self::Name,
            Self::Date,
            Self::CompressedSize,
            Self::UncompressedSize,
            Self::Ratio,
            Self::Type,
        ]
        .into_iter()
        .find(|sort| sort.as_setting() == value)
    }
}

impl ZargApp {
    pub fn apply_settings(&mut self, settings: &crate::settings::Settings) {
        if let Some(view) = ArchiveView::from_setting(&settings.view) {
            self.archive_view = view;
        }
        if let Some(sort) = SortBy::from_setting(&settings.sort) {
            self.sort_by = sort;
            self.sort_reversed = settings.sort_reversed;
        }
        self.recent_archives = settings
            .recent_archives
            .iter()
            .take(MAX_RECENT_ARCHIVES)
            .cloned()
            .collect();
        self.start_recent_archive_check();
        self.dialog_sizes = settings
            .dialog_sizes
            .iter()
            .map(|(key, size)| (key.clone(), *size))
            .collect();
        self.column_widths = settings
            .column_widths
            .iter()
            .map(|(key, width)| (key.clone(), *width))
            .collect();
    }

    fn start_recent_archive_check(&mut self) {
        let archives = self.recent_archives.clone();
        if archives.is_empty() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.recent_check = Some(receiver);
        std::thread::spawn(move || {
            let invalid = archives
                .into_iter()
                .filter(|archive| !std::path::Path::new(archive).is_file())
                .collect();
            let _ = sender.send(invalid);
        });
    }

    fn poll_benchmark(&mut self, context: &egui::Context) {
        let Some(receiver) = &self.benchmark.receiver else {
            return;
        };
        match receiver.try_recv() {
            Ok(result) => {
                self.benchmark.receiver = None;
                self.benchmark.result = Some(result);
            }
            Err(mpsc::TryRecvError::Empty) => {
                context.request_repaint_after(std::time::Duration::from_millis(100));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.benchmark.receiver = None;
                self.benchmark.result = Some(Err("Benchmark worker stopped unexpectedly.".into()));
            }
        }
    }

    fn render_benchmark(&mut self, context: &egui::Context) {
        if !self.benchmark.open {
            return;
        }
        let running = self.benchmark.receiver.is_some();
        let cancelling = running
            && self
                .benchmark
                .cancel
                .load(std::sync::atomic::Ordering::Relaxed);
        let mut open = true;
        let mut start = false;
        let mut cancel = false;
        let size_key = "benchmark";
        let size = self.dialog_size(size_key, [560.0, 220.0]);
        let mut actual_size = None;
        let archive = self.archive.clone();
        let state = &mut self.benchmark;
        let viewport_id = egui::ViewportId::from_hash_of("benchmark-dialog");
        context.show_viewport_immediate(
            viewport_id,
            Self::centered_dialog_viewport_builder(context, "Benchmark", size, true),
            |ui, _class| {
                actual_size = Self::viewport_inner_size(ui.ctx());
                if ui.ctx().input(|input| input.viewport().close_requested()) {
                    if running {
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    } else {
                        open = false;
                    }
                }
                ui.label(format!("Archive: {archive}"));
                ui.weak(
                    "Unpacks the archive to a temporary folder, then times creating and \
                     extracting a new archive with these settings. Nothing else is written.",
                );
                ui.add_enabled_ui(!running, |ui| {
                    egui::Grid::new("benchmark_settings")
                        .num_columns(2)
                        .spacing([16.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("Block size");
                            egui::ComboBox::from_id_salt("bench_block")
                                .selected_text(&state.settings.block_size)
                                .show_ui(ui, |ui| {
                                    for size in
                                        ["64K", "256K", "1M", "2M", "4M", "8M", "16M", "32M"]
                                    {
                                        ui.selectable_value(
                                            &mut state.settings.block_size,
                                            size.to_owned(),
                                            size,
                                        );
                                    }
                                });
                            ui.end_row();
                            ui.label("Compression level");
                            ui.add(egui::Slider::new(
                                &mut state.settings.compression_level,
                                -7..=22,
                            ));
                            ui.end_row();
                        });
                    ui.checkbox(
                        &mut state.settings.owner_permissions,
                        "Save owner and permissions",
                    );
                    ui.checkbox(&mut state.settings.dates, "Save modification dates");
                    ui.checkbox(
                        &mut state.settings.special_files,
                        "Include symbolic links and special files",
                    );
                });
                ui.separator();
                match &state.result {
                    _ if running => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(if cancelling {
                                "Cancelling after the current step..."
                            } else {
                                "Running benchmark..."
                            });
                        });
                    }
                    Some(Ok(result)) => {
                        egui::Grid::new("benchmark_result")
                            .num_columns(2)
                            .spacing([16.0, 4.0])
                            .show(ui, |ui| {
                                info_row(ui, "Files", result.files.to_string());
                                info_row(ui, "Original size", display_size(result.uncompressed));
                                info_row(ui, "Archive size", display_size(result.archive_size));
                                ui.label(egui::RichText::new("Ratio").weak());
                                info_ratio_bar(
                                    ui,
                                    info_ratio(result.archive_size, result.uncompressed),
                                );
                                ui.end_row();
                                info_row(
                                    ui,
                                    "Create",
                                    format!(
                                        "{:.2} s, {}",
                                        result.create_seconds,
                                        throughput(result.uncompressed, result.create_seconds)
                                    ),
                                );
                                info_row(
                                    ui,
                                    "Extract",
                                    format!(
                                        "{:.2} s, {}",
                                        result.extract_seconds,
                                        throughput(result.uncompressed, result.extract_seconds)
                                    ),
                                );
                            });
                    }
                    Some(Err(error)) => {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                    None => {}
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!running, egui::Button::new("Start"))
                        .clicked()
                    {
                        start = true;
                    }
                    if ui
                        .add_enabled(running && !cancelling, egui::Button::new("Cancel"))
                        .clicked()
                    {
                        cancel = true;
                    }
                });
                if !open {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            },
        );
        self.record_dialog_size(size_key, actual_size);
        if !open && !running {
            self.benchmark.open = false;
        }
        if cancel {
            self.benchmark
                .cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if start {
            let settings = BenchmarkSettings {
                block_size: self.benchmark.settings.block_size.clone(),
                ..self.benchmark.settings
            };
            let archive = self.archive.clone();
            let password = self
                .archive_password
                .as_ref()
                .map(|p| p.as_bytes().to_vec());
            let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
            self.benchmark.cancel = cancel_flag.clone();
            let (sender, receiver) = mpsc::channel();
            self.benchmark.receiver = Some(receiver);
            self.benchmark.result = None;
            std::thread::spawn(move || {
                let _ = sender.send(run_benchmark(&archive, password, &settings, &cancel_flag));
            });
        }
    }

    fn show_archive_info(&mut self) {
        let archive = self.archive.clone();
        let (sender, receiver) = mpsc::channel();
        self.info_loading = Some(receiver);
        self.info_view = None;
        self.status = "Reading archive information...".to_owned();
        std::thread::spawn(move || {
            let result = (|| {
                let commits =
                    operations::info_archive(&archive, true).map_err(|error| error.to_string())?;
                let metadata = operations::read_custom_metadata(&archive)
                    .map(|items| {
                        items
                            .iter()
                            .map(|(key, value)| {
                                (key.clone(), operations::format_metadata_value(value))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(ArchiveInfoView {
                    archive,
                    commits,
                    metadata,
                })
            })();
            let _ = sender.send(result);
        });
    }

    fn poll_archive_info(&mut self, context: &egui::Context) {
        let Some(receiver) = &self.info_loading else {
            return;
        };
        match receiver.try_recv() {
            Ok(result) => {
                self.info_loading = None;
                match result {
                    Ok(view) => {
                        self.status = format!("{} commit(s)", view.commits.len());
                        self.info_view = Some(view);
                    }
                    Err(error) => {
                        self.status = error.clone();
                        self.report = Some(error);
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => {
                context.request_repaint_after(std::time::Duration::from_millis(100));
            }
            Err(mpsc::TryRecvError::Disconnected) => self.info_loading = None,
        }
    }

    fn render_archive_info(&mut self, context: &egui::Context) {
        let Some(view) = &self.info_view else {
            return;
        };
        let size_key = "archive-information";
        let size = self.dialog_size(size_key, [600.0, 520.0]);
        let mut actual_size = None;
        let viewport_id = egui::ViewportId::from_hash_of("archive-information-dialog");
        let mut open = true;
        context.show_viewport_immediate(
            viewport_id,
            Self::centered_dialog_viewport_builder(context, "Archive information", size, true),
            |ui, _class| {
                actual_size = Self::viewport_inner_size(ui.ctx());
                if ui.ctx().input(|input| input.viewport().close_requested()) {
                    open = false;
                }
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| render_info_contents(ui, view));
                if !open {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            },
        );
        self.record_dialog_size(size_key, actual_size);
        if !open {
            self.info_view = None;
        }
    }

    fn poll_recent_archive_check(&mut self, context: &egui::Context) {
        let Some(receiver) = &self.recent_check else {
            return;
        };
        match receiver.try_recv() {
            Ok(invalid) => {
                self.recent_archives
                    .retain(|archive| !invalid.contains(archive));
                self.recent_check = None;
            }
            Err(mpsc::TryRecvError::Empty) => {
                context.request_repaint_after(std::time::Duration::from_millis(100));
            }
            Err(mpsc::TryRecvError::Disconnected) => self.recent_check = None,
        }
    }

    fn current_settings(&self) -> crate::settings::Settings {
        crate::settings::Settings {
            window: self.window_geometry,
            dialog_sizes: self
                .dialog_sizes
                .iter()
                .map(|(key, size)| (key.clone(), *size))
                .collect(),
            monitor_count: crate::settings::monitor_count(),
            view: self.archive_view.as_setting().to_owned(),
            sort: self.sort_by.as_setting().to_owned(),
            sort_reversed: self.sort_reversed,
            recent_archives: self.recent_archives.clone(),
            column_widths: self
                .column_widths
                .iter()
                .map(|(key, width)| (key.clone(), *width))
                .collect(),
        }
    }

    fn track_window_geometry(&mut self, context: &egui::Context) {
        let (inner, outer, normal) = context.input(|input| {
            let viewport = input.viewport();
            (
                viewport.inner_rect,
                viewport.outer_rect,
                !viewport.maximized.unwrap_or(false)
                    && !viewport.minimized.unwrap_or(false)
                    && !viewport.fullscreen.unwrap_or(false),
            )
        });
        if let (true, Some(inner)) = (normal, inner) {
            self.window_geometry = Some(crate::settings::WindowGeometry {
                size: [inner.width(), inner.height()],
                position: outer.map(|outer| [outer.min.x, outer.min.y]),
            });
        }
    }
}

impl eframe::App for ZargApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.current_settings().save();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        self.track_window_geometry(&context);
        self.poll_recent_archive_check(&context);
        self.poll_worker(&context);
        egui::Panel::top("menus").show(ui, |ui| self.render_menus(ui));
        egui::Panel::top("toolbar").show(ui, |ui| {
            self.render_toolbar(ui);
            ui.separator();
            ui.horizontal(|ui| {
                if icon_button(
                    ui,
                    &self.icons,
                    UiIcon::OpenArchive,
                    "Open archive...",
                    true,
                )
                .clicked()
                {
                    self.open_dialog();
                }
                ui.add_enabled_ui(false, |ui| {
                    ui.text_edit_singleline(&mut self.location);
                });
                if !self.archive.is_empty() {
                    ui.label(format!("{} entries", self.entries.len()));
                }
            });
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            if self.busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(&self.progress_message);
                    if self.progress_known {
                        ui.add(egui::ProgressBar::new(self.progress).desired_width(200.0));
                    }
                });
            } else {
                ui.label(&self.status);
            }
        });
        egui::CentralPanel::default().show(ui, |ui| {
            if self.archive.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.heading("Open or create a ZAR archive");
                });
                return;
            }
            if self.archive_view == ArchiveView::Structured {
                self.render_structured_archive_list(ui);
                return;
            }
            if self.archive_view != ArchiveView::Details {
                self.render_icon_archive_list(ui, &context);
                return;
            }
            let has_dates = self.has_entry_dates;
            let (has_owner, has_permissions) = (self.has_entry_owner, self.has_entry_permissions);
            egui::ScrollArea::horizontal()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let mut labels = vec!["Name"];
                    let mut defaults = vec![360.0];
                    let mut sorts = vec![Some(SortBy::Name)];
                    if has_dates {
                        labels.push("Modified");
                        defaults.push(160.0);
                        sorts.push(Some(SortBy::Date));
                    }
                    labels.extend(["Compressed", "Uncompressed", "Ratio"]);
                    defaults.extend([110.0, 110.0, 80.0]);
                    sorts.extend([
                        Some(SortBy::CompressedSize),
                        Some(SortBy::UncompressedSize),
                        Some(SortBy::Ratio),
                    ]);
                    if has_owner {
                        labels.push("Owner");
                        defaults.push(110.0);
                        sorts.push(None);
                    }
                    if has_permissions {
                        labels.push("Permissions");
                        defaults.push(100.0);
                        sorts.push(None);
                    }
                    labels.push("Type");
                    defaults.push(110.0);
                    sorts.push(Some(SortBy::Type));
                    let mut columns = self.column_widths_for("details", &labels, &defaults);
                    let content_width = columns.iter().sum::<f32>() + 8.0 * columns.len() as f32;
                    let spacing = ui.spacing().item_spacing.x;
                    let header_columns = labels
                        .iter()
                        .copied()
                        .zip(sorts.iter().copied())
                        .collect::<Vec<_>>();
                    let clicked_sort = render_column_header(
                        ui,
                        ui.scope_id().with("details-header"),
                        &header_columns,
                        &mut columns,
                        (self.sort_by, self.sort_reversed),
                        &HeaderLayout {
                            offset: 24.0 + spacing,
                            trim: 28.0,
                            gap: spacing,
                            pad: 0.0,
                        },
                    );
                    self.store_column_widths("details", &labels, &columns);
                    if let Some(sort_by) = clicked_sort {
                        self.apply_sort_choice(sort_by);
                    }
                    ui.separator();
                    let row_height = ui.spacing().interact_size.y;
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show_rows(
                            ui,
                            row_height,
                            self.visible_entry_indices.len(),
                            |ui, row_range| {
                                ui.set_min_width(content_width);
                                for row in row_range {
                                    let (path, data) =
                                        &self.entries[self.visible_entry_indices[row]];
                                    let selected = self.selected.contains(path);
                                    let row_rect = egui::Rect::from_min_size(
                                        ui.cursor().min,
                                        egui::vec2(content_width, row_height),
                                    );
                                    if selected {
                                        ui.painter().rect_filled(
                                            row_rect,
                                            0.0,
                                            ui.visuals().selection.bg_fill,
                                        );
                                    }
                                    ui.horizontal(|ui| {
                                        ui.set_min_width(content_width);
                                        let icon = match data {
                                            EntryData::Directory => UiIcon::Folder,
                                            EntryData::File(_) => file_icon(path),
                                            EntryData::Symlink(_) => UiIcon::Symlink,
                                            EntryData::NamedPipe | EntryData::Device(_) => {
                                                UiIcon::SpecialData
                                            }
                                        };
                                        let hover_text = match data {
                                            EntryData::Symlink(target) => {
                                                format!("{path} → {target}")
                                            }
                                            _ => path.clone(),
                                        };
                                        let texture = icon_texture(&context, &self.icons, icon);
                                        left_aligned_cell(
                                            ui,
                                            24.0,
                                            row_height,
                                            egui::Image::new(&texture)
                                                .fit_to_exact_size(egui::vec2(20.0, 20.0)),
                                        );
                                        let name_response = left_aligned_cell(
                                            ui,
                                            columns[0] - 28.0,
                                            row_height,
                                            egui::Label::new(egui::RichText::new(path).color(
                                                if selected {
                                                    ui.visuals().selection.stroke.color
                                                } else {
                                                    ui.visuals().text_color()
                                                },
                                            ))
                                            .halign(egui::Align::Min)
                                            .truncate()
                                            .sense(egui::Sense::click()),
                                        )
                                        .on_hover_text(hover_text);
                                        if name_response.double_clicked() {
                                            self.pending_open = Some(path.clone());
                                        }
                                        if name_response.clicked() {
                                            let modifiers = ui.input(|input| input.modifiers);
                                            let visible_paths = self
                                                .visible_entry_indices
                                                .iter()
                                                .map(|index| self.entries[*index].0.clone())
                                                .collect::<Vec<_>>();
                                            let selection = extended_selection(
                                                &visible_paths,
                                                &self.selected,
                                                &self.selection_anchor,
                                                path,
                                                modifiers,
                                            );
                                            self.selected = selection.selected;
                                            self.selection_anchor = selection.anchor;
                                        }
                                        let details = self.entry_details.get(path);
                                        let (compressed, uncompressed, ratio, kind) = match data {
                                            EntryData::File(size) => {
                                                let compressed_size = details
                                                    .and_then(|details| details.compressed_size);
                                                let ratio = compressed_size
                                                    .map(|compressed| {
                                                        if *size == 0 {
                                                            "—".to_owned()
                                                        } else {
                                                            format!(
                                                                "{:.1}%",
                                                                compressed as f64 * 100.0
                                                                    / *size as f64
                                                            )
                                                        }
                                                    })
                                                    .unwrap_or_else(|| "—".to_owned());
                                                (
                                                    compressed_size
                                                        .map(display_size)
                                                        .unwrap_or_else(|| "—".to_owned()),
                                                    display_size(*size),
                                                    ratio,
                                                    "File",
                                                )
                                            }
                                            EntryData::Directory => (
                                                "—".to_owned(),
                                                "—".to_owned(),
                                                "—".to_owned(),
                                                "Folder",
                                            ),
                                            EntryData::Symlink(_) => (
                                                "—".to_owned(),
                                                "—".to_owned(),
                                                "—".to_owned(),
                                                "Symbolic link",
                                            ),
                                            EntryData::NamedPipe => (
                                                "—".to_owned(),
                                                "—".to_owned(),
                                                "—".to_owned(),
                                                "Named pipe",
                                            ),
                                            EntryData::Device(kind) => (
                                                "—".to_owned(),
                                                "—".to_owned(),
                                                "—".to_owned(),
                                                kind.as_str(),
                                            ),
                                        };
                                        let mut values = Vec::with_capacity(7);
                                        if has_dates {
                                            values.push(format_modified(details));
                                        }
                                        values.extend([compressed, uncompressed, ratio]);
                                        if has_owner {
                                            values.push(format_owner(details));
                                        }
                                        if has_permissions {
                                            values.push(format_permissions(details));
                                        }
                                        values.push(kind.to_owned());
                                        for (value, width) in
                                            values.into_iter().zip(columns[1..].iter().copied())
                                        {
                                            left_aligned_cell(
                                                ui,
                                                width,
                                                row_height,
                                                egui::Label::new(value)
                                                    .halign(egui::Align::Min)
                                                    .truncate(),
                                            );
                                        }
                                    });
                                }
                            },
                        );
                });
        });

        if let Some(path) = self.pending_open.take() {
            self.open_entry_externally(path);
        }

        self.poll_archive_info(&context);
        self.render_operation_dialog(&context);
        self.render_archive_info(&context);
        self.poll_benchmark(&context);
        self.render_benchmark(&context);

        if let Some(prompt) = self.confirmation.clone() {
            let viewport_id = egui::ViewportId::from_hash_of("confirmation-dialog");
            let size_key = "confirmation";
            let size = self.dialog_size(size_key, [380.0, 150.0]);
            let mut actual_size = None;
            let mut open = true;
            context.show_viewport_immediate(
                viewport_id,
                Self::centered_dialog_viewport_builder(&context, "Confirmation", size, false),
                |ui, _class| {
                    actual_size = Self::viewport_inner_size(ui.ctx());
                    if ui.ctx().input(|input| input.viewport().close_requested()) {
                        open = false;
                    }
                    ui.label(&prompt);
                    ui.horizontal(|ui| {
                        if icon_button(ui, &self.icons, UiIcon::Confirm, "Yes", true).clicked() {
                            self.answer_confirmation(true);
                        }
                        if icon_button(ui, &self.icons, UiIcon::Cancel, "No", true).clicked() {
                            self.answer_confirmation(false);
                        }
                    });
                    if !open || self.confirmation.is_none() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                },
            );
            self.record_dialog_size(size_key, actual_size);
            if !open && self.confirmation.is_some() {
                self.answer_confirmation(false);
            }
        }

        if self.show_about {
            let mut close_about = false;
            let mut open = true;
            let size_key = "about";
            let size = self.dialog_size(size_key, [440.0, 220.0]);
            let mut actual_size = None;
            let icons = &self.icons;
            let viewport_id = egui::ViewportId::from_hash_of("about-dialog");
            context.show_viewport_immediate(
                viewport_id,
                Self::centered_dialog_viewport_builder(&context, "About zarg", size, false),
                |ui, _class| {
                    actual_size = Self::viewport_inner_size(ui.ctx());
                    if ui.ctx().input(|input| input.viewport().close_requested()) {
                        open = false;
                    }
                    ui.horizontal(|ui| {
                        let icon = icon_texture(ui.ctx(), &self.icons, UiIcon::Application);
                        ui.add(egui::Image::new(&icon).fit_to_exact_size(egui::vec2(88.0, 88.0)));
                        ui.vertical(|ui| {
                            ui.heading("ZAR Archive Manager");
                            ui.label("A graphical archive manager for the ZAR format.");
                            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                            ui.hyperlink_to(
                                "GitHub repository",
                                "https://github.com/brmmm3/zar-rs-inter",
                            );
                        });
                    });
                    ui.separator();
                    ui.label(format!("Author: {}", env!("CARGO_PKG_AUTHORS")));
                    ui.label(format!("License: {}", env!("CARGO_PKG_LICENSE")));
                    ui.separator();
                    ui.label("Icons: KDE4 Crystal Diamond 1.1 by Paolino (Paolo Campitelli)");
                    ui.horizontal(|ui| {
                        ui.label("Licensed under the");
                        ui.hyperlink_to(
                            "GNU LGPL 3.0",
                            "https://www.gnu.org/licenses/lgpl-3.0.html",
                        );
                        ui.label("-");
                        ui.hyperlink_to("paolocampitelli.com", "https://www.paolocampitelli.com/");
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() - 90.0).max(0.0));
                        if icon_button(ui, icons, UiIcon::Cancel, "Close", true).clicked() {
                            close_about = true;
                        }
                    });
                    if !open || close_about {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                },
            );
            self.record_dialog_size(size_key, actual_size);
            if !open || close_about {
                self.show_about = false;
            }
        }
        if let Some(report) = self.report.clone() {
            let viewport_id = egui::ViewportId::from_hash_of("operation-error-dialog");
            let size_key = "operation-error";
            let size = self.dialog_size(size_key, [540.0, 240.0]);
            let mut actual_size = None;
            let mut open = true;
            context.show_viewport_immediate(
                viewport_id,
                Self::centered_dialog_viewport_builder(&context, "Operation error", size, true),
                |ui, _class| {
                    actual_size = Self::viewport_inner_size(ui.ctx());
                    if ui.ctx().input(|input| input.viewport().close_requested()) {
                        open = false;
                    }
                    ui.label(&report);
                    if !open {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                },
            );
            self.record_dialog_size(size_key, actual_size);
            if !open {
                self.report = None;
            }
        }

        // Custom file dialog (separate OS window via multi-viewport)
        if self.file_dialog.open {
            let viewport_id = egui::ViewportId::from_hash_of("file-dialog");
            let size_key = "file-selection";
            let size = self.dialog_size(size_key, [780.0, 400.0]);
            let mut actual_size = None;
            let mut confirmed = false;
            let mut closed = false;

            context.show_viewport_immediate(
                viewport_id,
                Self::centered_dialog_viewport_builder(
                    &context,
                    "Select files and folders",
                    size,
                    true,
                )
                .with_min_inner_size([360.0, 300.0]),
                |ui, _class| {
                    actual_size = Self::viewport_inner_size(ui.ctx());
                    // Check if the OS close button was clicked
                    if ui.ctx().input(|i| i.viewport().close_requested()) {
                        closed = true;
                    }

                    confirmed = self.file_dialog.ui(ui, &self.icons);

                    if confirmed || closed {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                },
            );
            self.record_dialog_size(size_key, actual_size);

            if confirmed || closed {
                self.file_dialog.open = false;
                let paths = self.file_dialog.selected_paths();
                self.operation.input_paths.extend(paths);
            }
        }
    }
}

impl ZargApp {
    fn answer_confirmation(&mut self, answer: bool) {
        if let Some(worker) = &self.worker {
            let _ = worker.answers.send(answer);
        }
        self.confirmation = None;
    }
}

fn invert_selection(selected: &BTreeSet<String>, visible: &[String]) -> BTreeSet<String> {
    let mut inverted = selected
        .iter()
        .filter(|path| !visible.contains(path))
        .cloned()
        .collect::<BTreeSet<_>>();
    inverted.extend(
        visible
            .iter()
            .filter(|path| !selected.contains(*path))
            .cloned(),
    );
    inverted
}

const MAX_RECENT_ARCHIVES: usize = 10;

fn remember_recent_archive(recent_archives: &mut Vec<String>, archive: &str) {
    if archive.is_empty() {
        return;
    }
    recent_archives.retain(|recent| recent != archive);
    recent_archives.insert(0, archive.to_owned());
    recent_archives.truncate(MAX_RECENT_ARCHIVES);
}

fn named_archive_path(input: &std::path::Path) -> Result<PathBuf, String> {
    let name = if input.is_dir() {
        input.file_name()
    } else {
        input.file_stem()
    }
    .ok_or_else(|| format!("Cannot derive an archive name from {}", input.display()))?;
    let mut archive = input
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();
    archive.push(format!("{}.zar", name.to_string_lossy()));
    Ok(archive)
}

fn archive_folder_destination(archive: &std::path::Path) -> Result<PathBuf, String> {
    let name = archive.file_name().ok_or_else(|| {
        format!(
            "Cannot derive an extraction folder from {}",
            archive.display()
        )
    })?;
    let folder_name = std::path::Path::new(name).file_stem().ok_or_else(|| {
        format!(
            "Cannot derive an extraction folder from {}",
            archive.display()
        )
    })?;
    Ok(archive
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join(folder_name))
}

fn build_job(
    kind: OperationKind,
    form: OperationForm,
    _events: Sender<UiEvent>,
    progress: UiProgress,
) -> OperationBuildResult {
    let archive = form.archive.clone();
    let password = form
        .password_is_set
        .then(|| form.password.as_bytes().to_vec());
    let progress = Arc::new(progress);
    let job = match kind {
        OperationKind::Create => {
            if form.input_paths.is_empty() {
                return Err("Choose one or more files or folders to archive.".into());
            }
            let options = form.encoding()?;
            let filters = form.filters()?;
            let paths = form.input_paths;
            let strict = form.strict;
            let input = CreateInputOptions {
                save_metadata: form.metadata,
                save_dates: form.dates,
                include_special_files: form.special_files,
                overwrite_existing: form.overwrite,
                verbose: form.verbose,
            };
            let aead = [ContentAead::Aes256Gcm, ContentAead::ChaCha20Poly1305][form.aead.min(1)];
            let scope = [
                EncryptionScope::HeaderAndData,
                EncryptionScope::Data,
                EncryptionScope::Header,
            ][form.encryption_scope.min(2)];
            Box::new(move || {
                let encryption = password.as_deref().map(|password| (password, aead, scope));
                operations::create_archive_with_password_progress_and_filters_and_encryption(
                    &archive,
                    &paths,
                    strict,
                    options,
                    encryption,
                    Some(progress.as_ref()),
                    CreateFilterOptions {
                        input,
                        filters: &filters,
                    },
                )
                .map(|report| {
                    if report.warnings.is_empty() {
                        format!("Created archive {archive}")
                    } else {
                        report.warnings.join("\n")
                    }
                })
                .map_err(|error| error.to_string())
            }) as OperationJob
        }
        OperationKind::Add => {
            if form.input_paths.is_empty() {
                return Err("Choose one or more files or folders to add.".into());
            }
            let options = form.encoding()?;
            let filters = form.filters()?;
            let paths = form.input_paths;
            let input = AddInputOptions {
                save_metadata: form.metadata,
                save_dates: form.dates,
                include_special_files: form.special_files,
                strict: form.strict,
                verbose: form.verbose,
            };
            let new_password = password;
            let aead = [ContentAead::Aes256Gcm, ContentAead::ChaCha20Poly1305][form.aead.min(1)];
            let scope = [
                EncryptionScope::HeaderAndData,
                EncryptionScope::Data,
                EncryptionScope::Header,
            ][form.encryption_scope.min(2)];
            let existing_password = form
                .existing_password_is_set
                .then(|| form.existing_password.as_bytes().to_vec());
            let passwords = PasswordSet::new(existing_password.or_else(|| new_password.clone()));
            Box::new(move || {
                let encryption = new_password
                    .as_deref()
                    .map(|password| (password, aead, scope));
                operations::add_archive_with_encoding_progress_and_filters_and_encryption(
                    &archive,
                    &paths,
                    options,
                    Some(progress.as_ref()),
                    input,
                    &filters,
                    encryption,
                    &passwords,
                )
                .map(|()| format!("Added selected inputs to {archive}"))
                .map_err(|error| error.to_string())
            })
        }
        OperationKind::Metadata => {
            let remove: Vec<String> = form
                .meta_items
                .iter()
                .filter(|item| item.3)
                .map(|item| item.0.clone())
                .collect();
            let mut set: Vec<(String, zarlib::MetadataType)> = form
                .meta_items
                .iter()
                .filter(|item| !item.3 && item.1 != item.2)
                .map(|item| (item.0.clone(), operations::parse_metadata_value(&item.2)))
                .collect();
            for (key, value) in &form.meta_new {
                let key = key.trim();
                if !key.is_empty() {
                    set.push((key.to_owned(), operations::parse_metadata_value(value)));
                }
            }
            if set.is_empty() && remove.is_empty() {
                return Err("No metadata changes to apply.".into());
            }
            Box::new(move || {
                operations::update_custom_metadata(&archive, &set, &remove)
                    .map(|()| {
                        format!(
                            "Metadata updated: {} set, {} removed",
                            set.len(),
                            remove.len()
                        )
                    })
                    .map_err(|error| error.to_string())
            })
        }
        OperationKind::Remove => {
            let paths: Vec<PathBuf> = lines(&form.remove_text).iter().map(PathBuf::from).collect();
            if paths.is_empty() {
                return Err("Enter or select one or more archive paths to remove.".into());
            }
            Box::new(move || {
                operations::remove_archive_with_progress(&archive, &paths, Some(progress.as_ref()))
                    .map(|()| format!("Removed {} archive path(s)", paths.len()))
                    .map_err(|error| error.to_string())
            })
        }
        OperationKind::Join => {
            let options = form.encoding()?;
            let from = form.from_id;
            let through = form.through_id;
            let output = form.join_output.trim().to_owned();
            Box::new(move || {
                // Joining rewrites in place, so work on a copy to keep the original.
                let target = if output.is_empty() || output == archive {
                    archive.clone()
                } else {
                    std::fs::copy(&archive, &output)
                        .map_err(|error| format!("cannot write {output}: {error}"))?;
                    output.clone()
                };
                let result = operations::join_archive_with_encoding_progress(
                    &target,
                    &from,
                    &through,
                    options,
                    Some(progress.as_ref()),
                );
                if let Err(error) = result {
                    if target != archive {
                        let _ = std::fs::remove_file(&target);
                    }
                    return Err(error.to_string());
                }
                Ok(if target == archive {
                    format!("Joined commits {from} through {through}")
                } else {
                    format!("Joined commits {from} through {through} into {target}")
                })
            })
        }
        OperationKind::Check => {
            let recover = form.recover;
            Box::new(move || {
                let report = operations::check_archive_with_recovery_progress(
                    &archive,
                    recover,
                    password.as_deref(),
                    Some(progress.as_ref()),
                )
                .map_err(|error| error.to_string())?;
                Ok(check_summary(&report))
            })
        }
        OperationKind::List => {
            let through = (!form.through.trim().is_empty()).then(|| form.through.clone());
            Box::new(move || {
                operations::list_archive(&archive, through.as_deref(), password.as_deref())
                    .map(|toc| format!("Loaded {} entries from {archive}", toc.len()))
                    .map_err(|error| error.to_string())
            })
        }
        OperationKind::Extract => {
            let filters = form.filters()?;
            let destination = PathBuf::from(form.destination);
            let through = (!form.through.trim().is_empty()).then(|| form.through.clone());
            let dry_run = form.dry_run;
            let collision = collision_policy(form.collision);
            let existing = existing_policy(form.existing);
            let restore_metadata = form.metadata;
            let restore_symlinks = form.restore_symlinks;
            let restore_devices = form.restore_devices;
            let strip_top_level = form.strip_top_level;
            Box::new(move || {
                let options = ExtractionOptions {
                    dry_run,
                    through: through.as_deref(),
                    collision_policy: collision,
                    existing_policy: existing,
                    restore_metadata,
                    restore_symlinks,
                    restore_devices,
                    strip_top_level,
                };
                operations::extract_archive_with_progress_and_filters(
                    &archive,
                    &destination,
                    options,
                    password.as_deref(),
                    Some(progress.as_ref()),
                    &filters,
                )
                .map(|plan| {
                    let summary = format!(
                        "{} file(s) and {} directory/directories {}",
                        plan.files.len(),
                        plan.directories.len(),
                        if options.dry_run {
                            "planned"
                        } else {
                            "extracted"
                        }
                    );
                    if options.dry_run {
                        let mut lines: Vec<String> = plan
                            .directories
                            .iter()
                            .take(100)
                            .map(|path| format!("Create directory {}", path.display()))
                            .collect();
                        lines.extend(
                            plan.files
                                .iter()
                                .take(100_usize.saturating_sub(lines.len()))
                                .map(|(path, _, _)| format!("Extract {}", path.display())),
                        );
                        if plan.directories.len() + plan.files.len() > lines.len() {
                            lines.push("...".to_owned());
                        }
                        format!("{summary}\n{}", lines.join("\n"))
                    } else {
                        summary
                    }
                })
                .map_err(|error| error.to_string())
            })
        }
    };
    Ok(job)
}

fn extract_and_open(archive: &str, path: &str, password: Option<&str>) -> Result<String, String> {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let mut target = std::env::temp_dir().join(format!("zarg-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&target).map_err(|error| error.to_string())?;
    target.push(path.rsplit('/').next().unwrap_or(path));
    let passwords = PasswordSet::new(password.map(|password| password.as_bytes().to_vec()));
    operations::extract_single_file(archive, path, &target, &passwords)
        .map_err(|error| error.to_string())?;
    open_with_default_program(&target)?;
    Ok(format!("Opened {path}"))
}

fn open_with_default_program(file: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", ""]).arg(file);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(file);
        command
    };
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(file);
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Cannot open {}: {error}", file.display()))
}

fn finish_with_events(events: Sender<UiEvent>, outcome: Result<String, String>) {
    let _ = events.send(UiEvent::Finished(outcome));
}

fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

enum EntryFilter {
    All,
    Substring(String, bool),
    Glob(globset::GlobMatcher),
    Regex(regex::Regex),
}

impl EntryFilter {
    fn new(text: &str, use_regex: bool, match_case: bool) -> Result<Self, String> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(Self::All);
        }
        if use_regex {
            return regex::RegexBuilder::new(text)
                .case_insensitive(!match_case)
                .build()
                .map(Self::Regex)
                .map_err(|error| error.to_string());
        }
        // Text without wildcards keeps matching as a plain substring.
        if !text.contains(['*', '?', '[']) {
            return Ok(Self::Substring(text.to_owned(), match_case));
        }
        globset::GlobBuilder::new(text)
            .case_insensitive(!match_case)
            .literal_separator(false)
            .backslash_escape(false)
            .build()
            .map(|glob| Self::Glob(glob.compile_matcher()))
            .map_err(|error| error.to_string())
    }

    fn matches(&self, path: &str) -> bool {
        match self {
            Self::All => true,
            Self::Substring(text, true) => path.contains(text.as_str()),
            Self::Substring(text, false) => path.to_lowercase().contains(&text.to_lowercase()),
            Self::Glob(glob) => {
                glob.is_match(path) || glob.is_match(path.rsplit('/').next().unwrap_or(path))
            }
            Self::Regex(regex) => regex.is_match(path),
        }
    }
}

fn filtered_entry_indices(entries: &[(String, EntryData)], filter: &EntryFilter) -> Vec<usize> {
    entries
        .iter()
        .enumerate()
        .filter_map(|(index, (path, _))| filter.matches(path).then_some(index))
        .collect()
}

fn format_modified(details: Option<&ArchiveEntryDetails>) -> String {
    details
        .and_then(|details| details.modified_unix_millis)
        .map(format_unix_millis)
        .unwrap_or_else(|| "—".to_owned())
}

fn format_owner(details: Option<&ArchiveEntryDetails>) -> String {
    details
        .and_then(|details| match (details.owner, details.group) {
            (Some(owner), Some(group)) => Some(format!("{owner}:{group}")),
            (Some(owner), None) => Some(owner.to_string()),
            _ => None,
        })
        .unwrap_or_else(|| "—".to_owned())
}

fn format_permissions(details: Option<&ArchiveEntryDetails>) -> String {
    details
        .and_then(|details| {
            details.permissions.map(|permissions| {
                if details.metadata_platform.as_deref() == Some("unix") {
                    format!("{permissions:04o}")
                } else if permissions == 0 {
                    "writable".to_owned()
                } else {
                    "read-only".to_owned()
                }
            })
        })
        .unwrap_or_else(|| "—".to_owned())
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct SortKey {
    missing: bool,
    number: i128,
    text: String,
}

/// Ascending order of the key matches the displayed order: newest date, largest
/// size, and lowest (best) ratio first; entries without the value come last.
fn sort_key(
    sort_by: SortBy,
    path: &str,
    data: &EntryData,
    details: &std::collections::BTreeMap<String, ArchiveEntryDetails>,
) -> SortKey {
    let (number, text) = match sort_by {
        SortBy::Name => (Some(0), String::new()),
        SortBy::Date => (
            details
                .get(path)
                .and_then(|details| details.modified_unix_millis)
                .map(|date| -i128::from(date)),
            String::new(),
        ),
        SortBy::CompressedSize => (
            details
                .get(path)
                .and_then(|details| details.compressed_size)
                .map(|size| -i128::from(size)),
            String::new(),
        ),
        SortBy::UncompressedSize => (
            match data {
                EntryData::File(size) => Some(-i128::from(*size)),
                _ => None,
            },
            String::new(),
        ),
        SortBy::Ratio => (
            match (data, details.get(path).and_then(|d| d.compressed_size)) {
                (EntryData::File(size), Some(compressed)) if *size > 0 => {
                    Some(i128::from(compressed) * 1_000_000 / i128::from(*size))
                }
                _ => None,
            },
            String::new(),
        ),
        SortBy::Type => (Some(0), entry_type_key(path, data)),
    };
    SortKey {
        missing: number.is_none(),
        number: number.unwrap_or(0),
        text,
    }
}

fn sort_entry_indices(
    indices: &mut [usize],
    entries: &[(String, EntryData)],
    details: &std::collections::BTreeMap<String, ArchiveEntryDetails>,
    sort_by: SortBy,
) {
    // Keys are computed once per entry; the stable sorts keep name order for ties.
    indices.sort_by(|left, right| entries[*left].0.cmp(&entries[*right].0));
    if sort_by != SortBy::Name {
        indices.sort_by_cached_key(|index| {
            let (path, data) = &entries[*index];
            sort_key(sort_by, path, data, details)
        });
    }
}

fn entry_type_key(path: &str, data: &EntryData) -> String {
    match data {
        EntryData::Directory => "0-folder".to_owned(),
        EntryData::File(_) => format!(
            "1-{}",
            std::path::Path::new(path)
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
        ),
        EntryData::Symlink(_) => "2-link".to_owned(),
        EntryData::NamedPipe => "3-pipe".to_owned(),
        EntryData::Device(_) => "4-device".to_owned(),
    }
}

pub(crate) fn format_unix_millis(millis: i64) -> String {
    let seconds = millis.div_euclid(1_000);
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let shifted_days = days + 719_468;
    let era = if shifted_days >= 0 {
        shifted_days
    } else {
        shifted_days - 146_096
    } / 146_097;
    let day_of_era = shifted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let hour = day_seconds / 3_600;
    let minute = day_seconds % 3_600 / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

fn structured_rows(
    entries: &[(String, EntryData)],
    visible_indices: &[usize],
    expanded_dirs: &BTreeSet<String>,
    reveal_filtered: bool,
    details: &std::collections::BTreeMap<String, ArchiveEntryDetails>,
    sort_by: SortBy,
    reversed: bool,
) -> Vec<StructuredRow> {
    let mut nodes = std::collections::BTreeMap::<String, EntryData>::new();
    for &index in visible_indices {
        let (path, data) = &entries[index];
        let components = path.split('/').filter(|component| !component.is_empty());
        let mut current = String::new();
        let components = components.collect::<Vec<_>>();
        for (component_index, component) in components.iter().enumerate() {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(component);
            if component_index + 1 == components.len() {
                nodes.insert(current.clone(), data.clone());
            } else {
                nodes.entry(current.clone()).or_insert(EntryData::Directory);
            }
        }
    }

    let mut children = std::collections::BTreeMap::<String, Vec<String>>::new();
    for path in nodes.keys() {
        let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
        children
            .entry(parent.to_owned())
            .or_default()
            .push(path.clone());
    }

    #[allow(clippy::too_many_arguments)]
    fn append_children(
        parent: &str,
        depth: usize,
        nodes: &std::collections::BTreeMap<String, EntryData>,
        children: &std::collections::BTreeMap<String, Vec<String>>,
        expanded_dirs: &BTreeSet<String>,
        reveal_filtered: bool,
        details: &std::collections::BTreeMap<String, ArchiveEntryDetails>,
        sort_by: SortBy,
        reversed: bool,
        rows: &mut Vec<StructuredRow>,
    ) {
        let Some(child_paths) = children.get(parent) else {
            return;
        };
        let mut ordered_paths = child_paths.clone();
        ordered_paths.sort_by_cached_key(|path| {
            let data = nodes.get(path).expect("tree child has a node");
            (sort_key(sort_by, path, data, details), path.clone())
        });
        if reversed {
            ordered_paths.reverse();
        }
        for path in &ordered_paths {
            let grandchildren = children.get(path).map(Vec::as_slice).unwrap_or_default();
            let has_children = !grandchildren.is_empty();
            let expanded = has_children && (reveal_filtered || expanded_dirs.contains(path));
            let data = if has_children {
                EntryData::Directory
            } else {
                nodes[path].clone()
            };
            rows.push(StructuredRow {
                path: path.clone(),
                data,
                depth,
                has_children,
                expanded,
                child_count: grandchildren.len(),
            });
            if expanded {
                append_children(
                    path,
                    depth + 1,
                    nodes,
                    children,
                    expanded_dirs,
                    reveal_filtered,
                    details,
                    sort_by,
                    reversed,
                    rows,
                );
            }
        }
    }

    let mut rows = Vec::with_capacity(nodes.len());
    append_children(
        "",
        0,
        &nodes,
        &children,
        expanded_dirs,
        reveal_filtered,
        details,
        sort_by,
        reversed,
        &mut rows,
    );
    rows
}

struct ExtendedSelection<T> {
    selected: BTreeSet<T>,
    anchor: Option<T>,
}

fn extended_selection<T: Clone + Ord>(
    visible: &[T],
    current: &BTreeSet<T>,
    anchor: &Option<T>,
    clicked: &T,
    modifiers: egui::Modifiers,
) -> ExtendedSelection<T> {
    let additive = modifiers.command || modifiers.ctrl;
    let mut selected = current.clone();

    let next_anchor = if modifiers.shift {
        let anchor = anchor
            .as_ref()
            .filter(|anchor| visible.contains(anchor))
            .unwrap_or(clicked);
        if let (Some(start), Some(end)) = (
            visible.iter().position(|item| item == anchor),
            visible.iter().position(|item| item == clicked),
        ) {
            if !additive {
                selected.clear();
            }
            for item in &visible[start.min(end)..=start.max(end)] {
                selected.insert(item.clone());
            }
        } else {
            selected.clear();
            selected.insert(clicked.clone());
        }
        Some(anchor.clone())
    } else if additive {
        if !selected.remove(clicked) {
            selected.insert(clicked.clone());
        }
        Some(clicked.clone())
    } else {
        selected.clear();
        selected.insert(clicked.clone());
        Some(clicked.clone())
    };

    ExtendedSelection {
        selected,
        anchor: next_anchor,
    }
}

fn valid_destination_path(value: &str) -> bool {
    let path = PathBuf::from(value.trim());
    if path.as_os_str().is_empty() {
        return false;
    }
    let mut ancestor = path.as_path();
    loop {
        if ancestor.exists() {
            return ancestor.is_dir();
        }
        let Some(parent) = ancestor.parent() else {
            return false;
        };
        if parent == ancestor {
            return false;
        }
        ancestor = parent;
    }
}

fn selected_commit_label(selected: &str, commits: &[String]) -> String {
    if selected.is_empty() {
        return "Select commit".to_owned();
    }
    if commits.last().is_some_and(|last| last == selected) {
        format!("Last commit ({selected})")
    } else {
        selected.to_owned()
    }
}

fn password_requirements(password: &str) -> Vec<&'static str> {
    let mut missing = Vec::new();
    if password.chars().count() < 12 {
        missing.push("at least 12 characters");
    }
    if !password.chars().any(char::is_uppercase) {
        missing.push("at least one uppercase letter");
    }
    if !password.chars().any(char::is_lowercase) {
        missing.push("at least one lowercase letter");
    }
    if !password.chars().any(char::is_numeric) {
        missing.push("at least one digit");
    }
    if !password
        .chars()
        .any(|character| !character.is_alphanumeric())
    {
        missing.push("at least one symbol");
    }
    missing
}

fn parse_block_size(value: &str) -> Result<usize, Box<dyn std::error::Error>> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("auto") {
        return Ok(0);
    }
    let (digits, multiplier) = match value.as_bytes().last().copied() {
        Some(b'k' | b'K') => (&value[..value.len() - 1], 1024_usize),
        Some(b'm' | b'M') => (&value[..value.len() - 1], 1024 * 1024),
        _ => (value, 1),
    };
    let size = digits
        .parse::<usize>()
        .ok()
        .and_then(|size| size.checked_mul(multiplier))
        .ok_or("invalid block size")?;
    if !(64 * 1024..=32 * 1024 * 1024).contains(&size) {
        return Err("block size must be 64 KiB through 32 MiB, or `auto`".into());
    }
    Ok(size)
}

fn collision_policy(index: usize) -> CollisionPolicy {
    match index {
        0 => CollisionPolicy::Error,
        1 => CollisionPolicy::Latest,
        2 => CollisionPolicy::First,
        4 => CollisionPolicy::Ask,
        _ => CollisionPolicy::Rename,
    }
}

fn existing_policy(index: usize) -> ExistingPolicy {
    match index {
        0 => ExistingPolicy::Overwrite,
        1 => ExistingPolicy::Skip,
        3 => ExistingPolicy::Ask,
        _ => ExistingPolicy::Error,
    }
}

fn info_ratio(compressed: u64, uncompressed: u64) -> Option<f64> {
    (uncompressed > 0).then(|| compressed as f64 / uncompressed as f64)
}

fn info_row(ui: &mut egui::Ui, label: &str, value: impl Into<egui::WidgetText>) {
    ui.label(egui::RichText::new(label).weak());
    ui.label(value);
    ui.end_row();
}

fn info_ratio_bar(ui: &mut egui::Ui, ratio: Option<f64>) {
    match ratio {
        Some(ratio) => {
            ui.add(
                egui::ProgressBar::new(ratio.clamp(0.0, 1.0) as f32)
                    .desired_width(110.0)
                    .text(format!("{:.1}%", ratio * 100.0)),
            );
        }
        None => {
            ui.label("n/a");
        }
    }
}

fn render_info_contents(ui: &mut egui::Ui, view: &ArchiveInfoView) {
    let stored: u64 = view.commits.iter().map(|commit| commit.size).sum();
    let uncompressed: Option<u64> = view
        .commits
        .iter()
        .map(|commit| commit.uncompressed_size)
        .sum();
    ui.heading("Summary");
    egui::Grid::new("info_summary")
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            info_row(ui, "Archive", view.archive.as_str());
            info_row(ui, "Commits", view.commits.len().to_string());
            info_row(ui, "Stored size", display_size(stored));
            if let Some(total) = uncompressed {
                info_row(ui, "Uncompressed size", display_size(total));
                ui.label(egui::RichText::new("Ratio").weak());
                info_ratio_bar(ui, info_ratio(stored, total));
                ui.end_row();
            }
        });
    ui.add_space(6.0);
    ui.heading("Custom metadata");
    if view.metadata.is_empty() {
        ui.weak("None");
    } else {
        egui::Grid::new("info_metadata")
            .num_columns(2)
            .spacing([16.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                for (key, value) in &view.metadata {
                    ui.label(egui::RichText::new(key).strong());
                    ui.label(value);
                    ui.end_row();
                }
            });
    }
    ui.add_space(6.0);
    ui.heading("Commits");
    let last = view.commits.len().saturating_sub(1);
    for (index, commit) in view.commits.iter().enumerate() {
        let id = format!("{:08x}", u32::from_le_bytes(commit.id));
        egui::CollapsingHeader::new(format!("Commit {id} - {}", display_size(commit.size)))
            .id_salt(("info_commit", index))
            .default_open(index == last)
            .show(ui, |ui| render_commit_info(ui, index, commit));
    }
}

fn render_commit_info(ui: &mut egui::Ui, index: usize, commit: &CommitInfo) {
    let scope = match (commit.header_encrypted, commit.data_encrypted) {
        (false, false) => "none",
        (false, true) => "data",
        (true, false) => "header",
        (true, true) => "header and data",
    };
    let algorithm = commit
        .encryption_algorithm
        .map(|algorithm| match algorithm {
            ContentAead::Aes256Gcm => "AES-256-GCM",
            ContentAead::ChaCha20Poly1305 => "ChaCha20-Poly1305",
        });
    egui::Grid::new(("info_commit_grid", index))
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            info_row(ui, "Stored size", display_size(commit.size));
            info_row(ui, "Header", display_size(u64::from(commit.header_size)));
            info_row(ui, "Data", display_size(commit.data_size));
            info_row(
                ui,
                "Uncompressed",
                commit
                    .uncompressed_size
                    .map(display_size)
                    .unwrap_or_else(|| "unavailable (encrypted header)".to_owned()),
            );
            ui.label(egui::RichText::new("Ratio").weak());
            info_ratio_bar(ui, commit.ratio_percent.map(|percent| percent / 100.0));
            ui.end_row();
            if let Some(header) = &commit.header_compression {
                info_row(
                    ui,
                    "Header compression",
                    format!(
                        "{} -> {}{}",
                        header
                            .uncompressed_size
                            .map(display_size)
                            .unwrap_or_else(|| "?".to_owned()),
                        display_size(header.compressed_size),
                        header
                            .ratio_percent
                            .map(|ratio| format!(" ({ratio:.1}%)"))
                            .unwrap_or_default()
                    ),
                );
            }
            info_row(
                ui,
                "Encryption",
                match algorithm {
                    Some(algorithm) => format!("{scope} ({algorithm})"),
                    None => scope.to_owned(),
                },
            );
        });
    let Some(details) = &commit.details else {
        if commit.header_encrypted {
            ui.weak("File statistics unavailable (encrypted header)");
        }
        return;
    };
    ui.add_space(4.0);
    ui.label(egui::RichText::new("File groups").strong());
    egui::Grid::new(("info_groups", index))
        .num_columns(5)
        .spacing([16.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            for header in ["Group", "Files", "Stored", "Uncompressed", "Ratio"] {
                ui.label(egui::RichText::new(header).weak());
            }
            ui.end_row();
            for name in [
                "empty files",
                "tiny files",
                "small files",
                "big files",
                "files (block size unknown)",
            ] {
                let Some(group) = details.groups.get(name) else {
                    continue;
                };
                ui.label(name);
                ui.label(group.files.to_string());
                ui.label(display_size(group.compressed_size));
                ui.label(display_size(group.uncompressed_size));
                info_ratio_bar(
                    ui,
                    info_ratio(group.compressed_size, group.uncompressed_size),
                );
                ui.end_row();
            }
        });
    ui.add_space(4.0);
    egui::Grid::new(("info_extra", index))
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            if details.deduplicated_files > 0 {
                info_row(
                    ui,
                    "Deduplicated files",
                    format!(
                        "{} (saved {})",
                        details.deduplicated_files,
                        display_size(details.deduplicated_uncompressed_size)
                    ),
                );
            }
            if details.symlinks > 0 || details.named_pipes > 0 || details.devices > 0 {
                info_row(
                    ui,
                    "Special entries",
                    format!(
                        "{} symlinks, {} pipes, {} devices",
                        details.symlinks, details.named_pipes, details.devices
                    ),
                );
            }
            let metadata = &details.entry_metadata;
            let saved = [
                ("owner", metadata.owner),
                ("group", metadata.group),
                ("permissions", metadata.permissions),
                ("date", metadata.dates),
            ]
            .iter()
            .filter(|(_, count)| *count > 0)
            .map(|(name, count)| format!("{name} ({count})"))
            .collect::<Vec<_>>();
            info_row(
                ui,
                "Saved metadata",
                if saved.is_empty() {
                    "none".to_owned()
                } else {
                    saved.join(", ")
                },
            );
        });
}

fn display_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

fn check_summary(report: &CheckReport) -> String {
    let mut summary = format!(
        "Validated {} commit(s); checked {} unencrypted block(s); {} encrypted block(s) not content-checked; {} recovery gap(s).",
        report.commits, report.checked_blocks, report.encrypted_blocks, report.recoveries
    );
    if !report.recovery_messages.is_empty() {
        summary.push('\n');
        summary.push_str(&report.recovery_messages.join("\n"));
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_contents_default_to_normal_view() {
        assert_eq!(ArchiveView::default(), ArchiveView::Normal);
    }

    #[test]
    fn destination_path_requires_existing_directory_or_parent() {
        let directory =
            std::env::temp_dir().join(format!("zarg-destination-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        assert!(valid_destination_path(directory.to_str().unwrap()));
        assert!(valid_destination_path(
            directory.join("new-output").to_str().unwrap()
        ));
        assert!(valid_destination_path(
            directory.join("missing-parent/output").to_str().unwrap()
        ));
        let file = directory.join("existing-file");
        std::fs::write(&file, b"test").unwrap();
        assert!(!valid_destination_path(file.to_str().unwrap()));
        assert!(!valid_destination_path(""));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn named_archive_uses_selected_item_stem_and_parent() {
        let parent = std::env::temp_dir();
        assert_eq!(
            named_archive_path(&parent.join("a file.txt")).unwrap(),
            parent.join("a file.zar")
        );
        assert_eq!(
            named_archive_path(std::path::Path::new("relative-folder")).unwrap(),
            PathBuf::from("relative-folder.zar")
        );
    }

    #[test]
    fn archive_folder_destination_uses_archive_name_and_parent() {
        let parent = std::env::temp_dir();
        assert_eq!(
            archive_folder_destination(&parent.join("foo.zar")).unwrap(),
            parent.join("foo")
        );
        assert_eq!(
            archive_folder_destination(std::path::Path::new("foo.zar")).unwrap(),
            PathBuf::from("foo")
        );
    }

    #[test]
    fn create_context_action_prefills_archive_path_from_first_selection() {
        let directory = std::env::temp_dir().join(format!(
            "zarg-create-context-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let first = directory.join("first file.txt");
        let second = directory.join("second.txt");
        std::fs::write(&first, b"first").unwrap();
        std::fs::write(&second, b"second").unwrap();

        let app = ZargApp::from_context_action(
            ContextAction::Create,
            vec![first.clone(), second.clone()],
        )
        .unwrap();

        assert_eq!(
            app.operation.archive,
            directory
                .join("first file.zar")
                .to_string_lossy()
                .into_owned()
        );
        assert_eq!(app.operation.input_paths, [first, second]);

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn password_requirements_report_missing_rules() {
        assert_eq!(
            password_requirements("short"),
            vec![
                "at least 12 characters",
                "at least one uppercase letter",
                "at least one digit",
                "at least one symbol",
            ]
        );
        assert!(password_requirements("Correct-Horse9").is_empty());
    }

    #[test]
    fn block_sizes_match_cli_limits() {
        assert_eq!(parse_block_size("64K").unwrap(), 64 * 1024);
        assert_eq!(parse_block_size("1M").unwrap(), 1024 * 1024);
        assert_eq!(parse_block_size("auto").unwrap(), 0);
        assert!(parse_block_size("63K").is_err());
        assert!(parse_block_size("33M").is_err());
    }

    #[test]
    fn archive_list_cells_preserve_column_widths() {
        let mut row_width = 0.0;
        egui::__run_test_ui(|ui| {
            ui.horizontal(|ui| {
                left_aligned_cell(ui, 110.0, 20.0, egui::Label::new("1.2 KiB"));
                left_aligned_cell(ui, 110.0, 20.0, egui::Label::new("4.8 KiB"));
                row_width = ui.min_rect().width();
            });
        });
        assert!(row_width >= 220.0, "row width collapsed to {row_width}");
    }

    #[test]
    fn repeated_filter_text_lines_are_parsed() {
        assert_eq!(lines(" *.rs \n\n*.toml"), ["*.rs", "*.toml"]);
    }

    #[test]
    fn archive_list_filter_returns_matching_entry_indexes() {
        let entries = vec![
            ("docs/ReadMe.md".to_owned(), EntryData::File(12)),
            ("src/main.rs".to_owned(), EntryData::File(20)),
            ("docs/guide.md".to_owned(), EntryData::File(30)),
        ];
        assert_eq!(
            filtered_entry_indices(&entries, &EntryFilter::new("", false, false).unwrap()),
            [0, 1, 2]
        );
        assert_eq!(
            filtered_entry_indices(&entries, &EntryFilter::new("DOCS/", false, false).unwrap()),
            [0, 2]
        );
    }

    #[test]
    fn archive_entries_sort_by_requested_criterion() {
        let entries = vec![
            ("b.txt".to_owned(), EntryData::File(10)),
            ("a.rs".to_owned(), EntryData::File(100)),
            ("dir".to_owned(), EntryData::Directory),
            ("c.txt".to_owned(), EntryData::File(20)),
        ];
        let details = std::collections::BTreeMap::from([
            (
                "b.txt".to_owned(),
                ArchiveEntryDetails {
                    modified_unix_millis: Some(100),
                    ..ArchiveEntryDetails::default()
                },
            ),
            (
                "a.rs".to_owned(),
                ArchiveEntryDetails {
                    modified_unix_millis: Some(300),
                    ..ArchiveEntryDetails::default()
                },
            ),
            (
                "dir".to_owned(),
                ArchiveEntryDetails {
                    modified_unix_millis: Some(200),
                    ..ArchiveEntryDetails::default()
                },
            ),
        ]);
        let mut indices = (0..entries.len()).collect::<Vec<_>>();

        sort_entry_indices(&mut indices, &entries, &details, SortBy::Name);
        assert_eq!(indices, [1, 0, 3, 2]);
        sort_entry_indices(&mut indices, &entries, &details, SortBy::Date);
        assert_eq!(indices, [1, 2, 0, 3]);
        sort_entry_indices(&mut indices, &entries, &details, SortBy::UncompressedSize);
        assert_eq!(indices, [1, 3, 0, 2]);
        sort_entry_indices(&mut indices, &entries, &details, SortBy::Type);
        assert_eq!(indices, [2, 1, 0, 3]);

        let sizes = std::collections::BTreeMap::from([
            (
                "b.txt".to_owned(),
                ArchiveEntryDetails {
                    compressed_size: Some(5),
                    ..ArchiveEntryDetails::default()
                },
            ),
            (
                "a.rs".to_owned(),
                ArchiveEntryDetails {
                    compressed_size: Some(20),
                    ..ArchiveEntryDetails::default()
                },
            ),
            (
                "c.txt".to_owned(),
                ArchiveEntryDetails {
                    compressed_size: Some(18),
                    ..ArchiveEntryDetails::default()
                },
            ),
        ]);
        sort_entry_indices(&mut indices, &entries, &sizes, SortBy::CompressedSize);
        assert_eq!(indices, [1, 3, 0, 2]);
        // Ratios: a.rs 20%, b.txt 50%, c.txt 90%; the folder has none.
        sort_entry_indices(&mut indices, &entries, &sizes, SortBy::Ratio);
        assert_eq!(indices, [1, 0, 3, 2]);
    }

    #[test]
    fn modified_time_format_uses_utc_and_handles_pre_epoch_values() {
        assert_eq!(format_unix_millis(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_unix_millis(-1), "1969-12-31 23:59 UTC");
    }

    #[test]
    fn structured_rows_build_and_expand_directory_hierarchy() {
        let entries = vec![
            ("folder/a.txt".to_owned(), EntryData::File(12)),
            ("folder/sub/b.txt".to_owned(), EntryData::File(24)),
            ("loose.txt".to_owned(), EntryData::File(6)),
        ];
        let all = [0, 1, 2];
        let details = std::collections::BTreeMap::new();
        let collapsed = structured_rows(
            &entries,
            &all,
            &BTreeSet::new(),
            false,
            &details,
            SortBy::Name,
            false,
        );
        assert_eq!(
            collapsed
                .iter()
                .map(|row| row.path.as_str())
                .collect::<Vec<_>>(),
            ["folder", "loose.txt"]
        );
        let expanded = structured_rows(
            &entries,
            &all,
            &BTreeSet::from(["folder".to_owned()]),
            false,
            &details,
            SortBy::Name,
            false,
        );
        assert_eq!(
            expanded
                .iter()
                .map(|row| (row.path.as_str(), row.depth))
                .collect::<Vec<_>>(),
            [
                ("folder", 0),
                ("folder/a.txt", 1),
                ("folder/sub", 1),
                ("loose.txt", 0)
            ]
        );
    }

    #[test]
    fn structured_filter_reveals_matching_entry_ancestors() {
        let entries = vec![
            ("folder/sub/match.txt".to_owned(), EntryData::File(12)),
            ("folder/other.txt".to_owned(), EntryData::File(24)),
        ];
        let rows = structured_rows(
            &entries,
            &[0],
            &BTreeSet::new(),
            true,
            &std::collections::BTreeMap::new(),
            SortBy::Name,
            false,
        );
        assert_eq!(
            rows.iter().map(|row| row.path.as_str()).collect::<Vec<_>>(),
            ["folder", "folder/sub", "folder/sub/match.txt"]
        );
    }

    #[test]
    fn extended_selection_supports_replace_toggle_and_ranges() {
        let visible = [1, 2, 3, 4, 5];
        let empty = BTreeSet::new();
        let plain = extended_selection(&visible, &empty, &None, &2, egui::Modifiers::NONE);
        assert_eq!(plain.selected, BTreeSet::from([2]));
        let toggled = extended_selection(
            &visible,
            &plain.selected,
            &plain.anchor,
            &4,
            egui::Modifiers {
                ctrl: true,
                ..Default::default()
            },
        );
        assert_eq!(toggled.selected, BTreeSet::from([2, 4]));
        let ranged = extended_selection(
            &visible,
            &toggled.selected,
            &Some(2),
            &5,
            egui::Modifiers {
                shift: true,
                ..Default::default()
            },
        );
        assert_eq!(ranged.selected, BTreeSet::from([2, 3, 4, 5]));
        let additive_range = extended_selection(
            &visible,
            &plain.selected,
            &Some(2),
            &4,
            egui::Modifiers {
                shift: true,
                ctrl: true,
                ..Default::default()
            },
        );
        assert_eq!(additive_range.selected, BTreeSet::from([2, 3, 4]));
    }

    #[test]
    fn file_icons_follow_mime_type_extensions() {
        assert_eq!(file_icon("images/photo.JPEG"), UiIcon::Image);
        assert_eq!(file_icon("music/song.flac"), UiIcon::Audio);
        assert_eq!(file_icon("src/main.rs"), UiIcon::Code);
        assert_eq!(file_icon("archives/backup.ZAR"), UiIcon::ZarArchive);
        assert_eq!(file_icon("archives/backup.zip"), UiIcon::Archive);
        assert_eq!(file_icon("unknown.data"), UiIcon::Generic);
    }

    #[test]
    fn bundled_kde_icons_are_valid_images() {
        let icons = [
            UiIcon::OpenArchive,
            UiIcon::CreateArchive,
            UiIcon::Quit,
            UiIcon::SelectAll,
            UiIcon::ClearSelection,
            UiIcon::Refresh,
            UiIcon::ListContents,
            UiIcon::TestArchive,
            UiIcon::Join,
            UiIcon::About,
            UiIcon::Add,
            UiIcon::AddFolder,
            UiIcon::Extract,
            UiIcon::Info,
            UiIcon::Remove,
            UiIcon::Browse,
            UiIcon::ClearInputs,
            UiIcon::Confirm,
            UiIcon::Cancel,
            UiIcon::AddFilterList,
            UiIcon::Folder,
            UiIcon::Symlink,
            UiIcon::Image,
            UiIcon::Audio,
            UiIcon::Video,
            UiIcon::Archive,
            UiIcon::ZarArchive,
            UiIcon::Pdf,
            UiIcon::Code,
            UiIcon::Python,
            UiIcon::Java,
            UiIcon::Script,
            UiIcon::JavaScript,
            UiIcon::Text,
            UiIcon::Word,
            UiIcon::Spreadsheet,
            UiIcon::Presentation,
            UiIcon::Generic,
            UiIcon::SpecialData,
        ];
        for icon in icons {
            image::load_from_memory(icon_bytes(icon))
                .unwrap_or_else(|error| panic!("invalid bundled icon {icon:?}: {error}"));
        }
    }

    #[test]
    fn invert_selection_flips_visible_entries_only() {
        let selected = BTreeSet::from(["a".to_owned(), "hidden".to_owned()]);
        let visible = ["a".to_owned(), "b".to_owned(), "c".to_owned()];
        assert_eq!(
            invert_selection(&selected, &visible),
            BTreeSet::from(["b".to_owned(), "c".to_owned(), "hidden".to_owned()])
        );
    }

    #[test]
    fn filter_supports_wildcards_and_regex() {
        let entries = [
            ("docs/Readme.md", EntryData::File(1)),
            ("src/main.rs", EntryData::File(1)),
            ("src/lib.rs", EntryData::File(1)),
        ]
        .map(|(path, data)| (path.to_owned(), data));
        let run = |text: &str, regex: bool| {
            filtered_entry_indices(&entries, &EntryFilter::new(text, regex, false).unwrap())
        };
        assert_eq!(run("*.RS", false), [1, 2]);
        assert_eq!(run("m?in.rs", false), [1]);
        assert_eq!(run("src/*", false), [1, 2]);
        assert_eq!(run(r"^src/(main|lib)\.rs$", true), [1, 2]);
        assert_eq!(run("readme", true), [0]);
        assert!(EntryFilter::new("(", true, false).is_err());
        let case = |text: &str, regex: bool| {
            filtered_entry_indices(&entries, &EntryFilter::new(text, regex, true).unwrap())
        };
        assert!(case("*.RS", false).is_empty());
        assert!(case("README", false).is_empty());
        assert_eq!(case("Readme", false), [0]);
        assert!(case("readme", true).is_empty());
    }

    #[test]
    fn recent_archives_are_unique_newest_first_and_capped() {
        let mut recent = Vec::new();
        for index in 0..12 {
            remember_recent_archive(&mut recent, &format!("/a{index}.zar"));
        }
        remember_recent_archive(&mut recent, "/a5.zar");
        assert_eq!(recent.len(), MAX_RECENT_ARCHIVES);
        assert_eq!(recent[0], "/a5.zar");
        assert_eq!(recent.iter().filter(|path| *path == "/a5.zar").count(), 1);
        assert_eq!(recent[1], "/a11.zar");
    }

    #[test]
    fn benchmark_reports_sizes_and_cleans_up() {
        let dir = std::env::temp_dir().join(format!("zarg-bench-test-{}", std::process::id()));
        let data = dir.join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("a.txt"), vec![b'a'; 100_000]).unwrap();
        std::fs::write(data.join("b.txt"), b"hello").unwrap();
        let archive = dir.join("in.zar").to_string_lossy().into_owned();
        operations::create_archive(&archive, &[data]).unwrap();
        let result = run_benchmark(
            &archive,
            None,
            &BenchmarkSettings::default(),
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(result.files, 2);
        assert_eq!(result.uncompressed, 100_005);
        assert!(result.archive_size > 0 && result.archive_size < result.uncompressed);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
