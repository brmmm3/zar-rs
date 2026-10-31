use eframe::egui;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use zarlib::operations::{
    self, AddInputOptions, ArchiveEntryDetails, CheckReport, CollisionPolicy, CommitInfo,
    CreateFilterOptions, CreateInputOptions, EncodingOptions, ExistingPolicy, ExtractionOptions,
    FilterSet, Progress,
};
use zarlib::{ArchivePath, Toc, TocEntry};

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OperationKind {
    Create,
    Add,
    Remove,
    Join,
    Check,
    Info,
    List,
    Extract,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum UiIcon {
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
    Symlink,
    Image,
    Audio,
    Video,
    Archive,
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
        UiIcon::Symlink => include_bytes!("../assets/icons/mimetypes/emblem-symbolic-link.png"),
        UiIcon::Image => include_bytes!("../assets/icons/mimetypes/image-x-generic.png"),
        UiIcon::Audio => include_bytes!("../assets/icons/mimetypes/audio-x-generic.png"),
        UiIcon::Video => include_bytes!("../assets/icons/mimetypes/video-x-generic.png"),
        UiIcon::Archive => include_bytes!("../assets/icons/mimetypes/application-x-archive.png"),
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

fn icon_texture(
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

fn file_icon(path: &str) -> UiIcon {
    let extension = path.rsplit_once('.').map(|(_, extension)| extension);
    match extension.map(str::to_ascii_lowercase).as_deref() {
        Some("png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "tif" | "tiff") => {
            UiIcon::Image
        }
        Some("mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac") => UiIcon::Audio,
        Some("mp4" | "mkv" | "mov" | "avi" | "webm" | "mpeg" | "mpg") => UiIcon::Video,
        Some("zip" | "7z" | "rar" | "tar" | "gz" | "bz2" | "xz" | "zar") => UiIcon::Archive,
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
            Self::Add => "Add files",
            Self::Remove => "Remove entries",
            Self::Join => "Join commits",
            Self::Check => "Test archive",
            Self::Info => "Archive information",
            Self::List => "List archive",
            Self::Extract => "Extract",
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
    destination: String,
    from_id: String,
    through_id: String,
    block_size: String,
    compression_level: String,
    include_files: String,
    exclude_files: String,
    include_folders: String,
    exclude_folders: String,
    filelists: Vec<PathBuf>,
    password: String,
    strict: bool,
    metadata: bool,
    special_files: bool,
    overwrite: bool,
    recover: bool,
    verbose: bool,
    json: bool,
    dry_run: bool,
    restore_symlinks: bool,
    restore_devices: bool,
    password_is_set: bool,
    collision: usize,
    existing: usize,
    through: String,
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
    Finished(Result<String, String>),
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

#[derive(Default)]
pub struct ZargApp {
    icons: RefCell<HashMap<UiIcon, egui::TextureHandle>>,
    archive: String,
    location: String,
    entries: Vec<(String, EntryData)>,
    entry_details: std::collections::BTreeMap<String, ArchiveEntryDetails>,
    visible_entry_indices: Vec<usize>,
    selected: BTreeSet<String>,
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
    show_about: bool,
    opening_archive: bool,
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
        let opened_archive = archive.clone();
        self.archive.clone_from(&archive);
        self.location.clone_from(&archive);
        self.selected.clear();
        self.report = None;
        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();
        drop(answer_rx);
        let password = password.unwrap_or_default();
        self.opening_archive = true;
        self.begin_worker(events_rx, answer_tx, "Reading archive");
        std::thread::spawn(move || {
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
        if kind.needs_archive() && form.archive.trim().is_empty() {
            self.status = "Choose an archive first.".to_owned();
            return;
        }

        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();
        let progress = UiProgress {
            events: events_tx.clone(),
            answers: Arc::new(Mutex::new(answer_rx)),
        };
        let archive = form.archive.clone();
        let refresh_password = form.password_is_set.then(|| form.password.clone());
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
                UiEvent::Loaded(result) => match result {
                    Ok((toc, details)) => {
                        self.opening_archive = false;
                        self.set_entries(toc, details);
                    }
                    Err(error) => {
                        self.status = error.clone();
                        if self.opening_archive && error.contains("password") {
                            self.operation = OperationForm::new(OperationKind::List, &self.archive);
                            self.operation.password_is_set = true;
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
        self.entries.sort_by(|left, right| left.0.cmp(&right.0));
        self.rebuild_visible_entry_indices();
    }

    fn rebuild_visible_entry_indices(&mut self) {
        self.visible_entry_indices = filtered_entry_indices(&self.entries, &self.filter_text);
    }

    fn choose_inputs(&mut self) {
        if let Some(paths) = rfd::FileDialog::new().pick_files() {
            self.operation.input_paths.extend(paths);
        }
    }

    fn choose_inputs_folder(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_folder() {
            self.operation.input_paths.push(path);
        }
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
                        ui.close();
                    }
                },
            );
            icon_menu_button(
                ui,
                icon_texture(ui.ctx(), &self.icons, UiIcon::Refresh),
                "View",
                |ui| {
                    if ui.text_edit_singleline(&mut self.filter_text).changed() {
                        self.rebuild_visible_entry_indices();
                    }
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
                icon_texture(ui.ctx(), &self.icons, UiIcon::OpenArchive),
                "Favorites",
                |ui| {
                    ui.label("Recently opened archives");
                    if !self.archive.is_empty()
                        && icon_button(ui, &self.icons, UiIcon::OpenArchive, &self.archive, true)
                            .clicked()
                    {
                        self.open_archive(PathBuf::from(&self.archive), None);
                        ui.close();
                    }
                },
            );
            icon_menu_button(
                ui,
                icon_texture(ui.ctx(), &self.icons, UiIcon::Archive),
                "Archive",
                |ui| {
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::ListContents,
                        "List contents...",
                        !self.busy && !self.archive.is_empty(),
                    )
                    .clicked()
                    {
                        self.show_operation(OperationKind::List);
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
    }

    fn render_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (label, icon, kind) in [
                ("Add", UiIcon::Add, OperationKind::Add),
                ("Extract", UiIcon::Extract, OperationKind::Extract),
                ("Test", UiIcon::TestArchive, OperationKind::Check),
                ("Info", UiIcon::Info, OperationKind::Info),
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
                UiIcon::Remove,
                "Remove",
                !self.busy && !self.archive.is_empty(),
            )
            .clicked()
            {
                let mut form = OperationForm::new(OperationKind::Remove, &self.archive);
                form.remove_text = self.selected.iter().cloned().collect::<Vec<_>>().join("\n");
                self.operation = form;
            }
            if icon_button(
                ui,
                &self.icons,
                UiIcon::CreateArchive,
                "Create",
                !self.busy && !self.archive.is_empty(),
            )
            .clicked()
            {
                self.show_operation(OperationKind::Create);
            }
        });
    }

    fn render_operation_dialog(&mut self, context: &egui::Context) {
        let Some(kind) = self.operation.kind else {
            return;
        };
        let mut open = true;
        egui::Window::new(kind.title())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .show(context, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(560.0)
                    .show(ui, |ui| {
                        self.render_operation_fields(ui, kind);
                    });
            });
        if !open {
            self.operation = OperationForm::default();
        }
    }

    fn render_operation_fields(&mut self, ui: &mut egui::Ui, kind: OperationKind) {
        if kind.needs_archive() || kind == OperationKind::Create {
            ui.horizontal(|ui| {
                ui.label("Archive:");
                ui.text_edit_singleline(&mut self.operation.archive);
                if icon_button(ui, &self.icons, UiIcon::Browse, "Browse...", true).clicked() {
                    self.choose_archive(kind == OperationKind::Create);
                }
            });
        }

        match kind {
            OperationKind::Create | OperationKind::Add => {
                ui.horizontal(|ui| {
                    if icon_button(ui, &self.icons, UiIcon::Add, "Add files...", true).clicked() {
                        self.choose_inputs();
                    }
                    if icon_button(ui, &self.icons, UiIcon::AddFolder, "Add folder...", true)
                        .clicked()
                    {
                        self.choose_inputs_folder();
                    }
                    if icon_button(ui, &self.icons, UiIcon::ClearInputs, "Clear inputs", true)
                        .clicked()
                    {
                        self.operation.input_paths.clear();
                    }
                });
                for path in &self.operation.input_paths {
                    ui.label(path.display().to_string());
                }
                ui.horizontal(|ui| {
                    ui.label("Block size:");
                    ui.text_edit_singleline(&mut self.operation.block_size);
                    ui.label("Compression level:");
                    ui.text_edit_singleline(&mut self.operation.compression_level);
                });
                ui.checkbox(&mut self.operation.metadata, "Preserve metadata");
                ui.checkbox(&mut self.operation.special_files, "Include special files");
                ui.checkbox(&mut self.operation.strict, "Strict input validation");
                ui.checkbox(&mut self.operation.verbose, "Show symbolic-link warnings");
                if kind == OperationKind::Create {
                    ui.checkbox(&mut self.operation.overwrite, "Overwrite existing archive");
                    ui.checkbox(&mut self.operation.password_is_set, "Encrypt with password");
                    if self.operation.password_is_set {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.operation.password).password(true),
                        );
                    }
                }
                self.render_filter_fields(ui);
            }
            OperationKind::Remove => {
                ui.label("Archive paths to remove:");
                ui.add(
                    egui::TextEdit::multiline(&mut self.operation.remove_text)
                        .desired_rows(5)
                        .hint_text("One archive path per line"),
                );
                ui.label("The selected entries are prefilled when any are selected.");
            }
            OperationKind::Join => {
                ui.horizontal(|ui| {
                    ui.label("From commit:");
                    ui.text_edit_singleline(&mut self.operation.from_id);
                    ui.label("Through:");
                    ui.text_edit_singleline(&mut self.operation.through_id);
                });
                ui.horizontal(|ui| {
                    ui.label("Block size:");
                    ui.text_edit_singleline(&mut self.operation.block_size);
                    ui.label("Compression level:");
                    ui.text_edit_singleline(&mut self.operation.compression_level);
                });
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
            OperationKind::Info => {
                if ui
                    .checkbox(&mut self.operation.verbose, "Show verbose file statistics")
                    .changed()
                    && self.operation.verbose
                {
                    self.operation.json = false;
                }
                if ui
                    .checkbox(&mut self.operation.json, "Show JSON summary")
                    .changed()
                    && self.operation.json
                {
                    self.operation.verbose = false;
                }
            }
            OperationKind::List => {
                ui.horizontal(|ui| {
                    ui.label("Through commit (optional):");
                    ui.text_edit_singleline(&mut self.operation.through);
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
                    ui.text_edit_singleline(&mut self.operation.destination);
                    if icon_button(ui, &self.icons, UiIcon::Browse, "Browse...", true).clicked()
                        && let Some(path) = rfd::FileDialog::new().pick_folder()
                    {
                        self.operation.destination = path.to_string_lossy().into_owned();
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Through commit (optional):");
                    ui.text_edit_singleline(&mut self.operation.through);
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
                OperationKind::Remove => (UiIcon::Remove, "Remove"),
                OperationKind::Join => (UiIcon::Join, "Join"),
                OperationKind::Check => (UiIcon::TestArchive, "Test"),
                OperationKind::Info => (UiIcon::Info, "Info"),
                OperationKind::List => (UiIcon::ListContents, "List"),
                OperationKind::Extract => (UiIcon::Extract, "Extract"),
            };
            if icon_button(ui, &self.icons, icon, label, true).clicked() {
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
}

impl eframe::App for ZargApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
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
            let columns = [360.0, 110.0, 110.0, 80.0, 110.0, 100.0, 110.0];
            let headers = [
                "Name",
                "Compressed",
                "Uncompressed",
                "Ratio",
                "Owner",
                "Permissions",
                "Type",
            ];
            let content_width = columns.iter().sum::<f32>() + 8.0 * columns.len() as f32;
            ui.horizontal(|ui| {
                ui.set_min_width(content_width);
                left_aligned_cell(ui, 24.0, ui.spacing().interact_size.y, egui::Label::new(""));
                left_aligned_cell(
                    ui,
                    columns[0] - 28.0,
                    ui.spacing().interact_size.y,
                    egui::Label::new(headers[0]).halign(egui::Align::Min),
                );
                for (header, width) in headers.iter().skip(1).zip(columns.iter().skip(1)) {
                    left_aligned_cell(
                        ui,
                        *width,
                        ui.spacing().interact_size.y,
                        egui::Label::new(*header).halign(egui::Align::Min),
                    );
                }
            });
            ui.separator();
            let row_height = ui.spacing().interact_size.y;
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show_rows(
                    ui,
                    row_height,
                    self.visible_entry_indices.len(),
                    |ui, row_range| {
                        ui.set_min_width(content_width);
                        for row in row_range {
                            let (path, data) = &self.entries[self.visible_entry_indices[row]];
                            ui.horizontal(|ui| {
                                ui.set_min_width(content_width);
                                let selected = self.selected.contains(path);
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
                                if name_response.clicked() {
                                    if selected {
                                        self.selected.remove(path);
                                    } else {
                                        self.selected.insert(path.clone());
                                    }
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
                                                        compressed as f64 * 100.0 / *size as f64
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
                                    EntryData::Directory => {
                                        ("—".to_owned(), "—".to_owned(), "—".to_owned(), "Folder")
                                    }
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
                                let owner = details
                                    .and_then(|details| match (details.owner, details.group) {
                                        (Some(owner), Some(group)) => {
                                            Some(format!("{owner}:{group}"))
                                        }
                                        (Some(owner), None) => Some(owner.to_string()),
                                        _ => None,
                                    })
                                    .unwrap_or_else(|| "—".to_owned());
                                let permissions = details
                                    .and_then(|details| {
                                        details.permissions.map(|permissions| {
                                            if details.metadata_platform.as_deref() == Some("unix")
                                            {
                                                format!("{permissions:04o}")
                                            } else if permissions == 0 {
                                                "writable".to_owned()
                                            } else {
                                                "read-only".to_owned()
                                            }
                                        })
                                    })
                                    .unwrap_or_else(|| "—".to_owned());
                                for (value, width) in [
                                    (compressed, columns[1]),
                                    (uncompressed, columns[2]),
                                    (ratio, columns[3]),
                                    (owner, columns[4]),
                                    (permissions, columns[5]),
                                    (kind.to_owned(), columns[6]),
                                ] {
                                    left_aligned_cell(
                                        ui,
                                        width,
                                        row_height,
                                        egui::Label::new(value).halign(egui::Align::Min).truncate(),
                                    );
                                }
                            });
                        }
                    },
                );
        });

        self.render_operation_dialog(&context);

        if let Some(prompt) = self.confirmation.clone() {
            egui::Window::new("Confirmation")
                .collapsible(false)
                .resizable(false)
                .show(&context, |ui| {
                    ui.label(prompt);
                    ui.horizontal(|ui| {
                        if icon_button(ui, &self.icons, UiIcon::Confirm, "Yes", true).clicked() {
                            self.answer_confirmation(true);
                        }
                        if icon_button(ui, &self.icons, UiIcon::Cancel, "No", true).clicked() {
                            self.answer_confirmation(false);
                        }
                    });
                });
        }

        if self.show_about {
            egui::Window::new("About zarg")
                .open(&mut self.show_about)
                .collapsible(false)
                .show(&context, |ui| {
                    ui.heading("ZAR Archive Manager");
                    ui.label("A graphical archive manager for the ZAR format.");
                    ui.label("Built with Rust, egui and zarlib.");
                });
        }
        if let Some(report) = self.report.clone() {
            let mut open = true;
            egui::Window::new("Operation error")
                .open(&mut open)
                .resizable(true)
                .default_width(520.0)
                .show(&context, |ui| {
                    ui.label(report);
                });
            if !open {
                self.report = None;
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
                include_special_files: form.special_files,
                overwrite_existing: form.overwrite,
                verbose: form.verbose,
            };
            Box::new(move || {
                operations::create_archive_with_password_progress_and_filters(
                    &archive,
                    &paths,
                    strict,
                    options,
                    password.as_deref(),
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
                include_special_files: form.special_files,
                strict: form.strict,
                verbose: form.verbose,
            };
            Box::new(move || {
                operations::add_archive_with_encoding_progress_and_options_and_filters(
                    &archive,
                    &paths,
                    options,
                    Some(progress.as_ref()),
                    input,
                    &filters,
                )
                .map(|()| format!("Added selected inputs to {archive}"))
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
            Box::new(move || {
                operations::join_archive_with_encoding_progress(
                    &archive,
                    &from,
                    &through,
                    options,
                    Some(progress.as_ref()),
                )
                .map(|()| format!("Joined commits {from} through {through}"))
                .map_err(|error| error.to_string())
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
        OperationKind::Info => {
            let verbose = form.verbose;
            let json = form.json;
            Box::new(move || {
                operations::info_archive(&archive, verbose)
                    .map(|commits| format_info(&commits, verbose, json))
                    .map_err(|error| error.to_string())
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
            Box::new(move || {
                let options = ExtractionOptions {
                    dry_run,
                    through: through.as_deref(),
                    collision_policy: collision,
                    existing_policy: existing,
                    restore_metadata,
                    restore_symlinks,
                    restore_devices,
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

fn filtered_entry_indices(entries: &[(String, EntryData)], filter: &str) -> Vec<usize> {
    let filter = filter.trim().to_lowercase();
    entries
        .iter()
        .enumerate()
        .filter_map(|(index, (path, _))| {
            (filter.is_empty() || path.to_lowercase().contains(&filter)).then_some(index)
        })
        .collect()
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

fn format_info(commits: &[CommitInfo], verbose: bool, json: bool) -> String {
    if json {
        let entries = commits
            .iter()
            .map(|commit| {
                let id = u32::from_le_bytes(commit.id);
                format!(
                    "{{\"id\":\"{id:08x}\",\"size\":{},\"header_size\":{},\"data_size\":{}}}",
                    commit.size, commit.header_size, commit.data_size
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        return format!(
            "{{\"commit_count\":{},\"commits\":[{entries}]}}",
            commits.len()
        );
    }

    commits
        .iter()
        .map(|commit| {
            let id = u32::from_le_bytes(commit.id);
            let mut line = format!(
                "Commit {id:08x} — stored {}, header {}, data {}, uncompressed {}, ratio {}",
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
                    .unwrap_or_else(|| "n/a".to_owned())
            );
            if verbose && let Some(details) = &commit.details {
                for (name, group) in &details.groups {
                    let ratio = if group.uncompressed_size == 0 {
                        "n/a".to_owned()
                    } else {
                        format!(
                            "{:.2}%",
                            group.compressed_size as f64 * 100.0 / group.uncompressed_size as f64
                        )
                    };
                    line.push_str(&format!(
                        "\n  {name}: {} file(s), stored {}, uncompressed {}, ratio {}",
                        group.files,
                        display_size(group.compressed_size),
                        display_size(group.uncompressed_size),
                        ratio
                    ));
                }
                if details.deduplicated_files > 0 {
                    line.push_str(&format!(
                        "\n  deduplicated files: {}, saved {}",
                        details.deduplicated_files,
                        display_size(details.deduplicated_uncompressed_size)
                    ));
                }
                line.push_str(&format!(
                    "\n  special entries: {} symlinks, {} pipes, {} devices",
                    details.symlinks, details.named_pipes, details.devices
                ));
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use zarlib::operations::HeaderCompressionStats;

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
        assert_eq!(filtered_entry_indices(&entries, ""), [0, 1, 2]);
        assert_eq!(filtered_entry_indices(&entries, "DOCS/"), [0, 2]);
    }

    #[test]
    fn file_icons_follow_mime_type_extensions() {
        assert_eq!(file_icon("images/photo.JPEG"), UiIcon::Image);
        assert_eq!(file_icon("music/song.flac"), UiIcon::Audio);
        assert_eq!(file_icon("src/main.rs"), UiIcon::Code);
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
    fn info_view_includes_size_ratio_and_json_option() {
        let commit = CommitInfo {
            id: [1, 0, 0, 0],
            size: 20,
            header_size: 4,
            data_size: 16,
            uncompressed_size: Some(10),
            ratio_percent: Some(200.0),
            header_compression: Some(HeaderCompressionStats {
                compressed_size: 4,
                uncompressed_size: Some(8),
                ratio_percent: Some(50.0),
            }),
            details: None,
            header_encrypted: false,
            data_encrypted: false,
            encryption_algorithm: None,
        };
        assert!(
            format_info(std::slice::from_ref(&commit), false, false)
                .contains("uncompressed 10 B, ratio 200.00%")
        );
        assert_eq!(
            format_info(&[commit], false, true),
            "{\"commit_count\":1,\"commits\":[{\"id\":\"00000001\",\"size\":20,\"header_size\":4,\"data_size\":16}]}"
        );
    }
}
