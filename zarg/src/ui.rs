use eframe::egui;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use zarlib::operations::{
    self, AddInputOptions, CheckReport, CollisionPolicy, CommitInfo, CreateFilterOptions,
    CreateInputOptions, EncodingOptions, ExistingPolicy, ExtractionOptions, FilterSet, Progress,
};
use zarlib::{Toc, TocEntry};

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
    Loaded(Result<Toc, String>),
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
    archive: String,
    location: String,
    entries: Vec<(String, EntryData)>,
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
        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();
        drop(answer_rx);
        let password = password.unwrap_or_default();
        self.opening_archive = true;
        self.begin_worker(events_rx, answer_tx, "Reading archive");
        std::thread::spawn(move || {
            let result = operations::list_archive(
                &archive,
                None,
                (!password.is_empty()).then_some(password.as_bytes()),
            )
            .map_err(|error| error.to_string());
            match result {
                Ok(toc) => {
                    let count = toc.len();
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
            let outcome = result();
            if outcome.is_ok()
                && let Ok(toc) = operations::list_archive(
                    &archive,
                    refresh_through.as_deref(),
                    refresh_password.as_deref().map(str::as_bytes),
                )
            {
                let _ = events_tx.send(UiEvent::Loaded(Ok(toc)));
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
                    Ok(toc) => {
                        self.opening_archive = false;
                        self.set_entries(toc);
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
                            let report = if self.warnings.is_empty() {
                                summary
                            } else {
                                format!("{}\n\n{}", summary, self.warnings.join("\n"))
                            };
                            self.report = Some(report);
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

    fn set_entries(&mut self, toc: Toc) {
        self.entries = toc
            .iter()
            .map(|(path, entry)| {
                let data = match entry {
                    TocEntry::Directory => EntryData::Directory,
                    TocEntry::File(file) => EntryData::File(file.size),
                    TocEntry::Symlink { target } => EntryData::Symlink(target.clone()),
                    TocEntry::NamedPipe => EntryData::NamedPipe,
                    TocEntry::Device { kind, major, minor } => {
                        EntryData::Device(format!("{kind:?} {major}:{minor}"))
                    }
                };
                (path.as_str().to_owned(), data)
            })
            .collect();
        self.entries.sort_by(|left, right| left.0.cmp(&right.0));
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
            ui.menu_button("File", |ui| {
                if ui.button("Open archive...").clicked() {
                    self.open_dialog();
                    ui.close();
                }
                if ui.button("Create archive...").clicked() {
                    self.show_operation(OperationKind::Create);
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Quit"))
                    .clicked()
                {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui
                    .add_enabled(
                        !self.selected.is_empty() && !self.busy,
                        egui::Button::new("Select all"),
                    )
                    .clicked()
                {
                    self.selected
                        .extend(self.entries.iter().map(|(path, _)| path.clone()));
                    ui.close();
                }
                if ui.button("Clear selection").clicked() {
                    self.selected.clear();
                    ui.close();
                }
            });
            ui.menu_button("View", |ui| {
                ui.text_edit_singleline(&mut self.filter_text);
                if ui.button("Refresh").clicked() && !self.archive.is_empty() {
                    self.open_archive(PathBuf::from(&self.archive), None);
                    ui.close();
                }
            });
            ui.menu_button("Favorites", |ui| {
                ui.label("Recently opened archives");
                if !self.archive.is_empty() && ui.button(&self.archive).clicked() {
                    self.open_archive(PathBuf::from(&self.archive), None);
                    ui.close();
                }
            });
            ui.menu_button("Archive", |ui| {
                if ui
                    .add_enabled(
                        !self.busy && !self.archive.is_empty(),
                        egui::Button::new("List contents..."),
                    )
                    .clicked()
                {
                    self.show_operation(OperationKind::List);
                    ui.close();
                }
            });
            ui.menu_button("Tools", |ui| {
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Test archive"))
                    .clicked()
                {
                    self.show_operation(OperationKind::Check);
                    ui.close();
                }
                if ui
                    .add_enabled(!self.busy, egui::Button::new("Join commits..."))
                    .clicked()
                {
                    self.show_operation(OperationKind::Join);
                    ui.close();
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("About zarg").clicked() {
                    self.show_about = true;
                    ui.close();
                }
            });
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
            for (label, kind) in [
                ("＋ Add", OperationKind::Add),
                ("⇩ Extract", OperationKind::Extract),
                ("✓ Test", OperationKind::Check),
                ("ℹ Info", OperationKind::Info),
            ] {
                if ui
                    .add_enabled(
                        !self.busy && !self.archive.is_empty(),
                        egui::Button::new(label),
                    )
                    .clicked()
                {
                    self.show_operation(kind);
                }
            }
            if ui
                .add_enabled(
                    !self.busy && !self.archive.is_empty(),
                    egui::Button::new("− Remove"),
                )
                .clicked()
            {
                let mut form = OperationForm::new(OperationKind::Remove, &self.archive);
                form.remove_text = self.selected.iter().cloned().collect::<Vec<_>>().join("\n");
                self.operation = form;
            }
            if ui
                .add_enabled(
                    !self.busy && !self.archive.is_empty(),
                    egui::Button::new("＋ Create"),
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
                if ui.button("Browse...").clicked() {
                    self.choose_archive(kind == OperationKind::Create);
                }
            });
        }

        match kind {
            OperationKind::Create | OperationKind::Add => {
                ui.horizontal(|ui| {
                    if ui.button("Add files...").clicked() {
                        self.choose_inputs();
                    }
                    if ui.button("Add folder...").clicked() {
                        self.choose_inputs_folder();
                    }
                    if ui.button("Clear inputs").clicked() {
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
                    if ui.button("Browse...").clicked()
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
            if ui.button(kind.title()).clicked() {
                let form = std::mem::take(&mut self.operation);
                self.start_operation(form);
            }
            if ui.button("Cancel").clicked() {
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
                if ui.button("Add filter list...").clicked()
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
                if ui.button("📂").clicked() {
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
            ui.horizontal(|ui| {
                ui.strong("Name");
                ui.separator();
                ui.strong("Size");
                ui.separator();
                ui.strong("Type");
            });
            ui.separator();
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("archive_entries")
                        .striped(true)
                        .num_columns(3)
                        .show(ui, |ui| {
                            let filter = self.filter_text.to_lowercase();
                            for (path, data) in &self.entries {
                                if !filter.is_empty() && !path.to_lowercase().contains(&filter) {
                                    continue;
                                }
                                let selected = self.selected.contains(path);
                                let display = match data {
                                    EntryData::Directory => format!("📁 {path}"),
                                    EntryData::File(_) => format!("📄 {path}"),
                                    EntryData::Symlink(target) => format!("↗ {path} → {target}"),
                                    EntryData::NamedPipe => format!("◉ {path}"),
                                    EntryData::Device(_) => format!("▣ {path}"),
                                };
                                let response =
                                    ui.selectable_label(selected, display).on_hover_text(path);
                                if response.clicked() {
                                    if selected {
                                        self.selected.remove(path);
                                    } else {
                                        self.selected.insert(path.clone());
                                    }
                                }
                                let (size, kind) = match data {
                                    EntryData::Directory => (String::new(), "Folder".to_owned()),
                                    EntryData::File(size) => {
                                        (display_size(*size), "File".to_owned())
                                    }
                                    EntryData::Symlink(_) => {
                                        (String::new(), "Symbolic link".to_owned())
                                    }
                                    EntryData::NamedPipe => {
                                        (String::new(), "Named pipe".to_owned())
                                    }
                                    EntryData::Device(kind) => (String::new(), kind.clone()),
                                };
                                ui.label(size);
                                ui.label(kind);
                                ui.end_row();
                            }
                        });
                });
        });

        self.render_operation_dialog(&context);

        if let Some(prompt) = self.confirmation.clone() {
            egui::Window::new("Confirmation")
                .collapsible(false)
                .resizable(false)
                .show(&context, |ui| {
                    ui.label(prompt);
                    ui.horizontal(|ui| {
                        if ui.button("Yes").clicked() {
                            self.answer_confirmation(true);
                        }
                        if ui.button("No").clicked() {
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
            egui::Window::new("Operation result")
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
    fn repeated_filter_text_lines_are_parsed() {
        assert_eq!(lines(" *.rs \n\n*.toml"), ["*.rs", "*.toml"]);
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
