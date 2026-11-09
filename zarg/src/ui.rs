use eframe::egui;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use zarlib::operations::{
    self, AddInputOptions, ArchiveEntryDetails, BlockInfo, CheckReport, CollisionPolicy,
    CommitInfoWithBlocks, CreateFilterOptions, CreateInputOptions, EncodingOptions, ExistingPolicy,
    ExtractionOptions, FilterSet, PasswordSet, Progress,
};
use zarlib::{ArchivePath, ContentAead, EncryptionScope, Toc, TocEntry};
mod dialogs;
use dialogs::OperationKind;
use dialogs::benchmark::BenchmarkState;

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
    SaveArchive,
    NewArchive,
    Quit,
    SelectAll,
    ClearSelection,
    Refresh,
    ListContents,
    Tools,
    Configure,
    TestArchive,
    Undo,
    ChangeCommits,
    Benchmark,
    Join,
    About,
    Add,
    AddFolder,
    OnlineHelp,
    Extract,
    Info,
    Remove,
    Browse,
    ClearInputs,
    Confirm,
    ConfirmApply,
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
    Error,
    Question,
}

fn icon_bytes(icon: UiIcon) -> &'static [u8] {
    match icon {
        UiIcon::Application => include_bytes!("../assets/zarg-icon.png"),
        UiIcon::NewArchive => include_bytes!("../assets/icons/actions/document-new.png"),
        UiIcon::OpenArchive => include_bytes!("../assets/icons/actions/document-open.png"),
        UiIcon::SaveArchive => include_bytes!("../assets/icons/actions/document-save.png"),
        UiIcon::Quit => include_bytes!("../assets/icons/actions/application-exit.png"),
        UiIcon::SelectAll => include_bytes!("../assets/icons/actions/edit-select-all.png"),
        UiIcon::ClearSelection | UiIcon::ClearInputs => {
            include_bytes!("../assets/icons/actions/edit-clear.png")
        }
        UiIcon::Refresh => include_bytes!("../assets/icons/actions/view-refresh.png"),
        UiIcon::ListContents => {
            include_bytes!("../assets/icons/actions/document-preview-archive.png")
        }
        UiIcon::Tools => include_bytes!("../assets/icons/actions/tools.png"),
        UiIcon::Configure => include_bytes!("../assets/icons/actions/configure.png"),
        UiIcon::TestArchive => include_bytes!("../assets/icons/actions/archive-test.png"),
        UiIcon::ChangeCommits => include_bytes!("../assets/icons/actions/change-commits.png"),
        UiIcon::Undo => include_bytes!("../assets/icons/actions/undo.png"),
        UiIcon::Join => include_bytes!("../assets/icons/actions/join-commits.png"),
        UiIcon::Benchmark => include_bytes!("../assets/icons/actions/benchmark.png"),
        UiIcon::About => include_bytes!("../assets/icons/actions/help-about.png"),
        UiIcon::Add => include_bytes!("../assets/icons/actions/archive-insert.png"),
        UiIcon::AddFolder => {
            include_bytes!("../assets/icons/actions/archive-insert-directory.png")
        }
        UiIcon::Extract => include_bytes!("../assets/icons/actions/archive-extract.png"),
        UiIcon::OnlineHelp => include_bytes!("../assets/icons/actions/online-help.png"),
        UiIcon::Info => include_bytes!("../assets/icons/actions/document-properties.png"),
        UiIcon::Remove => include_bytes!("../assets/icons/actions/archive-remove.png"),
        UiIcon::Browse => include_bytes!("../assets/icons/actions/document-open-folder.png"),
        UiIcon::Confirm => include_bytes!("../assets/icons/actions/dialog-ok.png"),
        UiIcon::ConfirmApply => include_bytes!("../assets/icons/actions/dialog-ok-apply.png"),
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
        UiIcon::Error => include_bytes!("../assets/icons/actions/error.png"),
        UiIcon::Question => include_bytes!("../assets/icons/actions/question.png"),
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
        Some("zip" | "7z" | "rar" | "tar" | "gz" | "tgz" | "bz2" | "xz") => UiIcon::Archive,
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

#[derive(Default)]
pub(super) struct OperationForm {
    kind: Option<OperationKind>,
    archive: String,
    archive_backend: Option<(String, String)>,
    input_paths: Vec<PathBuf>,
    selected_input_paths: BTreeSet<PathBuf>,
    input_selection_anchor: Option<PathBuf>,
    destination: String,
    from_id: String,
    to_id: String,
    join_output: String,
    undo_commits: usize,
    transform_output: String,
    transform_action: usize,
    transform_strict: bool,
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
    save_user: bool,
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
    to: String,
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
            .map_err(|_| "compression level must be an integer from -7 to 22")?;
        if !(-7..=22).contains(&compression_level) {
            return Err("compression level must be from -7 to 22".into());
        }
        Ok(EncodingOptions {
            block_size,
            compression_level,
        })
    }

    fn has_metadata_changes(&self) -> bool {
        self.meta_items
            .iter()
            .any(|(_, original, edited, remove)| *remove || original != edited)
            || self.meta_new.iter().any(|(key, _)| !key.trim().is_empty())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AddArchiveSettings {
    block_size: String,
    compression_level: String,
    include_files: String,
    exclude_files: String,
    include_folders: String,
    exclude_folders: String,
    filelists: Vec<PathBuf>,
    aead: usize,
    encryption_scope: usize,
    password_is_set: bool,
    existing_password_is_set: bool,
    strict: bool,
    metadata: bool,
    dates: bool,
    special_files: bool,
    verbose: bool,
}

impl AddArchiveSettings {
    fn from_form(form: &OperationForm) -> Self {
        Self {
            block_size: form.block_size.clone(),
            compression_level: form.compression_level.clone(),
            include_files: form.include_files.clone(),
            exclude_files: form.exclude_files.clone(),
            include_folders: form.include_folders.clone(),
            exclude_folders: form.exclude_folders.clone(),
            filelists: form.filelists.clone(),
            aead: form.aead,
            encryption_scope: form.encryption_scope,
            password_is_set: form.password_is_set,
            existing_password_is_set: form.existing_password_is_set,
            strict: form.strict,
            metadata: form.metadata,
            dates: form.dates,
            special_files: form.special_files,
            verbose: form.verbose,
        }
    }

    fn apply_to(&self, form: &mut OperationForm) {
        form.block_size.clone_from(&self.block_size);
        form.compression_level.clone_from(&self.compression_level);
        form.include_files.clone_from(&self.include_files);
        form.exclude_files.clone_from(&self.exclude_files);
        form.include_folders.clone_from(&self.include_folders);
        form.exclude_folders.clone_from(&self.exclude_folders);
        form.filelists.clone_from(&self.filelists);
        form.aead = self.aead;
        form.encryption_scope = self.encryption_scope;
        form.password_is_set = self.password_is_set;
        form.existing_password_is_set = self.existing_password_is_set;
        form.strict = self.strict;
        form.metadata = self.metadata;
        form.dates = self.dates;
        form.special_files = self.special_files;
        form.verbose = self.verbose;
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
    ImportedArchive {
        directory: tempfile::TempDir,
        path: String,
    },
    WorkingArchive {
        directory: tempfile::TempDir,
        path: String,
        logical_path: String,
        dirty: bool,
        new_archive_settings: Option<Box<AddArchiveSettings>>,
    },
    ArchiveModified,
    EntriesRemoved(Vec<String>),
    ArchiveSaved {
        path: String,
    },
    CommitIds(Vec<String>),
    ArchivePath {
        path: String,
        password: Option<String>,
    },
    Finished(Result<String, String>),
}

#[derive(Clone)]
enum ArchiveSwitchAction {
    New,
    OpenDialog,
    OpenPath(PathBuf, Option<String>),
}

pub(super) struct ArchiveInfoView {
    pub(super) archive: String,
    pub(super) commits: Vec<CommitInfoWithBlocks>,
    pub(super) metadata: Vec<(String, String)>,
}

/// Unpacks the archive to a scratch folder, then times creating and extracting a
/// new archive from it with the given settings. Nothing outside the scratch folder is written.
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
    read_archive: String,
    displayed_window_title: Option<String>,
    imported_archive_dir: Option<tempfile::TempDir>,
    working_archive_dir: Option<tempfile::TempDir>,
    has_unsaved_changes: bool,
    add_archive_settings: Option<AddArchiveSettings>,
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
    pending_archive_switch: Option<ArchiveSwitchAction>,
    pending_switch_after_save: bool,
    report: Option<String>,
    warnings: Vec<String>,
    filter_text: String,
    filter_regex: bool,
    filter_match_case: bool,
    filter_invalid: bool,
    archive_view: ArchiveView,
    sort_by: SortBy,
    sort_reversed: bool,
    details_horizontal_offset: f32,
    structured_horizontal_offset: f32,
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
    show_settings: bool,
    ui_scale: f32,
    applied_ui_scale: Option<f32>,
    theme_mode: crate::settings::ThemeMode,
    default_block_size: String,
    default_compression_level: i32,
    default_encryption_method: crate::settings::DefaultEncryptionMethod,
    archive_defaults: crate::settings::ArchiveDefaults,
    theme_controller: Option<crate::theme::ThemeController>,
    opening_archive: bool,
    commit_ids: Vec<String>,
    file_dialog: dialogs::file_picker::FileDialog,
}

impl Default for ZargApp {
    fn default() -> Self {
        Self {
            icons: RefCell::new(HashMap::new()),
            archive: String::new(),
            read_archive: String::new(),
            displayed_window_title: None,
            imported_archive_dir: None,
            working_archive_dir: None,
            has_unsaved_changes: false,
            add_archive_settings: None,
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
            pending_archive_switch: None,
            pending_switch_after_save: false,
            report: None,
            warnings: Vec::new(),
            filter_text: String::new(),
            filter_regex: false,
            filter_match_case: false,
            filter_invalid: false,
            archive_view: ArchiveView::default(),
            sort_by: SortBy::default(),
            sort_reversed: false,
            details_horizontal_offset: 0.0,
            structured_horizontal_offset: 0.0,
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
            show_settings: false,
            ui_scale: 1.0,
            applied_ui_scale: None,
            theme_mode: crate::settings::ThemeMode::System,
            default_block_size: "8M".to_owned(),
            default_compression_level: 3,
            default_encryption_method: crate::settings::DefaultEncryptionMethod::Aes256Gcm,
            archive_defaults: crate::settings::ArchiveDefaults::default(),
            theme_controller: None,
            opening_archive: false,
            commit_ids: Vec::new(),
            file_dialog: dialogs::file_picker::FileDialog::new(),
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
    pub fn from_context_action(
        action: ContextAction,
        paths: Vec<PathBuf>,
        settings: &crate::settings::Settings,
    ) -> Result<Self, String> {
        let mut app = Self::default();
        app.apply_settings(settings);
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
                    let mut form = app
                        .new_operation_form(OperationKind::NewArchive, &archive.to_string_lossy());
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
                    app.operation = app.new_operation_form(OperationKind::NewArchive, &archive);
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
                        let mut form =
                            app.new_operation_form(OperationKind::ExtractArchive, &archive);
                        form.destination = PathBuf::from(&archive)
                            .parent()
                            .unwrap_or_else(|| std::path::Path::new("."))
                            .to_string_lossy()
                            .into_owned();
                        app.start_operation(form);
                    }
                    ContextAction::ExtractTo => {
                        app.operation =
                            app.new_operation_form(OperationKind::ExtractArchive, &archive);
                        app.operation.destination = PathBuf::from(&archive)
                            .parent()
                            .unwrap_or_else(|| std::path::Path::new("."))
                            .to_string_lossy()
                            .into_owned();
                    }
                    ContextAction::ExtractToFolder => {
                        let destination =
                            archive_folder_destination(PathBuf::from(&archive).as_path())?;
                        let mut form =
                            app.new_operation_form(OperationKind::ExtractArchive, &archive);
                        form.destination = destination.to_string_lossy().into_owned();
                        app.start_operation(form);
                    }
                    ContextAction::Test => {
                        app.start_operation(
                            app.new_operation_form(OperationKind::TestArchive, &archive),
                        );
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
        self.request_archive_switch(ArchiveSwitchAction::OpenDialog);
    }

    fn request_archive_switch(&mut self, action: ArchiveSwitchAction) {
        if self.busy {
            return;
        }
        if self.has_unsaved_changes {
            self.pending_archive_switch = Some(action);
            self.pending_switch_after_save = false;
        } else {
            self.execute_archive_switch(action);
        }
    }

    fn execute_archive_switch(&mut self, action: ArchiveSwitchAction) {
        match action {
            ArchiveSwitchAction::New => self.show_operation_now(OperationKind::NewArchive),
            ArchiveSwitchAction::OpenDialog => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(
                        "All supported archives",
                        &["zar", "zip", "7z", "tar", "tgz", "tar.gz"],
                    )
                    .add_filter("ZAR archive", &["zar"])
                    .add_filter("ZIP archive", &["zip"])
                    .add_filter("7z archive", &["7z"])
                    .add_filter("TAR archive", &["tar"])
                    .add_filter("gz archive", &["tgz", "tar.gz"])
                    .pick_file()
                {
                    self.open_archive_now(path, None);
                }
            }
            ArchiveSwitchAction::OpenPath(path, password) => {
                self.open_archive_now(path, password);
            }
        }
    }

    fn open_archive(&mut self, path: PathBuf, password: Option<String>) {
        if self.busy {
            return;
        }
        if self.has_unsaved_changes {
            self.pending_archive_switch = Some(ArchiveSwitchAction::OpenPath(path, password));
            self.pending_switch_after_save = false;
        } else {
            self.open_archive_now(path, password);
        }
    }

    fn open_archive_now(&mut self, path: PathBuf, password: Option<String>) {
        self.add_archive_settings = None;
        if self.operation.kind == Some(OperationKind::AddToArchive) {
            self.operation = OperationForm::default();
        }
        let archive = path.to_string_lossy().into_owned();
        self.commit_ids.clear();
        let opened_archive = archive.clone();
        self.imported_archive_dir = None;
        self.working_archive_dir = None;
        self.has_unsaved_changes = false;
        self.read_archive.clone_from(&archive);
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
        std::thread::spawn(move || {
            let prepared_archive =
                if operations::is_supported_import_archive(PathBuf::from(&archive).as_path()) {
                    (|| {
                        let directory = tempfile::Builder::new()
                            .prefix("zarg-open-archive-")
                            .tempdir()?;
                        let imported_path = directory.path().join("opened.zar");
                        let imported_path = imported_path.to_string_lossy().into_owned();
                        operations::create_archive(&imported_path, &[PathBuf::from(&archive)])?;
                        Ok::<_, Box<dyn std::error::Error>>(Some((directory, imported_path)))
                    })()
                } else {
                    (|| {
                        let directory = tempfile::Builder::new()
                            .prefix("zarg-working-archive-")
                            .tempdir()?;
                        let working_path = directory.path().join("working.zar");
                        std::fs::copy(&archive, &working_path)?;
                        Ok::<_, Box<dyn std::error::Error>>(Some((
                            directory,
                            working_path.to_string_lossy().into_owned(),
                        )))
                    })()
                };
            let (archive_to_read, imported_directory) = match prepared_archive {
                Ok(Some((directory, imported_path))) => (imported_path, Some(directory)),
                Ok(None) => (archive.clone(), None),
                Err(error) => {
                    let error = error.to_string();
                    let _ = events_tx.send(UiEvent::Loaded(Err(error.clone())));
                    finish_with_events(events_tx, Err(error));
                    return;
                }
            };
            let is_imported =
                operations::is_supported_import_archive(PathBuf::from(&archive).as_path());
            if let Some(directory) = imported_directory {
                if is_imported {
                    let _ = events_tx.send(UiEvent::ImportedArchive {
                        directory,
                        path: archive_to_read.clone(),
                    });
                } else {
                    let _ = events_tx.send(UiEvent::WorkingArchive {
                        directory,
                        path: archive_to_read.clone(),
                        logical_path: archive.clone(),
                        dirty: false,
                        new_archive_settings: None,
                    });
                }
            }
            if !is_imported && let Ok(commits) = operations::info_archive(&archive_to_read, false) {
                let ids = commits
                    .iter()
                    .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                    .collect();
                let _ = events_tx.send(UiEvent::CommitIds(ids));
            }
            let result = operations::list_archive_with_details(
                &archive_to_read,
                None,
                (!password.is_empty()).then_some(password.as_bytes()),
            )
            .map_err(|error| error.to_string());
            match result {
                Ok(toc) => {
                    let count = toc.0.len();
                    let _ = events_tx.send(UiEvent::Loaded(Ok(toc)));
                    let summary = if is_imported {
                        format!("{count} entries loaded (read-only archive)")
                    } else {
                        format!("{count} entries loaded")
                    };
                    finish_with_events(events_tx, Ok(summary));
                }
                Err(error) => {
                    let _ = events_tx.send(UiEvent::Loaded(Err(error.clone())));
                    finish_with_events(events_tx, Err(error));
                }
            }
        });
        self.status = format!("Opened {opened_archive}");
    }

    fn discard_current_archive(&mut self) {
        self.add_archive_settings = None;
        if self.operation.kind == Some(OperationKind::AddToArchive) {
            self.operation = OperationForm::default();
        }
        self.archive.clear();
        self.read_archive.clear();
        self.location.clear();
        self.imported_archive_dir = None;
        self.working_archive_dir = None;
        self.has_unsaved_changes = false;
        self.archive_password = None;
        self.commit_ids.clear();
        self.selected.clear();
        self.selection_anchor = None;
        self.set_entries(Toc::default(), Default::default());
    }

    fn update_window_title(&mut self, context: &egui::Context) {
        let title = match Path::new(&self.archive).file_name() {
            Some(name) => format!("{} - {}", crate::WINDOW_TITLE, name.to_string_lossy()),
            None => crate::WINDOW_TITLE.to_owned(),
        };
        if self.displayed_window_title.as_ref() != Some(&title) {
            context.send_viewport_cmd_to(
                egui::ViewportId::ROOT,
                egui::ViewportCommand::Title(title.clone()),
            );
            self.displayed_window_title = Some(title);
        }
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

    fn start_operation(&mut self, mut form: OperationForm) {
        let Some(kind) = form.kind else {
            return;
        };
        let new_archive_settings = (kind == OperationKind::NewArchive)
            .then(|| Box::new(AddArchiveSettings::from_form(&form)));
        if kind == OperationKind::AddToArchive {
            self.add_archive_settings = Some(AddArchiveSettings::from_form(&form));
        }
        if kind == OperationKind::NewArchive {
            form.archive.clear();
        }
        if kind.needs_archive() && form.archive.trim().is_empty() {
            self.status = "Choose an archive first.".to_owned();
            return;
        }
        if matches!(
            kind,
            OperationKind::NewArchive | OperationKind::AddToArchive
        ) && form.password_is_set
        {
            if form.password.is_empty() {
                self.status = "Enter an encryption password.".to_owned();
                return;
            }
            if form.password != form.password_confirm {
                self.status = "The encryption passwords do not match.".to_owned();
                return;
            }
        }
        if kind == OperationKind::ChangeCommits
            && form.transform_action != 0
            && form.existing_password.is_empty()
            && (form.transform_action == 2 || form.transform_strict)
        {
            self.status = "Enter the current password for selected encrypted commits.".to_owned();
            return;
        }
        if kind == OperationKind::ChangeCommits && form.transform_action != 1 {
            if form.password.is_empty() {
                self.status = "Enter a new encryption password.".to_owned();
                return;
            }
            if form.password != form.password_confirm {
                self.status = "New encryption passwords do not match.".to_owned();
                return;
            }
        }
        if kind == OperationKind::AddToArchive
            && form.existing_password_is_set
            && form.existing_password.is_empty()
        {
            self.status = "Enter the password for existing commits.".to_owned();
            return;
        }

        let logical_archive = form.archive.clone();
        let mut new_working_archive = None;
        if kind == OperationKind::NewArchive {
            match tempfile::Builder::new()
                .prefix("zarg-working-archive-")
                .tempdir()
            {
                Ok(directory) => {
                    let path = directory.path().join("working.zar");
                    form.archive = path.to_string_lossy().into_owned();
                    new_working_archive = Some((directory, path));
                }
                Err(error) => {
                    self.status = format!("Cannot create a temporary archive: {error}");
                    return;
                }
            }
        } else if form.archive == self.archive
            && !self.archive.is_empty()
            && self.imported_archive_dir.is_none()
            && self.working_archive_dir.is_none()
            && !self.read_archive.is_empty()
        {
            let result = (|| {
                let directory = tempfile::Builder::new()
                    .prefix("zarg-working-archive-")
                    .tempdir()?;
                let path = directory.path().join("working.zar");
                std::fs::copy(&self.read_archive, &path)?;
                Ok::<_, Box<dyn std::error::Error>>((directory, path))
            })();
            match result {
                Ok((directory, path)) => {
                    self.read_archive = path.to_string_lossy().into_owned();
                    self.working_archive_dir = Some(directory);
                }
                Err(error) => {
                    self.status = format!("Cannot prepare a temporary archive: {error}");
                    return;
                }
            }
        }

        if kind == OperationKind::JoinCommits && form.join_output.trim() == logical_archive {
            form.join_output.clear();
        }
        if kind == OperationKind::ChangeCommits && form.transform_output.trim() == logical_archive {
            form.transform_output.clear();
        }

        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();
        let progress = UiProgress {
            events: events_tx.clone(),
            answers: Arc::new(Mutex::new(answer_rx)),
        };
        let archive = form
            .archive_backend
            .as_ref()
            .filter(|(source, _)| source == &form.archive)
            .map(|(_, backend)| backend.clone())
            .unwrap_or_else(|| {
                if kind != OperationKind::NewArchive
                    && form.archive == self.archive
                    && !self.read_archive.is_empty()
                    && self.imported_archive_dir.is_none()
                {
                    self.read_archive.clone()
                } else {
                    form.archive.clone()
                }
            });
        form.archive.clone_from(&archive);
        let refresh_password = if kind == OperationKind::UndoCommits {
            self.archive_password.clone()
        } else if kind == OperationKind::ChangeCommits {
            match form.transform_action {
                0 => Some(form.password.clone()),
                1 => None,
                _ => Some(form.password.clone()),
            }
        } else {
            (kind == OperationKind::AddToArchive && form.existing_password_is_set)
                .then(|| form.existing_password.clone())
                .or_else(|| form.password_is_set.then(|| form.password.clone()))
        };
        if kind == OperationKind::ChangeCommits {
            self.archive_password = refresh_password.clone();
        } else if let Some(password) = &refresh_password {
            self.archive_password = Some(password.clone());
        }
        let refresh_archive =
            if kind == OperationKind::ChangeCommits && !form.transform_output.trim().is_empty() {
                form.transform_output.trim().to_owned()
            } else {
                archive.clone()
            };
        let refresh_to = (kind == OperationKind::ListArchive && !form.to.trim().is_empty())
            .then(|| form.to.clone());
        let refresh_commit_ids = matches!(
            kind,
            OperationKind::NewArchive | OperationKind::UndoCommits | OperationKind::ChangeCommits
        );
        let is_encryption_transform = kind == OperationKind::ChangeCommits;
        let changes_archive = match kind {
            OperationKind::NewArchive
            | OperationKind::AddToArchive
            | OperationKind::RemoveEntries
            | OperationKind::UndoCommits
            | OperationKind::CustomMetadata => true,
            OperationKind::JoinCommits => form.join_output.trim().is_empty(),
            OperationKind::ChangeCommits => form.transform_output.trim().is_empty(),
            _ => false,
        };
        let saves_transform_output =
            kind == OperationKind::ChangeCommits && refresh_archive != archive;
        let refresh_removed_paths =
            (kind == OperationKind::RemoveEntries).then(|| lines(&form.remove_text));
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
                if let Some((directory, path)) = new_working_archive {
                    let _ = events_tx.send(UiEvent::WorkingArchive {
                        directory,
                        path: path.to_string_lossy().into_owned(),
                        logical_path: logical_archive,
                        dirty: true,
                        new_archive_settings,
                    });
                } else if changes_archive && !saves_transform_output {
                    let _ = events_tx.send(UiEvent::ArchiveModified);
                }
            }
            if outcome.is_ok() && refresh_commit_ids {
                let result_password = refresh_password.clone();
                let _ = events_tx.send(UiEvent::ArchivePath {
                    path: refresh_archive.clone(),
                    password: result_password.clone(),
                });
                match operations::info_archive(&refresh_archive, false) {
                    Ok(commits) => {
                        let ids = commits
                            .iter()
                            .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                            .collect();
                        let _ = events_tx.send(UiEvent::CommitIds(ids));
                    }
                    Err(error) => {
                        outcome = Err(format!(
                            "{} completed, but commit refresh failed: {error}",
                            if is_encryption_transform {
                                "Encryption transform"
                            } else {
                                "Undo"
                            }
                        ));
                    }
                }
            }
            if outcome.is_ok()
                && let Some(paths) = refresh_removed_paths
            {
                let _ = events_tx.send(UiEvent::EntriesRemoved(paths));
            } else if outcome.is_ok() {
                let list_password =
                    if kind == OperationKind::ChangeCommits && refresh_password.is_none() {
                        None
                    } else {
                        refresh_password.as_deref().map(str::as_bytes)
                    };
                match operations::list_archive_with_details(
                    &refresh_archive,
                    refresh_to.as_deref(),
                    list_password,
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
        let archive = self.read_archive.clone();
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
                UiEvent::ArchivePath { path, password } => {
                    if path != self.read_archive {
                        self.archive.clone_from(&path);
                        self.read_archive = path.clone();
                        self.location = path;
                        self.working_archive_dir = None;
                        self.has_unsaved_changes = false;
                    }
                    self.archive_password = password;
                }
                UiEvent::ImportedArchive { directory, path } => {
                    self.read_archive = path;
                    self.imported_archive_dir = Some(directory);
                }
                UiEvent::WorkingArchive {
                    directory,
                    path,
                    logical_path,
                    dirty,
                    new_archive_settings,
                } => {
                    if dirty {
                        if self.operation.kind == Some(OperationKind::AddToArchive) {
                            self.operation = OperationForm::default();
                        }
                        self.add_archive_settings = new_archive_settings.map(|settings| *settings);
                    }
                    if dirty && logical_path.is_empty() {
                        self.set_entries(Toc::default(), Default::default());
                        self.commit_ids.clear();
                    }
                    self.archive.clone_from(&logical_path);
                    self.location = logical_path;
                    self.read_archive = path;
                    self.working_archive_dir = Some(directory);
                    self.imported_archive_dir = None;
                    self.has_unsaved_changes = dirty;
                }
                UiEvent::ArchiveModified => self.has_unsaved_changes = true,
                UiEvent::EntriesRemoved(paths) => self.remove_entries(&paths),
                UiEvent::ArchiveSaved { path } => {
                    self.archive.clone_from(&path);
                    if self.working_archive_dir.is_none() {
                        self.read_archive.clone_from(&path);
                    }
                    self.location = path;
                    if self.imported_archive_dir.is_some() {
                        self.imported_archive_dir = None;
                        self.working_archive_dir = None;
                    }
                    self.has_unsaved_changes = false;
                }
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
                        self.imported_archive_dir = None;
                        self.read_archive.clone_from(&self.archive);
                        if self.opening_archive && error.contains("password") {
                            self.operation =
                                self.new_operation_form(OperationKind::ListArchive, &self.archive);
                            self.operation.commit_ids.clone_from(&self.commit_ids);
                            self.operation.to = self.commit_ids.last().cloned().unwrap_or_default();
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
                    let switch_after_save = self.pending_switch_after_save;
                    self.pending_switch_after_save = false;
                    match result {
                        Ok(summary) => {
                            self.status = summary.clone();
                            if switch_after_save
                                && let Some(action) = self.pending_archive_switch.take()
                            {
                                self.execute_archive_switch(action);
                            }
                        }
                        Err(error) => {
                            if switch_after_save {
                                self.pending_archive_switch = None;
                            }
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
            if self.pending_switch_after_save {
                self.pending_archive_switch = None;
                self.pending_switch_after_save = false;
            }
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
        let current_paths: BTreeSet<String> = toc
            .iter()
            .map(|(path, _)| path.as_str().to_owned())
            .collect();
        self.selected.retain(|path| current_paths.contains(path));
        if self
            .selection_anchor
            .as_ref()
            .is_some_and(|path| !current_paths.contains(path))
        {
            self.selection_anchor = None;
        }
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

    fn remove_entries(&mut self, paths: &[String]) {
        let is_removed = |entry: &str| {
            paths.iter().any(|path| {
                entry == path
                    || entry
                        .strip_prefix(path)
                        .is_some_and(|suffix| suffix.starts_with('/'))
            })
        };
        self.entries.retain(|(entry, _)| !is_removed(entry));
        self.entry_details
            .retain(|entry, _| !is_removed(entry.as_str()));
        self.selected.retain(|entry| !is_removed(entry));
        if self
            .selection_anchor
            .as_ref()
            .is_some_and(|entry| is_removed(entry))
        {
            self.selection_anchor = None;
        }
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

    fn save_archive(&mut self) -> bool {
        if self.has_unsaved_changes && !self.busy {
            if self.archive.is_empty() {
                return self.save_archive_as();
            } else {
                return self.save_archive_to(self.archive.clone());
            }
        }
        false
    }

    fn save_archive_as(&mut self) -> bool {
        if self.read_archive.is_empty() || self.busy {
            return false;
        }
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("ZAR archive", &["zar"])
            .save_file()
        {
            return self.save_archive_to(path.to_string_lossy().into_owned());
        }
        false
    }

    fn save_archive_to(&mut self, path: String) -> bool {
        if self.read_archive.is_empty() || self.busy {
            return false;
        }
        let source = self.read_archive.clone();
        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();
        drop(answer_rx);
        self.begin_worker(events_rx, answer_tx, "Saving archive");
        self.report = None;
        self.warnings.clear();
        std::thread::spawn(move || {
            let result = save_archive_copy(&source, &path).map(|()| {
                let _ = events_tx.send(UiEvent::ArchiveSaved { path: path.clone() });
                format!("Saved archive {path}")
            });
            finish_with_events(events_tx, result);
        });
        true
    }

    fn can_add_to_archive(&self) -> bool {
        !self.busy && !self.read_archive.is_empty() && self.imported_archive_dir.is_none()
    }

    fn has_current_archive(&self) -> bool {
        !self.read_archive.is_empty()
    }

    fn render_menus(&mut self, ui: &mut egui::Ui) {
        let is_imported_archive = self.imported_archive_dir.is_some();
        egui::MenuBar::new().ui(ui, |ui| {
            icon_menu_button(
                ui,
                icon_texture(ui.ctx(), &self.icons, UiIcon::OpenArchive),
                "File",
                |ui| {
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::NewArchive,
                        "New archive",
                        !self.busy,
                    )
                    .clicked()
                    {
                        self.show_operation(OperationKind::NewArchive);
                        ui.close();
                    }
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::OpenArchive,
                        "Open archive",
                        !self.busy,
                    )
                    .clicked()
                    {
                        self.open_dialog();
                        ui.close();
                    }
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::Add,
                        "Add to archive",
                        self.can_add_to_archive(),
                    )
                    .clicked()
                    {
                        self.show_operation(OperationKind::AddToArchive);
                        ui.close();
                    }
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::SaveArchive,
                        "Save archive",
                        self.has_unsaved_changes && !self.busy,
                    )
                    .clicked()
                    {
                        self.save_archive();
                        ui.close();
                    }
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::SaveArchive,
                        "Save as...",
                        !self.read_archive.is_empty() && !self.busy,
                    )
                    .clicked()
                    {
                        self.save_archive_as();
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
                        !self.selected.is_empty() && !self.busy,
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
                icon_texture(ui.ctx(), &self.icons, UiIcon::Tools),
                "Tools",
                |ui| {
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::Info,
                        "Custom metadata",
                        !self.busy && !self.archive.is_empty() && !is_imported_archive,
                    )
                    .clicked()
                    {
                        self.show_operation(OperationKind::CustomMetadata);
                        ui.close();
                    }
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::ChangeCommits,
                        "Change commits",
                        !self.busy && !self.archive.is_empty() && !is_imported_archive,
                    )
                    .clicked()
                    {
                        self.show_operation(OperationKind::ChangeCommits);
                        ui.close();
                    }
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::Join,
                        "Join commits",
                        !self.busy
                            && !self.archive.is_empty()
                            && self.commit_ids.len() > 1
                            && !is_imported_archive,
                    )
                    .clicked()
                    {
                        self.show_operation(OperationKind::JoinCommits);
                        ui.close();
                    }
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::Undo,
                        "Undo commits",
                        !self.busy
                            && !self.archive.is_empty()
                            && self.commit_ids.len() > 1
                            && !is_imported_archive,
                    )
                    .clicked()
                    {
                        self.show_operation(OperationKind::UndoCommits);
                        ui.close();
                    }
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::TestArchive,
                        "Test archive",
                        !self.busy && !self.archive.is_empty() && !is_imported_archive,
                    )
                    .clicked()
                    {
                        self.show_operation(OperationKind::TestArchive);
                        ui.close();
                    }
                    ui.separator();
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::Benchmark,
                        "Benchmark",
                        !self.busy && !self.archive.is_empty() && !is_imported_archive,
                    )
                    .clicked()
                    {
                        self.benchmark.open = true;
                        ui.close();
                    }
                    ui.separator();
                    if icon_button(ui, &self.icons, UiIcon::Configure, "Settings...", true)
                        .clicked()
                    {
                        self.show_settings = true;
                        ui.close();
                    }
                },
            );
            icon_menu_button(
                ui,
                icon_texture(ui.ctx(), &self.icons, UiIcon::About),
                "Help",
                |ui| {
                    if icon_button(ui, &self.icons, UiIcon::OnlineHelp, "Online Help", true)
                        .clicked()
                    {
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

    fn new_operation_form(&self, kind: OperationKind, archive: &str) -> OperationForm {
        let mut form = OperationForm::new(kind, archive);
        form.block_size.clone_from(&self.default_block_size);
        form.compression_level = self.default_compression_level.to_string();
        form.aead = match self.default_encryption_method {
            crate::settings::DefaultEncryptionMethod::Aes256Gcm => 0,
            crate::settings::DefaultEncryptionMethod::ChaCha20Poly1305 => 1,
        };
        if matches!(
            kind,
            OperationKind::NewArchive | OperationKind::AddToArchive
        ) {
            let defaults = &self.archive_defaults;
            form.metadata = defaults.metadata;
            form.dates = defaults.dates;
            form.special_files = defaults.special_files;
            form.strict = defaults.strict;
            form.save_user = kind == OperationKind::NewArchive && defaults.save_user;
            form.include_files.clone_from(&defaults.include_files);
            form.exclude_files.clone_from(&defaults.exclude_files);
            form.include_folders.clone_from(&defaults.include_folders);
            form.exclude_folders.clone_from(&defaults.exclude_folders);
        }
        form
    }

    fn show_operation(&mut self, kind: OperationKind) {
        if kind == OperationKind::NewArchive {
            self.request_archive_switch(ArchiveSwitchAction::New);
            return;
        }
        self.show_operation_now(kind);
    }

    fn show_operation_now(&mut self, kind: OperationKind) {
        self.remember_add_archive_settings();
        let archive = if kind == OperationKind::NewArchive {
            String::new()
        } else if kind == OperationKind::AddToArchive && self.archive.is_empty() {
            self.read_archive.clone()
        } else {
            self.archive.clone()
        };
        self.operation = self.new_operation_form(kind, &archive);
        if kind == OperationKind::AddToArchive
            && let Some(settings) = &self.add_archive_settings
        {
            settings.apply_to(&mut self.operation);
        }
        if self.imported_archive_dir.is_some()
            && matches!(
                kind,
                OperationKind::ListArchive | OperationKind::ExtractArchive
            )
        {
            self.operation.archive_backend =
                Some((self.archive.clone(), self.read_archive.clone()));
        }
        if matches!(
            kind,
            OperationKind::ListArchive | OperationKind::ExtractArchive
        ) {
            self.operation.commit_ids.clone_from(&self.commit_ids);
            self.operation.to = self.commit_ids.last().cloned().unwrap_or_default();
        }
        if kind == OperationKind::CustomMetadata {
            match operations::read_custom_metadata(&self.read_archive) {
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
        if kind == OperationKind::JoinCommits {
            self.operation.commit_ids.clone_from(&self.commit_ids);
            self.operation.from_id = self.commit_ids.first().cloned().unwrap_or_default();
            self.operation.to_id = self.commit_ids.last().cloned().unwrap_or_default();
        }
        if kind == OperationKind::UndoCommits {
            self.operation.undo_commits = 1;
            self.operation.commit_ids.clone_from(&self.commit_ids);
        }
        if kind == OperationKind::ChangeCommits {
            self.operation.commit_ids.clone_from(&self.commit_ids);
            self.operation.from_id = self.commit_ids.first().cloned().unwrap_or_default();
            self.operation.to_id = self.commit_ids.last().cloned().unwrap_or_default();
            self.operation.existing_password = self.archive_password.clone().unwrap_or_default();
            self.operation.existing_password_is_set = !self.operation.existing_password.is_empty();
            self.operation.transform_strict = true;
        }
    }

    fn remember_add_archive_settings(&mut self) {
        if self.operation.kind == Some(OperationKind::AddToArchive) {
            self.add_archive_settings = Some(AddArchiveSettings::from_form(&self.operation));
        }
    }

    fn close_operation(&mut self) {
        self.remember_add_archive_settings();
        self.operation = OperationForm::default();
    }

    fn render_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if icon_button(ui, &self.icons, UiIcon::NewArchive, "New", !self.busy).clicked() {
                self.show_operation(OperationKind::NewArchive);
            }
            for (label, icon, kind) in [
                ("Add", UiIcon::Add, OperationKind::AddToArchive),
                ("Extract", UiIcon::Extract, OperationKind::ExtractArchive),
            ] {
                let enabled = if kind == OperationKind::AddToArchive {
                    self.can_add_to_archive()
                } else {
                    !self.busy && !self.archive.is_empty()
                };
                if icon_button(ui, &self.icons, icon, label, enabled).clicked() {
                    self.show_operation(kind);
                }
            }
            if icon_button(
                ui,
                &self.icons,
                UiIcon::Remove,
                "Remove",
                !self.busy
                    && !self.archive.is_empty()
                    && !self.selected.is_empty()
                    && self.imported_archive_dir.is_none(),
            )
            .clicked()
            {
                let mut form = self.new_operation_form(OperationKind::RemoveEntries, &self.archive);
                form.remove_text = self.selected.iter().cloned().collect::<Vec<_>>().join("\n");
                self.start_operation(form);
            }
            if icon_button(
                ui,
                &self.icons,
                UiIcon::Info,
                "Info",
                !self.busy && !self.archive.is_empty() && self.imported_archive_dir.is_none(),
            )
            .clicked()
            {
                self.show_archive_info();
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

    pub(super) fn dialog_size(&self, key: &str, default: [f32; 2]) -> [f32; 2] {
        self.dialog_sizes.get(key).copied().unwrap_or(default)
    }

    pub(super) fn record_dialog_size(&mut self, key: &str, size: Option<[f32; 2]>) {
        if let Some(size) = size.filter(|size| {
            size.iter()
                .all(|dimension| dimension.is_finite() && *dimension >= 100.0)
        }) {
            self.dialog_sizes.insert(key.to_owned(), size);
        }
    }

    pub(super) fn viewport_inner_size(context: &egui::Context) -> Option<[f32; 2]> {
        context.input(|input| {
            input
                .viewport()
                .inner_rect
                .map(|rect| [rect.width(), rect.height()])
        })
    }

    pub(super) fn centered_dialog_viewport_builder(
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
                                        egui::Label::new(&path)
                                            .truncate()
                                            .show_tooltip_when_elided(false),
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
        let clicked_sort = egui::ScrollArea::horizontal()
            .id_salt("structured-header-scroll")
            .auto_shrink([false, true])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .scroll_source(egui::scroll_area::ScrollSource::NONE)
            .horizontal_scroll_offset(self.structured_horizontal_offset)
            .show(ui, |ui| {
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
                ui.set_min_width(content_width);
                clicked_sort
            })
            .inner;
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
        let row_output = egui::ScrollArea::both()
            .id_salt("structured-rows-scroll")
            .auto_shrink([false, false])
            .show_rows(ui, row_height, rows.len(), |ui, row_range| {
                for row_index in row_range {
                    let row = rows[row_index].clone();
                    let (_, row_rect) = ui.allocate_space(egui::vec2(content_width, row_height));
                    let response = ui.interact(
                        row_rect,
                        list_id.with(("structured-entry", row_index)),
                        egui::Sense::click(),
                    );
                    let painter = ui.painter().with_clip_rect(row_rect);
                    if self.selected.contains(&row.path) {
                        painter.rect_filled(row_rect, 0.0, ui.visuals().selection.bg_fill);
                    } else if response.hovered() {
                        painter.rect_filled(row_rect, 0.0, ui.visuals().widgets.hovered.bg_fill);
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
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
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
                            (permissions_width > 0.0)
                                .then(|| format_permissions(self.entry_details.get(&row.path))),
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
        self.structured_horizontal_offset = row_output.state.unclamped_offset().x;
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
        self.ui_scale = settings.ui_scale;
        self.theme_mode = settings.theme_mode;
        self.default_block_size
            .clone_from(&settings.default_block_size);
        self.default_compression_level = settings.default_compression_level;
        self.default_encryption_method = settings.default_encryption_method;
        self.archive_defaults.clone_from(&settings.archive_defaults);
    }

    pub fn set_theme_controller(&mut self, controller: crate::theme::ThemeController) {
        self.theme_controller = Some(controller);
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
            ui_scale: self.ui_scale,
            theme_mode: self.theme_mode,
            default_block_size: self.default_block_size.clone(),
            default_compression_level: self.default_compression_level,
            default_encryption_method: self.default_encryption_method,
            archive_defaults: self.archive_defaults.clone(),
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

    fn apply_control_size(&mut self, context: &egui::Context) {
        if self.applied_ui_scale != Some(self.ui_scale) {
            context.set_zoom_factor(self.ui_scale);
            self.applied_ui_scale = Some(self.ui_scale);
        }
    }

    pub(super) fn set_theme_mode(
        &mut self,
        context: &egui::Context,
        theme_mode: crate::settings::ThemeMode,
    ) {
        self.theme_mode = theme_mode;
        if let Some(controller) = &self.theme_controller {
            controller.set_mode(theme_mode);
        } else {
            context.set_theme(match theme_mode {
                crate::settings::ThemeMode::System => egui::ThemePreference::System,
                crate::settings::ThemeMode::Light => {
                    egui::ThemePreference::from(egui::Theme::Light)
                }
                crate::settings::ThemeMode::Dark => egui::ThemePreference::from(egui::Theme::Dark),
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
        self.apply_control_size(&context);
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
                    "Open archive",
                    !self.busy,
                )
                .clicked()
                {
                    self.open_dialog();
                }
                if self.has_unsaved_changes
                    && icon_button(ui, &self.icons, UiIcon::SaveArchive, "Save", !self.busy)
                        .clicked()
                {
                    self.save_archive();
                }
                ui.allocate_ui_with_layout(
                    ui.available_size(),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        if self.has_current_archive() {
                            ui.label(format!("{} entries", self.entries.len()));
                        }
                        ui.add_enabled_ui(false, |ui| {
                            ui.add_sized(
                                [ui.available_width(), ui.spacing().interact_size.y],
                                egui::TextEdit::singleline(&mut self.location),
                            )
                            .on_hover_text(&self.location);
                        });
                    },
                );
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
        // Fusion draws item views on the Base color rather than the window color.
        let list_frame = egui::Frame::central_panel(ui.style()).fill(ui.visuals().extreme_bg_color);
        egui::CentralPanel::default()
            .frame(list_frame)
            .show(ui, |ui| {
                if !self.has_current_archive() {
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
                let (has_owner, has_permissions) =
                    (self.has_entry_owner, self.has_entry_permissions);
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
                let row_height = ui.spacing().interact_size.y;
                let clicked_sort = egui::ScrollArea::horizontal()
                    .id_salt("details-header-scroll")
                    .auto_shrink([false, true])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .scroll_source(egui::scroll_area::ScrollSource::NONE)
                    .horizontal_scroll_offset(self.details_horizontal_offset)
                    .show(ui, |ui| {
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
                        ui.set_min_width(content_width);
                        clicked_sort
                    })
                    .inner;
                self.store_column_widths("details", &labels, &columns);
                if let Some(sort_by) = clicked_sort {
                    self.apply_sort_choice(sort_by);
                }
                ui.separator();
                let row_output = egui::ScrollArea::both()
                    .id_salt("details-rows-scroll")
                    .auto_shrink([false, false])
                    .show_rows(
                        ui,
                        row_height,
                        self.visible_entry_indices.len(),
                        |ui, row_range| {
                            ui.set_min_width(content_width);
                            for row in row_range {
                                let (path, data) = &self.entries[self.visible_entry_indices[row]];
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
                                        // The hover text below already shows the full path.
                                        .show_tooltip_when_elided(false)
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
                                            let compressed_size =
                                                details.and_then(|details| details.compressed_size);
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
                self.details_horizontal_offset = row_output.state.unclamped_offset().x;
            });

        if let Some(path) = self.pending_open.take() {
            self.open_entry_externally(path);
        }

        self.poll_archive_info(&context);
        self.render_operation_dialog(&context);
        self.render_archive_info(&context);
        self.poll_benchmark(&context);
        self.render_benchmark(&context);
        self.render_confirmation_dialog(&context);
        self.render_unsaved_archive_dialog(&context);
        self.render_about_dialog(&context);
        self.render_operation_error_dialog(&context);
        self.render_file_picker_dialog(&context);
        self.render_settings_dialog(&context);
        self.update_window_title(&context);
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

fn save_archive_copy(source: &str, target: &str) -> Result<(), String> {
    let target_path = Path::new(target);
    let parent = target_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("Cannot prepare archive save: {error}"))?;
    let mut source_file = std::fs::File::open(source)
        .map_err(|error| format!("Cannot read working archive {source}: {error}"))?;
    let permissions = source_file
        .metadata()
        .map_err(|error| format!("Cannot inspect working archive {source}: {error}"))?
        .permissions();
    std::io::copy(&mut source_file, &mut temporary)
        .map_err(|error| format!("Cannot copy archive to {target}: {error}"))?;
    temporary
        .as_file()
        .set_permissions(permissions)
        .map_err(|error| format!("Cannot preserve archive permissions at {target}: {error}"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("Cannot flush archive save to {target}: {error}"))?;
    temporary
        .persist(target_path)
        .map_err(|error| format!("Cannot save archive to {target}: {}", error.error))?;
    Ok(())
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
        OperationKind::NewArchive => {
            let options = form.encoding()?;
            let filters = form.filters()?;
            let paths = form.input_paths;
            let strict = form.strict;
            let input = CreateInputOptions {
                save_metadata: form.metadata,
                save_dates: form.dates,
                save_user: form.save_user,
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
        OperationKind::AddToArchive => {
            if form.input_paths.is_empty() {
                return Err("Choose one or more files or folders to add.".into());
            }
            let options = form.encoding()?;
            let filters = form.filters()?;
            let paths = form.input_paths;
            let input = AddInputOptions {
                save_metadata: form.metadata,
                save_dates: form.dates,
                save_user: false,
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
        OperationKind::CustomMetadata => {
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
        OperationKind::RemoveEntries => {
            let paths: Vec<ArchivePath> = lines(&form.remove_text)
                .iter()
                .map(|path| ArchivePath::new(path.clone()))
                .collect::<Result<_, _>>()?;
            if paths.is_empty() {
                return Err("Enter or select one or more archive paths to remove.".into());
            }
            Box::new(move || {
                operations::remove_archive_entries_with_progress(
                    &archive,
                    &paths,
                    Some(progress.as_ref()),
                )
                .map(|()| format!("Removed {} archive path(s)", paths.len()))
                .map_err(|error| error.to_string())
            })
        }
        OperationKind::UndoCommits => {
            let commits = form.undo_commits;
            Box::new(move || {
                operations::undo_archive(&archive, commits)
                    .map(|()| format!("Undid {commits} trailing commit(s)"))
                    .map_err(|error| error.to_string())
            })
        }
        OperationKind::JoinCommits => {
            let options = form.encoding()?;
            let from = form.from_id;
            let to = form.to_id;
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
                    &to,
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
                    format!("Joined commits {from} to {to}")
                } else {
                    format!("Joined commits {from} to {to} into {target}")
                })
            })
        }
        OperationKind::ChangeCommits => {
            if !commit_range_is_valid(&form.commit_ids, &form.from_id, &form.to_id) {
                return Err("Choose a valid inclusive commit range.".into());
            }
            let from = form.from_id.trim().to_owned();
            let to = form.to_id.trim().to_owned();
            let output = (!form.transform_output.trim().is_empty())
                .then(|| form.transform_output.trim().to_owned());
            let strict = form.transform_strict;
            let passwords = PasswordSet::new(
                (!form.existing_password.is_empty())
                    .then(|| form.existing_password.as_bytes().to_vec()),
            );
            let new_password = form.password.into_bytes();
            let algorithm =
                [ContentAead::Aes256Gcm, ContentAead::ChaCha20Poly1305][form.aead.min(1)];
            let scope = [
                EncryptionScope::HeaderAndData,
                EncryptionScope::Data,
                EncryptionScope::Header,
            ][form.encryption_scope.min(2)];
            let action = form.transform_action;
            let encrypt = action != 1;
            let change_only = action == 2;
            let result_archive = output.clone().unwrap_or_else(|| archive.clone());
            let result_password = if encrypt {
                Some(String::from_utf8_lossy(&new_password).into_owned())
            } else {
                None
            };
            let events = _events;
            Box::new(move || {
                if change_only {
                    let range = operations::CommitRangeOptions {
                        from_id: &from,
                        to_id: &to,
                        output: output.as_deref(),
                        passwords: &passwords,
                        strict,
                    };
                    operations::change_encryption_for_commits(
                        &archive,
                        range,
                        &new_password,
                        algorithm,
                        scope,
                    )
                    .map_err(|error| error.to_string())?;
                } else {
                    let range = operations::CommitRangeOptions {
                        from_id: &from,
                        to_id: &to,
                        output: output.as_deref(),
                        passwords: &passwords,
                        strict,
                    };
                    if encrypt {
                        operations::encrypt_archive_commits(
                            &archive,
                            range,
                            &new_password,
                            algorithm,
                            scope,
                        )
                        .map_err(|error| error.to_string())?;
                    } else {
                        operations::decrypt_archive_commits(&archive, range)
                            .map_err(|error| error.to_string())?;
                    }
                }
                let _ = events.send(UiEvent::ArchivePath {
                    path: result_archive.clone(),
                    password: result_password,
                });
                Ok(if change_only {
                    format!("Changed encryption for commits {from} to {to} in {result_archive}")
                } else if encrypt {
                    format!("Encrypted commits {from} to {to} into {result_archive}")
                } else {
                    format!("Decrypted commits {from} to {to} into {result_archive}")
                })
            })
        }
        OperationKind::TestArchive => {
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
        OperationKind::ListArchive => {
            let to = (!form.to.trim().is_empty()).then(|| form.to.clone());
            Box::new(move || {
                operations::list_archive(&archive, to.as_deref(), password.as_deref())
                    .map(|toc| format!("Loaded {} entries from {archive}", toc.len()))
                    .map_err(|error| error.to_string())
            })
        }
        OperationKind::ExtractArchive => {
            let filters = form.filters()?;
            let destination = PathBuf::from(form.destination);
            let to = (!form.to.trim().is_empty()).then(|| form.to.clone());
            let dry_run = form.dry_run;
            let collision = collision_policy(form.collision);
            let existing = existing_policy(form.existing);
            let restore_metadata = form.metadata;
            let restore_symlinks = form.restore_symlinks;
            let restore_devices = form.restore_devices;
            let strip_top_level = form.strip_top_level;
            let import_archive = operations::is_supported_import_archive(Path::new(&archive));
            Box::new(move || {
                let imported_directory = if import_archive {
                    Some(
                        tempfile::Builder::new()
                            .prefix("zarg-extract-archive-")
                            .tempdir()
                            .map_err(|error| error.to_string())?,
                    )
                } else {
                    None
                };
                let archive_to_extract = if let Some(directory) = &imported_directory {
                    let imported_path = directory
                        .path()
                        .join("imported.zar")
                        .to_string_lossy()
                        .into_owned();
                    operations::create_archive(&imported_path, &[PathBuf::from(&archive)])
                        .map_err(|error| error.to_string())?;
                    imported_path
                } else {
                    archive.clone()
                };
                let options = ExtractionOptions {
                    dry_run,
                    to: to.as_deref(),
                    collision_policy: collision,
                    existing_policy: existing,
                    restore_metadata,
                    restore_symlinks,
                    restore_devices,
                    strip_top_level,
                };
                operations::extract_archive_with_progress_and_filters(
                    &archive_to_extract,
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
    operations::format_unix_millis(millis)
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

fn join_range_is_valid(commits: &[String], from: &str, to: &str) -> bool {
    let from = commits.iter().position(|id| id == from);
    let to = commits.iter().position(|id| id == to);
    matches!((from, to), (Some(from), Some(to)) if to > from)
}

fn commit_range_is_valid(commits: &[String], from: &str, to: &str) -> bool {
    let from = commits.iter().position(|id| id == from);
    let to = commits.iter().position(|id| id == to);
    matches!((from, to), (Some(from), Some(to)) if to >= from)
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
        return Err("block size must be 64 KiB to 32 MiB, or `auto`".into());
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
    use dialogs::benchmark::{BenchmarkSettings, run_benchmark};

    #[test]
    fn window_title_tracks_logical_archive_and_resets() {
        let mut app = ZargApp::default();
        let context = egui::Context::default();
        let title_commands = |app: &mut ZargApp| {
            let output = context.run_ui(egui::RawInput::default(), |ui| {
                app.update_window_title(ui.ctx());
            });
            let commands = output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .iter()
                .filter_map(|command| match command {
                    egui::ViewportCommand::Title(title) => Some(title.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            output.drop_without_applying_deltas();
            commands
        };
        assert_eq!(title_commands(&mut app), ["ZAR Archive Manager"]);
        assert!(title_commands(&mut app).is_empty());
        app.archive = "/archives/a file.zar".to_owned();
        app.read_archive = "/tmp/working.zar".to_owned();
        assert_eq!(
            title_commands(&mut app),
            ["ZAR Archive Manager - a file.zar"]
        );
        app.archive = "/archives/import.zip".to_owned();
        assert_eq!(
            title_commands(&mut app),
            ["ZAR Archive Manager - import.zip"]
        );
        app.archive = "/archives/saved.zar".to_owned();
        assert_eq!(
            title_commands(&mut app),
            ["ZAR Archive Manager - saved.zar"]
        );
        app.discard_current_archive();
        assert_eq!(title_commands(&mut app), ["ZAR Archive Manager"]);
        app.read_archive = "/tmp/unsaved.zar".to_owned();
        assert!(title_commands(&mut app).is_empty());
    }

    #[test]
    fn metadata_changes_require_an_edited_pair() {
        let mut form = OperationForm::new(OperationKind::CustomMetadata, "");
        assert!(!form.has_metadata_changes());

        form.meta_new.push(("key".to_owned(), String::new()));
        assert!(form.has_metadata_changes());
        form.meta_new.clear();

        form.meta_items
            .push(("key".to_owned(), "old".to_owned(), "old".to_owned(), false));
        assert!(!form.has_metadata_changes());
        form.meta_items[0].2 = "new".to_owned();
        assert!(form.has_metadata_changes());
        form.meta_items[0].2 = "old".to_owned();
        form.meta_items[0].3 = true;
        assert!(form.has_metadata_changes());
    }

    #[test]
    fn join_requires_to_commit_after_from_commit() {
        let commits = vec!["first".to_owned(), "middle".to_owned(), "last".to_owned()];
        assert!(join_range_is_valid(&commits, "first", "middle"));
        assert!(join_range_is_valid(&commits, "first", "last"));
        assert!(!join_range_is_valid(&commits, "middle", "middle"));
        assert!(!join_range_is_valid(&commits, "last", "first"));
        assert!(!join_range_is_valid(&commits, "missing", "last"));
    }

    #[test]
    fn encryption_transform_ranges_allow_one_or_more_commits_and_reject_bad_ranges() {
        let commits = vec!["first".to_owned(), "middle".to_owned(), "last".to_owned()];
        assert!(commit_range_is_valid(&commits, "first", "first"));
        assert!(commit_range_is_valid(&commits, "first", "last"));
        assert!(!commit_range_is_valid(&commits, "last", "first"));
        assert!(!commit_range_is_valid(&commits, "missing", "last"));
    }

    #[test]
    fn encryption_operation_forms_prefill_commit_range_and_password() {
        let mut app = ZargApp {
            archive: "archive.zar".to_owned(),
            commit_ids: vec!["00000001".to_owned(), "00000002".to_owned()],
            archive_password: Some("existing password".to_owned()),
            ..ZargApp::default()
        };

        app.show_operation(OperationKind::ChangeCommits);

        assert_eq!(app.operation.from_id, "00000001");
        assert_eq!(app.operation.to_id, "00000002");
        assert_eq!(app.operation.existing_password, "existing password");
        assert!(app.operation.transform_strict);
    }

    #[test]
    fn archive_operation_forms_use_saved_defaults_without_enabling_encryption() {
        let settings = crate::settings::Settings {
            default_block_size: "16M".to_owned(),
            default_compression_level: 9,
            default_encryption_method: crate::settings::DefaultEncryptionMethod::ChaCha20Poly1305,
            archive_defaults: crate::settings::ArchiveDefaults {
                metadata: true,
                dates: true,
                special_files: true,
                strict: true,
                save_user: true,
                include_files: "*.rs\n*.txt".to_owned(),
                exclude_files: "*.tmp".to_owned(),
                include_folders: "src/**".to_owned(),
                exclude_folders: "target".to_owned(),
            },
            ..crate::settings::Settings::default()
        };
        let mut app = ZargApp::default();
        app.apply_settings(&settings);

        for kind in [OperationKind::NewArchive, OperationKind::AddToArchive] {
            app.show_operation(kind);
            assert_eq!(app.operation.block_size, "16M");
            assert_eq!(app.operation.compression_level, "9");
            assert_eq!(app.operation.aead, 1);
            assert!(!app.operation.password_is_set);
            assert!(
                app.operation.metadata
                    && app.operation.dates
                    && app.operation.special_files
                    && app.operation.strict
            );
            assert_eq!(app.operation.save_user, kind == OperationKind::NewArchive);
            assert_eq!(app.operation.include_files, "*.rs\n*.txt");
            assert_eq!(app.operation.exclude_files, "*.tmp");
            assert_eq!(app.operation.include_folders, "src/**");
            assert_eq!(app.operation.exclude_folders, "target");
            let filters = app.operation.filters().unwrap();
            assert!(filters.includes("src/main.rs", false));
            assert!(!filters.includes("src/cache.tmp", false));
            assert!(!filters.includes("target/main.rs", false));
        }
        let extract = app.new_operation_form(OperationKind::ExtractArchive, "archive.zar");
        assert!(extract.include_files.is_empty());
        assert!(!extract.metadata);
        assert_eq!(
            app.current_settings().archive_defaults,
            settings.archive_defaults
        );
    }

    #[test]
    fn new_archive_seeds_add_settings_and_later_dialogs_remember_edits() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = ZargApp {
            default_block_size: "16M".to_owned(),
            default_compression_level: 9,
            ..Default::default()
        };
        app.archive_defaults.include_files = "*.rs".to_owned();
        app.show_operation(OperationKind::NewArchive);
        assert_eq!(app.operation.block_size, "16M");
        assert_eq!(app.operation.compression_level, "9");
        assert_eq!(app.operation.include_files, "*.rs");

        app.operation.block_size = "4M".to_owned();
        app.operation.compression_level = "5".to_owned();
        app.operation.metadata = true;
        app.operation.dates = true;
        app.operation.special_files = true;
        app.operation.strict = true;
        app.operation.verbose = true;
        app.operation.aead = 1;
        app.operation.encryption_scope = 1;
        app.operation.include_files = "*.txt".to_owned();
        app.operation.exclude_files = "*.tmp".to_owned();
        app.operation.include_folders = "src/**".to_owned();
        app.operation.exclude_folders = "target".to_owned();
        let expected = AddArchiveSettings::from_form(&app.operation);
        let form = std::mem::take(&mut app.operation);
        app.start_operation(form);
        wait_for_worker(&mut app);
        assert!(app.report.is_none(), "{:?}", app.report);

        app.show_operation(OperationKind::AddToArchive);
        assert_eq!(AddArchiveSettings::from_form(&app.operation), expected);
        app.operation.block_size = "2M".to_owned();
        app.operation
            .filelists
            .push(directory.path().join("filters.txt"));
        app.operation
            .input_paths
            .push(directory.path().join("input.txt"));
        app.operation.password_is_set = true;
        app.operation.password = "secret".to_owned();
        app.operation.password_confirm = "secret".to_owned();
        app.operation.existing_password_is_set = true;
        app.operation.existing_password = "old secret".to_owned();
        let remembered = AddArchiveSettings::from_form(&app.operation);
        app.close_operation();

        app.show_operation(OperationKind::AddToArchive);
        assert_eq!(AddArchiveSettings::from_form(&app.operation), remembered);
        assert!(app.operation.input_paths.is_empty());
        assert!(app.operation.password.is_empty());
        assert!(app.operation.password_confirm.is_empty());
        assert!(app.operation.existing_password.is_empty());

        app.show_operation_now(OperationKind::NewArchive);
        assert_eq!(app.operation.block_size, "16M");
        assert_eq!(app.operation.include_files, "*.rs");
        app.close_operation();
        app.show_operation(OperationKind::AddToArchive);
        assert_eq!(AddArchiveSettings::from_form(&app.operation), remembered);
    }

    #[test]
    fn archive_switches_reset_session_settings_only_on_successful_creation() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input.txt");
        let archive = directory.path().join("opened.zar");
        std::fs::write(&input, b"contents").unwrap();
        operations::create_archive(archive.to_str().unwrap(), &[input]).unwrap();

        let mut app = ZargApp {
            default_compression_level: 8,
            ..Default::default()
        };
        let mut first = app.new_operation_form(OperationKind::NewArchive, "");
        first.compression_level = "4".to_owned();
        app.start_operation(first);
        wait_for_worker(&mut app);
        app.show_operation(OperationKind::AddToArchive);
        assert_eq!(app.operation.compression_level, "4");
        app.operation.compression_level = "6".to_owned();
        app.close_operation();

        let mut failed = app.new_operation_form(OperationKind::NewArchive, "");
        failed.block_size = "invalid".to_owned();
        app.start_operation(failed);
        if app.busy {
            wait_for_worker(&mut app);
        }
        app.show_operation(OperationKind::AddToArchive);
        assert_eq!(app.operation.compression_level, "6");

        // A successful new archive starts a fresh session even if another form was open.
        app.close_operation();
        let mut second = app.new_operation_form(OperationKind::NewArchive, "");
        second.compression_level = "2".to_owned();
        app.start_operation(second);
        wait_for_worker(&mut app);
        app.show_operation(OperationKind::AddToArchive);
        assert_eq!(app.operation.compression_level, "2");

        app.open_archive_now(archive, None);
        wait_for_worker(&mut app);
        assert!(app.operation.kind.is_none());
        app.show_operation(OperationKind::AddToArchive);
        assert_eq!(app.operation.compression_level, "8");
        app.operation.compression_level = "3".to_owned();
        app.close_operation();
        app.show_operation(OperationKind::AddToArchive);
        assert_eq!(app.operation.compression_level, "3");
        app.discard_current_archive();
        assert!(app.add_archive_settings.is_none());
    }

    #[test]
    fn context_created_archive_seeds_add_settings_from_global_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input.txt");
        std::fs::write(&input, b"contents").unwrap();
        let settings = crate::settings::Settings {
            default_compression_level: 6,
            ..Default::default()
        };
        let mut app =
            ZargApp::from_context_action(ContextAction::CreateNamed, vec![input], &settings)
                .unwrap();
        wait_for_worker(&mut app);
        assert!(app.report.is_none(), "{:?}", app.report);
        app.show_operation(OperationKind::AddToArchive);
        assert_eq!(app.operation.compression_level, "6");
    }

    #[test]
    fn new_encrypted_archive_add_requires_reentering_password() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input.txt");
        std::fs::write(&input, b"contents").unwrap();
        let mut app = ZargApp::default();
        let mut form = app.new_operation_form(OperationKind::NewArchive, "");
        form.password_is_set = true;
        form.password = "GoodPassword9!".to_owned();
        form.password_confirm = form.password.clone();
        form.aead = 1;
        app.start_operation(form);
        wait_for_worker(&mut app);
        assert!(app.report.is_none(), "{:?}", app.report);

        app.show_operation(OperationKind::AddToArchive);
        assert!(app.operation.password_is_set);
        assert_eq!(app.operation.aead, 1);
        assert!(app.operation.password.is_empty());
        assert!(app.operation.password_confirm.is_empty());
        app.operation.input_paths.push(input);
        app.operation.password = "GoodPassword9!".to_owned();
        app.operation.password_confirm = app.operation.password.clone();
        click_add_archive_action(&mut app);
        wait_for_worker(&mut app);
        assert!(app.report.is_none(), "{:?}", app.report);
        assert!(app.entries.iter().any(|(path, _)| path == "input.txt"));
    }

    #[test]
    fn change_commits_encrypt_job_writes_to_requested_archive() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.txt");
        let archive = directory.path().join("source.zar");
        let output = directory.path().join("encrypted.zar");
        std::fs::write(&source, b"secret contents").unwrap();
        operations::create_archive(archive.to_str().unwrap(), &[source]).unwrap();
        let commits = operations::info_archive(archive.to_str().unwrap(), false).unwrap();
        let commit_id = format!("{:08x}", u32::from_le_bytes(commits[0].id));

        let mut form = OperationForm::new(OperationKind::ChangeCommits, archive.to_str().unwrap());
        form.commit_ids = vec![commit_id.clone()];
        form.from_id.clone_from(&commit_id);
        form.to_id.clone_from(&commit_id);
        form.transform_output = output.to_string_lossy().into_owned();
        form.password = "GoodPassword9!".to_owned();
        form.password_confirm = form.password.clone();
        form.transform_strict = true;
        form.transform_action = 0;
        let (events, event_rx) = mpsc::channel();
        let (_answers, answer_rx) = mpsc::channel();
        let progress = UiProgress {
            events: events.clone(),
            answers: Arc::new(Mutex::new(answer_rx)),
        };
        let job = build_job(OperationKind::ChangeCommits, form, events.clone(), progress).unwrap();
        job().unwrap();

        assert!(!commits[0].header_encrypted);
        let encrypted = operations::info_archive(output.to_str().unwrap(), false).unwrap();
        assert!(encrypted[0].header_encrypted);
        assert!(encrypted[0].data_encrypted);
        let password_set = PasswordSet::new(Some(b"GoodPassword9!".to_vec()));
        operations::list_archive_with_passwords(output.to_str().unwrap(), None, &password_set)
            .unwrap();
        assert!(
            matches!(event_rx.try_recv().unwrap(), UiEvent::ArchivePath { path, password: Some(ref value) } if path == output.to_string_lossy() && value == "GoodPassword9!")
        );
    }

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
    fn saving_archive_copies_staged_contents_and_replaces_target() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("working.zar");
        let target = directory.path().join("archive.zar");
        std::fs::write(&source, b"updated archive").unwrap();
        std::fs::write(&target, b"old archive").unwrap();

        save_archive_copy(source.to_str().unwrap(), target.to_str().unwrap()).unwrap();

        assert_eq!(std::fs::read(target).unwrap(), b"updated archive");
    }

    #[test]
    fn archive_modifications_are_staged_until_saved() {
        let directory = tempfile::tempdir().unwrap();
        let original_file = directory.path().join("original.txt");
        let added_file = directory.path().join("added.txt");
        let archive = directory.path().join("archive.zar");
        std::fs::write(&original_file, b"original").unwrap();
        std::fs::write(&added_file, b"added").unwrap();
        operations::create_archive(archive.to_str().unwrap(), &[original_file]).unwrap();

        let mut app = ZargApp::default();
        assert!(!app.can_add_to_archive());
        assert!(!app.has_current_archive());
        app.open_archive(archive.clone(), None);
        wait_for_worker(&mut app);
        assert!(app.can_add_to_archive());
        assert!(app.has_current_archive());
        assert_ne!(app.read_archive, archive.to_string_lossy());
        assert!(!app.has_unsaved_changes);

        app.show_operation(OperationKind::AddToArchive);
        app.operation.input_paths.push(added_file);
        click_add_archive_action(&mut app);
        wait_for_worker(&mut app);
        assert!(app.has_unsaved_changes);
        assert!(app.report.is_none(), "{:?}", app.report);
        assert!(app.entries.iter().any(|(path, _)| path == "added.txt"));

        let original_contents =
            operations::list_archive(archive.to_str().unwrap(), None, None).unwrap();
        assert!(
            !original_contents
                .iter()
                .any(|(path, _)| path.as_str() == "added.txt")
        );
        let staged_contents = operations::list_archive(&app.read_archive, None, None).unwrap();
        assert!(
            staged_contents
                .iter()
                .any(|(path, _)| path.as_str() == "added.txt")
        );

        app.save_archive();
        wait_for_worker(&mut app);
        assert!(!app.has_unsaved_changes);
        let saved_contents =
            operations::list_archive(archive.to_str().unwrap(), None, None).unwrap();
        assert!(
            saved_contents
                .iter()
                .any(|(path, _)| path.as_str() == "added.txt")
        );
    }

    #[test]
    fn remove_archive_entries_is_staged_and_updates_ui_without_relisting() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("README");
        let archive = directory.path().join("archive.zar");
        std::fs::write(&input, b"contents").unwrap();
        operations::create_archive(archive.to_str().unwrap(), &[input]).unwrap();

        let mut app = ZargApp::default();
        app.open_archive(archive.clone(), None);
        wait_for_worker(&mut app);
        app.selected.insert("README".to_owned());
        app.selection_anchor = Some("README".to_owned());

        let mut form = app.new_operation_form(OperationKind::RemoveEntries, &app.archive);
        form.remove_text = "README".to_owned();
        app.start_operation(form);
        wait_for_worker(&mut app);

        assert!(app.has_unsaved_changes);
        assert!(app.entries.is_empty());
        assert!(!app.selected.contains("README"));
        assert!(app.selection_anchor.is_none());
        assert_eq!(
            operations::info_archive(&app.read_archive, false)
                .unwrap()
                .len(),
            2
        );
        assert!(
            operations::list_archive(archive.to_str().unwrap(), None, None)
                .unwrap()
                .iter()
                .any(|(path, _)| path.as_str() == "README")
        );
        assert!(
            !operations::list_archive(&app.read_archive, None, None)
                .unwrap()
                .iter()
                .any(|(path, _)| path.as_str() == "README")
        );
    }

    #[test]
    fn new_archive_is_created_in_temporary_storage_until_saved() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input.txt");
        let archive = directory.path().join("new.zar");
        std::fs::write(&input, b"new archive").unwrap();

        let mut app = ZargApp::default();
        let mut form = app.new_operation_form(OperationKind::NewArchive, archive.to_str().unwrap());
        form.input_paths.push(input);
        form.save_user = true;
        app.start_operation(form);
        wait_for_worker(&mut app);

        assert!(app.has_unsaved_changes);
        assert!(!archive.exists());
        assert!(app.archive.is_empty());
        assert_ne!(app.read_archive, archive.to_string_lossy());
        assert!(
            operations::info_archive(&app.read_archive, false).unwrap()[0]
                .user
                .is_some()
        );
        app.show_operation(OperationKind::AddToArchive);
        app.operation.compression_level = "7".to_owned();
        app.close_operation();
        assert!(app.save_archive_to(archive.to_string_lossy().into_owned()));
        wait_for_worker(&mut app);
        assert!(archive.exists());
        assert_eq!(app.archive, archive.to_string_lossy());
        assert!(!app.has_unsaved_changes);
        app.show_operation(OperationKind::AddToArchive);
        assert_eq!(app.operation.compression_level, "7");
    }

    #[test]
    fn empty_new_archive_is_created_in_temporary_storage() {
        let mut app = ZargApp::default();
        let form = app.new_operation_form(OperationKind::NewArchive, "");
        app.start_operation(form);
        wait_for_worker(&mut app);

        assert!(app.has_unsaved_changes);
        assert!(app.archive.is_empty());
        assert!(!app.read_archive.is_empty());
        assert!(
            operations::list_archive(&app.read_archive, None, None)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn add_to_unsaved_new_archive_uses_working_archive() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("added.txt");
        let other_input = directory.path().join("second.txt");
        std::fs::write(&input, b"added contents").unwrap();
        std::fs::write(&other_input, b"second contents").unwrap();

        let mut app = ZargApp::default();
        assert!(!app.can_add_to_archive());
        app.start_operation(app.new_operation_form(OperationKind::NewArchive, ""));
        assert!(!app.can_add_to_archive());
        wait_for_worker(&mut app);
        assert!(app.can_add_to_archive());
        assert!(app.has_current_archive());

        app.show_operation(OperationKind::AddToArchive);
        assert_eq!(app.operation.archive, app.read_archive);
        app.operation.compression_level = "6".to_owned();
        app.operation.input_paths.extend([input, other_input]);
        click_add_archive_action(&mut app);
        assert!(!app.can_add_to_archive());
        wait_for_worker(&mut app);

        assert!(app.can_add_to_archive());
        assert!(app.archive.is_empty());
        assert!(app.has_unsaved_changes);
        assert!(app.report.is_none(), "{:?}", app.report);
        assert!(app.entries.iter().any(|(path, _)| path == "added.txt"));
        assert!(app.entries.iter().any(|(path, _)| path == "second.txt"));
        app.show_operation(OperationKind::AddToArchive);
        assert_eq!(app.operation.compression_level, "6");
        assert!(app.operation.input_paths.is_empty());
        let visible_paths: Vec<_> = app
            .visible_entry_indices
            .iter()
            .map(|&index| app.entries[index].0.as_str())
            .collect();
        assert!(visible_paths.contains(&"added.txt"));
        assert!(visible_paths.contains(&"second.txt"));
        assert!(
            operations::list_archive(&app.read_archive, None, None)
                .unwrap()
                .iter()
                .any(|(path, _)| path.as_str() == "added.txt")
        );
        assert!(
            operations::list_archive(&app.read_archive, None, None)
                .unwrap()
                .iter()
                .any(|(path, _)| path.as_str() == "second.txt")
        );
        app.imported_archive_dir = Some(tempfile::tempdir().unwrap());
        assert!(!app.can_add_to_archive());
    }

    fn click_add_archive_action(app: &mut ZargApp) {
        let context = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(500.0, 360.0));
        let mut button_rect = egui::Rect::NOTHING;
        for frame in 0..3 {
            let events = if frame == 0 {
                Vec::new()
            } else {
                vec![
                    egui::Event::PointerMoved(button_rect.center()),
                    egui::Event::PointerButton {
                        pos: button_rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed: frame == 1,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            };
            let input = egui::RawInput {
                screen_rect: Some(rect),
                events,
                ..Default::default()
            };
            context
                .run_ui(input, |ui| {
                    egui::Panel::bottom("operation-actions")
                        .default_size(ui.spacing().interact_size.y.max(16.0) + 24.0)
                        .frame(egui::Frame::new().inner_margin(3))
                        .show_separator_line(false)
                        .show(ui, |ui| {
                            let response = app
                                .render_operation_actions(ui, OperationKind::AddToArchive)
                                .unwrap();
                            assert!(rect.contains_rect(response.rect));
                            assert!(response.enabled());
                            button_rect = response.rect;
                        });
                })
                .drop_without_applying_deltas();
        }
    }

    #[test]
    fn archive_switch_is_deferred_when_changes_are_unsaved() {
        let mut app = ZargApp {
            archive: "current.zar".to_owned(),
            has_unsaved_changes: true,
            ..Default::default()
        };
        let target = PathBuf::from("next.zar");

        app.open_archive(target.clone(), None);

        assert_eq!(app.archive, "current.zar");
        assert!(app.has_unsaved_changes);
        assert!(matches!(
            app.pending_archive_switch,
            Some(ArchiveSwitchAction::OpenPath(path, None)) if path == target
        ));
    }

    fn wait_for_worker(app: &mut ZargApp) {
        let context = egui::Context::default();
        for _ in 0..12_000 {
            app.poll_worker(&context);
            if !app.busy {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!(
            "archive worker did not finish in time: progress={}, confirmation={:?}",
            app.progress_message, app.confirmation
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
            &crate::settings::Settings {
                archive_defaults: crate::settings::ArchiveDefaults {
                    dates: true,
                    include_files: "*.txt".into(),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap();
        assert!(app.operation.dates);
        assert_eq!(app.operation.include_files, "*.txt");

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
            ("docs/README.md".to_owned(), EntryData::File(12)),
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
        assert_eq!(file_icon("archives/backup.7z"), UiIcon::Archive);
        assert_eq!(file_icon("archives/backup.tar"), UiIcon::Archive);
        assert_eq!(file_icon("archives/backup.tar.gz"), UiIcon::Archive);
        assert_eq!(file_icon("archives/backup.tgz"), UiIcon::Archive);
        assert_eq!(file_icon("unknown.data"), UiIcon::Generic);
    }

    #[test]
    fn bundled_kde_icons_are_valid_images() {
        let icons = [
            UiIcon::OpenArchive,
            UiIcon::NewArchive,
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
            UiIcon::ConfirmApply,
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
            UiIcon::Error,
        ];
        for icon in icons {
            image::load_from_memory(icon_bytes(icon))
                .unwrap_or_else(|error| panic!("invalid bundled icon {icon:?}: {error}"));
        }
    }

    #[test]
    fn error_dialog_window_icon_is_valid() {
        let icon = dialogs::operation_error::error_dialog_icon();
        assert!(icon.width > 0 && icon.height > 0);
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
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
