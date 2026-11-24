use eframe::egui;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};
use zarlib::operations::{
    self, AddInputOptions, ArchiveEntryDetails, BlockInfo, CheckReport, CollisionPolicy,
    CommitInfoWithBlocks, CreateFilterOptions, CreateInputOptions, EncodingOptions, ExistingPolicy,
    ExtractionOptions, FilterSet, OverwriteDecision, PasswordSet, Progress,
};
use zarlib::{ArchivePath, ContentAead, EncryptionScope, Toc, TocEntry};
mod dialogs;
use crate::settings::ExtractionChecksum;
use dialogs::OperationKind;
use dialogs::benchmark::BenchmarkState;
use dialogs::find::FindState;

fn native_drop_position(frame: &eframe::Frame, context: &egui::Context) -> Option<egui::Pos2> {
    #[cfg(any(windows, target_os = "linux"))]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        match frame.window_handle().ok()?.as_raw() {
            #[cfg(windows)]
            RawWindowHandle::Win32(window) => {
                use windows_sys::Win32::Foundation::POINT;
                use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
                use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
                let mut point = POINT { x: 0, y: 0 };
                let success = unsafe {
                    GetCursorPos(&mut point) != 0
                        && ScreenToClient(window.hwnd.get() as _, &mut point) != 0
                };
                success.then(|| {
                    egui::pos2(point.x as f32, point.y as f32) / context.pixels_per_point()
                })
            }
            #[cfg(target_os = "linux")]
            RawWindowHandle::Xlib(window) => x11_drop_position(window.window as u32, context),
            #[cfg(target_os = "linux")]
            RawWindowHandle::Xcb(window) => x11_drop_position(window.window.get(), context),
            _ => None,
        }
    }
    #[cfg(target_os = "macos")]
    {
        use core_graphics::event::CGEvent;
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
        let _ = frame;
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).ok()?;
        let location = CGEvent::new(source).ok()?.location();
        let origin = context.input(|input| input.viewport().inner_rect)?.min;
        let screen = egui::pos2(location.x as f32, location.y as f32) / context.zoom_factor();
        Some(egui::pos2(screen.x - origin.x, screen.y - origin.y))
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = (frame, context);
        None
    }
}

#[cfg(target_os = "linux")]
fn x11_drop_position(window: u32, context: &egui::Context) -> Option<egui::Pos2> {
    use x11rb::protocol::xproto::ConnectionExt;
    let (connection, _) = x11rb::connect(None).ok()?;
    let pointer = connection.query_pointer(window).ok()?.reply().ok()?;
    pointer.same_screen.then(|| {
        egui::pos2(pointer.win_x as f32, pointer.win_y as f32) / context.pixels_per_point()
    })
}

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
    MTime,
    CTime,
    ATime,
    CompressedSize,
    UncompressedSize,
    Ratio,
    Type,
}

const TIME_COLUMNS: [(&str, &str, SortBy); 3] = [
    ("Modified", "Modification time", SortBy::MTime),
    ("Created", "Creation time", SortBy::CTime),
    ("Accessed", "Access time", SortBy::ATime),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct EntryTimes {
    modified: bool,
    created: bool,
    accessed: bool,
}

impl EntryTimes {
    fn from_details(details: &BTreeMap<String, ArchiveEntryDetails>) -> Self {
        let mut times = Self::default();
        for entry in details.values() {
            times.modified |= entry.modified_unix_millis.is_some();
            times.created |= entry.created_unix_millis.is_some();
            times.accessed |= entry.accessed_unix_millis.is_some();
        }
        times
    }

    fn supports(self, sort: SortBy) -> bool {
        match sort {
            SortBy::MTime => self.modified,
            SortBy::CTime => self.created,
            SortBy::ATime => self.accessed,
            _ => true,
        }
    }

    fn columns(self) -> impl Iterator<Item = (&'static str, &'static str, SortBy)> {
        TIME_COLUMNS
            .into_iter()
            .filter(move |(_, _, sort)| self.supports(*sort))
    }
}

fn details_columns(
    times: EntryTimes,
    has_owner: bool,
    has_permissions: bool,
) -> Vec<(&'static str, f32, Option<SortBy>)> {
    let mut columns = vec![
        ("Name", 360.0, Some(SortBy::Name)),
        ("Compressed", 110.0, Some(SortBy::CompressedSize)),
        ("Uncompressed", 110.0, Some(SortBy::UncompressedSize)),
        ("Ratio", 80.0, Some(SortBy::Ratio)),
    ];
    if has_owner {
        columns.push(("Owner", 110.0, None));
    }
    if has_permissions {
        columns.push(("Permissions", 100.0, None));
    }
    columns.push(("Type", 110.0, Some(SortBy::Type)));
    columns.extend(
        times
            .columns()
            .map(|(label, _, sort)| (label, 160.0, Some(sort))),
    );
    columns
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

struct ArchiveSelectionBand {
    origin: egui::Pos2,
    base: BTreeSet<String>,
    modifiers: egui::Modifiers,
    rows: BTreeMap<String, egui::Rect>,
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
    Find,
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
    View,
    GoUp,
    ClearInputs,
    Confirm,
    ConfirmApply,
    Cancel,
    Close,
    Lock,
    Unlock,
    AddFilterList,
    Folder,
    Bookmarks,
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

#[derive(Default)]
pub(crate) struct IconCache {
    theme: Cell<crate::settings::IconTheme>,
    textures: RefCell<HashMap<UiIcon, egui::TextureHandle>>,
}

impl IconCache {
    fn set_theme(&self, theme: crate::settings::IconTheme) {
        if self.theme.replace(theme) != theme {
            self.textures.borrow_mut().clear();
        }
    }
}

pub(crate) fn icon_bytes(icon: UiIcon, theme: crate::settings::IconTheme) -> &'static [u8] {
    macro_rules! themed_icon {
        ($path:literal) => {
            match theme {
                crate::settings::IconTheme::Crystal => {
                    include_bytes!(concat!("../assets/crystal/", $path)) as &'static [u8]
                }
                crate::settings::IconTheme::Flat => {
                    include_bytes!(concat!("../assets/flat/", $path)) as &'static [u8]
                }
            }
        };
    }
    match icon {
        UiIcon::Application => include_bytes!("../assets/zarg-icon.png"),
        UiIcon::NewArchive => themed_icon!("actions/document-new.png"),
        UiIcon::OpenArchive => themed_icon!("actions/document-open.png"),
        UiIcon::SaveArchive => themed_icon!("actions/document-save.png"),
        UiIcon::Quit => themed_icon!("actions/application-exit.png"),
        UiIcon::Find => themed_icon!("actions/edit-find.png"),
        UiIcon::SelectAll => themed_icon!("actions/edit-select-all.png"),
        UiIcon::ClearSelection | UiIcon::ClearInputs => {
            themed_icon!("actions/edit-clear.png")
        }
        UiIcon::Refresh => themed_icon!("actions/view-refresh.png"),
        UiIcon::ListContents => {
            themed_icon!("actions/document-preview-archive.png")
        }
        UiIcon::Tools => themed_icon!("actions/tools.png"),
        UiIcon::Configure => themed_icon!("actions/configure.png"),
        UiIcon::TestArchive => themed_icon!("actions/archive-test.png"),
        UiIcon::ChangeCommits => themed_icon!("actions/change-commits.png"),
        UiIcon::Undo => themed_icon!("actions/undo.png"),
        UiIcon::Join => themed_icon!("actions/join-commits.png"),
        UiIcon::Benchmark => themed_icon!("actions/benchmark.png"),
        UiIcon::About => themed_icon!("actions/help-about.png"),
        UiIcon::Add => themed_icon!("actions/archive-insert.png"),
        UiIcon::AddFolder => {
            themed_icon!("actions/archive-insert-directory.png")
        }
        UiIcon::Extract => themed_icon!("actions/archive-extract.png"),
        UiIcon::OnlineHelp => themed_icon!("actions/online-help.png"),
        UiIcon::Info => themed_icon!("actions/document-properties.png"),
        UiIcon::Remove => themed_icon!("actions/archive-remove.png"),
        UiIcon::Browse => themed_icon!("actions/document-open-folder.png"),
        UiIcon::View => themed_icon!("actions/view.png"),
        UiIcon::GoUp => themed_icon!("actions/go-up.png"),
        UiIcon::Confirm => themed_icon!("actions/dialog-ok.png"),
        UiIcon::ConfirmApply => themed_icon!("actions/dialog-ok-apply.png"),
        UiIcon::Cancel => themed_icon!("actions/dialog-cancel.png"),
        UiIcon::Close => themed_icon!("actions/dialog-close.png"),
        UiIcon::Lock => themed_icon!("actions/document-encrypt.png"),
        UiIcon::Unlock => themed_icon!("actions/document-decrypt.png"),
        UiIcon::AddFilterList => themed_icon!("actions/list-add.png"),
        UiIcon::Folder => themed_icon!("places/folder.png"),
        UiIcon::Bookmarks => themed_icon!("places/bookmarks.png"),
        UiIcon::Drive => themed_icon!("devices/drive-harddisk.png"),
        UiIcon::Symlink => themed_icon!("mimetypes/emblem-symbolic-link.png"),
        UiIcon::Image => themed_icon!("mimetypes/image-x-generic.png"),
        UiIcon::Audio => themed_icon!("mimetypes/audio-x-generic.png"),
        UiIcon::Video => themed_icon!("mimetypes/video-x-generic.png"),
        UiIcon::Archive => themed_icon!("mimetypes/application-x-archive.png"),
        UiIcon::ZarArchive => include_bytes!("../assets/application-x-zar.png"),
        UiIcon::Pdf => themed_icon!("mimetypes/application-pdf.png"),
        UiIcon::Code => themed_icon!("mimetypes/text-x-csrc.png"),
        UiIcon::Python => themed_icon!("mimetypes/text-x-python.png"),
        UiIcon::Java => themed_icon!("mimetypes/text-x-java.png"),
        UiIcon::Script => themed_icon!("mimetypes/text-x-script.png"),
        UiIcon::JavaScript => {
            themed_icon!("mimetypes/application-x-javascript.png")
        }
        UiIcon::Text => themed_icon!("mimetypes/text-plain.png"),
        UiIcon::Word => themed_icon!("mimetypes/application-vnd.ms-word.png"),
        UiIcon::Spreadsheet => {
            themed_icon!("mimetypes/application-vnd.ms-excel.png")
        }
        UiIcon::Presentation => {
            themed_icon!("mimetypes/application-vnd.ms-powerpoint.png")
        }
        UiIcon::Generic => themed_icon!("mimetypes/unknown.png"),
        UiIcon::SpecialData => {
            themed_icon!("mimetypes/application-octet-stream.png")
        }
        UiIcon::Error => themed_icon!("actions/error.png"),
        UiIcon::Question => themed_icon!("actions/question.png"),
    }
}

pub(crate) fn icon_texture(
    context: &egui::Context,
    cache: &IconCache,
    icon: UiIcon,
) -> egui::TextureHandle {
    if let Some(texture) = cache.textures.borrow().get(&icon) {
        return texture.clone();
    }

    let image = image::load_from_memory(icon_bytes(icon, cache.theme.get()))
        .expect("bundled icon must be a valid image")
        .into_rgba8();
    let [width, height] = [image.width() as usize, image.height() as usize];
    let texture = context.load_texture(
        format!("zarg-icon-{icon:?}"),
        egui::ColorImage::from_rgba_unmultiplied([width, height], image.as_raw()),
        egui::TextureOptions::LINEAR,
    );
    cache.textures.borrow_mut().insert(icon, texture.clone());
    texture
}

fn icon_button(
    ui: &mut egui::Ui,
    cache: &IconCache,
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

fn icon_shortcut_button(
    ui: &mut egui::Ui,
    cache: &IconCache,
    icon: UiIcon,
    label: &str,
    shortcut: &str,
    enabled: bool,
) -> egui::Response {
    let texture = icon_texture(ui.ctx(), cache, icon);
    ui.add_enabled(
        enabled,
        egui::Button::image_and_text(
            egui::Image::new(&texture).fit_to_exact_size(egui::vec2(16.0, 16.0)),
            label,
        )
        .shortcut_text(shortcut),
    )
}

fn consume_alt_key(context: &egui::Context, key: egui::Key) -> bool {
    context.input_mut(|input| {
        input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key {
                    key: pressed_key,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::ALT,
                    ..
                } if *pressed_key == key
            )
        }) && input.consume_key(egui::Modifiers::ALT, key)
    })
}

fn close_or_cancel(response: egui::Response, shortcut_enabled: bool) -> bool {
    let response = response.on_hover_text("Alt+C");
    response.clicked()
        || (shortcut_enabled && response.enabled() && consume_alt_key(&response.ctx, egui::Key::C))
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
        Some("zip" | "7z" | "rar" | "iso" | "tar" | "gz" | "tgz" | "bz2" | "xz") => UiIcon::Archive,
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
struct CommitUnlock {
    id: [u8; 4],
    selected: bool,
    password: String,
}

#[derive(Default)]
pub(super) struct OperationForm {
    kind: Option<OperationKind>,
    new_archive_tab: dialogs::operation::NewArchiveTab,
    add_archive_tab: dialogs::operation::AddArchiveTab,
    extract_tab: dialogs::operation::ExtractTab,
    extract_has_encrypted_commits: bool,
    extract_selected_only: bool,
    extract_selected_paths: BTreeSet<String>,
    archive: String,
    archive_backend: Option<(String, String)>,
    input_paths: Vec<PathBuf>,
    input_archive_folder: Option<ArchivePath>,
    selected_input_paths: BTreeSet<PathBuf>,
    input_selection_anchor: Option<PathBuf>,
    destination: String,
    from_id: String,
    to_id: String,
    join_output: String,
    remove_commits_output: String,
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
    commit_unlocks: Vec<CommitUnlock>,
    archive_passwords: BTreeMap<[u8; 4], String>,
    show_password: bool,
    aead: usize,
    encryption_scope: usize,
    strict: bool,
    metadata: bool,
    dates: bool,
    creation_time: bool,
    access_time: bool,
    special_files: bool,
    save_user: bool,
    overwrite: bool,
    recover: bool,
    verbose: bool,
    dry_run: bool,
    restore_symlinks: bool,
    restore_devices: bool,
    strip_top_level: bool,
    skip_unselected_frame_validation: bool,
    password_is_set: bool,
    collision: usize,
    existing: usize,
    to: String,
    commit_ids: Vec<String>,
    encrypted_commit_ids: BTreeSet<String>,
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

    fn unlock_passwords(&self) -> Result<PasswordSet, Box<dyn std::error::Error>> {
        let mut passwords = PasswordSet::default();
        for commit in &self.commit_unlocks {
            if commit.selected {
                if commit.password.is_empty() {
                    return Err(
                        "Enter a password for every selected commit on the Unlock tab.".into(),
                    );
                }
                passwords.insert_for_commit(commit.id, commit.password.as_bytes().to_vec());
            }
        }
        Ok(passwords)
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
    strict: bool,
    metadata: bool,
    dates: bool,
    creation_time: bool,
    access_time: bool,
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
            strict: form.strict,
            metadata: form.metadata,
            dates: form.dates,
            creation_time: form.creation_time,
            access_time: form.access_time,
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
        form.strict = self.strict;
        form.metadata = self.metadata;
        form.dates = self.dates;
        form.creation_time = self.creation_time;
        form.access_time = self.access_time;
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
    OpenPasswords {
        commits: Vec<operations::CommitInfo>,
        password: Option<String>,
        remembered: BTreeMap<[u8; 4], String>,
        answers: Sender<Option<BTreeMap<[u8; 4], String>>>,
    },
    OpenPasswordError(String),
    RememberPasswords(BTreeMap<[u8; 4], String>),
    ReplacePasswords(BTreeMap<[u8; 4], String>),
    OpeningArchive {
        archive: String,
        passwords: BTreeMap<[u8; 4], String>,
    },
    ArchiveLock(Option<ArchiveLock>),
    BackgroundExtraction(BackgroundExtraction),
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
    ExpandDropFolder(String),
    CacheEncryption(Option<(String, ContentAead, EncryptionScope)>),
    RewriteSnapshot(RewriteSnapshot),
    RewriteUndone {
        steps: usize,
        dirty: bool,
        password: Option<String>,
        passwords: BTreeMap<[u8; 4], String>,
    },
    EntriesRemoved(Vec<String>),
    ArchiveSaved {
        path: String,
    },
    FirstCommitEmpty(bool),
    CommitIds {
        ids: Vec<String>,
        encrypted: BTreeSet<String>,
    },
    SelectedCommit(String),
    ArchivePath {
        path: String,
        password: Option<String>,
    },
    Finished(Result<String, String>),
}

struct RewriteSnapshot {
    directory: tempfile::TempDir,
    dirty: bool,
    password: Option<String>,
    passwords: BTreeMap<[u8; 4], String>,
}

#[derive(Clone)]
struct BackgroundExtraction {
    archive: String,
    commit: Option<String>,
    to: Option<String>,
    passwords: BTreeMap<[u8; 4], String>,
    size: u64,
    modified: std::time::SystemTime,
    state: Arc<Mutex<BackgroundExtractionState>>,
}

type PreparedResult = Result<Arc<operations::PreparedExtraction>, String>;

struct BackgroundExtractionState {
    receiver: Receiver<PreparedResult>,
    result: Option<PreparedResult>,
}

struct BackgroundProgress(Weak<Mutex<BackgroundExtractionState>>);

struct ArchiveLock {
    _file: std::fs::File,
    path: PathBuf,
}

fn lock_archive(path: &Path) -> Option<ArchiveLock> {
    let path = std::fs::canonicalize(path).ok()?;
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .ok()?
    };
    #[cfg(not(windows))]
    let file = {
        let file = std::fs::File::open(&path).ok()?;
        file.try_lock_shared().ok()?;
        file
    };
    Some(ArchiveLock { _file: file, path })
}

impl Progress for BackgroundProgress {
    fn message(&self, _: &str) {}
    fn advance(&self, _: u64) {}
    fn is_cancelled(&self) -> bool {
        self.0.strong_count() == 0
    }
}

impl BackgroundExtraction {
    fn start(
        archive: String,
        commit: Option<String>,
        to: Option<String>,
        passwords: BTreeMap<[u8; 4], String>,
        expected_toc: Toc,
    ) -> Option<Self> {
        let metadata = std::fs::metadata(&archive).ok()?;
        let size = metadata.len();
        let modified = metadata.modified().ok()?;
        let (sender, receiver) = mpsc::channel();
        let state = Arc::new(Mutex::new(BackgroundExtractionState {
            receiver,
            result: None,
        }));
        let progress = BackgroundProgress(Arc::downgrade(&state));
        let worker_archive = archive.clone();
        let worker_passwords = passwords.clone();
        let worker_to = to.clone();
        std::thread::spawn(move || {
            let passwords = password_set(&worker_passwords, None);
            let result = operations::PreparedExtraction::new_with_progress(
                &worker_archive,
                worker_to.as_deref(),
                &passwords,
                Some(&progress),
            )
            .and_then(|prepared| {
                let metadata = std::fs::metadata(&worker_archive)?;
                if metadata.len() != size
                    || metadata.modified()? != modified
                    || prepared.toc() != &expected_toc
                {
                    return Err("archive changed during background validation".into());
                }
                Ok(Arc::new(prepared))
            })
            .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
        Some(Self {
            archive,
            commit,
            to,
            passwords,
            size,
            modified,
            state,
        })
    }

    fn matches(
        &self,
        archive: &str,
        commit: Option<&str>,
        passwords: &BTreeMap<[u8; 4], String>,
    ) -> bool {
        self.archive == archive
            && if commit.is_none_or(str::is_empty) {
                self.to.is_none()
            } else {
                commit == self.commit.as_deref()
            }
            && self.passwords == *passwords
            && self.unchanged()
    }

    fn unchanged(&self) -> bool {
        std::fs::metadata(&self.archive).is_ok_and(|metadata| {
            metadata.len() == self.size && metadata.modified().ok() == Some(self.modified)
        })
    }

    fn wait(
        &self,
        progress: Option<&dyn Progress>,
    ) -> Result<Arc<operations::PreparedExtraction>, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Background validation stopped unexpectedly".to_owned())?;
        loop {
            if let Some(result) = state.result.as_ref() {
                return result.clone();
            }
            if progress.is_some_and(Progress::is_cancelled) {
                return Err("operation cancelled".to_owned());
            }
            match state
                .receiver
                .recv_timeout(std::time::Duration::from_millis(100))
            {
                Ok(result) => state.result = Some(result),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("Background validation stopped unexpectedly".to_owned());
                }
            }
        }
    }
}

struct OverlayCache {
    directory: tempfile::TempDir,
    base_length: u64,
    base_modified: std::time::SystemTime,
    encryption: Option<(String, ContentAead, EncryptionScope)>,
}

impl OverlayCache {
    fn new(base: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let metadata = std::fs::metadata(base)?;
        let directory = tempfile::Builder::new()
            .prefix("zarg-undo-cache-")
            .tempdir()?;
        operations::create_overlay_archive(base, &directory.path().join("cache.zar"))?;
        Ok(Self {
            directory,
            base_length: metadata.len(),
            base_modified: metadata.modified()?,
            encryption: None,
        })
    }

    fn path(&self) -> PathBuf {
        self.directory.path().join("cache.zar")
    }

    fn verify_base(&self, base: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let metadata = std::fs::metadata(base)?;
        if metadata.len() != self.base_length || metadata.modified()? != self.base_modified {
            return Err("The archive changed externally; staged changes were not applied.".into());
        }
        Ok(())
    }
}

#[derive(Clone)]
enum ArchiveSwitchAction {
    New,
    OpenDialog,
    OpenPath(PathBuf, Option<String>),
    Quit,
}

pub(super) struct ArchiveInfoView {
    pub(super) archive: String,
    pub(super) commits: Vec<CommitInfoWithBlocks>,
    pub(super) metadata: Vec<(String, String)>,
}

pub(super) struct ArchiveInfoLoad {
    pub(super) view: ArchiveInfoView,
    pub(super) passwords: BTreeMap<[u8; 4], String>,
}

pub(super) struct ArchiveInfoUnlockPrompt {
    pub(super) commit_id: [u8; 4],
    pub(super) password: String,
    pub(super) error: Option<String>,
    pub(super) busy: bool,
}

struct OpenPasswordRow {
    id: [u8; 4],
    encrypted: bool,
    password: String,
}

struct OpenPasswordDialog {
    rows: Vec<OpenPasswordRow>,
    answers: Sender<Option<BTreeMap<[u8; 4], String>>>,
    error: Option<String>,
    busy: bool,
}

type UnlockCommitsResult = Receiver<Result<BTreeMap<[u8; 4], String>, String>>;

struct UnlockCommitsDialog {
    rows: Vec<OpenPasswordRow>,
    result: Option<UnlockCommitsResult>,
    error: Option<String>,
    busy: bool,
}

fn password_set(passwords: &BTreeMap<[u8; 4], String>, default: Option<&str>) -> PasswordSet {
    let mut result = PasswordSet::new(default.map(|value| value.as_bytes().to_vec()));
    for (id, password) in passwords {
        result.insert_for_commit(*id, password.as_bytes().to_vec());
    }
    result
}

#[cfg(any(test, windows, target_os = "macos"))]
fn drag_preview_icon() -> image::ImageResult<Vec<u8>> {
    const DRAG_ICON_SIZE: u32 = 48;
    let icon = image::load_from_memory(include_bytes!("../assets/zarg-icon.png"))?.resize(
        DRAG_ICON_SIZE,
        DRAG_ICON_SIZE,
        image::imageops::FilterType::Lanczos3,
    );
    let mut encoded = std::io::Cursor::new(Vec::new());
    icon.write_to(&mut encoded, image::ImageFormat::Png)?;
    Ok(encoded.into_inner())
}

#[cfg(any(test, windows, target_os = "macos", target_os = "linux"))]
struct PreparedDrag {
    directory: tempfile::TempDir,
    paths: Vec<PathBuf>,
}

#[cfg(any(test, windows, target_os = "macos", target_os = "linux"))]
fn prepare_drag(
    archive: &str,
    selected: &BTreeSet<String>,
    commit: Option<&str>,
    passwords: &PasswordSet,
    progress: Option<&dyn Progress>,
    extraction: (Option<&operations::PreparedExtraction>, bool),
) -> Result<PreparedDrag, Box<dyn std::error::Error>> {
    let (prepared, validate_blocks) = extraction;
    let paths = selected
        .iter()
        .map(|path| ArchivePath::new(path.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    if paths.is_empty() {
        return Err("No archive entries are selected.".into());
    }
    let roots = paths
        .iter()
        .filter(|path| {
            !path
                .as_str()
                .match_indices('/')
                .any(|(index, _)| selected.contains(&path.as_str()[..index]))
        })
        .collect::<Vec<_>>();
    let directory = tempfile::Builder::new().prefix("zarg-drag-").tempdir()?;
    let mut filters = FilterSet::default();
    filters.restrict_to_paths(paths.clone());
    let loaded_toc;
    let toc = if let Some(prepared) = prepared {
        prepared.toc()
    } else {
        loaded_toc = operations::list_archive_with_passwords(archive, commit, passwords)?;
        &loaded_toc
    };
    for (path, entry) in toc.iter() {
        if !matches!(entry, TocEntry::File(_) | TocEntry::Directory)
            && filters.includes(path.as_str(), false)
        {
            return Err(format!("Cannot drag special archive entry: {path}").into());
        }
    }
    let options = ExtractionOptions {
        dry_run: false,
        to: commit,
        collision_policy: CollisionPolicy::Error,
        existing_policy: ExistingPolicy::Error,
        restore_metadata: false,
        restore_symlinks: false,
        restore_devices: false,
        strip_top_level: false,
    };
    if let Some(prepared) = prepared {
        prepared.extract_with_block_validation(
            directory.path(),
            ExtractionOptions {
                to: prepared.to(),
                ..options
            },
            progress,
            &filters,
            validate_blocks,
        )?;
    } else {
        operations::extract_archive_with_validation_options(
            archive,
            directory.path(),
            options,
            passwords,
            progress,
            &filters,
            operations::ExtractionFrameValidation {
                skip_unselected: false,
                validate_on_read: validate_blocks,
            },
        )?;
    }
    let paths = roots
        .into_iter()
        .map(|path| {
            let output = directory.path().join(path.as_str());
            if !output.exists() {
                return Err(format!("Cannot extract selected item: {path}").into());
            }
            Ok(output)
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    Ok(PreparedDrag { directory, paths })
}

#[cfg(any(test, windows, target_os = "macos", target_os = "linux"))]
fn prepare_drag_with_overlay(
    archive: &str,
    overlay: Option<&Path>,
    selected: &BTreeSet<String>,
    commit: Option<&str>,
    passwords: &PasswordSet,
    progress: Option<&dyn Progress>,
    extraction: (Option<&operations::PreparedExtraction>, ExtractionChecksum),
) -> Result<PreparedDrag, Box<dyn std::error::Error>> {
    let (prepared, checksum) = extraction;
    if progress.is_some_and(|progress| progress.is_cancelled()) {
        return Err("Drag preparation cancelled".into());
    }
    let view = if let Some(overlay) = overlay {
        let directory = tempfile::Builder::new()
            .prefix("zarg-drag-view-")
            .tempdir()?;
        operations::combine_archives_with_progress(
            Path::new(archive),
            overlay,
            &directory.path().join("view.zar"),
            progress,
        )?;
        Some(directory)
    } else {
        None
    };
    if progress.is_some_and(|progress| progress.is_cancelled()) {
        return Err("Drag preparation cancelled".into());
    }
    let archive_path = view
        .as_ref()
        .map(|directory| directory.path().join("view.zar"));
    let source = match &archive_path {
        Some(path) => path
            .to_str()
            .ok_or("Temporary archive path is not valid UTF-8")?,
        None => archive,
    };
    prepare_drag(
        source,
        selected,
        commit,
        passwords,
        progress,
        (
            prepared.filter(|_| view.is_none()),
            checksum.recheck(Path::new(source)),
        ),
    )
}

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
struct NativeDragPreparation {
    receiver: Receiver<Result<PreparedDrag, String>>,
    events: Sender<UiEvent>,
    cancellation: Arc<AtomicBool>,
}

#[cfg(target_os = "linux")]
struct DragPreparation {
    press: crate::wayland_drag::DragPress,
    receiver: Receiver<Result<PreparedDrag, String>>,
    events: Sender<UiEvent>,
    cancellation: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<()>,
}

fn archive_entry_sense(drag_supported: bool) -> egui::Sense {
    if drag_supported {
        egui::Sense::click_and_drag()
    } else {
        egui::Sense::click()
    }
}

/// Unpacks the archive to a scratch folder, then times creating and extracting a
/// new archive from it with the given settings. Nothing outside the scratch folder is written.
type OperationJob = Box<dyn FnOnce() -> Result<String, String> + Send>;
type OperationBuildResult = Result<OperationJob, Box<dyn std::error::Error>>;

struct UiProgress {
    events: Sender<UiEvent>,
    answers: Arc<Mutex<Receiver<Option<OverwriteDecision>>>>,
    cancellation: Option<Arc<AtomicBool>>,
    limit_status_updates: bool,
    last_status_update: Mutex<Option<Instant>>,
}

const ADD_STATUS_UPDATE_INTERVAL: Duration = Duration::from_millis(250);

fn allow_status_update(last_update: &mut Option<Instant>, now: Instant) -> bool {
    if last_update.is_some_and(|last| now.duration_since(last) < ADD_STATUS_UPDATE_INTERVAL) {
        return false;
    }
    *last_update = Some(now);
    true
}

impl Progress for UiProgress {
    fn message(&self, message: &str) {
        if self.limit_status_updates
            && let Ok(mut last_update) = self.last_status_update.lock()
            && !allow_status_update(&mut last_update, Instant::now())
        {
            return;
        }
        let _ = self.events.send(UiEvent::Message(message.to_owned()));
    }

    fn advance(&self, amount: u64) {
        let _ = self.events.send(UiEvent::Advance(amount));
    }

    fn is_cancelled(&self) -> bool {
        self.cancellation
            .as_ref()
            .is_some_and(|token| token.load(Ordering::Relaxed))
    }

    fn set_total(&self, total: u64) {
        let _ = self.events.send(UiEvent::Total(total));
    }

    fn warning(&self, warning: &str) {
        let _ = self.events.send(UiEvent::Warning(warning.to_owned()));
    }

    fn is_interactive(&self) -> bool {
        true
    }

    fn confirm_overwrite(
        &self,
        prompt: &str,
    ) -> Result<OverwriteDecision, Box<dyn std::error::Error>> {
        let prompt = prompt
            .strip_suffix(" [y/N/Always/Never] ")
            .or_else(|| prompt.strip_suffix(" [y/N] "))
            .unwrap_or(prompt);
        self.events.send(UiEvent::Confirm(prompt.to_owned()))?;
        self.answers
            .lock()
            .map_err(|_| "confirmation channel poisoned")?
            .recv()
            .map_err(|_| "confirmation dialog closed")?
            .ok_or_else(|| "operation cancelled".into())
    }

    fn confirm(&self, prompt: &str) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(matches!(
            self.confirm_overwrite(prompt)?,
            OverwriteDecision::Yes | OverwriteDecision::Always
        ))
    }
}

struct WorkerChannels {
    events: Receiver<UiEvent>,
    answers: Sender<Option<OverwriteDecision>>,
    cancellation: Option<Arc<AtomicBool>>,
}

pub struct ZargApp {
    icons: IconCache,
    archive: String,
    read_archive: String,
    displayed_window_title: Option<String>,
    startup_focus_requested: bool,
    imported_archive_dir: Option<tempfile::TempDir>,
    working_archive_dir: Option<tempfile::TempDir>,
    overlay_cache: Option<OverlayCache>,
    archive_lock: Option<ArchiveLock>,
    background_extraction: Option<BackgroundExtraction>,
    rewrite_snapshots: Vec<RewriteSnapshot>,
    has_unsaved_changes: bool,
    latest_archive_empty: Option<bool>,
    add_archive_settings: Option<AddArchiveSettings>,
    location: String,
    entries: Vec<(String, EntryData)>,
    entry_details: std::collections::BTreeMap<String, ArchiveEntryDetails>,
    entry_summary: EntrySummary,
    visible_entry_indices: Vec<usize>,
    selected: BTreeSet<String>,
    selection_anchor: Option<String>,
    selection_current: Option<String>,
    selection_range_base: Option<BTreeSet<String>>,
    archive_list_focused: bool,
    archive_list_focus_id: Option<egui::Id>,
    archive_list_columns: usize,
    archive_list_page_rows: usize,
    selection_press: Option<(String, egui::Modifiers, bool)>,
    archive_list_rect: Option<egui::Rect>,
    archive_row_rects: Vec<(String, egui::Rect)>,
    selection_rubber_band: Option<ArchiveSelectionBand>,
    archive_scroll_offset: f32,
    archive_viewport_height: f32,
    archive_scroll_to: Option<String>,
    archive_scroll_override: Option<f32>,
    archive_max_scroll: f32,
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
    exit_requested: bool,
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
    rename_dialog: Option<(String, String)>,
    internal_drag: bool,
    drop_target: Option<String>,
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    pending_drag: bool,
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    drag_directories: Vec<tempfile::TempDir>,
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    native_drag_preparation: Option<NativeDragPreparation>,
    #[cfg(target_os = "linux")]
    wayland_drag: Option<crate::wayland_drag::WaylandDrag>,
    #[cfg(target_os = "linux")]
    x11_drag: Option<crate::x11_drag::X11Drag>,
    #[cfg(target_os = "linux")]
    linux_drag_initialized: bool,
    #[cfg(target_os = "linux")]
    preparing_drag: Option<DragPreparation>,
    recent_archives: Vec<String>,
    recent_check: Option<Receiver<Vec<String>>>,
    info_loading: Option<Receiver<Result<ArchiveInfoLoad, String>>>,
    info_view: Option<ArchiveInfoView>,
    info_passwords: BTreeMap<[u8; 4], String>,
    info_unlock_prompt: Option<ArchiveInfoUnlockPrompt>,
    benchmark: BenchmarkState,
    find: FindState,
    pending_recent_archive: Option<String>,
    window_geometry: Option<crate::settings::WindowGeometry>,
    geometry_monitor_count: usize,
    dialog_sizes: HashMap<String, [f32; 2]>,
    archive_password: Option<String>,
    archive_passwords: BTreeMap<[u8; 4], String>,
    session_passwords: HashMap<PathBuf, BTreeMap<[u8; 4], String>>,
    open_password_dialog: Option<OpenPasswordDialog>,
    unlock_commits_dialog: Option<UnlockCommitsDialog>,
    column_widths: HashMap<String, f32>,
    entry_times: EntryTimes,
    has_entry_owner: bool,
    has_entry_permissions: bool,
    expanded_tree_dirs: BTreeSet<String>,
    structured_rows_cache: Vec<StructuredRow>,
    structured_rows_dirty: bool,
    show_about: bool,
    show_settings: bool,
    settings_tab: dialogs::settings::SettingsTab,
    ui_scale: f32,
    applied_ui_scale: Option<f32>,
    theme_mode: crate::settings::ThemeMode,
    default_block_size: String,
    default_compression_level: i32,
    default_encryption_method: crate::settings::DefaultEncryptionMethod,
    validate_blocks_before_extraction: bool,
    validate_blocks_on_extraction: ExtractionChecksum,
    performance: crate::settings::PerformanceSettings,
    archive_defaults: crate::settings::ArchiveDefaults,
    default_password: String,
    default_password_input: String,
    show_default_password: bool,
    default_password_status: Option<String>,
    default_password_loading: Option<Receiver<Result<Option<String>, String>>>,
    theme_controller: Option<crate::theme::ThemeController>,
    opening_archive: bool,
    commit_ids: Vec<String>,
    encrypted_commit_ids: BTreeSet<String>,
    selected_commit: String,
    file_dialog: dialogs::file_picker::FileDialog,
}

impl Default for ZargApp {
    fn default() -> Self {
        Self {
            icons: IconCache::default(),
            archive: String::new(),
            read_archive: String::new(),
            displayed_window_title: None,
            startup_focus_requested: false,
            imported_archive_dir: None,
            working_archive_dir: None,
            overlay_cache: None,
            archive_lock: None,
            background_extraction: None,
            rewrite_snapshots: Vec::new(),
            has_unsaved_changes: false,
            latest_archive_empty: None,
            add_archive_settings: None,
            location: String::new(),
            entries: Vec::new(),
            entry_details: std::collections::BTreeMap::new(),
            entry_summary: EntrySummary::default(),
            visible_entry_indices: Vec::new(),
            selected: BTreeSet::new(),
            selection_anchor: None,
            selection_current: None,
            selection_range_base: None,
            archive_list_focused: false,
            archive_list_focus_id: None,
            archive_list_columns: 1,
            archive_list_page_rows: 1,
            selection_press: None,
            archive_list_rect: None,
            archive_row_rects: Vec::new(),
            selection_rubber_band: None,
            archive_scroll_offset: 0.0,
            archive_viewport_height: 0.0,
            archive_scroll_to: None,
            archive_scroll_override: None,
            archive_max_scroll: 0.0,
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
            exit_requested: false,
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
            rename_dialog: None,
            internal_drag: false,
            drop_target: None,
            #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
            pending_drag: false,
            #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
            drag_directories: Vec::new(),
            #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
            native_drag_preparation: None,
            #[cfg(target_os = "linux")]
            wayland_drag: None,
            #[cfg(target_os = "linux")]
            x11_drag: None,
            #[cfg(target_os = "linux")]
            linux_drag_initialized: false,
            #[cfg(target_os = "linux")]
            preparing_drag: None,
            recent_archives: Vec::new(),
            recent_check: None,
            info_loading: None,
            info_view: None,
            info_passwords: BTreeMap::new(),
            info_unlock_prompt: None,
            benchmark: BenchmarkState::default(),
            find: FindState::default(),
            pending_recent_archive: None,
            window_geometry: None,
            geometry_monitor_count: 0,
            dialog_sizes: HashMap::new(),
            archive_password: None,
            archive_passwords: BTreeMap::new(),
            session_passwords: HashMap::new(),
            open_password_dialog: None,
            unlock_commits_dialog: None,
            column_widths: HashMap::new(),
            entry_times: EntryTimes::default(),
            has_entry_owner: false,
            has_entry_permissions: false,
            expanded_tree_dirs: BTreeSet::new(),
            structured_rows_cache: Vec::new(),
            structured_rows_dirty: false,
            show_about: false,
            show_settings: false,
            settings_tab: dialogs::settings::SettingsTab::default(),
            ui_scale: 1.0,
            applied_ui_scale: None,
            theme_mode: crate::settings::ThemeMode::System,
            default_block_size: "8M".to_owned(),
            default_compression_level: 3,
            default_encryption_method: crate::settings::DefaultEncryptionMethod::Aes256Gcm,
            validate_blocks_before_extraction: false,
            validate_blocks_on_extraction: ExtractionChecksum::Auto,
            performance: crate::settings::PerformanceSettings::default(),
            archive_defaults: crate::settings::ArchiveDefaults::default(),
            default_password: String::new(),
            default_password_input: String::new(),
            show_default_password: false,
            default_password_status: None,
            default_password_loading: None,
            theme_controller: None,
            opening_archive: false,
            commit_ids: Vec::new(),
            encrypted_commit_ids: BTreeSet::new(),
            selected_commit: String::new(),
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

#[derive(Default)]
struct EntrySummary {
    compressed: Option<u64>,
    uncompressed: Option<u64>,
    ratio: Option<f64>,
}

impl EntrySummary {
    fn from_entries(
        entries: &[(String, EntryData)],
        details: &std::collections::BTreeMap<String, ArchiveEntryDetails>,
    ) -> Self {
        let mut compressed = Some(0_u64);
        let mut uncompressed = Some(0_u64);
        let mut weighted_compressed = Some(0.0);
        for (path, entry) in entries {
            if let EntryData::File(size) = entry {
                uncompressed = uncompressed.and_then(|total| total.checked_add(*size));
                if *size == 0 {
                    continue;
                }
                let detail = details.get(path);
                compressed =
                    compressed.and_then(|total| total.checked_add(detail?.compressed_size?));
                weighted_compressed = weighted_compressed.and_then(|total| {
                    let ratio = detail?.compression_ratio?;
                    (ratio.is_finite() && ratio >= 0.0).then_some(total + ratio * *size as f64)
                });
            }
        }
        let ratio = uncompressed
            .filter(|total| *total > 0)
            .zip(weighted_compressed)
            .map(|(total, compressed)| compressed / total as f64)
            .filter(|ratio| ratio.is_finite());
        Self {
            compressed,
            uncompressed,
            ratio,
        }
    }

    fn display(&self, entries: usize, commits: usize) -> String {
        let size = |bytes: Option<u64>| bytes.map(display_size).unwrap_or_else(|| "—".to_owned());
        let ratio = self
            .ratio
            .map(|ratio| format!("{:.1}%", ratio * 100.0))
            .unwrap_or_else(|| "—".to_owned());
        format!(
            "{entries} {} | Compressed: {} | Uncompressed: {} | Ratio: {ratio} | {commits} {}",
            if entries == 1 { "entry" } else { "entries" },
            size(self.compressed),
            size(self.uncompressed),
            if commits == 1 { "commit" } else { "commits" },
        )
    }
}

impl ZargApp {
    fn release_archive_lock_for(&mut self, path: &Path) -> bool {
        if self
            .archive_lock
            .as_ref()
            .is_some_and(|lock| std::fs::canonicalize(path).is_ok_and(|path| path == lock.path))
        {
            self.archive_lock = None;
            true
        } else {
            false
        }
    }

    fn remember_commit_passwords(&mut self, passwords: &BTreeMap<[u8; 4], String>) {
        self.archive_passwords.extend(passwords.clone());
        if !self.archive.is_empty() && !self.has_unsaved_changes {
            let path = PathBuf::from(&self.archive);
            let key = std::fs::canonicalize(&path).unwrap_or(path);
            self.session_passwords
                .insert(key, self.archive_passwords.clone());
        }
    }

    pub fn create_default_archive(&mut self) {
        let mut form = self.new_operation_form(OperationKind::NewArchive, "");
        form.password_is_set = false;
        self.start_operation(form);
    }

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
                        let form = app.new_operation_form(OperationKind::ExtractArchive, &archive);
                        app.start_operation(form);
                    }
                    ContextAction::ExtractTo => {
                        app.operation =
                            app.new_operation_form(OperationKind::ExtractArchive, &archive);
                        if !operations::is_supported_import_archive(Path::new(&archive)) {
                            let commits =
                                operations::info_archive(&archive, false).map_err(|error| {
                                    format!("Cannot inspect archive commits: {error}")
                                })?;
                            app.operation.extract_has_encrypted_commits = commits
                                .iter()
                                .any(|commit| commit.header_encrypted || commit.data_encrypted);
                            app.operation.commit_ids = commits
                                .iter()
                                .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                                .collect();
                            app.operation.encrypted_commit_ids = commits
                                .iter()
                                .filter(|commit| commit.header_encrypted || commit.data_encrypted)
                                .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                                .collect();
                            app.operation.to =
                                app.operation.commit_ids.last().cloned().unwrap_or_default();
                        }
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
        if self.has_unsaved_changes
            && self.latest_archive_empty != Some(true)
            && matches!(action, ArchiveSwitchAction::New)
        {
            self.check_first_commit_for_new_archive();
        } else if self.has_unsaved_changes && self.latest_archive_empty != Some(true) {
            self.pending_archive_switch = Some(action);
            self.pending_switch_after_save = false;
        } else {
            self.execute_archive_switch(action);
        }
    }

    fn request_quit(&mut self, context: &egui::Context) {
        if self.busy {
            context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            return;
        }
        if self.has_unsaved_changes && self.latest_archive_empty != Some(true) {
            context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        self.request_archive_switch(ArchiveSwitchAction::Quit);
    }

    fn check_first_commit_for_new_archive(&mut self) {
        let archive = self.read_archive.clone();
        let passwords = password_set(&self.archive_passwords, self.archive_password.as_deref());
        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, _answer_rx) = mpsc::channel();
        self.begin_worker(events_rx, answer_tx, "Checking first commit");
        self.status = "Checking first commit before creating a new archive.".to_owned();
        std::thread::spawn(move || {
            let result = (|| -> Result<bool, String> {
                let commits =
                    operations::info_archive(&archive, false).map_err(|error| error.to_string())?;
                let first = commits.first().ok_or("Archive has no commits.")?;
                let id = format!("{:08x}", u32::from_le_bytes(first.id));
                let toc = operations::list_archive_with_passwords(&archive, Some(&id), &passwords)
                    .map_err(|error| error.to_string())?;
                Ok(toc.is_empty())
            })();
            match result {
                Ok(empty) => {
                    let _ = events_tx.send(UiEvent::FirstCommitEmpty(empty));
                    finish_with_events(events_tx, Ok("First commit checked.".to_owned()));
                }
                Err(error) => finish_with_events(
                    events_tx,
                    Err(format!("Cannot check the first archive commit: {error}")),
                ),
            }
        });
    }

    fn execute_archive_switch(&mut self, action: ArchiveSwitchAction) {
        match action {
            ArchiveSwitchAction::New => self.show_operation_now(OperationKind::NewArchive),
            ArchiveSwitchAction::OpenDialog => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter(
                        "All supported archives",
                        &["zar", "zip", "7z", "rar", "tar", "tgz", "tar.gz"],
                    )
                    .add_filter("ZAR archive", &["zar"])
                    .add_filter("ZIP archive", &["zip"])
                    .add_filter("7z archive", &["7z"])
                    .add_filter("RAR archive", &["rar"])
                    .add_filter("ISO image", &["iso"])
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
            ArchiveSwitchAction::Quit => self.exit_requested = true,
        }
    }

    fn open_archive(&mut self, path: PathBuf, password: Option<String>) {
        self.request_archive_switch(ArchiveSwitchAction::OpenPath(path, password));
    }

    fn open_archive_now(&mut self, path: PathBuf, password: Option<String>) {
        let key = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        let remembered = self
            .session_passwords
            .get(&key)
            .cloned()
            .unwrap_or_default();
        let archive = path.to_string_lossy().into_owned();
        self.report = None;
        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();
        drop(answer_rx);
        self.begin_worker(events_rx, answer_tx, "Reading archive");
        std::thread::spawn(move || {
            let is_imported =
                operations::is_supported_import_archive(PathBuf::from(&archive).as_path());
            let mut archive_lock = (!is_imported)
                .then(|| lock_archive(Path::new(&archive)))
                .flatten();
            let mut selected_passwords = BTreeMap::new();
            if !is_imported {
                let commits = match operations::info_archive(&archive, false) {
                    Ok(commits) => commits,
                    Err(error) => {
                        finish_with_events(
                            events_tx,
                            Err(format!("Cannot read archive commits: {error}")),
                        );
                        return;
                    }
                };
                if commits
                    .iter()
                    .any(|commit| commit.header_encrypted || commit.data_encrypted)
                {
                    let (password_tx, password_rx) = mpsc::channel();
                    if events_tx
                        .send(UiEvent::OpenPasswords {
                            commits,
                            password,
                            remembered,
                            answers: password_tx,
                        })
                        .is_err()
                    {
                        return;
                    }
                    loop {
                        let Ok(Some(passwords)) = password_rx.recv() else {
                            finish_with_events(
                                events_tx,
                                Ok("Opening archive cancelled.".to_owned()),
                            );
                            return;
                        };
                        let set = password_set(&passwords, None);
                        match operations::list_archive_with_details_and_passwords(
                            &archive, None, &set,
                        ) {
                            Ok(_) => {
                                selected_passwords = passwords;
                                break;
                            }
                            Err(error) => {
                                if events_tx
                                    .send(UiEvent::OpenPasswordError(error.to_string()))
                                    .is_err()
                                {
                                    return;
                                }
                            }
                        }
                    }
                }
            }
            let prepared_archive = if is_imported {
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
                Ok(None)
            };
            let (archive_to_read, imported_directory) = match prepared_archive {
                Ok(Some((directory, imported_path))) => (imported_path, Some(directory)),
                Ok(None) => (archive.clone(), None),
                Err(error) => {
                    let error = error.to_string();
                    finish_with_events(events_tx, Err(error));
                    return;
                }
            };
            if is_imported {
                archive_lock = lock_archive(Path::new(&archive_to_read));
            }
            let set = password_set(&selected_passwords, None);
            let result =
                operations::list_archive_with_details_and_passwords(&archive_to_read, None, &set);
            let toc = match result {
                Ok(toc) => toc,
                Err(error) => {
                    finish_with_events(events_tx, Err(error.to_string()));
                    return;
                }
            };
            let (ids, encrypted) = match operations::info_archive(&archive_to_read, false) {
                Ok(commits) => {
                    let ids: Vec<String> = commits
                        .iter()
                        .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                        .collect();
                    let encrypted = commits
                        .iter()
                        .filter(|commit| commit.header_encrypted || commit.data_encrypted)
                        .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                        .collect();
                    (ids, encrypted)
                }
                Err(error) => {
                    finish_with_events(
                        events_tx,
                        Err(format!("Cannot read archive commits: {error}")),
                    );
                    return;
                }
            };
            let background = BackgroundExtraction::start(
                archive_to_read.clone(),
                ids.last().cloned(),
                None,
                selected_passwords.clone(),
                toc.0.clone(),
            );
            if events_tx
                .send(UiEvent::OpeningArchive {
                    archive: archive.clone(),
                    passwords: selected_passwords,
                })
                .is_err()
            {
                return;
            }
            let _ = events_tx.send(UiEvent::ArchiveLock(archive_lock));
            if let Some(background) = background {
                let _ = events_tx.send(UiEvent::BackgroundExtraction(background));
            }
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
            let _ = events_tx.send(UiEvent::CommitIds { ids, encrypted });
            let count = toc.0.len();
            let _ = events_tx.send(UiEvent::SelectedCommit(String::new()));
            let _ = events_tx.send(UiEvent::Loaded(Ok(toc)));
            let summary = if is_imported {
                format!("{count} entries loaded (read-only archive)")
            } else {
                format!("{count} entries loaded")
            };
            finish_with_events(events_tx, Ok(summary));
        });
        self.status = "Reading archive information...".to_owned();
    }

    fn discard_current_archive(&mut self) {
        self.find.clear_results();
        self.archive_lock = None;
        self.background_extraction = None;
        self.unlock_commits_dialog = None;
        self.clear_archive_info();
        self.add_archive_settings = None;
        if self.operation.kind == Some(OperationKind::AddToArchive) {
            self.operation = OperationForm::default();
        }
        self.archive.clear();
        self.read_archive.clear();
        self.location.clear();
        self.imported_archive_dir = None;
        self.working_archive_dir = None;
        self.overlay_cache = None;
        self.rewrite_snapshots.clear();
        self.has_unsaved_changes = false;
        self.latest_archive_empty = None;
        self.archive_password = None;
        self.archive_passwords.clear();
        self.commit_ids.clear();
        self.encrypted_commit_ids.clear();
        self.selected_commit.clear();
        self.selected.clear();
        self.selection_anchor = None;
        self.selection_current = None;
        self.selection_range_base = None;
        self.selection_press = None;
        self.archive_scroll_to = None;
        self.set_entries(Toc::default(), Default::default());
    }

    fn update_window_title(&mut self, context: &egui::Context) {
        let title = match Path::new(&self.archive).file_name() {
            Some(name) => format!("{} - {}", name.to_string_lossy(), crate::WINDOW_TITLE),
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

    fn begin_worker(
        &mut self,
        events: Receiver<UiEvent>,
        answers: Sender<Option<OverwriteDecision>>,
        message: &str,
    ) {
        self.begin_worker_with_cancellation(events, answers, message, None);
    }

    fn begin_worker_with_cancellation(
        &mut self,
        events: Receiver<UiEvent>,
        answers: Sender<Option<OverwriteDecision>>,
        message: &str,
        cancellation: Option<Arc<AtomicBool>>,
    ) {
        self.busy = true;
        self.progress = 0.0;
        self.progress_done = 0;
        self.progress_total = 0;
        self.progress_known = false;
        self.progress_message = message.to_owned();
        self.worker = Some(WorkerChannels {
            events,
            answers,
            cancellation,
        });
    }

    fn start_cached_add(&mut self, form: OperationForm) {
        let prepared = (|| -> Result<_, Box<dyn std::error::Error>> {
            if form.input_paths.is_empty() {
                return Err("Choose one or more files or folders to add.".into());
            }
            let options = form.encoding()?;
            let filters = form.filters()?;
            let passwords = form.unlock_passwords()?;
            if self.overlay_cache.is_none() {
                self.overlay_cache = Some(OverlayCache::new(Path::new(&self.read_archive))?);
            }
            let cache = self.overlay_cache.as_ref().ok_or("Missing undo cache")?;
            cache.verify_base(Path::new(&self.read_archive))?;
            let encryption = form.password_is_set.then(|| {
                (
                    form.password.clone(),
                    [ContentAead::Aes256Gcm, ContentAead::ChaCha20Poly1305][form.aead.min(1)],
                    [
                        EncryptionScope::HeaderAndData,
                        EncryptionScope::Data,
                        EncryptionScope::Header,
                    ][form.encryption_scope.min(2)],
                )
            });
            if self.has_unsaved_changes && cache.encryption != encryption {
                return Err("All staged additions must use the same encryption settings.".into());
            }
            Ok((options, filters, passwords, cache.path(), encryption))
        })();
        let (options, filters, mut passwords, cache_path, encryption) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let base = self.read_archive.clone();
        let dirty = self.has_unsaved_changes;
        let previous_password = self.archive_password.clone();
        let previous_passwords = self.archive_passwords.clone();
        let input = AddInputOptions {
            save_metadata: form.metadata,
            save_modification_time: form.dates,
            save_creation_time: form.creation_time,
            save_access_time: form.access_time,
            save_user: false,
            include_special_files: form.special_files,
            strict: form.strict,
            verbose: form.verbose,
        };
        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();
        let cancellation = Arc::new(AtomicBool::new(false));
        let progress = UiProgress {
            events: events_tx.clone(),
            answers: Arc::new(Mutex::new(answer_rx)),
            cancellation: Some(Arc::clone(&cancellation)),
            limit_status_updates: true,
            last_status_update: Mutex::new(None),
        };
        self.begin_worker_with_cancellation(events_rx, answer_tx, "Adding", Some(cancellation));
        self.report = None;
        self.warnings.clear();
        std::thread::spawn(move || {
            let result = (|| -> Result<_, Box<dyn std::error::Error>> {
                let directory = tempfile::tempdir()?;
                let snapshot_path = directory.path().join("before.zar");
                std::fs::copy(&cache_path, &snapshot_path)?;
                let snapshot = RewriteSnapshot {
                    directory,
                    dirty,
                    password: previous_password,
                    passwords: previous_passwords,
                };
                let cache = cache_path.to_str().ok_or("Cache path is not valid UTF-8")?;
                let result = (|| -> Result<_, Box<dyn std::error::Error>> {
                    let add_encryption = encryption
                        .as_ref()
                        .map(|(password, aead, scope)| (password.as_bytes(), *aead, *scope));
                    if let Some(folder) = &form.input_archive_folder {
                        operations::add_overlay_in_folder_with_encoding_progress_and_filters_and_encryption(
                            &base, cache, &form.input_paths, folder, options, Some(&progress),
                            input, &filters, add_encryption, &passwords,
                        )?;
                    } else {
                        operations::add_overlay_with_encoding_progress_and_filters_and_encryption(
                            &base, cache, &form.input_paths, options, Some(&progress), input,
                            &filters, add_encryption, &passwords,
                        )?;
                    }
                    let mut commits = operations::info_archive(&base, false)?;
                    let overlay_commits = operations::info_archive(cache, false)?;
                    let mut remembered: BTreeMap<_, _> = form
                        .commit_unlocks
                        .iter()
                        .filter(|commit| commit.selected)
                        .map(|commit| (commit.id, commit.password.clone()))
                        .collect();
                    if let (Some(last), Some((password, _, _))) =
                        (overlay_commits.last(), &encryption)
                    {
                        passwords.insert_for_commit(last.id, password.as_bytes().to_vec());
                        remembered.insert(last.id, password.clone());
                    }
                    commits.extend(overlay_commits);
                    let toc = operations::list_archive_with_overlay_and_passwords(
                        &base, cache, &passwords,
                    )?;
                    Ok((commits, toc, remembered))
                })();
                match result {
                    Ok((commits, toc, remembered)) => {
                        let ids = commits
                            .iter()
                            .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                            .collect();
                        let encrypted = commits
                            .iter()
                            .filter(|commit| commit.header_encrypted || commit.data_encrypted)
                            .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                            .collect();
                        let _ = events_tx.send(UiEvent::RewriteSnapshot(snapshot));
                        let _ = events_tx.send(UiEvent::CacheEncryption(encryption));
                        let _ = events_tx.send(UiEvent::ArchiveModified);
                        if let Some(folder) = &form.input_archive_folder {
                            let _ = events_tx
                                .send(UiEvent::ExpandDropFolder(folder.as_str().to_owned()));
                        }
                        let _ = events_tx.send(UiEvent::CommitIds { ids, encrypted });
                        let _ = events_tx.send(UiEvent::RememberPasswords(remembered));
                        let _ = events_tx.send(UiEvent::SelectedCommit(String::new()));
                        let _ = events_tx.send(UiEvent::Loaded(Ok(toc)));
                        Ok(format!("Added selected inputs to {base}"))
                    }
                    Err(error) => {
                        save_archive_copy(&snapshot_path.to_string_lossy(), cache).map_err(
                            |restore| format!("{error}; cannot restore undo cache: {restore}"),
                        )?;
                        Err(error)
                    }
                }
            })()
            .map_err(|error| error.to_string());
            finish_with_events(events_tx, result);
        });
        self.operation = OperationForm::default();
        self.status = "Adding in progress".into();
    }

    fn start_cached_undo(&mut self, steps: usize) {
        if steps == 0 || steps > self.rewrite_snapshots.len() {
            self.status = "Choose a valid number of undo steps.".into();
            return;
        }
        let Some(cache) = &self.overlay_cache else {
            self.status = "No undo cache is available.".into();
            return;
        };
        if let Err(error) = cache.verify_base(Path::new(&self.read_archive)) {
            self.status = error.to_string();
            return;
        }
        let snapshot = &self.rewrite_snapshots[self.rewrite_snapshots.len() - steps];
        let snapshot_path = snapshot.directory.path().join("before.zar");
        let dirty = snapshot.dirty;
        let password = snapshot.password.clone();
        let passwords = snapshot.passwords.clone();
        let cache_path = cache.path();
        let base = self.read_archive.clone();
        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();
        drop(answer_rx);
        self.begin_worker(events_rx, answer_tx, "Undoing changes");
        self.report = None;
        std::thread::spawn(move || {
            let result = (|| -> Result<_, Box<dyn std::error::Error>> {
                let cache = cache_path.to_str().ok_or("Cache path is not valid UTF-8")?;
                let set = password_set(&passwords, password.as_deref());
                let mut commits = operations::info_archive(&base, false)?;
                let toc = if dirty {
                    let snapshot = snapshot_path
                        .to_str()
                        .ok_or("Snapshot path is not valid UTF-8")?;
                    commits.extend(operations::info_archive(snapshot, false)?);
                    operations::list_archive_with_overlay_and_passwords(&base, snapshot, &set)?
                } else {
                    operations::list_archive_with_details_and_passwords(&base, None, &set)?
                };
                save_archive_copy(&snapshot_path.to_string_lossy(), cache)?;
                let ids = commits
                    .iter()
                    .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                    .collect();
                let encrypted = commits
                    .iter()
                    .filter(|commit| commit.header_encrypted || commit.data_encrypted)
                    .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                    .collect();
                let _ = events_tx.send(UiEvent::RewriteUndone {
                    steps,
                    dirty,
                    password,
                    passwords,
                });
                if !dirty {
                    let _ = events_tx.send(UiEvent::CacheEncryption(None));
                }
                let _ = events_tx.send(UiEvent::CommitIds { ids, encrypted });
                let _ = events_tx.send(UiEvent::SelectedCommit(String::new()));
                let _ = events_tx.send(UiEvent::Loaded(Ok(toc)));
                Ok(format!("Undid {steps} step(s)"))
            })()
            .map_err(|error| error.to_string());
            finish_with_events(events_tx, result);
        });
    }

    fn handle_global_shortcuts(&mut self, context: &egui::Context) {
        let save_as = context.input_mut(|input| {
            let shift = input.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key: egui::Key::S,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if modifiers.ctrl && !modifiers.alt && !modifiers.mac_cmd => {
                    Some(modifiers.shift)
                }
                _ => None,
            });
            if let Some(shift) = shift {
                let modifiers = if shift {
                    egui::Modifiers::CTRL | egui::Modifiers::SHIFT
                } else {
                    egui::Modifiers::CTRL
                };
                input.consume_key(modifiers, egui::Key::S);
            }
            shift
        });
        if let Some(save_as) = save_as {
            if save_as {
                self.save_archive_as();
            } else {
                self.save_archive();
            }
        }

        let key = context.input_mut(|input| {
            let key = input.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat,
                    modifiers: egui::Modifiers::ALT,
                    ..
                } if (!repeat || *key == egui::Key::S)
                    && matches!(
                        key,
                        egui::Key::S
                            | egui::Key::N
                            | egui::Key::O
                            | egui::Key::A
                            | egui::Key::E
                            | egui::Key::I
                            | egui::Key::J
                    ) =>
                {
                    Some(*key)
                }
                _ => None,
            });
            if let Some(key) = key {
                input.consume_key(egui::Modifiers::ALT, key);
            }
            key
        });
        match key {
            Some(egui::Key::S) => self.show_settings = true,
            Some(egui::Key::N) if !self.busy => self.show_operation(OperationKind::NewArchive),
            Some(egui::Key::O) if !self.busy => self.open_dialog(),
            Some(egui::Key::A) if self.can_add_to_archive() => {
                self.show_operation(OperationKind::AddToArchive);
            }
            Some(egui::Key::E) if self.can_extract_archive() => {
                self.show_operation(OperationKind::ExtractArchive);
            }
            Some(egui::Key::I) if self.can_show_archive_info() => self.show_archive_info(),
            Some(egui::Key::J) if self.can_join_commits() => {
                self.show_operation(OperationKind::JoinCommits);
            }
            _ => {}
        }
    }

    fn handle_archive_shortcuts(&mut self, context: &egui::Context) {
        if !self.busy
            && self.operation.kind.is_none()
            && self.rename_dialog.is_none()
            && !context.text_edit_focused()
            && self.archive_list_focused
            && context.memory(|memory| {
                memory
                    .focused()
                    .is_none_or(|id| Some(id) == self.archive_list_focus_id)
            })
        {
            if context.input(|input| input.key_pressed(egui::Key::Delete)) {
                self.remove_selected_entries();
            } else if context.input(|input| {
                input.modifiers == egui::Modifiers::NONE && input.key_pressed(egui::Key::Enter)
            }) {
                self.pending_open = self
                    .archive_visible_paths()
                    .into_iter()
                    .find(|path| self.selected.contains(path))
                    .or_else(|| self.selected.iter().next().cloned());
            } else {
                self.handle_archive_selection_keys(context);
            }
        }
        if !self.busy
            && self.operation.kind.is_none()
            && self.rename_dialog.is_none()
            && !context.text_edit_focused()
            && context.input(|input| input.key_pressed(egui::Key::F2))
            && self.selected.len() == 1
        {
            let path = self.selected.iter().next().unwrap().clone();
            let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
            self.rename_dialog = Some((path, name));
        }
    }

    fn archive_visible_paths(&mut self) -> Vec<String> {
        if self.archive_view == ArchiveView::Structured {
            self.refresh_structured_rows();
            self.structured_rows_cache
                .iter()
                .map(|row| row.path.clone())
                .collect()
        } else {
            self.visible_entry_indices
                .iter()
                .map(|index| self.entries[*index].0.clone())
                .collect()
        }
    }

    fn select_all_visible_entries(&mut self, visible: Vec<String>) {
        self.selected = visible.into_iter().collect();
        self.selection_range_base = None;
    }

    fn invert_visible_selection(&mut self, visible: &[String]) {
        self.selected = invert_selection(&self.selected, visible);
        self.selection_anchor = None;
        self.selection_range_base = None;
    }

    fn handle_archive_list_pointer(
        &mut self,
        context: &egui::Context,
        selection_color: egui::Color32,
    ) {
        let (pressed, down, released, position, modifiers) = context.input(|input| {
            (
                input.pointer.primary_pressed(),
                input.pointer.primary_down(),
                input.pointer.primary_released(),
                input.pointer.interact_pos(),
                input.modifiers,
            )
        });
        if pressed {
            self.archive_list_focused = position.is_some_and(|position| {
                self.archive_list_rect
                    .is_some_and(|rect| rect.contains(position))
            });
            if self.archive_list_focused && context.text_edit_focused() {
                context.memory_mut(|memory| {
                    if let Some(id) = memory.focused() {
                        memory.surrender_focus(id);
                    }
                });
            }
            self.archive_list_focus_id = if self.archive_list_focused {
                context.memory(|memory| memory.focused())
            } else {
                None
            };
            if self.archive_list_focused
                && let Some(position) = position
                && !self
                    .archive_row_rects
                    .iter()
                    .any(|(_, rect)| rect.contains(position))
            {
                let base = if modifiers.shift || modifiers.command || modifiers.ctrl {
                    self.selected.clone()
                } else {
                    BTreeSet::new()
                };
                self.selected = base.clone();
                self.selection_anchor = None;
                self.selection_range_base = None;
                self.selection_rubber_band = Some(ArchiveSelectionBand {
                    origin: position + egui::vec2(0.0, self.archive_scroll_offset),
                    base,
                    modifiers,
                    rows: BTreeMap::new(),
                });
            }
        }
        if down && let (Some(band), Some(position)) = (&mut self.selection_rubber_band, position) {
            let scroll = egui::vec2(0.0, self.archive_scroll_offset);
            let rect = egui::Rect::from_two_pos(band.origin, position + scroll);
            self.selected = band.base.clone();
            for (path, row_rect) in &self.archive_row_rects {
                band.rows.insert(path.clone(), row_rect.translate(scroll));
            }
            for (path, row_rect) in &band.rows {
                if rect.intersects(*row_rect) {
                    if (band.modifiers.command || band.modifiers.ctrl) && band.base.contains(path) {
                        self.selected.remove(path);
                    } else {
                        self.selected.insert(path.clone());
                    }
                }
            }
            if let Some(viewport) = self.archive_list_rect {
                let margin = 16.0;
                let delta = if position.y < viewport.top() + margin {
                    (position.y - viewport.top() - margin).max(-16.0)
                } else if position.y > viewport.bottom() - margin {
                    (position.y - viewport.bottom() + margin).min(16.0)
                } else {
                    0.0
                };
                let next_offset =
                    (self.archive_scroll_offset + delta).clamp(0.0, self.archive_max_scroll);
                if next_offset != self.archive_scroll_offset {
                    self.archive_scroll_override = Some(next_offset);
                    context.request_repaint();
                }
            }
            if rect.width() > 0.0 || rect.height() > 0.0 {
                context
                    .layer_painter(egui::LayerId::new(
                        egui::Order::Foreground,
                        egui::Id::unique("archive-selection-band"),
                    ))
                    .with_clip_rect(self.archive_list_rect.unwrap_or(egui::Rect::EVERYTHING))
                    .rect_stroke(
                        rect.translate(-scroll),
                        0.0,
                        egui::Stroke::new(1.0, selection_color),
                        egui::StrokeKind::Inside,
                    );
            }
        }
        if released {
            self.selection_rubber_band = None;
            self.selection_press = None;
            self.archive_scroll_override = None;
        }
    }

    fn handle_archive_selection_keys(&mut self, context: &egui::Context) {
        let (key, modifiers) = context.input(|input| {
            let key = [
                egui::Key::A,
                egui::Key::I,
                egui::Key::ArrowUp,
                egui::Key::ArrowDown,
                egui::Key::ArrowLeft,
                egui::Key::ArrowRight,
                egui::Key::Home,
                egui::Key::End,
                egui::Key::PageUp,
                egui::Key::PageDown,
                egui::Key::Space,
            ]
            .into_iter()
            .find(|key| input.key_pressed(*key));
            (key, input.modifiers)
        });
        let Some(key) = key else { return };
        let visible = self.archive_visible_paths();
        if visible.is_empty() {
            return;
        }
        let command = modifiers.command || modifiers.ctrl;
        if key == egui::Key::A && command {
            self.select_all_visible_entries(visible);
            return;
        }
        if key == egui::Key::I && command {
            self.invert_visible_selection(&visible);
            return;
        }
        let current = self
            .selection_current
            .as_ref()
            .and_then(|path| visible.iter().position(|item| item == path));
        if key == egui::Key::Space {
            if let Some(index) = current {
                let path = &visible[index];
                if command && self.selected.contains(path) {
                    self.selected.remove(path);
                } else {
                    self.selected.insert(path.clone());
                }
                self.selection_anchor = Some(path.clone());
                self.selection_range_base = None;
            }
            return;
        }
        let columns = if matches!(
            self.archive_view,
            ArchiveView::Normal | ArchiveView::Compact
        ) {
            self.archive_list_columns.max(1)
        } else {
            1
        };
        let page = self.archive_list_page_rows.max(1) * columns;
        let index = current.unwrap_or(0);
        if self.archive_view == ArchiveView::Structured && current.is_some() {
            let row = &self.structured_rows_cache[index];
            if key == egui::Key::ArrowRight && row.has_children && !row.expanded {
                self.expanded_tree_dirs.insert(row.path.clone());
                self.structured_rows_dirty = true;
                return;
            }
            if key == egui::Key::ArrowLeft && row.has_children && row.expanded {
                self.expanded_tree_dirs.remove(&row.path);
                self.structured_rows_dirty = true;
                return;
            }
        }
        let next = match key {
            egui::Key::ArrowUp => index.saturating_sub(columns),
            egui::Key::ArrowDown => (index + columns).min(visible.len() - 1),
            egui::Key::ArrowLeft if columns > 1 => index.saturating_sub(1),
            egui::Key::ArrowRight if columns > 1 => (index + 1).min(visible.len() - 1),
            egui::Key::ArrowRight
                if self.archive_view == ArchiveView::Structured
                    && self.structured_rows_cache[index].expanded =>
            {
                (index + 1).min(visible.len() - 1)
            }
            egui::Key::ArrowLeft if self.archive_view == ArchiveView::Structured => self
                .structured_rows_cache[..index]
                .iter()
                .rposition(|row| row.depth < self.structured_rows_cache[index].depth)
                .unwrap_or(index),
            egui::Key::Home => 0,
            egui::Key::End => visible.len() - 1,
            egui::Key::PageUp => index.saturating_sub(page),
            egui::Key::PageDown => (index + page).min(visible.len() - 1),
            _ => return,
        };
        if current == Some(next) {
            return;
        }
        self.archive_scroll_to = Some(visible[next].clone());
        if command {
            self.selection_current = Some(visible[next].clone());
            self.selection_anchor = self.selection_current.clone();
            self.selection_range_base = None;
        } else {
            select_archive_entry(
                &visible,
                &mut self.selected,
                &mut self.selection_anchor,
                &mut self.selection_current,
                &mut self.selection_range_base,
                &visible[next],
                modifiers,
            );
        }
    }

    fn archive_scroll_to_current(
        &mut self,
        row_height: f32,
        columns: usize,
        spacing: f32,
    ) -> Option<f32> {
        if let Some(offset) = self.archive_scroll_override.take() {
            return Some(offset);
        }
        let target = self.archive_scroll_to.take()?;
        let index = self
            .archive_visible_paths()
            .iter()
            .position(|path| path == &target)?;
        let top = (index / columns.max(1)) as f32 * (row_height + spacing);
        let bottom = top + row_height;
        if top < self.archive_scroll_offset {
            Some(top)
        } else if bottom > self.archive_scroll_offset + self.archive_viewport_height {
            Some((bottom - self.archive_viewport_height).max(0.0))
        } else {
            None
        }
    }

    fn start_cached_move(&mut self, paths: Vec<(ArchivePath, ArchivePath)>) {
        self.start_cached_move_to(paths, None);
    }

    fn start_cached_move_to(
        &mut self,
        paths: Vec<(ArchivePath, ArchivePath)>,
        drop_folder: Option<String>,
    ) {
        if self.busy
            || self.read_archive.is_empty()
            || self.imported_archive_dir.is_some()
            || (!self.selected_commit.is_empty()
                && self.commit_ids.last() != Some(&self.selected_commit))
        {
            self.status = "Entries can only be moved in the current archive view.".into();
            return;
        }
        let direct = self.working_archive_dir.is_some() || self.archive.is_empty();
        if !direct && self.overlay_cache.is_none() {
            match OverlayCache::new(Path::new(&self.read_archive)) {
                Ok(cache) => self.overlay_cache = Some(cache),
                Err(error) => {
                    self.status = error.to_string();
                    return;
                }
            }
        }
        if let Some(cache) = &self.overlay_cache
            && let Err(error) = cache.verify_base(Path::new(&self.read_archive))
        {
            self.status = error.to_string();
            return;
        }
        let cache_path = self
            .overlay_cache
            .as_ref()
            .map_or_else(|| PathBuf::from(&self.read_archive), OverlayCache::path);
        let encryption = self
            .overlay_cache
            .as_ref()
            .and_then(|cache| cache.encryption.clone());
        let base = self.read_archive.clone();
        let dirty = self.has_unsaved_changes;
        let previous_password = self.archive_password.clone();
        let previous_passwords = self.archive_passwords.clone();
        let mut passwords = password_set(&previous_passwords, previous_password.as_deref());
        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, _answer_rx) = mpsc::channel();
        let relock = direct && self.release_archive_lock_for(&cache_path);
        self.begin_worker(events_rx, answer_tx, "Moving entries");
        self.report = None;
        self.warnings.clear();
        std::thread::spawn(move || {
            let result = (|| -> Result<_, Box<dyn std::error::Error>> {
                let directory = tempfile::tempdir()?;
                let snapshot_path = directory.path().join("before.zar");
                std::fs::copy(&cache_path, &snapshot_path)?;
                let snapshot = RewriteSnapshot {
                    directory,
                    dirty,
                    password: previous_password,
                    passwords: previous_passwords,
                };
                let cache = cache_path.to_str().ok_or("Cache path is not valid UTF-8")?;
                let result = (|| -> Result<_, Box<dyn std::error::Error>> {
                    let commit_encryption = encryption
                        .as_ref()
                        .map(|(password, aead, scope)| (password.as_bytes(), *aead, *scope));
                    if direct {
                        operations::rename_archive_entries_with_encryption(
                            cache,
                            &paths,
                            commit_encryption,
                            &passwords,
                        )?;
                    } else {
                        operations::rename_overlay_entries_with_encryption(
                            &base,
                            cache,
                            &paths,
                            commit_encryption,
                            &passwords,
                        )?;
                    }
                    let overlay_commits = operations::info_archive(cache, false)?;
                    let mut remembered = BTreeMap::new();
                    if let (Some(last), Some((password, _, _))) =
                        (overlay_commits.last(), &encryption)
                    {
                        passwords.insert_for_commit(last.id, password.as_bytes().to_vec());
                        remembered.insert(last.id, password.clone());
                    }
                    let (commits, toc) = if direct {
                        (
                            overlay_commits,
                            operations::list_archive_with_details_and_passwords(
                                cache, None, &passwords,
                            )?,
                        )
                    } else {
                        let mut commits = operations::info_archive(&base, false)?;
                        commits.extend(overlay_commits);
                        (
                            commits,
                            operations::list_archive_with_overlay_and_passwords(
                                &base, cache, &passwords,
                            )?,
                        )
                    };
                    Ok((commits, toc, remembered))
                })();
                match result {
                    Ok((commits, toc, remembered)) => {
                        let ids = commits
                            .iter()
                            .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                            .collect();
                        let encrypted = commits
                            .iter()
                            .filter(|commit| commit.header_encrypted || commit.data_encrypted)
                            .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                            .collect();
                        let _ = events_tx.send(UiEvent::RewriteSnapshot(snapshot));
                        let _ = events_tx.send(UiEvent::ArchiveModified);
                        if let Some(folder) = drop_folder {
                            let _ = events_tx.send(UiEvent::ExpandDropFolder(folder));
                        }
                        let _ = events_tx.send(UiEvent::CommitIds { ids, encrypted });
                        let _ = events_tx.send(UiEvent::RememberPasswords(remembered));
                        let _ = events_tx.send(UiEvent::SelectedCommit(String::new()));
                        let _ = events_tx.send(UiEvent::Loaded(Ok(toc)));
                        Ok(format!("Moved {} archive path(s)", paths.len()))
                    }
                    Err(error) => {
                        save_archive_copy(&snapshot_path.to_string_lossy(), cache).map_err(
                            |restore| format!("{error}; cannot restore undo cache: {restore}"),
                        )?;
                        Err(error)
                    }
                }
            })()
            .map_err(|error| error.to_string());
            if relock {
                let _ = events_tx.send(UiEvent::ArchiveLock(lock_archive(&cache_path)));
            }
            finish_with_events(events_tx, result);
        });
    }

    fn start_cached_removal(&mut self, form: OperationForm) {
        let paths = match lines(&form.remove_text)
            .into_iter()
            .map(ArchivePath::new)
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(paths) if !paths.is_empty() => paths,
            Ok(_) => {
                self.status = "Enter or select one or more archive paths to remove.".into();
                return;
            }
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        if self.overlay_cache.is_none() {
            match OverlayCache::new(Path::new(&self.read_archive)) {
                Ok(cache) => self.overlay_cache = Some(cache),
                Err(error) => {
                    self.status = error.to_string();
                    return;
                }
            }
        }
        let Some(cache) = &self.overlay_cache else {
            return;
        };
        if let Err(error) = cache.verify_base(Path::new(&self.read_archive)) {
            self.status = error.to_string();
            return;
        }
        let cache_path = cache.path();
        let encryption = cache.encryption.clone();
        let base = self.read_archive.clone();
        let dirty = self.has_unsaved_changes;
        let previous_password = self.archive_password.clone();
        let previous_passwords = self.archive_passwords.clone();
        let mut passwords = password_set(&previous_passwords, previous_password.as_deref());
        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();
        let progress = UiProgress {
            events: events_tx.clone(),
            answers: Arc::new(Mutex::new(answer_rx)),
            cancellation: None,
            limit_status_updates: false,
            last_status_update: Mutex::new(None),
        };
        self.begin_worker(events_rx, answer_tx, "Removing entries");
        self.report = None;
        self.warnings.clear();
        std::thread::spawn(move || {
            let result = (|| -> Result<_, Box<dyn std::error::Error>> {
                let directory = tempfile::tempdir()?;
                let snapshot_path = directory.path().join("before.zar");
                std::fs::copy(&cache_path, &snapshot_path)?;
                let snapshot = RewriteSnapshot {
                    directory,
                    dirty,
                    password: previous_password,
                    passwords: previous_passwords,
                };
                let cache = cache_path.to_str().ok_or("Cache path is not valid UTF-8")?;
                let result = (|| -> Result<_, Box<dyn std::error::Error>> {
                    operations::remove_overlay_entries_with_progress_and_encryption(
                        &base,
                        cache,
                        &paths,
                        Some(&progress),
                        encryption
                            .as_ref()
                            .map(|(password, aead, scope)| (password.as_bytes(), *aead, *scope)),
                        &passwords,
                    )?;
                    let mut commits = operations::info_archive(&base, false)?;
                    let overlay_commits = operations::info_archive(cache, false)?;
                    let mut remembered = BTreeMap::new();
                    if let (Some(last), Some((password, _, _))) =
                        (overlay_commits.last(), &encryption)
                    {
                        passwords.insert_for_commit(last.id, password.as_bytes().to_vec());
                        remembered.insert(last.id, password.clone());
                    }
                    commits.extend(overlay_commits);
                    let toc = operations::list_archive_with_overlay_and_passwords(
                        &base, cache, &passwords,
                    )?;
                    Ok((commits, toc, remembered))
                })();
                match result {
                    Ok((commits, toc, remembered)) => {
                        let ids = commits
                            .iter()
                            .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                            .collect();
                        let encrypted = commits
                            .iter()
                            .filter(|commit| commit.header_encrypted || commit.data_encrypted)
                            .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                            .collect();
                        let _ = events_tx.send(UiEvent::RewriteSnapshot(snapshot));
                        let _ = events_tx.send(UiEvent::ArchiveModified);
                        let _ = events_tx.send(UiEvent::CommitIds { ids, encrypted });
                        let _ = events_tx.send(UiEvent::RememberPasswords(remembered));
                        let _ = events_tx.send(UiEvent::SelectedCommit(String::new()));
                        let _ = events_tx.send(UiEvent::Loaded(Ok(toc)));
                        Ok(format!("Removed {} archive path(s)", paths.len()))
                    }
                    Err(error) => {
                        save_archive_copy(&snapshot_path.to_string_lossy(), cache).map_err(
                            |restore| format!("{error}; cannot restore undo cache: {restore}"),
                        )?;
                        Err(error)
                    }
                }
            })()
            .map_err(|error| error.to_string());
            finish_with_events(events_tx, result);
        });
        self.operation = OperationForm::default();
    }

    fn start_cached_metadata(&mut self, form: OperationForm) {
        let remove: Vec<_> = form
            .meta_items
            .iter()
            .filter(|item| item.3)
            .map(|item| item.0.clone())
            .collect();
        let mut set: Vec<_> = form
            .meta_items
            .iter()
            .filter(|item| !item.3 && item.1 != item.2)
            .map(|item| (item.0.clone(), operations::parse_metadata_value(&item.2)))
            .collect();
        for (key, value) in &form.meta_new {
            if !key.trim().is_empty() {
                set.push((
                    key.trim().to_owned(),
                    operations::parse_metadata_value(value),
                ));
            }
        }
        if set.is_empty() && remove.is_empty() {
            self.status = "No metadata changes to apply.".into();
            return;
        }
        if self.overlay_cache.is_none() {
            match OverlayCache::new(Path::new(&self.read_archive)) {
                Ok(cache) => self.overlay_cache = Some(cache),
                Err(error) => {
                    self.status = error.to_string();
                    return;
                }
            }
        }
        let Some(cache) = &self.overlay_cache else {
            return;
        };
        if let Err(error) = cache.verify_base(Path::new(&self.read_archive)) {
            self.status = error.to_string();
            return;
        }
        let cache_path = cache.path();
        let base = self.read_archive.clone();
        let dirty = self.has_unsaved_changes;
        let previous_password = self.archive_password.clone();
        let previous_passwords = self.archive_passwords.clone();
        let passwords = password_set(&previous_passwords, previous_password.as_deref());
        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();
        drop(answer_rx);
        self.begin_worker(events_rx, answer_tx, "Updating metadata");
        self.report = None;
        std::thread::spawn(move || {
            let result = (|| -> Result<_, Box<dyn std::error::Error>> {
                let directory = tempfile::tempdir()?;
                let snapshot_path = directory.path().join("before.zar");
                std::fs::copy(&cache_path, &snapshot_path)?;
                let snapshot = RewriteSnapshot {
                    directory,
                    dirty,
                    password: previous_password,
                    passwords: previous_passwords,
                };
                let cache = cache_path.to_str().ok_or("Cache path is not valid UTF-8")?;
                let result = (|| -> Result<_, Box<dyn std::error::Error>> {
                    operations::update_overlay_custom_metadata(&base, cache, &set, &remove)?;
                    let mut commits = operations::info_archive(&base, false)?;
                    commits.extend(operations::info_archive(cache, false)?);
                    let toc = operations::list_archive_with_overlay_and_passwords(
                        &base, cache, &passwords,
                    )?;
                    Ok((commits, toc))
                })();
                match result {
                    Ok((commits, toc)) => {
                        let ids = commits
                            .iter()
                            .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                            .collect();
                        let encrypted = commits
                            .iter()
                            .filter(|commit| commit.header_encrypted || commit.data_encrypted)
                            .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                            .collect();
                        let _ = events_tx.send(UiEvent::RewriteSnapshot(snapshot));
                        let _ = events_tx.send(UiEvent::ArchiveModified);
                        let _ = events_tx.send(UiEvent::CommitIds { ids, encrypted });
                        let _ = events_tx.send(UiEvent::SelectedCommit(String::new()));
                        let _ = events_tx.send(UiEvent::Loaded(Ok(toc)));
                        Ok(format!(
                            "Metadata updated: {} set, {} removed",
                            set.len(),
                            remove.len()
                        ))
                    }
                    Err(error) => {
                        save_archive_copy(&snapshot_path.to_string_lossy(), cache).map_err(
                            |restore| format!("{error}; cannot restore undo cache: {restore}"),
                        )?;
                        Err(error)
                    }
                }
            })()
            .map_err(|error| error.to_string());
            finish_with_events(events_tx, result);
        });
        self.operation = OperationForm::default();
    }

    fn materialize_overlay(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(cache) = &self.overlay_cache else {
            return Ok(());
        };
        cache.verify_base(Path::new(&self.read_archive))?;
        if !self.has_unsaved_changes {
            self.overlay_cache = None;
            return Ok(());
        }
        let directory = tempfile::Builder::new()
            .prefix("zarg-working-archive-")
            .tempdir()?;
        let path = directory.path().join("working.zar");
        operations::combine_archives(Path::new(&self.read_archive), &cache.path(), &path)?;
        let mut snapshots = Vec::with_capacity(self.rewrite_snapshots.len());
        for snapshot in &self.rewrite_snapshots {
            let snapshot_directory = tempfile::tempdir()?;
            let target = snapshot_directory.path().join("before.zar");
            if snapshot.dirty {
                operations::combine_archives(
                    Path::new(&self.read_archive),
                    &snapshot.directory.path().join("before.zar"),
                    &target,
                )?;
            } else {
                std::fs::copy(&self.read_archive, &target)?;
            }
            snapshots.push(RewriteSnapshot {
                directory: snapshot_directory,
                dirty: snapshot.dirty,
                password: snapshot.password.clone(),
                passwords: snapshot.passwords.clone(),
            });
        }
        self.read_archive = path.to_string_lossy().into_owned();
        self.working_archive_dir = Some(directory);
        self.rewrite_snapshots = snapshots;
        self.overlay_cache = None;
        Ok(())
    }

    fn temporary_archive_view(
        &self,
    ) -> Result<Option<tempfile::TempDir>, Box<dyn std::error::Error>> {
        let Some(cache) = self
            .overlay_cache
            .as_ref()
            .filter(|_| self.has_unsaved_changes)
        else {
            return Ok(None);
        };
        cache.verify_base(Path::new(&self.read_archive))?;
        let directory = tempfile::Builder::new()
            .prefix("zarg-archive-view-")
            .tempdir()?;
        operations::combine_archives(
            Path::new(&self.read_archive),
            &cache.path(),
            &directory.path().join("view.zar"),
        )?;
        Ok(Some(directory))
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
        let mut add_refresh_passwords = if kind == OperationKind::AddToArchive {
            match form.unlock_passwords() {
                Ok(passwords) => Some(passwords),
                Err(error) => {
                    self.status = error.to_string();
                    return;
                }
            }
        } else {
            None
        };
        let add_new_password = (kind == OperationKind::AddToArchive && form.password_is_set)
            .then(|| form.password.as_bytes().to_vec());
        let current_archive_operation = form.archive == self.archive;
        let mut operation_passwords: BTreeMap<[u8; 4], String> =
            if kind == OperationKind::AddToArchive && current_archive_operation {
                form.commit_unlocks
                    .iter()
                    .filter(|commit| commit.selected)
                    .map(|commit| (commit.id, commit.password.clone()))
                    .collect()
            } else {
                BTreeMap::new()
            };
        let new_commit_password = ((kind == OperationKind::NewArchive
            || (kind == OperationKind::AddToArchive && current_archive_operation))
            && form.password_is_set)
            .then(|| form.password.clone());
        let supplied_operation_password = (current_archive_operation
            && matches!(
                kind,
                OperationKind::ListArchive
                    | OperationKind::ExtractArchive
                    | OperationKind::TestArchive
            )
            && form.password_is_set
            && !form.password.is_empty())
        .then(|| {
            (
                form.password.clone(),
                (kind != OperationKind::TestArchive && !form.to.trim().is_empty())
                    .then(|| form.to.trim().to_owned()),
            )
        });

        let logical_archive = form.archive.clone();
        let output = match kind {
            OperationKind::JoinCommits => form.join_output.trim(),
            OperationKind::RemoveCommits => form.remove_commits_output.trim(),
            OperationKind::ChangeCommits => form.transform_output.trim(),
            _ => "",
        };
        let explicit_output = !output.is_empty();
        if explicit_output {
            let destination = std::fs::canonicalize(output).ok();
            if output == form.archive
                || output == self.archive
                || output == self.read_archive
                || destination.as_ref().is_some_and(|destination| {
                    [&form.archive, &self.archive, &self.read_archive]
                        .iter()
                        .any(|source| {
                            std::fs::canonicalize(source).ok().as_ref() == Some(destination)
                        })
                })
            {
                self.status =
                    "Write to must be a different archive; leave it empty for an in-place change."
                        .to_owned();
                return;
            }
        }
        let changes_archive = match kind {
            OperationKind::NewArchive
            | OperationKind::AddToArchive
            | OperationKind::RemoveEntries
            | OperationKind::UndoCommits
            | OperationKind::CustomMetadata => true,
            OperationKind::JoinCommits
            | OperationKind::RemoveCommits
            | OperationKind::ChangeCommits => !explicit_output,
            _ => false,
        };
        if current_archive_operation
            && !self.archive.is_empty()
            && self.imported_archive_dir.is_none()
            && self.working_archive_dir.is_none()
        {
            match kind {
                OperationKind::AddToArchive => {
                    self.start_cached_add(form);
                    return;
                }
                OperationKind::RemoveEntries => {
                    self.start_cached_removal(form);
                    return;
                }
                OperationKind::CustomMetadata => {
                    self.start_cached_metadata(form);
                    return;
                }
                OperationKind::UndoCommits if self.overlay_cache.is_some() => {
                    self.start_cached_undo(form.undo_commits);
                    return;
                }
                _ => {}
            }
        }
        if current_archive_operation
            && changes_archive
            && self.overlay_cache.is_some()
            && let Err(error) = self.materialize_overlay()
        {
            self.status = format!("Cannot prepare working archive: {error}");
            return;
        }
        let archive_view = if current_archive_operation && !changes_archive {
            match self.temporary_archive_view() {
                Ok(view) => view,
                Err(error) => {
                    self.status = format!("Cannot prepare archive view: {error}");
                    return;
                }
            }
        } else {
            None
        };
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
        } else if changes_archive
            && form.archive == self.archive
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

        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();
        let cancellation = matches!(
            kind,
            OperationKind::NewArchive
                | OperationKind::AddToArchive
                | OperationKind::ExtractArchive
                | OperationKind::TestArchive
        )
        .then(|| Arc::new(AtomicBool::new(false)));
        let progress = UiProgress {
            events: events_tx.clone(),
            answers: Arc::new(Mutex::new(answer_rx)),
            cancellation: cancellation.clone(),
            limit_status_updates: kind == OperationKind::AddToArchive,
            last_status_update: Mutex::new(None),
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
        let archive = archive_view.as_ref().map_or(archive, |directory| {
            directory
                .path()
                .join("view.zar")
                .to_string_lossy()
                .into_owned()
        });
        form.archive.clone_from(&archive);
        if kind == OperationKind::UndoCommits
            && (!self.has_unsaved_changes
                || form.undo_commits == 0
                || form.undo_commits > self.maximum_undo_steps())
        {
            self.status = "Choose a valid number of undo steps.".to_owned();
            return;
        }
        let undo_snapshots =
            if kind == OperationKind::UndoCommits && !self.rewrite_snapshots.is_empty() {
                let count = form.undo_commits.min(self.rewrite_snapshots.len());
                let snapshot = &self.rewrite_snapshots[self.rewrite_snapshots.len() - count];
                Some((
                    count,
                    snapshot.directory.path().join("before.zar"),
                    snapshot.dirty,
                    snapshot.password.clone(),
                    snapshot.passwords.clone(),
                ))
            } else {
                None
            };
        let restoring_snapshot = undo_snapshots.is_some();
        let refresh_password = if let Some((_, _, _, password, _)) = &undo_snapshots {
            password.clone()
        } else if kind == OperationKind::UndoCommits || explicit_output {
            self.archive_password.clone()
        } else if kind == OperationKind::ChangeCommits && !explicit_output {
            match form.transform_action {
                0 => Some(form.password.clone()),
                1 => None,
                _ => Some(form.password.clone()),
            }
        } else {
            form.commit_unlocks
                .iter()
                .find(|commit| kind == OperationKind::AddToArchive && commit.selected)
                .map(|commit| commit.password.clone())
                .or_else(|| form.password_is_set.then(|| form.password.clone()))
                .or_else(|| {
                    (kind != OperationKind::NewArchive)
                        .then(|| self.archive_password.clone())
                        .flatten()
                })
        };
        let refresh_archive = archive.clone();
        let mut refresh_commit_passwords = if let Some((_, _, _, _, passwords)) = &undo_snapshots {
            passwords.clone()
        } else if kind == OperationKind::ChangeCommits && !explicit_output {
            BTreeMap::new()
        } else {
            self.archive_passwords.clone()
        };
        let is_encryption_transform = kind == OperationKind::ChangeCommits;
        let refresh_commit_ids = changes_archive;
        let take_snapshot = changes_archive
            && kind != OperationKind::NewArchive
            && kind != OperationKind::UndoCommits;
        let snapshot_dirty = self.has_unsaved_changes;
        let snapshot_password = self.archive_password.clone();
        let snapshot_passwords = self.archive_passwords.clone();
        let transform_passwords =
            (kind == OperationKind::ChangeCommits && !explicit_output).then(|| {
                (
                    form.from_id.clone(),
                    form.to_id.clone(),
                    (form.transform_action != 1).then(|| form.password.clone()),
                    self.archive_passwords.clone(),
                    !form.transform_strict && form.existing_password.is_empty(),
                )
            });
        let refresh_to = if kind == OperationKind::ListArchive {
            (!form.to.trim().is_empty()).then(|| form.to.clone())
        } else if !refresh_commit_ids && !self.selected_commit.is_empty() {
            Some(self.selected_commit.clone())
        } else {
            None
        };
        let refresh_removed_paths = (kind == OperationKind::RemoveEntries
            && self.commit_ids.last() == Some(&self.selected_commit)
            && self.latest_archive_empty.is_some())
        .then(|| lines(&form.remove_text));
        let drop_folder = (kind == OperationKind::AddToArchive)
            .then_some(form.input_archive_folder.as_ref())
            .flatten()
            .map(|folder| folder.as_str().to_owned());
        let background = self
            .background_extraction
            .as_ref()
            .filter(|background| {
                kind == OperationKind::ExtractArchive
                    && !self.validate_blocks_before_extraction
                    && !self.has_unsaved_changes
                    && !form.password_is_set
                    && background.matches(&archive, Some(form.to.as_str()), &form.archive_passwords)
            })
            .cloned();
        let result = match if let Some((steps, source, dirty, password, passwords)) = undo_snapshots
        {
            let target = archive.clone();
            let events = events_tx.clone();
            Ok(Box::new(move || {
                let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
                let candidate = directory.path().join("undo.zar");
                std::fs::copy(source, &candidate)
                    .map_err(|error| format!("Cannot prepare undo: {error}"))?;
                save_archive_copy(&candidate.to_string_lossy(), &target)?;
                let _ = events.send(UiEvent::RewriteUndone {
                    steps,
                    dirty,
                    password,
                    passwords,
                });
                Ok(format!("Undid {steps} step(s)"))
            }) as OperationJob)
        } else {
            build_job(
                kind,
                form,
                events_tx.clone(),
                progress,
                background,
                self.validate_blocks_on_extraction,
            )
        } {
            Ok(job) => job,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let relock = changes_archive && self.release_archive_lock_for(Path::new(&archive));
        self.begin_worker_with_cancellation(events_rx, answer_tx, kind.title(), cancellation);
        self.report = None;
        self.warnings.clear();
        std::thread::spawn(move || {
            let _archive_view = archive_view;
            let skipped_encrypted = if transform_passwords
                .as_ref()
                .is_some_and(|(_, _, _, _, skip)| *skip)
            {
                match operations::info_archive(&archive, false) {
                    Ok(commits) => commits
                        .into_iter()
                        .filter(|commit| commit.header_encrypted || commit.data_encrypted)
                        .map(|commit| commit.id)
                        .collect::<BTreeSet<_>>(),
                    Err(error) => {
                        if relock {
                            let _ = events_tx
                                .send(UiEvent::ArchiveLock(lock_archive(Path::new(&archive))));
                        }
                        finish_with_events(
                            events_tx,
                            Err(format!("Cannot inspect archive commits: {error}")),
                        );
                        return;
                    }
                }
            } else {
                BTreeSet::new()
            };
            let snapshot = if take_snapshot {
                Some((|| {
                    let directory = tempfile::tempdir()
                        .map_err(|error| format!("Cannot prepare undo: {error}"))?;
                    std::fs::copy(&archive, directory.path().join("before.zar"))
                        .map_err(|error| format!("Cannot prepare undo: {error}"))?;
                    Ok::<_, String>(RewriteSnapshot {
                        directory,
                        dirty: snapshot_dirty,
                        password: snapshot_password,
                        passwords: snapshot_passwords,
                    })
                })())
            } else {
                None
            };
            let mut outcome = match snapshot {
                Some(Ok(snapshot)) => match result() {
                    Ok(summary) => {
                        let _ = events_tx.send(UiEvent::RewriteSnapshot(snapshot));
                        Ok(summary)
                    }
                    Err(error) => {
                        match save_archive_copy(
                            &snapshot
                                .directory
                                .path()
                                .join("before.zar")
                                .to_string_lossy(),
                            &archive,
                        ) {
                            Ok(()) => Err(error),
                            Err(restore_error) => Err(format!(
                                "{error}; cannot restore the working archive: {restore_error}"
                            )),
                        }
                    }
                },
                Some(Err(error)) => Err(error),
                None => result(),
            };
            if outcome.is_ok()
                && let Some((password, to)) = supplied_operation_password
            {
                match operations::info_archive(&archive, false) {
                    Ok(commits) => {
                        let mut candidates = BTreeMap::new();
                        for commit in commits {
                            if commit.header_encrypted || commit.data_encrypted {
                                candidates.insert(commit.id, password.clone());
                            }
                            if to.as_ref().is_some_and(|id| {
                                id.eq_ignore_ascii_case(&format!(
                                    "{:08x}",
                                    u32::from_le_bytes(commit.id)
                                ))
                            }) {
                                break;
                            }
                        }
                        let set = password_set(&candidates, None);
                        match operations::info_archive_with_blocks_and_passwords(
                            &archive, false, &set,
                        ) {
                            Ok(_) => {
                                refresh_commit_passwords.extend(candidates.clone());
                                operation_passwords.extend(candidates);
                            }
                            Err(error) => {
                                let _ = events_tx.send(UiEvent::Warning(format!(
                                    "Could not remember commit passwords: {error}"
                                )));
                            }
                        }
                    }
                    Err(error) => {
                        let _ = events_tx.send(UiEvent::Warning(format!(
                            "Could not inspect commit passwords: {error}"
                        )));
                    }
                }
            }
            if outcome.is_ok() {
                if let Some((directory, path)) = new_working_archive {
                    let _ = events_tx.send(UiEvent::WorkingArchive {
                        directory,
                        path: path.to_string_lossy().into_owned(),
                        logical_path: logical_archive,
                        dirty: true,
                        new_archive_settings,
                    });
                } else if changes_archive && !restoring_snapshot {
                    let _ = events_tx.send(UiEvent::ArchiveModified);
                }
                if let Some(folder) = drop_folder {
                    let _ = events_tx.send(UiEvent::ExpandDropFolder(folder));
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
                        if kind == OperationKind::RemoveCommits && !explicit_output {
                            let retained: BTreeSet<_> =
                                commits.iter().map(|commit| commit.id).collect();
                            refresh_commit_passwords.retain(|id, _| retained.contains(id));
                            let _ = events_tx
                                .send(UiEvent::ReplacePasswords(refresh_commit_passwords.clone()));
                        }
                        if let Some((from, to, new_password, mut passwords, _)) =
                            transform_passwords
                        {
                            let mut in_range = false;
                            for commit in &commits {
                                let id = format!("{:08x}", u32::from_le_bytes(commit.id));
                                if id == from {
                                    in_range = true;
                                }
                                if in_range && !skipped_encrypted.contains(&commit.id) {
                                    passwords.remove(&commit.id);
                                    if (commit.header_encrypted || commit.data_encrypted)
                                        && let Some(password) = &new_password
                                    {
                                        passwords.insert(commit.id, password.clone());
                                    }
                                }
                                if id == to {
                                    in_range = false;
                                }
                            }
                            refresh_commit_passwords = passwords.clone();
                            let _ = events_tx.send(UiEvent::ReplacePasswords(passwords));
                        }
                        if let (Some(commit), Some(password)) =
                            (commits.last(), &new_commit_password)
                        {
                            operation_passwords.insert(commit.id, password.clone());
                        }
                        if let (Some(passwords), Some(password)) =
                            (&mut add_refresh_passwords, add_new_password)
                        {
                            let Some(commit) = commits.last() else {
                                if relock {
                                    let _ = events_tx.send(UiEvent::ArchiveLock(lock_archive(
                                        Path::new(&archive),
                                    )));
                                }
                                finish_with_events(
                                    events_tx,
                                    Err("Operation completed, but archive refresh failed: no commits found".to_owned()),
                                );
                                return;
                            };
                            passwords.insert_for_commit(commit.id, password);
                        }
                        let ids = commits
                            .iter()
                            .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                            .collect();
                        let encrypted = commits
                            .iter()
                            .filter(|commit| commit.header_encrypted || commit.data_encrypted)
                            .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                            .collect();
                        let _ = events_tx.send(UiEvent::CommitIds { ids, encrypted });
                    }
                    Err(error) => {
                        outcome = Err(format!(
                            "{} completed, but commit refresh failed: {error}",
                            if is_encryption_transform {
                                "Encryption transform"
                            } else if kind == OperationKind::AddToArchive {
                                "Add"
                            } else {
                                kind.title()
                            }
                        ));
                    }
                }
            }
            if outcome.is_ok()
                && let Some(paths) = refresh_removed_paths
            {
                let _ = events_tx.send(UiEvent::EntriesRemoved(paths));
                let _ = events_tx.send(UiEvent::SelectedCommit(String::new()));
            } else if outcome.is_ok() {
                let list_password = if kind == OperationKind::ChangeCommits && !explicit_output {
                    None
                } else {
                    refresh_password.as_deref()
                };
                let result = if let Some(passwords) = &mut add_refresh_passwords {
                    operations::list_archive_with_details_and_passwords(
                        &refresh_archive,
                        refresh_to.as_deref(),
                        passwords,
                    )
                } else {
                    let passwords = password_set(&refresh_commit_passwords, list_password);
                    operations::list_archive_with_details_and_passwords(
                        &refresh_archive,
                        refresh_to.as_deref(),
                        &passwords,
                    )
                };
                match result {
                    Ok(toc) => {
                        if !operation_passwords.is_empty() {
                            let _ = events_tx.send(UiEvent::RememberPasswords(operation_passwords));
                        }
                        let _ = events_tx.send(UiEvent::SelectedCommit(
                            refresh_to.clone().unwrap_or_default(),
                        ));
                        let _ = events_tx.send(UiEvent::Loaded(Ok(toc)));
                    }
                    Err(error) => {
                        outcome = Err(format!(
                            "Operation completed, but archive refresh failed: {error}"
                        ));
                    }
                }
            }
            if relock {
                let _ = events_tx.send(UiEvent::ArchiveLock(lock_archive(Path::new(&archive))));
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

        let view = match self.temporary_archive_view() {
            Ok(view) => view,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, _answer_rx) = mpsc::channel();
        self.begin_worker(events_rx, answer_tx, "Extracting file");
        self.status = format!("Opening {path}");
        self.warnings.clear();
        let archive = view.as_ref().map_or_else(
            || self.read_archive.clone(),
            |directory| {
                directory
                    .path()
                    .join("view.zar")
                    .to_string_lossy()
                    .into_owned()
            },
        );
        let passwords = password_set(&self.archive_passwords, self.archive_password.as_deref());
        let to = (!self.selected_commit.is_empty()).then(|| self.selected_commit.clone());
        let background = self.matching_background(&archive, to.as_deref());
        let checksum = self.validate_blocks_on_extraction;
        std::thread::spawn(move || {
            let _view = view;
            let prepared = background.and_then(|background| {
                background
                    .wait(None)
                    .ok()
                    .filter(|_| background.unchanged())
            });
            let outcome = extract_and_open(
                &archive,
                &path,
                &passwords,
                to.as_deref(),
                prepared.as_deref(),
                checksum.recheck(Path::new(&archive)),
            );
            finish_with_events(events_tx, outcome);
        });
    }

    fn matching_background(
        &self,
        archive: &str,
        commit: Option<&str>,
    ) -> Option<BackgroundExtraction> {
        self.background_extraction
            .as_ref()
            .filter(|background| {
                !self.validate_blocks_before_extraction
                    && !self.has_unsaved_changes
                    && self.archive_password.is_none()
                    && background.matches(archive, commit, &self.archive_passwords)
            })
            .cloned()
    }

    fn drag_supported(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.wayland_drag.is_some() || self.x11_drag.is_some()
        }
        #[cfg(not(target_os = "linux"))]
        {
            cfg!(any(windows, target_os = "macos"))
        }
    }

    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    fn start_selected_drag(&mut self) {
        if self.busy || self.native_drag_preparation.is_some() {
            return;
        }
        let overlay = self
            .overlay_cache
            .as_ref()
            .filter(|_| self.has_unsaved_changes);
        if let Some(cache) = overlay
            && let Err(error) = cache.verify_base(Path::new(&self.read_archive))
        {
            self.report = Some(error.to_string());
            return;
        }
        let overlay = overlay.map(OverlayCache::path);
        let archive = self.read_archive.clone();
        let selected = self.selected.clone();
        let passwords = password_set(&self.archive_passwords, self.archive_password.as_deref());
        let commit = (!self.selected_commit.is_empty()).then(|| self.selected_commit.clone());
        let background = self.matching_background(&archive, commit.as_deref());
        let checksum = self.validate_blocks_on_extraction;
        let (sender, receiver) = mpsc::channel();
        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();
        let cancellation = Arc::new(AtomicBool::new(false));
        let progress = UiProgress {
            events: events_tx.clone(),
            answers: Arc::new(Mutex::new(answer_rx)),
            cancellation: Some(Arc::clone(&cancellation)),
            limit_status_updates: false,
            last_status_update: Mutex::new(None),
        };
        self.begin_worker_with_cancellation(
            events_rx,
            answer_tx,
            "Extracting dragged items; keep the mouse button pressed",
            Some(Arc::clone(&cancellation)),
        );
        std::thread::spawn(move || {
            let prepared = background.and_then(|background| {
                background
                    .wait(Some(&progress))
                    .ok()
                    .filter(|_| background.unchanged())
            });
            let result = prepare_drag_with_overlay(
                &archive,
                overlay.as_deref(),
                &selected,
                commit.as_deref(),
                &passwords,
                Some(&progress),
                (prepared.as_deref(), checksum),
            )
            .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
        self.native_drag_preparation = Some(NativeDragPreparation {
            receiver,
            events: events_tx,
            cancellation,
        });
        self.status = "Preparing drag; keep the mouse button pressed".into();
    }

    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    fn poll_native_drag(&mut self, context: &egui::Context, frame: &eframe::Frame) {
        #[cfg(target_os = "linux")]
        let _ = frame;
        let Some(preparation) = &self.native_drag_preparation else {
            return;
        };
        if !context.input(|input| input.pointer.primary_down()) {
            preparation.cancellation.store(true, Ordering::Relaxed);
        }
        match preparation.receiver.try_recv() {
            Ok(result) => {
                let preparation = self.native_drag_preparation.take().unwrap();
                let outcome = if preparation.cancellation.load(Ordering::Relaxed) {
                    Err("The mouse button was released before the selected items were ready to drag".into())
                } else {
                    result.and_then(|prepared| {
                        #[cfg(any(windows, target_os = "macos"))]
                        {
                            let icon = drag_preview_icon()
                                .map_err(|error| format!("Cannot prepare drag icon: {error}"))?;
                            drag::start_drag(
                                frame,
                                drag::DragItem::Files(prepared.paths),
                                drag::Image::Raw(icon),
                                |_, _| {},
                                drag::Options::default(),
                            )
                            .map_err(|error| {
                                format!("Cannot start dragging selected items: {error}")
                            })?;
                        }
                        #[cfg(target_os = "linux")]
                        self.x11_drag
                            .as_mut()
                            .ok_or_else(|| "X11 dragging is unavailable".to_string())?
                            .start(&prepared.paths, context)
                            .map_err(|error| {
                                format!("Cannot start dragging selected items: {error}")
                            })?;
                        self.drag_directories.push(prepared.directory);
                        Ok("Drag started".into())
                    })
                };
                finish_with_events(preparation.events, outcome);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                let preparation = self.native_drag_preparation.take().unwrap();
                finish_with_events(
                    preparation.events,
                    Err("The drag preparation worker stopped unexpectedly".into()),
                );
            }
            Err(mpsc::TryRecvError::Empty) => {
                context.request_repaint_after(Duration::from_millis(16))
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn prepare_wayland_drag(&mut self) {
        if self.preparing_drag.is_some()
            || self.wayland_drag.as_ref().is_none_or(|drag| drag.active())
        {
            return;
        }
        let Some(press) = self.wayland_drag.as_ref().and_then(|drag| drag.pressed()) else {
            self.report = Some("Cannot start dragging: mouse press is no longer active".into());
            return;
        };
        let overlay = self
            .overlay_cache
            .as_ref()
            .filter(|_| self.has_unsaved_changes);
        if let Some(cache) = overlay
            && let Err(error) = cache.verify_base(Path::new(&self.read_archive))
        {
            self.report = Some(error.to_string());
            return;
        }
        let overlay = overlay.map(OverlayCache::path);
        let archive = self.read_archive.clone();
        let selected = self.selected.clone();
        let commit = (!self.selected_commit.is_empty()).then(|| self.selected_commit.clone());
        let passwords = password_set(&self.archive_passwords, self.archive_password.as_deref());
        let background = self.matching_background(&archive, commit.as_deref());
        let checksum = self.validate_blocks_on_extraction;
        let (sender, receiver) = mpsc::channel();
        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();
        let cancellation = Arc::new(AtomicBool::new(false));
        let progress = UiProgress {
            events: events_tx.clone(),
            answers: Arc::new(Mutex::new(answer_rx)),
            cancellation: Some(Arc::clone(&cancellation)),
            limit_status_updates: false,
            last_status_update: Mutex::new(None),
        };
        self.begin_worker_with_cancellation(
            events_rx,
            answer_tx,
            "Extracting dragged items; keep the mouse button pressed",
            Some(Arc::clone(&cancellation)),
        );
        let thread = std::thread::spawn(move || {
            let prepared = background.and_then(|background| {
                background
                    .wait(Some(&progress))
                    .ok()
                    .filter(|_| background.unchanged())
            });
            let result = prepare_drag_with_overlay(
                &archive,
                overlay.as_deref(),
                &selected,
                commit.as_deref(),
                &passwords,
                Some(&progress),
                (prepared.as_deref(), checksum),
            )
            .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
        self.preparing_drag = Some(DragPreparation {
            press,
            receiver,
            events: events_tx,
            cancellation,
            thread,
        });
        self.status = "Preparing drag; keep the mouse button pressed".into();
    }

    #[cfg(target_os = "linux")]
    fn poll_wayland_drag(&mut self, context: &egui::Context) {
        if let Some(drag) = &mut self.wayland_drag {
            if let Err(error) = drag.poll() {
                self.report = Some(format!("Wayland drag failed: {error}"));
            }
            let dropped_paths = drag.take_dropped_paths();
            for (paths, position) in dropped_paths {
                let folder = self
                    .archive_folder_at(position.map(|position| position / context.zoom_factor()));
                if self.add_dropped_paths_to(paths, folder) {
                    Self::focus_add_dialog(context);
                }
            }
            // The guest Wayland queue does not itself wake eframe for incoming offers.
            context.request_repaint_after(Duration::from_millis(100));
        }
        if let Some(preparation) = &self.preparing_drag
            && self
                .wayland_drag
                .as_ref()
                .is_none_or(|drag| !drag.press_is_active(preparation.press))
            && !preparation.cancellation.swap(true, Ordering::Relaxed)
        {
            self.status = "Cancelling drag preparation".into();
            self.report = Some(
                "The mouse button was released before the selected items were ready to drag".into(),
            );
        }
        if let Some(preparation) = &self.preparing_drag {
            let press = preparation.press;
            match preparation.receiver.try_recv() {
                Ok(result) => {
                    let preparation = self.preparing_drag.take().unwrap();
                    let cancelled = preparation.cancellation.load(Ordering::Relaxed);
                    let outcome = if preparation.thread.join().is_err() {
                        Err("The drag preparation worker stopped unexpectedly".into())
                    } else if cancelled {
                        Ok("Drag preparation cancelled".into())
                    } else {
                        match result {
                            Ok(prepared) => {
                                let result = self
                                    .wayland_drag
                                    .as_mut()
                                    .ok_or_else(|| "Wayland drag is unavailable".to_string())
                                    .and_then(|drag| drag.start(&prepared.paths, press));
                                match result {
                                    Ok(()) => {
                                        self.drag_directories.push(prepared.directory);
                                        Ok("Drag started".into())
                                    }
                                    Err(error) => Err(format!(
                                        "Cannot start dragging selected items: {error}"
                                    )),
                                }
                            }
                            Err(error) => Err(format!(
                                "Cannot prepare selected items for dragging: {error}"
                            )),
                        }
                    };
                    finish_with_events(preparation.events, outcome);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    if let Some(preparation) = self.preparing_drag.take() {
                        let _ = preparation.thread.join();
                        finish_with_events(
                            preparation.events,
                            Err("The drag preparation worker stopped unexpectedly".into()),
                        );
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.preparing_drag.is_some()
            || self.wayland_drag.as_ref().is_some_and(|drag| drag.active())
        {
            context.request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    fn select_commit(&mut self, id: String) {
        if self.busy || id == self.selected_commit || !self.commit_ids.contains(&id) {
            return;
        }
        let archive = self.read_archive.clone();
        let overlay = self
            .overlay_cache
            .as_ref()
            .filter(|_| self.has_unsaved_changes)
            .map(OverlayCache::path);
        let can_prepare =
            overlay.is_none() && !self.has_unsaved_changes && self.archive_password.is_none();
        let background_passwords = self.archive_passwords.clone();
        let is_latest = self.commit_ids.last() == Some(&id);
        let passwords = password_set(&self.archive_passwords, self.archive_password.as_deref());
        let (events_tx, events_rx) = mpsc::channel();
        let (answer_tx, answer_rx) = mpsc::channel();
        drop(answer_rx);
        self.begin_worker(events_rx, answer_tx, "Reading commit");
        self.report = None;
        std::thread::spawn(move || {
            let result = if let Some(overlay) = overlay {
                operations::list_archive_with_overlay_through_and_passwords(
                    &archive,
                    &overlay.to_string_lossy(),
                    Some(&id),
                    &passwords,
                )
            } else {
                operations::list_archive_with_details_and_passwords(&archive, Some(&id), &passwords)
            };
            match result {
                Ok(toc) => {
                    let count = toc.0.len();
                    if can_prepare
                        && let Some(background) = BackgroundExtraction::start(
                            archive.clone(),
                            Some(id.clone()),
                            (!is_latest).then(|| id.clone()),
                            background_passwords,
                            toc.0.clone(),
                        )
                    {
                        let _ = events_tx.send(UiEvent::BackgroundExtraction(background));
                    }
                    let _ = events_tx.send(UiEvent::SelectedCommit(id));
                    let _ = events_tx.send(UiEvent::Loaded(Ok(toc)));
                    finish_with_events(events_tx, Ok(format!("{count} entries loaded")));
                }
                Err(error) => finish_with_events(events_tx, Err(error.to_string())),
            }
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
                UiEvent::OpenPasswords {
                    commits,
                    password,
                    remembered,
                    answers,
                } => {
                    self.open_password_dialog = Some(OpenPasswordDialog {
                        rows: commits
                            .into_iter()
                            .map(|commit| {
                                let encrypted = commit.header_encrypted || commit.data_encrypted;
                                OpenPasswordRow {
                                    id: commit.id,
                                    encrypted,
                                    password: if encrypted {
                                        remembered
                                            .get(&commit.id)
                                            .cloned()
                                            .or_else(|| password.clone())
                                            .unwrap_or_default()
                                    } else {
                                        String::new()
                                    },
                                }
                            })
                            .collect(),
                        answers,
                        error: None,
                        busy: false,
                    });
                }
                UiEvent::OpenPasswordError(error) => {
                    if let Some(dialog) = &mut self.open_password_dialog {
                        dialog.busy = false;
                        dialog.error = Some(error);
                    }
                }
                UiEvent::RememberPasswords(passwords) => {
                    self.remember_commit_passwords(&passwords);
                }
                UiEvent::ReplacePasswords(passwords) => {
                    self.archive_passwords = passwords;
                }
                UiEvent::OpeningArchive { archive, passwords } => {
                    self.find.clear_results();
                    self.archive_lock = None;
                    self.background_extraction = None;
                    self.open_password_dialog = None;
                    self.unlock_commits_dialog = None;
                    self.clear_archive_info();
                    self.add_archive_settings = None;
                    if self.operation.kind == Some(OperationKind::AddToArchive) {
                        self.operation = OperationForm::default();
                    }
                    self.commit_ids.clear();
                    self.encrypted_commit_ids.clear();
                    self.selected_commit.clear();
                    self.latest_archive_empty = None;
                    self.imported_archive_dir = None;
                    self.working_archive_dir = None;
                    self.overlay_cache = None;
                    self.rewrite_snapshots.clear();
                    self.has_unsaved_changes = false;
                    self.read_archive.clone_from(&archive);
                    self.pending_recent_archive = Some(archive.clone());
                    self.archive.clone_from(&archive);
                    self.location = archive;
                    self.selected.clear();
                    self.selection_anchor = None;
                    self.selection_current = None;
                    self.selection_range_base = None;
                    self.selection_press = None;
                    self.archive_password = None;
                    self.archive_passwords = passwords;
                    self.remember_commit_passwords(&BTreeMap::new());
                    self.opening_archive = true;
                }
                UiEvent::CommitIds { ids, encrypted } => {
                    self.commit_ids = ids;
                    self.encrypted_commit_ids = encrypted;
                }
                UiEvent::BackgroundExtraction(background) => {
                    self.background_extraction = Some(background);
                }
                UiEvent::ArchiveLock(lock) => self.archive_lock = lock,
                UiEvent::SelectedCommit(id) => {
                    self.selected_commit = if id.is_empty() {
                        self.commit_ids.last().cloned().unwrap_or_default()
                    } else {
                        id
                    };
                }
                UiEvent::ArchivePath { path, password } => {
                    if path != self.read_archive {
                        self.archive_lock = lock_archive(Path::new(&path));
                        self.background_extraction = None;
                        self.clear_archive_info();
                        self.archive_passwords.clear();
                        self.encrypted_commit_ids.clear();
                        self.archive.clone_from(&path);
                        self.read_archive = path.clone();
                        self.location = path;
                        self.working_archive_dir = None;
                        self.has_unsaved_changes = false;
                    }
                    self.archive_password = password;
                }
                UiEvent::ImportedArchive { directory, path } => {
                    self.clear_archive_info();
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
                    self.clear_archive_info();
                    if dirty {
                        self.background_extraction = None;
                        if self.operation.kind == Some(OperationKind::AddToArchive) {
                            self.operation = OperationForm::default();
                        }
                        self.add_archive_settings = new_archive_settings.map(|settings| *settings);
                    }
                    if dirty && logical_path.is_empty() {
                        self.archive_passwords.clear();
                        self.rewrite_snapshots.clear();
                        self.set_entries(Toc::default(), Default::default());
                        self.latest_archive_empty = None;
                        self.commit_ids.clear();
                        self.encrypted_commit_ids.clear();
                        self.selected_commit.clear();
                    }
                    self.archive.clone_from(&logical_path);
                    self.location = logical_path;
                    self.read_archive = path;
                    self.working_archive_dir = Some(directory);
                    self.imported_archive_dir = None;
                    self.has_unsaved_changes = dirty;
                }
                UiEvent::ArchiveModified => {
                    self.find.clear_results();
                    self.background_extraction = None;
                    self.clear_archive_info();
                    self.has_unsaved_changes = true;
                    self.latest_archive_empty = None;
                }
                UiEvent::ExpandDropFolder(folder) => {
                    if self.archive_view == ArchiveView::Structured {
                        self.expanded_tree_dirs.insert(folder);
                        self.structured_rows_dirty = true;
                    }
                }
                UiEvent::CacheEncryption(encryption) => {
                    if let Some(cache) = &mut self.overlay_cache {
                        cache.encryption = encryption;
                    }
                }
                UiEvent::RewriteSnapshot(snapshot) => self.rewrite_snapshots.push(snapshot),
                UiEvent::RewriteUndone {
                    steps,
                    dirty,
                    password,
                    passwords,
                } => {
                    self.rewrite_snapshots
                        .truncate(self.rewrite_snapshots.len() - steps);
                    self.has_unsaved_changes = dirty;
                    self.archive_password = password;
                    self.archive_passwords = passwords;
                    self.latest_archive_empty = None;
                }
                UiEvent::FirstCommitEmpty(empty) => {
                    if empty {
                        self.execute_archive_switch(ArchiveSwitchAction::New);
                    } else {
                        self.pending_archive_switch = Some(ArchiveSwitchAction::New);
                        self.pending_switch_after_save = false;
                    }
                }
                UiEvent::EntriesRemoved(paths) => {
                    self.remove_entries(&paths);
                    self.latest_archive_empty = Some(self.entries.is_empty());
                }
                UiEvent::ArchiveSaved { path } => {
                    self.background_extraction = None;
                    self.clear_archive_info();
                    self.archive.clone_from(&path);
                    self.read_archive.clone_from(&path);
                    self.location = path;
                    self.imported_archive_dir = None;
                    self.working_archive_dir = None;
                    self.overlay_cache = None;
                    self.rewrite_snapshots.clear();
                    self.has_unsaved_changes = false;
                    self.remember_commit_passwords(&BTreeMap::new());
                }
                UiEvent::Loaded(result) => match result {
                    Ok((toc, details)) => {
                        self.find.clear_results();
                        self.opening_archive = false;
                        if let Some(archive) = self.pending_recent_archive.take() {
                            remember_recent_archive(&mut self.recent_archives, &archive);
                        }
                        if self.commit_ids.last() == Some(&self.selected_commit) {
                            self.latest_archive_empty = Some(toc.is_empty());
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
                    self.open_password_dialog = None;
                    let cancellation_requested = self
                        .worker
                        .as_ref()
                        .and_then(|worker| worker.cancellation.as_ref())
                        .is_some_and(|token| token.load(Ordering::Relaxed));
                    self.busy = false;
                    self.worker = None;
                    let switch_after_save = self.pending_switch_after_save;
                    self.pending_switch_after_save = false;
                    match result {
                        Ok(summary) => {
                            self.status = if self.find.cancelled && summary == "Search complete." {
                                "Search cancelled.".to_owned()
                            } else {
                                summary.clone()
                            };
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
                            if error == "operation cancelled"
                                || (cancellation_requested && error.contains("operation cancelled"))
                            {
                                self.status = "Operation cancelled".to_owned();
                                self.report = None;
                            } else {
                                self.status = error.clone();
                                self.report = Some(error);
                            }
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
            self.open_password_dialog = None;
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
        self.selection_range_base = None;
        if self
            .selection_current
            .as_ref()
            .is_some_and(|path| !current_paths.contains(path))
        {
            self.selection_current = None;
        }
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
        self.entry_times = EntryTimes::from_details(&self.entry_details);
        self.entries.sort_by(|left, right| left.0.cmp(&right.0));
        self.entry_summary = EntrySummary::from_entries(&self.entries, &self.entry_details);
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
        self.entry_summary = EntrySummary::from_entries(&self.entries, &self.entry_details);
        self.selected.retain(|entry| !is_removed(entry));
        self.selection_range_base = None;
        if self
            .selection_current
            .as_ref()
            .is_some_and(|entry| is_removed(entry))
        {
            self.selection_current = None;
        }
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
        self.entry_times = EntryTimes::from_details(&self.entry_details);
        self.rebuild_visible_entry_indices();
        self.structured_rows_dirty = true;
    }

    fn rebuild_visible_entry_indices(&mut self) {
        self.selection_range_base = None;
        let matcher =
            EntryFilter::new(&self.filter_text, self.filter_regex, self.filter_match_case);
        self.filter_invalid = matcher.is_err();
        self.visible_entry_indices =
            filtered_entry_indices(&self.entries, &matcher.unwrap_or(EntryFilter::All));
        let (sort_by, reversed) = self.active_sort();
        sort_entry_indices(
            &mut self.visible_entry_indices,
            &self.entries,
            &self.entry_details,
            sort_by,
        );
        if reversed {
            self.visible_entry_indices.reverse();
        }
    }

    fn active_sort(&self) -> (SortBy, bool) {
        if self.entry_times.supports(self.sort_by) {
            (self.sort_by, self.sort_reversed)
        } else {
            (SortBy::Name, false)
        }
    }

    fn apply_sort_choice(&mut self, sort_by: SortBy) {
        if self.active_sort().0 == sort_by {
            self.sort_reversed = !self.active_sort().1;
            self.sort_by = sort_by;
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
        let last_selected_path = self.operation.input_paths.last().cloned();
        self.file_dialog
            .open_for_selection(last_selected_path.as_deref());
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
        if self.has_unsaved_changes && self.can_save_archive_as() {
            if self.archive.is_empty() {
                return self.save_archive_as();
            } else {
                return self.save_archive_to(self.archive.clone());
            }
        }
        false
    }

    fn save_archive_as(&mut self) -> bool {
        if !self.can_save_archive_as() {
            return false;
        }
        let mut dialog = rfd::FileDialog::new().add_filter("ZAR archive", &["zar"]);
        if let Some(file_name) = default_save_as_filename(
            &self.archive,
            self.entries.first().map(|(path, _)| path.as_str()),
        ) {
            dialog = dialog.set_file_name(file_name);
        }
        if let Some(path) = dialog.save_file() {
            return self.save_archive_to(path.to_string_lossy().into_owned());
        }
        false
    }

    fn save_archive_to(&mut self, path: String) -> bool {
        if !self.can_save_archive_as() {
            return false;
        }
        let destination = ensure_zar_extension(Path::new(&path));
        if destination != Path::new(&path)
            && destination.exists()
            && rfd::MessageDialog::new()
                .set_title("Replace archive?")
                .set_description(format!("Replace {}?", destination.display()))
                .set_buttons(rfd::MessageButtons::YesNo)
                .show()
                != rfd::MessageDialogResult::Yes
        {
            return false;
        }
        let path = destination.to_string_lossy().into_owned();
        let parent = Path::new(&path)
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        if let Ok(parent) = std::fs::canonicalize(parent) {
            let temporary_dirs = self
                .working_archive_dir
                .iter()
                .chain(self.imported_archive_dir.iter())
                .map(tempfile::TempDir::path)
                .chain(
                    self.overlay_cache
                        .iter()
                        .map(|cache| cache.directory.path()),
                )
                .chain(
                    self.rewrite_snapshots
                        .iter()
                        .map(|snapshot| snapshot.directory.path()),
                );
            if temporary_dirs
                .filter_map(|directory| std::fs::canonicalize(directory).ok())
                .any(|directory| parent.starts_with(directory))
            {
                self.status =
                    "Choose a destination outside the temporary archive storage.".to_owned();
                return false;
            }
        }
        if self.has_unsaved_changes
            && let Some(cache) = &self.overlay_cache
        {
            if let Err(error) = cache.verify_base(Path::new(&self.read_archive)) {
                self.status = error.to_string();
                return false;
            }
            let base = self.read_archive.clone();
            self.archive_lock = None;
            let overlay = cache.path();
            let encryption = cache.encryption.clone();
            let base_length = cache.base_length;
            let base_modified = cache.base_modified;
            let mut remembered = self.archive_passwords.clone();
            let passwords = password_set(&remembered, self.archive_password.as_deref());
            let (events_tx, events_rx) = mpsc::channel();
            let (answer_tx, answer_rx) = mpsc::channel();
            drop(answer_rx);
            self.begin_worker(events_rx, answer_tx, "Saving archive");
            self.report = None;
            self.warnings.clear();
            std::thread::spawn(move || {
                let mut saved = false;
                let result = (|| -> Result<_, Box<dyn std::error::Error>> {
                    let overlay_path = overlay.to_str().ok_or("Cache path is not valid UTF-8")?;
                    let staged = operations::info_archive(overlay_path, false)?;
                    let metadata = std::fs::metadata(&base)?;
                    if metadata.len() != base_length || metadata.modified()? != base_modified {
                        return Err("base archive changed since the undo cache was created".into());
                    }
                    let id = operations::save_overlay_archive(
                        Path::new(&base),
                        &overlay,
                        Path::new(&path),
                        EncodingOptions::default(),
                        &passwords,
                        encryption
                            .as_ref()
                            .map(|(password, aead, scope)| (password.as_bytes(), *aead, *scope)),
                    )?;
                    saved = true;
                    for commit in staged {
                        remembered.remove(&commit.id);
                    }
                    if let Some((password, _, _)) = encryption {
                        remembered.insert(id, password);
                    }
                    let _ = events_tx.send(UiEvent::ArchiveSaved { path: path.clone() });
                    let _ = events_tx.send(UiEvent::ReplacePasswords(remembered.clone()));
                    let set = password_set(&remembered, None);
                    let commits = operations::info_archive(&path, false)
                        .map_err(|error| format!("Archive saved, but refresh failed: {error}"))?;
                    let toc =
                        operations::list_archive_with_details_and_passwords(&path, None, &set)
                            .map_err(|error| {
                                format!("Archive saved, but refresh failed: {error}")
                            })?;
                    let ids = commits
                        .iter()
                        .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                        .collect();
                    let encrypted = commits
                        .iter()
                        .filter(|commit| commit.header_encrypted || commit.data_encrypted)
                        .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                        .collect();
                    let _ = events_tx.send(UiEvent::CommitIds { ids, encrypted });
                    let _ = events_tx.send(UiEvent::SelectedCommit(String::new()));
                    let _ = events_tx.send(UiEvent::Loaded(Ok(toc)));
                    Ok(format!("Saved archive {path}"))
                })()
                .map_err(|error| error.to_string());
                let lock_path = if saved { &path } else { &base };
                let _ = events_tx.send(UiEvent::ArchiveLock(lock_archive(Path::new(lock_path))));
                finish_with_events(events_tx, result);
            });
            return true;
        }
        let source = self.read_archive.clone();
        self.archive_lock = None;
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
            let lock_path = if result.is_ok() { &path } else { &source };
            let _ = events_tx.send(UiEvent::ArchiveLock(lock_archive(Path::new(lock_path))));
            finish_with_events(events_tx, result);
        });
        true
    }

    fn can_add_to_archive(&self) -> bool {
        !self.busy && !self.read_archive.is_empty() && self.imported_archive_dir.is_none()
    }

    fn can_extract_archive(&self) -> bool {
        !self.busy && !self.archive.is_empty()
    }

    fn can_show_archive_info(&self) -> bool {
        self.can_extract_archive() && self.imported_archive_dir.is_none()
    }

    fn can_join_commits(&self) -> bool {
        !self.busy
            && self.has_current_archive()
            && self.commit_ids.len() > 1
            && self.imported_archive_dir.is_none()
    }

    fn can_remove_selected_entries(&self) -> bool {
        !self.busy
            && !self.archive.is_empty()
            && !self.selected.is_empty()
            && self.commit_ids.last() == Some(&self.selected_commit)
            && self.imported_archive_dir.is_none()
    }

    fn remove_selected_entries(&mut self) {
        if !self.can_remove_selected_entries() {
            return;
        }
        let mut form = self.new_operation_form(OperationKind::RemoveEntries, &self.archive);
        form.remove_text = self.selected.iter().cloned().collect::<Vec<_>>().join("\n");
        self.start_operation(form);
    }

    fn maximum_undo_steps(&self) -> usize {
        self.rewrite_snapshots.len()
    }

    fn can_undo(&self) -> bool {
        !self.busy
            && self.has_current_archive()
            && self.has_unsaved_changes
            && self.maximum_undo_steps() > 0
            && self.imported_archive_dir.is_none()
    }

    fn can_save_archive_as(&self) -> bool {
        !self.busy && !self.read_archive.is_empty() && self.latest_archive_empty != Some(true)
    }

    fn has_current_archive(&self) -> bool {
        !self.read_archive.is_empty()
    }

    fn can_find(&self) -> bool {
        !self.busy && self.has_current_archive() && !self.entries.is_empty()
    }

    fn render_menus(&mut self, ui: &mut egui::Ui) {
        let is_imported_archive = self.imported_archive_dir.is_some();
        egui::MenuBar::new().ui(ui, |ui| {
            icon_menu_button(
                ui,
                icon_texture(ui.ctx(), &self.icons, UiIcon::OpenArchive),
                "File",
                |ui| {
                    if icon_shortcut_button(
                        ui,
                        &self.icons,
                        UiIcon::NewArchive,
                        "New archive",
                        "Alt+N",
                        !self.busy,
                    )
                    .clicked()
                    {
                        self.show_operation(OperationKind::NewArchive);
                        ui.close();
                    }
                    if icon_shortcut_button(
                        ui,
                        &self.icons,
                        UiIcon::OpenArchive,
                        "Open archive",
                        "Alt+O",
                        !self.busy,
                    )
                    .clicked()
                    {
                        self.open_dialog();
                        ui.close();
                    }
                    if icon_shortcut_button(
                        ui,
                        &self.icons,
                        UiIcon::Add,
                        "Add to archive",
                        "Alt+A",
                        self.can_add_to_archive(),
                    )
                    .clicked()
                    {
                        self.show_operation(OperationKind::AddToArchive);
                        ui.close();
                    }
                    if icon_shortcut_button(
                        ui,
                        &self.icons,
                        UiIcon::SaveArchive,
                        "Save archive",
                        "Ctrl+S",
                        self.has_unsaved_changes && self.can_save_archive_as(),
                    )
                    .clicked()
                    {
                        self.save_archive();
                        ui.close();
                    }
                    if icon_shortcut_button(
                        ui,
                        &self.icons,
                        UiIcon::SaveArchive,
                        "Save as...",
                        "Ctrl+Shift+S",
                        self.can_save_archive_as(),
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
                        self.request_quit(ui.ctx());
                    }
                },
            );
            icon_menu_button(
                ui,
                icon_texture(ui.ctx(), &self.icons, UiIcon::SelectAll),
                "Edit",
                |ui| {
                    if icon_button(ui, &self.icons, UiIcon::Undo, "Undo", self.can_undo()).clicked()
                    {
                        self.show_operation(OperationKind::UndoCommits);
                        ui.close();
                    }
                    if icon_button(ui, &self.icons, UiIcon::Find, "Find...", self.can_find())
                        .clicked()
                    {
                        self.find.open = true;
                        ui.close();
                    }
                    ui.separator();
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::SelectAll,
                        "Select all",
                        !self.visible_entry_indices.is_empty() && !self.busy,
                    )
                    .clicked()
                    {
                        let visible = self.archive_visible_paths();
                        self.select_all_visible_entries(visible);
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
                        let visible = self.archive_visible_paths();
                        self.invert_visible_selection(&visible);
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
                        self.selection_range_base = None;
                        ui.close();
                    }
                },
            );
            icon_menu_button(
                ui,
                icon_texture(ui.ctx(), &self.icons, UiIcon::View),
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
                        (SortBy::CompressedSize, "Compressed size"),
                        (SortBy::UncompressedSize, "Uncompressed size"),
                        (SortBy::Ratio, "Ratio"),
                        (SortBy::Type, "Type"),
                    ]
                    .into_iter()
                    .chain(
                        self.entry_times
                            .columns()
                            .map(|(_, label, sort)| (sort, label)),
                    ) {
                        if ui
                            .selectable_label(self.active_sort().0 == sort_by, label)
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
                    if icon_shortcut_button(
                        ui,
                        &self.icons,
                        UiIcon::Join,
                        "Join commits",
                        "Alt+J",
                        self.can_join_commits(),
                    )
                    .clicked()
                    {
                        self.show_operation(OperationKind::JoinCommits);
                        ui.close();
                    }
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::Remove,
                        "Remove commits",
                        !self.busy
                            && self.has_current_archive()
                            && self.commit_ids.len() > 1
                            && !is_imported_archive,
                    )
                    .clicked()
                    {
                        self.show_operation(OperationKind::RemoveCommits);
                        ui.close();
                    }
                    if icon_button(
                        ui,
                        &self.icons,
                        UiIcon::Unlock,
                        "Unlock commits",
                        !self.busy
                            && self.has_current_archive()
                            && dialogs::operation::locked_commit_count(
                                &self.encrypted_commit_ids,
                                &self.archive_passwords,
                            ) > 0
                            && !is_imported_archive,
                    )
                    .clicked()
                    {
                        self.show_unlock_commits();
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
                    if icon_shortcut_button(
                        ui,
                        &self.icons,
                        UiIcon::Configure,
                        "Settings...",
                        "Alt+S",
                        true,
                    )
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
        if kind == OperationKind::ExtractArchive {
            form.destination = Path::new(archive)
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."))
                .to_string_lossy()
                .into_owned();
        }
        form.block_size.clone_from(&self.default_block_size);
        form.compression_level = self.default_compression_level.to_string();
        if matches!(
            kind,
            OperationKind::NewArchive | OperationKind::AddToArchive
        ) {
            form.password.clone_from(&self.default_password);
            form.password_confirm.clone_from(&self.default_password);
            form.password_is_set = !form.password.is_empty();
        }
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
            form.creation_time = defaults.creation_time;
            form.access_time = defaults.access_time;
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

    fn add_dropped_paths(&mut self, paths: Vec<PathBuf>) -> bool {
        self.add_dropped_paths_to(paths, None)
    }

    fn add_dropped_paths_to(&mut self, paths: Vec<PathBuf>, folder: Option<ArchivePath>) -> bool {
        if paths.is_empty() {
            return false;
        }
        if self.busy {
            self.status =
                "Wait for the current operation to finish before dropping files.".to_owned();
            return false;
        }
        if self.read_archive.is_empty() {
            self.status = "Open an archive before dropping files or folders to add.".to_owned();
            return false;
        }
        if self.imported_archive_dir.is_some() {
            self.status = "Imported archives cannot be modified.".to_owned();
            return false;
        }
        if let Some(path) = paths.iter().find(|path| !path.exists()) {
            self.status = format!("Dropped path does not exist: {}", path.display());
            return false;
        }
        if self.operation.kind.is_some() && self.operation.kind != Some(OperationKind::AddToArchive)
        {
            self.status =
                "Close the current operation before dropping files or folders.".to_owned();
            return false;
        }

        if self.operation.kind != Some(OperationKind::AddToArchive) {
            self.show_operation(OperationKind::AddToArchive);
        }
        if self.operation.kind != Some(OperationKind::AddToArchive) {
            return false;
        }
        if !self.operation.input_paths.is_empty() && self.operation.input_archive_folder != folder {
            self.status =
                "Finish or clear the current Add operation before dropping into another folder."
                    .to_owned();
            return false;
        }
        self.operation.input_archive_folder = folder;
        for path in paths {
            if !self.operation.input_paths.contains(&path) {
                self.operation.input_paths.push(path);
            }
        }
        self.operation.selected_input_paths.clear();
        self.operation.input_selection_anchor = None;
        self.status =
            "Choose options and select Add to add the dropped files and folders.".to_owned();
        true
    }

    fn handle_dropped_files(&mut self, context: &egui::Context) {
        self.handle_dropped_files_at(context, context.input(|input| input.pointer.latest_pos()));
    }

    fn handle_dropped_files_at(&mut self, context: &egui::Context, position: Option<egui::Pos2>) {
        let paths: Vec<PathBuf> = context.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect()
        });
        if paths.iter().any(|path| {
            self.drag_directories
                .iter()
                .any(|directory| path.starts_with(directory.path()))
        }) {
            return;
        }
        let folder = self.archive_folder_at(position);
        let added = if folder.is_some() {
            self.add_dropped_paths_to(paths, folder)
        } else {
            self.add_dropped_paths(paths)
        };
        if added {
            Self::focus_add_dialog(context);
        }
    }

    fn archive_folder_at(&self, position: Option<egui::Pos2>) -> Option<ArchivePath> {
        let position = position?;
        if !self
            .archive_list_rect
            .is_some_and(|rect| rect.contains(position))
        {
            return None;
        }
        let (path, _) = self
            .archive_row_rects
            .iter()
            .find(|(_, rect)| rect.contains(position))?;
        let is_directory = if self.archive_view == ArchiveView::Structured {
            self.structured_rows_cache
                .iter()
                .any(|row| row.path == *path && matches!(row.data, EntryData::Directory))
        } else {
            self.entries
                .iter()
                .any(|(entry, data)| entry == path && matches!(data, EntryData::Directory))
        };
        let folder = if is_directory {
            path.as_str()
        } else {
            path.rsplit_once('/')?.0
        };
        ArchivePath::new(folder.to_owned()).ok()
    }

    fn handle_internal_archive_drag(
        &mut self,
        context: &egui::Context,
        panel: egui::Rect,
        window: egui::Rect,
    ) {
        if std::mem::take(&mut self.pending_drag) {
            self.internal_drag = true;
        }
        if !self.internal_drag {
            return;
        }
        let position = context.input(|input| input.pointer.interact_pos());
        self.drop_target = self
            .archive_folder_at(position)
            .map(|path| path.as_str().to_owned());
        if !context.input(|input| input.pointer.primary_released()) {
            if let Some(position) = position.filter(|position| window.contains(*position)) {
                let texture = icon_texture(context, &self.icons, UiIcon::Application);
                let rect = egui::Rect::from_min_size(
                    position + egui::vec2(12.0, 12.0),
                    egui::vec2(48.0, 48.0),
                );
                context
                    .layer_painter(egui::LayerId::new(
                        egui::Order::Tooltip,
                        egui::Id::unique("archive-drag-preview"),
                    ))
                    .image(
                        texture.id(),
                        rect,
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
            }
            return;
        }
        self.internal_drag = false;
        if !position.is_some_and(|position| panel.contains(position)) {
            return;
        }
        let target = self.drop_target.as_deref().unwrap_or("");
        let paths = self
            .selected
            .iter()
            .filter(|path| {
                !self.selected.iter().any(|parent| {
                    path.strip_prefix(parent)
                        .is_some_and(|suffix| suffix.starts_with('/'))
                })
            })
            .filter_map(|path| {
                let name = path.rsplit('/').next().unwrap_or(path);
                let destination = if target.is_empty() {
                    name.to_owned()
                } else {
                    format!("{target}/{name}")
                };
                (destination != *path).then(|| {
                    (
                        ArchivePath::new(path.clone()),
                        ArchivePath::new(destination),
                    )
                })
            })
            .map(|(from, to)| Ok((from?, to?)))
            .collect::<Result<Vec<_>, zarlib::ArchivePathError>>();
        match paths {
            Ok(paths) if !paths.is_empty() => {
                self.start_cached_move_to(paths, self.drop_target.clone());
            }
            Ok(_) => {}
            Err(error) => self.status = error.to_string(),
        }
    }

    fn focus_add_dialog(context: &egui::Context) {
        let viewport_id = egui::ViewportId::from_hash_of(format!(
            "operation-dialog:{}",
            OperationKind::AddToArchive.title()
        ));
        context.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Focus);
    }

    fn show_operation_now(&mut self, kind: OperationKind) {
        self.remember_add_archive_settings();
        let archive = if kind == OperationKind::NewArchive {
            String::new()
        } else if self.archive.is_empty() {
            self.read_archive.clone()
        } else {
            self.archive.clone()
        };
        let mut operation_commit_ids = None;
        let mut operation_encrypted_commit_ids = None;
        let encrypted_commits = if matches!(
            kind,
            OperationKind::AddToArchive | OperationKind::ExtractArchive
        ) {
            let path = if self.read_archive.is_empty() {
                &archive
            } else {
                &self.read_archive
            };
            match operations::info_archive(path, false) {
                Ok(mut commits) => {
                    if self.has_unsaved_changes
                        && let Some(cache) = &self.overlay_cache
                    {
                        let path = cache.path();
                        match operations::info_archive(&path.to_string_lossy(), false) {
                            Ok(overlay_commits) => commits.extend(overlay_commits),
                            Err(error) => {
                                self.status = format!("Cannot inspect undo cache commits: {error}");
                                return;
                            }
                        }
                    }
                    if kind == OperationKind::ExtractArchive {
                        operation_commit_ids = Some(
                            commits
                                .iter()
                                .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                                .collect(),
                        );
                        operation_encrypted_commit_ids = Some(
                            commits
                                .iter()
                                .filter(|commit| commit.header_encrypted || commit.data_encrypted)
                                .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
                                .collect(),
                        );
                    }
                    commits
                        .into_iter()
                        .filter(|commit| commit.header_encrypted || commit.data_encrypted)
                        .map(|commit| {
                            let password = self.archive_passwords.get(&commit.id).cloned();
                            CommitUnlock {
                                id: commit.id,
                                selected: password.is_some(),
                                password: password.unwrap_or_default(),
                            }
                        })
                        .collect()
                }
                Err(error) => {
                    self.status = format!("Cannot inspect archive commits: {error}");
                    return;
                }
            }
        } else {
            Vec::new()
        };
        self.operation = self.new_operation_form(kind, &archive);
        if kind == OperationKind::ExtractArchive {
            self.operation
                .extract_selected_paths
                .clone_from(&self.selected);
        }
        self.operation
            .encrypted_commit_ids
            .clone_from(&self.encrypted_commit_ids);
        if let Some(encrypted_commit_ids) = operation_encrypted_commit_ids {
            self.operation.encrypted_commit_ids = encrypted_commit_ids;
        }
        if archive == self.archive {
            self.operation
                .archive_passwords
                .clone_from(&self.archive_passwords);
        }
        if kind == OperationKind::ExtractArchive && self.archive.is_empty() {
            self.operation.destination = ".".to_owned();
        }
        self.operation.extract_has_encrypted_commits = !encrypted_commits.is_empty();
        self.operation.commit_unlocks = encrypted_commits;
        if kind == OperationKind::AddToArchive
            && let Some(settings) = &self.add_archive_settings
        {
            settings.apply_to(&mut self.operation);
            if !self.operation.password.is_empty() {
                self.operation.password_is_set = true;
            }
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
            if let Some(commit_ids) = operation_commit_ids {
                self.operation.commit_ids = commit_ids;
            } else {
                self.operation.commit_ids.clone_from(&self.commit_ids);
            }
            self.operation.to.clone_from(&self.selected_commit);
        }
        if kind == OperationKind::CustomMetadata {
            let metadata = if self.has_unsaved_changes
                && let Some(cache) = &self.overlay_cache
            {
                operations::read_custom_metadata_with_overlay(
                    &self.read_archive,
                    &cache.path().to_string_lossy(),
                )
            } else {
                operations::read_custom_metadata(&self.read_archive)
            };
            match metadata {
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
        if matches!(
            kind,
            OperationKind::JoinCommits | OperationKind::RemoveCommits
        ) {
            self.operation.commit_ids.clone_from(&self.commit_ids);
            self.operation.from_id = self.commit_ids.first().cloned().unwrap_or_default();
            self.operation.to_id = if kind == OperationKind::RemoveCommits {
                self.operation.from_id.clone()
            } else {
                self.commit_ids.last().cloned().unwrap_or_default()
            };
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
        let focus_filter = ui.input_mut(|input| {
            input.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::F,
                        pressed: true,
                        modifiers,
                        ..
                    } if modifiers.ctrl && !modifiers.alt && !modifiers.shift && !modifiers.mac_cmd
                )
            }) && input.consume_key(egui::Modifiers::CTRL, egui::Key::F)
        });
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
                    self.can_extract_archive()
                };
                let shortcut = if kind == OperationKind::AddToArchive {
                    "Alt+A"
                } else {
                    "Alt+E"
                };
                if icon_button(ui, &self.icons, icon, label, enabled)
                    .on_hover_text(shortcut)
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
                self.can_remove_selected_entries(),
            )
            .clicked()
            {
                self.remove_selected_entries();
            }
            if icon_button(
                ui,
                &self.icons,
                UiIcon::Info,
                "Info",
                self.can_show_archive_info(),
            )
            .on_hover_text("Alt+I")
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
                    .id(egui::Id::unique("archive-filter"))
                    .hint_text("Filter: *.txt or text")
                    .desired_width(ui.available_width().min(260.0))
                    .text_color_opt(text_color);
                let response = ui.add(edit).on_hover_text("Focus filter (Ctrl+F)");
                if focus_filter {
                    response.request_focus();
                }
                changed |= response.changed();
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

    fn focus_new_window(context: &egui::Context, viewport_id: egui::ViewportId, exists: bool) {
        if context.embed_viewports() {
            return;
        }

        #[derive(Clone, Copy)]
        struct PendingFocus {
            viewport_id: egui::ViewportId,
            next_attempt: f64,
            deadline: f64,
        }

        let key = egui::Id::unique("pending-window-focus");
        let now = context.input(|input| input.time);
        if !exists {
            context.data_mut(|data| {
                data.insert_temp(
                    key,
                    PendingFocus {
                        viewport_id,
                        next_attempt: now + 0.1,
                        deadline: now + 1.0,
                    },
                );
            });
            context.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Focus);
            context.request_repaint_after(Duration::from_millis(100));
            return;
        }

        let Some(mut pending) = context.data(|data| data.get_temp::<PendingFocus>(key)) else {
            return;
        };
        if pending.viewport_id != viewport_id {
            return;
        }
        let focused = context.input(|input| {
            input
                .raw
                .viewports
                .get(&viewport_id)
                .and_then(|viewport| viewport.focused)
                == Some(true)
        });
        if focused || now >= pending.deadline {
            context.data_mut(|data| data.remove::<PendingFocus>(key));
            return;
        }
        if now >= pending.next_attempt {
            context.send_viewport_cmd_to(viewport_id, egui::ViewportCommand::Focus);
            pending.next_attempt = now + 0.1;
            context.data_mut(|data| data.insert_temp(key, pending));
        }
        context.request_repaint_after(Duration::from_secs_f64(
            (pending.next_attempt - now).min(pending.deadline - now),
        ));
    }

    pub(super) fn centered_dialog_viewport_builder(
        context: &egui::Context,
        viewport_id: egui::ViewportId,
        title: &str,
        inner_size: [f32; 2],
        resizable: bool,
    ) -> egui::ViewportBuilder {
        let viewport_exists = context.input(|input| input.raw.viewports.contains_key(&viewport_id));
        let builder = egui::ViewportBuilder::default()
            .with_title(title)
            .with_resizable(resizable)
            .with_clamp_size_to_monitor_size(true);
        Self::focus_new_window(context, viewport_id, viewport_exists);
        if viewport_exists {
            return builder;
        }
        let parent_rect = context
            .input(|input| input.viewport().outer_rect)
            .unwrap_or_else(|| context.content_rect());
        let requested_inner_size = egui::vec2(inner_size[0], inner_size[1]);
        let (inner_size, max_inner_size, position) =
            if let Some(monitor_rect) = Self::dialog_monitor_rect(context, parent_rect) {
                let (inner_size, max_inner_size, position) = Self::clamp_dialog_placement(
                    parent_rect.center(),
                    monitor_rect,
                    requested_inner_size,
                );
                (inner_size, Some(max_inner_size), position)
            } else {
                (
                    requested_inner_size,
                    None,
                    parent_rect.center() - requested_inner_size * 0.5,
                )
            };
        let pixels_per_point = context.pixels_per_point();
        let inner_size = egui::vec2(
            Self::snap_to_device_pixel(inner_size.x, pixels_per_point),
            Self::snap_to_device_pixel(inner_size.y, pixels_per_point),
        );
        let max_inner_size = max_inner_size.map(|size| {
            egui::vec2(
                Self::snap_to_device_pixel(size.x, pixels_per_point),
                Self::snap_to_device_pixel(size.y, pixels_per_point),
            )
        });
        let position = egui::pos2(
            Self::snap_to_device_pixel(position.x, pixels_per_point),
            Self::snap_to_device_pixel(position.y, pixels_per_point),
        );

        let mut builder = builder
            .with_active(true)
            .with_inner_size(inner_size)
            .with_position(position);
        if let Some(max_inner_size) = max_inner_size {
            builder = builder.with_max_inner_size(max_inner_size);
        }
        builder
    }

    fn snap_to_device_pixel(value: f32, pixels_per_point: f32) -> f32 {
        if pixels_per_point.is_finite() && pixels_per_point > 0.0 {
            (value * pixels_per_point).round() / pixels_per_point
        } else {
            value
        }
    }

    #[cfg(not(all(target_os = "linux", target_env = "musl")))]
    fn dialog_monitor_rect(context: &egui::Context, parent_rect: egui::Rect) -> Option<egui::Rect> {
        let zoom_factor = context.zoom_factor();
        #[cfg(target_os = "windows")]
        let native_pixels_per_point = context
            .input(|input| input.viewport().native_pixels_per_point)
            .unwrap_or(1.0);
        #[cfg(target_os = "windows")]
        let (display_point, points_per_display_unit) = {
            let scale = (native_pixels_per_point * zoom_factor).max(f32::EPSILON);
            (parent_rect.center() * scale, 1.0 / scale)
        };
        #[cfg(not(target_os = "windows"))]
        let (display_point, points_per_display_unit) =
            (parent_rect.center() * zoom_factor, zoom_factor);

        let display = display_info::DisplayInfo::from_point(
            display_point.x.round() as i32,
            display_point.y.round() as i32,
        )
        .ok()?;
        Some(egui::Rect::from_min_size(
            egui::pos2(display.x as f32, display.y as f32) * points_per_display_unit,
            egui::vec2(display.width as f32, display.height as f32) * points_per_display_unit,
        ))
    }

    #[cfg(all(target_os = "linux", target_env = "musl"))]
    fn dialog_monitor_rect(
        _context: &egui::Context,
        _parent_rect: egui::Rect,
    ) -> Option<egui::Rect> {
        None
    }

    fn clamp_dialog_placement(
        parent_center: egui::Pos2,
        monitor_rect: egui::Rect,
        requested_inner_size: egui::Vec2,
    ) -> (egui::Vec2, egui::Vec2, egui::Pos2) {
        let margin = egui::vec2(8.0, 8.0);
        let window_chrome = egui::vec2(16.0, 48.0);
        let max_inner_size =
            (monitor_rect.size() - window_chrome - margin * 2.0).max(egui::Vec2::splat(1.0));
        let inner_size = requested_inner_size.min(max_inner_size);
        let outer_size = inner_size + window_chrome;
        let min_position = monitor_rect.min + margin;
        let max_position = egui::pos2(
            (monitor_rect.max.x - margin.x - outer_size.x).max(min_position.x),
            (monitor_rect.max.y - margin.y - outer_size.y).max(min_position.y),
        );
        let requested_position = parent_center - outer_size * 0.5;
        let position = egui::pos2(
            requested_position.x.clamp(min_position.x, max_position.x),
            requested_position.y.clamp(min_position.y, max_position.y),
        );

        (inner_size, max_inner_size, position)
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
        self.archive_list_columns = column_count;
        self.archive_list_page_rows = (ui.available_height()
            / (row_height + ui.spacing().item_spacing.y))
            .floor()
            .max(1.0) as usize;
        let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
        if let Some(offset) =
            self.archive_scroll_to_current(row_height, column_count, ui.spacing().item_spacing.y)
        {
            scroll = scroll.vertical_scroll_offset(offset);
        }
        let output = scroll.show_rows(ui, row_height, row_count, |ui, row_range| {
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
                            archive_entry_sense(true),
                        );
                        self.archive_row_rects
                            .push((path.clone(), rect.intersect(ui.clip_rect())));
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
                        paint_archive_current(
                            ui,
                            rect,
                            self.archive_list_focused
                                && self.selection_current.as_deref() == Some(path.as_str()),
                        );
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
                        let response = response.on_hover_text(match &data {
                            EntryData::Symlink(target) => format!("{path} → {target}"),
                            _ => path.clone(),
                        });
                        if response.is_pointer_button_down_on() || response.clicked() {
                            let visible_paths = self
                                .visible_entry_indices
                                .iter()
                                .map(|index| self.entries[*index].0.clone())
                                .collect::<Vec<_>>();
                            archive_row_selection(
                                &response,
                                &path,
                                &visible_paths,
                                &mut self.selected,
                                (&mut self.selection_anchor, &mut self.selection_current),
                                &mut self.selection_range_base,
                                &mut self.selection_press,
                            );
                        }
                        #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
                        if response.drag_started() && self.selected.contains(&path) && !self.busy {
                            self.pending_drag = true;
                        }
                        if (self.internal_drag || self.pending_drag)
                            && matches!(data, EntryData::Directory)
                            && ui.rect_contains_pointer(rect)
                        {
                            self.drop_target = Some(path.clone());
                        }
                        if response.double_clicked() {
                            self.pending_open = Some(path.clone());
                        }
                    }
                });
            }
        });
        self.archive_list_rect = Some(output.inner_rect);
        self.archive_scroll_offset = output.state.unclamped_offset().y;
        self.archive_viewport_height = output.inner_rect.height();
        self.archive_max_scroll = (output.content_size.y - output.inner_rect.height()).max(0.0);
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
        self.archive_list_columns = column_count;
        self.archive_list_page_rows = (ui.available_height()
            / (row_height + ui.spacing().item_spacing.y))
            .floor()
            .max(1.0) as usize;
        let text_font = egui::TextStyle::Body.resolve(ui.style());
        let list_id = ui.scope_id();

        let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
        if let Some(offset) =
            self.archive_scroll_to_current(row_height, column_count, ui.spacing().item_spacing.y)
        {
            scroll = scroll.vertical_scroll_offset(offset);
        }
        let output = scroll.show_rows(ui, row_height, row_count, |ui, row_range| {
            for row in row_range {
                let (_, row_rect) = ui.allocate_space(egui::vec2(ui.available_width(), row_height));
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
                    self.archive_row_rects
                        .push((path.clone(), cell_rect.intersect(ui.clip_rect())));
                    let response = ui.interact(
                        cell_rect,
                        list_id.with(("compact-entry", visible_index)),
                        archive_entry_sense(true),
                    );
                    let painter = ui.painter().with_clip_rect(cell_rect);
                    if self.selected.contains(path) {
                        painter.rect_filled(cell_rect, 2.0, ui.visuals().selection.bg_fill);
                    } else if response.hovered() {
                        painter.rect_filled(cell_rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
                    }
                    paint_archive_current(
                        ui,
                        cell_rect,
                        self.archive_list_focused
                            && self.selection_current.as_deref() == Some(path.as_str()),
                    );

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
                    if response.is_pointer_button_down_on() || response.clicked() {
                        let visible_paths = self
                            .visible_entry_indices
                            .iter()
                            .map(|index| self.entries[*index].0.clone())
                            .collect::<Vec<_>>();
                        archive_row_selection(
                            &response,
                            path,
                            &visible_paths,
                            &mut self.selected,
                            (&mut self.selection_anchor, &mut self.selection_current),
                            &mut self.selection_range_base,
                            &mut self.selection_press,
                        );
                    }
                    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
                    if response.drag_started() && self.selected.contains(path) && !self.busy {
                        self.pending_drag = true;
                    }
                    if (self.internal_drag || self.pending_drag)
                        && matches!(data, EntryData::Directory)
                        && response.contains_pointer()
                    {
                        self.drop_target = Some(path.clone());
                    }
                    if response.double_clicked() {
                        self.pending_open = Some(path.clone());
                    }
                }
            }
        });
        self.archive_list_rect = Some(output.inner_rect);
        self.archive_scroll_offset = output.state.unclamped_offset().y;
        self.archive_viewport_height = output.inner_rect.height();
        self.archive_max_scroll = (output.content_size.y - output.inner_rect.height()).max(0.0);
    }

    fn refresh_structured_rows(&mut self) {
        if self.structured_rows_dirty {
            self.structured_rows_cache = structured_rows(
                &self.entries,
                &self.visible_entry_indices,
                &self.expanded_tree_dirs,
                !self.filter_text.trim().is_empty(),
                &self.entry_details,
                self.active_sort().0,
                self.active_sort().1,
            );
            self.structured_rows_dirty = false;
        }
    }

    fn render_structured_archive_list(&mut self, ui: &mut egui::Ui) {
        self.refresh_structured_rows();
        let viewport_width = ui.available_width();
        let mut labels = vec!["Name", "Size"];
        let mut defaults = vec![390.0, 120.0];
        let mut sorts = vec![Some(SortBy::Name), Some(SortBy::UncompressedSize)];
        for (label, _, sort) in self.entry_times.columns() {
            labels.push(label);
            defaults.push(140.0);
            sorts.push(Some(sort));
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
        self.archive_list_columns = 1;
        self.archive_list_page_rows = (ui.available_height()
            / (row_height + ui.spacing().item_spacing.y))
            .floor()
            .max(1.0) as usize;
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
                    self.active_sort(),
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
        let created_width = width_of("Created");
        let accessed_width = width_of("Accessed");
        let owner_width = width_of("Owner");
        let permissions_width = width_of("Permissions");
        ui.separator();
        let drag_supported = true;
        let mut scroll = egui::ScrollArea::both()
            .id_salt("structured-rows-scroll")
            .auto_shrink([false, false]);
        if let Some(offset) =
            self.archive_scroll_to_current(row_height, 1, ui.spacing().item_spacing.y)
        {
            scroll = scroll.vertical_scroll_offset(offset);
        }
        let rows = &self.structured_rows_cache;
        let row_output = scroll.show_rows(ui, row_height, rows.len(), |ui, row_range| {
            for row_index in row_range {
                let row = rows[row_index].clone();
                let (_, row_rect) = ui.allocate_space(egui::vec2(content_width, row_height));
                self.archive_row_rects
                    .push((row.path.clone(), row_rect.intersect(ui.clip_rect())));
                let response = ui.interact(
                    row_rect,
                    list_id.with(("structured-entry", row_index)),
                    archive_entry_sense(drag_supported),
                );
                let painter = ui.painter().with_clip_rect(row_rect);
                if self.selected.contains(&row.path) {
                    painter.rect_filled(row_rect, 0.0, ui.visuals().selection.bg_fill);
                } else if response.hovered() {
                    painter.rect_filled(row_rect, 0.0, ui.visuals().widgets.hovered.bg_fill);
                }
                paint_archive_current(
                    ui,
                    row_rect,
                    self.archive_list_focused
                        && self.selection_current.as_deref() == Some(row.path.as_str()),
                );

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
                let mut column_left = row_rect.left() + name_width + size_width;
                for (width, text) in [
                    (
                        modified_width,
                        (modified_width > 0.0)
                            .then(|| format_modified(self.entry_details.get(&row.path))),
                    ),
                    (
                        created_width,
                        (created_width > 0.0)
                            .then(|| format_created(self.entry_details.get(&row.path))),
                    ),
                    (
                        accessed_width,
                        (accessed_width > 0.0)
                            .then(|| format_accessed(self.entry_details.get(&row.path))),
                    ),
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
                let on_disclosure = row.has_children
                    && response
                        .interact_pointer_pos()
                        .is_some_and(|position| disclosure_rect.contains(position));
                if !on_disclosure && (response.is_pointer_button_down_on() || response.clicked()) {
                    let visible_paths = rows
                        .iter()
                        .map(|visible_row| visible_row.path.clone())
                        .collect::<Vec<_>>();
                    archive_row_selection(
                        &response,
                        &row.path,
                        &visible_paths,
                        &mut self.selected,
                        (&mut self.selection_anchor, &mut self.selection_current),
                        &mut self.selection_range_base,
                        &mut self.selection_press,
                    );
                }
                #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
                if response.drag_started() && self.selected.contains(&row.path) && !self.busy {
                    self.pending_drag = true;
                }
                if (self.internal_drag || self.pending_drag)
                    && matches!(row.data, EntryData::Directory)
                    && response.contains_pointer()
                {
                    self.drop_target = Some(row.path.clone());
                }
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
                    }
                }
            }
        });
        self.structured_horizontal_offset = row_output.state.unclamped_offset().x;
        self.archive_list_rect = Some(row_output.inner_rect);
        self.archive_scroll_offset = row_output.state.unclamped_offset().y;
        self.archive_viewport_height = row_output.inner_rect.height();
        self.archive_max_scroll =
            (row_output.content_size.y - row_output.inner_rect.height()).max(0.0);
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
            Self::MTime => "mtime",
            Self::CTime => "ctime",
            Self::ATime => "atime",
            Self::CompressedSize => "compressed_size",
            Self::UncompressedSize => "uncompressed_size",
            Self::Ratio => "ratio",
            Self::Type => "type",
        }
    }

    fn from_setting(value: &str) -> Option<Self> {
        if value == "date" {
            return Some(Self::MTime);
        }
        [
            Self::Name,
            Self::MTime,
            Self::CTime,
            Self::ATime,
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
    fn apply_default_password(&mut self, password: String) {
        self.default_password.clone_from(&password);
        self.default_password_input = password;
        if matches!(
            self.operation.kind,
            Some(OperationKind::NewArchive | OperationKind::AddToArchive)
        ) && self.operation.password.is_empty()
        {
            self.operation.password.clone_from(&self.default_password);
            self.operation
                .password_confirm
                .clone_from(&self.default_password);
            self.operation.password_is_set = true;
        }
    }

    pub fn load_default_password(&mut self, context: &egui::Context) {
        let (sender, receiver) = mpsc::channel();
        self.default_password_loading = Some(receiver);
        let context = context.clone();
        std::thread::spawn(move || {
            let result =
                crate::password_store::load_default_password().map_err(|error| error.to_string());
            let _ = sender.send(result);
            context.request_repaint();
        });
    }

    fn poll_default_password(&mut self) {
        let Some(receiver) = &self.default_password_loading else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.default_password_loading = None;
                return;
            }
        };
        self.default_password_loading = None;
        if !self.default_password_input.is_empty() || self.default_password_status.is_some() {
            return;
        }
        match result {
            Ok(Some(password)) if !password.is_empty() => {
                self.apply_default_password(password);
            }
            Ok(Some(_)) | Ok(None) => {}
            Err(error) => {
                self.default_password_status = Some(format!(
                    "Could not load default password from the OS password safe: {error}"
                ));
            }
        }
    }

    pub fn apply_settings(&mut self, settings: &crate::settings::Settings) {
        self.geometry_monitor_count = crate::settings::monitor_count();
        self.window_geometry = settings.geometry_for_startup(self.geometry_monitor_count);
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
        self.file_dialog.set_bookmarks(&settings.bookmarks);
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
        self.icons.set_theme(settings.icon_theme);
        self.default_block_size
            .clone_from(&settings.default_block_size);
        self.default_compression_level = settings.default_compression_level;
        self.default_encryption_method = settings.default_encryption_method;
        self.validate_blocks_before_extraction = settings.validate_blocks_before_extraction;
        self.validate_blocks_on_extraction = settings.validate_blocks_on_extraction;
        self.performance = settings.performance.clamped();
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
        self.current_settings_with_monitor_count(crate::settings::monitor_count())
    }

    fn current_settings_with_monitor_count(
        &self,
        monitor_count: usize,
    ) -> crate::settings::Settings {
        let window = self.window_geometry.map(|mut geometry| {
            if monitor_count == 0 || monitor_count != self.geometry_monitor_count {
                geometry.position = None;
            }
            geometry
        });
        crate::settings::Settings {
            window,
            dialog_sizes: self
                .dialog_sizes
                .iter()
                .map(|(key, size)| (key.clone(), *size))
                .collect(),
            monitor_count,
            view: self.archive_view.as_setting().to_owned(),
            sort: self.sort_by.as_setting().to_owned(),
            sort_reversed: self.sort_reversed,
            ui_scale: self.ui_scale,
            theme_mode: self.theme_mode,
            icon_theme: self.icons.theme.get(),
            default_block_size: self.default_block_size.clone(),
            default_compression_level: self.default_compression_level,
            default_encryption_method: self.default_encryption_method,
            validate_blocks_before_extraction: self.validate_blocks_before_extraction,
            validate_blocks_on_extraction: self.validate_blocks_on_extraction,
            performance: self.performance,
            archive_defaults: self.archive_defaults.clone(),
            recent_archives: self.recent_archives.clone(),
            bookmarks: self.file_dialog.bookmarks().to_vec(),
            column_widths: self
                .column_widths
                .iter()
                .map(|(key, width)| (key.clone(), *width))
                .collect(),
        }
    }

    fn track_window_geometry(&mut self, context: &egui::Context) {
        let (size, outer, normal) = context.input(|input| {
            let viewport = input.viewport();
            (
                viewport
                    .inner_rect
                    .or(input.raw.screen_rect)
                    .map(|rect| rect.size()),
                viewport.outer_rect,
                !viewport.maximized.unwrap_or(false)
                    && !viewport.minimized.unwrap_or(false)
                    && !viewport.fullscreen.unwrap_or(false),
            )
        });
        if let (true, Some(size)) = (normal, size)
            && size.x.is_finite()
            && size.y.is_finite()
            && size.x >= 100.0
            && size.y >= 100.0
        {
            self.window_geometry = Some(crate::settings::WindowGeometry {
                size: [size.x, size.y],
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
            crate::theme::apply_theme(context, theme_mode);
        }
    }
}

impl eframe::App for ZargApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        #[cfg(target_os = "linux")]
        {
            if let Some(preparation) = self.preparing_drag.take() {
                preparation.cancellation.store(true, Ordering::Relaxed);
                if preparation.thread.join().is_err() {
                    eprintln!("zarg: drag preparation worker stopped unexpectedly");
                }
            }
            self.wayland_drag = None;
            self.x11_drag = None;
        }
        if let Err(error) = self.current_settings().save() {
            eprintln!("zarg: could not save settings: {error}");
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        Self::focus_new_window(
            &context,
            egui::ViewportId::ROOT,
            self.startup_focus_requested,
        );
        self.startup_focus_requested = true;
        let window_rect = ui.max_rect();
        self.drop_target = None;
        self.handle_global_shortcuts(&context);
        self.handle_archive_shortcuts(&context);
        #[cfg(target_os = "linux")]
        {
            if !self.linux_drag_initialized {
                self.linux_drag_initialized = true;
                match crate::wayland_drag::WaylandDrag::new(_frame, &context) {
                    Ok(drag) => self.wayland_drag = drag,
                    Err(error) => {
                        self.report = Some(format!("Cannot initialize Wayland dragging: {error}"));
                    }
                }
                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                match _frame.window_handle() {
                    Ok(handle) => match handle.as_raw() {
                        RawWindowHandle::Xlib(window) => match u32::try_from(window.window) {
                            Ok(window) => {
                                self.x11_drag = Some(crate::x11_drag::X11Drag::new(window));
                            }
                            Err(error) => {
                                self.report = Some(format!(
                                    "Cannot initialize X11 dragging: invalid window ID: {error}"
                                ));
                            }
                        },
                        RawWindowHandle::Xcb(window) => {
                            self.x11_drag =
                                Some(crate::x11_drag::X11Drag::new(window.window.get()));
                        }
                        _ => {}
                    },
                    Err(error) => {
                        self.report = Some(format!("Cannot initialize X11 dragging: {error}"));
                    }
                }
            }
            self.poll_wayland_drag(&context);
            if let Some(drag) = &mut self.x11_drag {
                if let Err(error) = drag.poll() {
                    self.report = Some(format!("X11 drag failed: {error}"));
                }
                if drag.active() {
                    context.request_repaint_after(Duration::from_millis(16));
                }
            }
        }
        let drag_supported = true;
        if context.input(|input| input.viewport().close_requested()) {
            self.request_quit(&context);
        }
        self.apply_control_size(&context);
        self.track_window_geometry(&context);
        self.poll_default_password();
        self.poll_recent_archive_check(&context);
        self.poll_worker(&context);
        if context.input(|input| input.raw.dropped_files.is_empty()) {
            self.handle_dropped_files(&context);
        } else {
            let position = native_drop_position(_frame, &context)
                .or_else(|| context.input(|input| input.pointer.latest_pos()));
            self.handle_dropped_files_at(&context, position);
        }
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
                    && self.latest_archive_empty != Some(true)
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
                            if !self.commit_ids.is_empty() {
                                let mut choice = self.selected_commit.clone();
                                ui.add_enabled_ui(!self.busy, |ui| {
                                    ui.horizontal(|ui| {
                                        let locked_count =
                                            dialogs::operation::locked_commit_count(
                                                &self.encrypted_commit_ids,
                                                &self.archive_passwords,
                                            );
                                        if locked_count > 0 {
                                            let response = icon_button(
                                                ui,
                                                &self.icons,
                                                UiIcon::Lock,
                                                locked_count.to_string(),
                                                true,
                                            )
                                            .on_hover_text(format!(
                                                "{locked_count} encrypted commit(s) locked"
                                            ));
                                            if response.clicked() {
                                                self.show_unlock_commits();
                                            }
                                        }
                                        egui::ComboBox::from_id_salt("main_commit_selector")
                                            .selected_text(&choice)
                                            .show_ui(ui, |ui| {
                                                for id in &self.commit_ids {
                                                    let response =
                                                        dialogs::operation::commit_selectable_option(
                                                            ui,
                                                            &self.icons,
                                                            id,
                                                            id,
                                                            choice == *id,
                                                            &self.encrypted_commit_ids,
                                                            &self.archive_passwords,
                                                        );
                                                    if response.clicked() {
                                                        choice.clone_from(id);
                                                    }
                                                }
                                            });
                                    });
                                });
                                if choice != self.selected_commit {
                                    self.select_commit(choice);
                                }
                            }
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
                let cancellation = self
                    .worker
                    .as_ref()
                    .and_then(|worker| worker.cancellation.as_ref())
                    .cloned();
                let show_progress = self.progress_known || cancellation.is_some();
                ui.horizontal(|ui| {
                    ui.spinner();
                    let spacing = ui.spacing().item_spacing.x;
                    let progress_width = if show_progress { 200.0 } else { 0.0 };
                    let cancel_width = if cancellation.is_some() { 88.0 } else { 0.0 };
                    let following_widgets =
                        1 + usize::from(show_progress) + usize::from(cancellation.is_some());
                    let message_width = (ui.available_width()
                        - progress_width
                        - cancel_width
                        - spacing * following_widgets as f32)
                        .max(0.0);
                    let message = if cancellation.is_some() {
                        let font = egui::TextStyle::Body.resolve(ui.style());
                        truncate_adding_message(&self.progress_message, message_width, |text| {
                            ui.painter()
                                .layout_no_wrap(
                                    text.to_owned(),
                                    font.clone(),
                                    ui.visuals().text_color(),
                                )
                                .size()
                                .x
                        })
                    } else {
                        self.progress_message.clone()
                    };
                    ui.add_sized(
                        [message_width, ui.spacing().interact_size.y],
                        egui::Label::new(message)
                            .truncate()
                            .halign(egui::Align::Min),
                    )
                    .on_hover_text(&self.progress_message);
                    if show_progress {
                        ui.add(egui::ProgressBar::new(self.progress).desired_width(progress_width));
                    }
                    if let Some(token) = cancellation {
                        let texture = icon_texture(ui.ctx(), &self.icons, UiIcon::Cancel);
                        let cancel = ui.add_enabled(
                            !token.load(Ordering::Relaxed),
                            egui::Button::image_and_text(
                                egui::Image::new(&texture)
                                    .fit_to_exact_size(egui::vec2(16.0, 16.0)),
                                "Cancel",
                            )
                            .min_size(egui::vec2(cancel_width, 0.0)),
                        );
                        if cancel.clicked() {
                            token.store(true, Ordering::Relaxed);
                        }
                    }
                });
            } else if self
                .unlock_commits_dialog
                .as_ref()
                .is_some_and(|dialog| dialog.busy)
            {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Unlocking commits...");
                });
            } else if self.info_loading.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(&self.status);
                });
            } else {
                if self.has_current_archive() {
                    let summary = self.entry_summary.display(self.entries.len(), self.commit_ids.len());
                    ui.horizontal(|ui| {
                        ui.label(summary).on_hover_text(
                            "Sizes are estimated for files in the selected commit view; commit count is for the whole archive.",
                        );
                        if !self.status.is_empty() {
                            ui.separator();
                            ui.label(&self.status);
                        }
                    });
                } else {
                    ui.label(&self.status);
                }
            }
        });
        // Fusion draws item views on the Base color rather than the window color.
        let list_frame = egui::Frame::central_panel(ui.style()).fill(ui.visuals().extreme_bg_color);
        self.archive_row_rects.clear();
        let archive_panel = egui::CentralPanel::default()
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
                let times = self.entry_times;
                let (has_owner, has_permissions) =
                    (self.has_entry_owner, self.has_entry_permissions);
                let column_specs = details_columns(times, has_owner, has_permissions);
                let labels = column_specs
                    .iter()
                    .map(|(label, _, _)| *label)
                    .collect::<Vec<_>>();
                let defaults = column_specs
                    .iter()
                    .map(|(_, width, _)| *width)
                    .collect::<Vec<_>>();
                let mut columns = self.column_widths_for("details", &labels, &defaults);
                let content_width = columns.iter().sum::<f32>() + 8.0 * columns.len() as f32;
                let spacing = ui.spacing().item_spacing.x;
                let header_columns = column_specs
                    .iter()
                    .map(|(label, _, sort)| (*label, *sort))
                    .collect::<Vec<_>>();
                let row_height = ui.spacing().interact_size.y;
                self.archive_list_columns = 1;
                self.archive_list_page_rows = (ui.available_height()
                    / (row_height + ui.spacing().item_spacing.y))
                    .floor()
                    .max(1.0) as usize;
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
                            self.active_sort(),
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
                let mut scroll = egui::ScrollArea::both()
                    .id_salt("details-rows-scroll")
                    .auto_shrink([false, false]);
                if let Some(offset) =
                    self.archive_scroll_to_current(row_height, 1, ui.spacing().item_spacing.y)
                {
                    scroll = scroll.vertical_scroll_offset(offset);
                }
                let row_output = scroll.show_rows(
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
                                egui::vec2(content_width.max(ui.available_width()), row_height),
                            );
                            self.archive_row_rects
                                .push((path.clone(), row_rect.intersect(ui.clip_rect())));
                            let row_response = ui
                                .interact(
                                    row_rect,
                                    ui.scope_id().with(("details-entry", row)),
                                    archive_entry_sense(drag_supported),
                                )
                                .on_hover_text(path);
                            if row_response.is_pointer_button_down_on() || row_response.clicked() {
                                let visible_paths = self
                                    .visible_entry_indices
                                    .iter()
                                    .map(|index| self.entries[*index].0.clone())
                                    .collect::<Vec<_>>();
                                archive_row_selection(
                                    &row_response,
                                    path,
                                    &visible_paths,
                                    &mut self.selected,
                                    (&mut self.selection_anchor, &mut self.selection_current),
                                    &mut self.selection_range_base,
                                    &mut self.selection_press,
                                );
                            }
                            #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
                            if row_response.drag_started()
                                && self.selected.contains(path)
                                && !self.busy
                            {
                                self.pending_drag = true;
                            }
                            if row_response.double_clicked() {
                                self.pending_open = Some(path.clone());
                            }
                            if selected {
                                ui.painter().rect_filled(
                                    row_rect,
                                    0.0,
                                    ui.visuals().selection.bg_fill,
                                );
                            }
                            paint_archive_current(
                                ui,
                                row_rect,
                                self.archive_list_focused
                                    && self.selection_current.as_deref() == Some(path.as_str()),
                            );
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
                                left_aligned_cell(
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
                                    .show_tooltip_when_elided(false),
                                )
                                .on_hover_text(hover_text);
                                if (self.internal_drag || self.pending_drag)
                                    && matches!(data, EntryData::Directory)
                                    && context.input(|input| {
                                        input
                                            .pointer
                                            .interact_pos()
                                            .is_some_and(|position| row_rect.contains(position))
                                    })
                                {
                                    self.drop_target = Some(path.clone());
                                }
                                let values = details_row_values(
                                    data,
                                    self.entry_details.get(path),
                                    times,
                                    has_owner,
                                    has_permissions,
                                );
                                for (value, width) in
                                    values.into_iter().zip(columns[1..].iter().copied())
                                {
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
                self.details_horizontal_offset = row_output.state.unclamped_offset().x;
                self.archive_list_rect = Some(row_output.inner_rect);
                self.archive_scroll_offset = row_output.state.unclamped_offset().y;
                self.archive_viewport_height = row_output.inner_rect.height();
                self.archive_max_scroll =
                    (row_output.content_size.y - row_output.inner_rect.height()).max(0.0);
            });

        self.handle_archive_list_pointer(&context, ui.visuals().selection.bg_fill);
        if let Some(path) = self.pending_open.take() {
            self.open_entry_externally(path);
        }

        #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
        self.poll_native_drag(&context, _frame);
        self.handle_internal_archive_drag(&context, archive_panel.response.rect, window_rect);
        let drag_outside = self.internal_drag
            && self.drag_supported()
            && context.input(|input| {
                input
                    .pointer
                    .latest_pos()
                    .is_none_or(|position| !window_rect.contains(position))
            });
        #[cfg(any(windows, target_os = "macos"))]
        if drag_outside {
            self.internal_drag = false;
            self.start_selected_drag();
            if self.native_drag_preparation.is_some() {
                context.request_repaint_after(Duration::from_millis(16));
            }
        }
        #[cfg(target_os = "linux")]
        if drag_outside {
            self.internal_drag = false;
            if let Some(drag) = &mut self.x11_drag {
                let paths = self.selected.iter().map(PathBuf::from).collect::<Vec<_>>();
                drag.show_preview(
                    &paths,
                    UiIcon::Application,
                    self.icons.theme.get(),
                    context.pixels_per_point(),
                );
                self.start_selected_drag();
            } else {
                self.prepare_wayland_drag();
            }
            context.request_repaint();
        }

        self.poll_archive_info(&context);
        if let Some((original, mut name)) = self.rename_dialog.take() {
            let mut accept = false;
            let mut cancel = false;
            egui::Window::new("Rename")
                .collapsible(false)
                .resizable(false)
                .show(&context, |ui| {
                    let response = ui.text_edit_singleline(&mut name);
                    if !response.has_focus() {
                        response.request_focus();
                    }
                    accept = ui.input(|input| input.key_pressed(egui::Key::Enter));
                    cancel = ui.input(|input| input.key_pressed(egui::Key::Escape));
                    ui.horizontal(|ui| {
                        accept |= ui.button("Rename").clicked();
                        cancel |= close_or_cancel(ui.button("Cancel"), true);
                    });
                });
            if accept {
                let destination = original
                    .rsplit_once('/')
                    .map_or_else(|| name.clone(), |(parent, _)| format!("{parent}/{name}"));
                match (ArchivePath::new(original), ArchivePath::new(destination)) {
                    (Ok(from), Ok(to)) if !name.contains('/') => {
                        self.start_cached_move(vec![(from, to)])
                    }
                    _ => self.status = "Enter a valid file name.".into(),
                }
            } else if !cancel {
                self.rename_dialog = Some((original, name));
            }
        }
        self.render_operation_dialog(&context);
        self.render_archive_info(&context);
        self.render_open_password_dialog(&context);
        self.render_unlock_commits_dialog(&context);
        self.poll_benchmark(&context);
        self.render_benchmark(&context);
        self.poll_find(&context);
        self.render_find(&context);
        self.render_confirmation_dialog(&context);
        self.render_unsaved_archive_dialog(&context);
        self.render_about_dialog(&context);
        self.render_operation_error_dialog(&context);
        self.render_file_picker_dialog(&context);
        self.render_settings_dialog(&context);
        #[cfg(windows)]
        crate::windows_move::install_viewports(_frame);
        self.update_window_title(&context);
        if self.exit_requested {
            self.exit_requested = false;
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn details_row_values(
    data: &EntryData,
    details: Option<&ArchiveEntryDetails>,
    times: EntryTimes,
    has_owner: bool,
    has_permissions: bool,
) -> Vec<String> {
    let (compressed, uncompressed, ratio, kind) = match data {
        EntryData::File(size) => {
            let (compressed, uncompressed, ratio) = file_size_columns(*size, details);
            (compressed, uncompressed, ratio, "File")
        }
        EntryData::Directory => ("—".to_owned(), "—".to_owned(), "—".to_owned(), "Folder"),
        EntryData::Symlink(_) => (
            "—".to_owned(),
            "—".to_owned(),
            "—".to_owned(),
            "Symbolic link",
        ),
        EntryData::NamedPipe => ("—".to_owned(), "—".to_owned(), "—".to_owned(), "Named pipe"),
        EntryData::Device(kind) => (
            "—".to_owned(),
            "—".to_owned(),
            "—".to_owned(),
            kind.as_str(),
        ),
    };
    let mut values = vec![compressed, uncompressed, ratio];
    if has_owner {
        values.push(format_owner(details));
    }
    if has_permissions {
        values.push(format_permissions(details));
    }
    values.push(kind.to_owned());
    if times.modified {
        values.push(format_modified(details));
    }
    if times.created {
        values.push(format_created(details));
    }
    if times.accessed {
        values.push(format_accessed(details));
    }
    values
}

fn file_size_columns(size: u64, details: Option<&ArchiveEntryDetails>) -> (String, String, String) {
    let compressed = details.and_then(|details| details.compressed_size);
    let ratio = details
        .and_then(|details| details.compression_ratio)
        .map(|ratio| format!("{:.1}%", ratio * 100.0))
        .unwrap_or_else(|| "—".to_owned());
    (
        compressed
            .map(display_size)
            .unwrap_or_else(|| "—".to_owned()),
        display_size(size),
        ratio,
    )
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
    events: Sender<UiEvent>,
    progress: UiProgress,
    background: Option<BackgroundExtraction>,
    checksum: ExtractionChecksum,
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
                save_modification_time: form.dates,
                save_creation_time: form.creation_time,
                save_access_time: form.access_time,
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
            let passwords = form.unlock_passwords()?;
            let paths = form.input_paths;
            let folder = form.input_archive_folder;
            let input = AddInputOptions {
                save_metadata: form.metadata,
                save_modification_time: form.dates,
                save_creation_time: form.creation_time,
                save_access_time: form.access_time,
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
            Box::new(move || {
                let encryption = new_password
                    .as_deref()
                    .map(|password| (password, aead, scope));
                let result = if let Some(folder) = &folder {
                    operations::add_archive_in_folder_with_encoding_progress_and_filters_and_encryption(
                        &archive, &paths, folder, options, Some(progress.as_ref()), input,
                        &filters, encryption, &passwords,
                    )
                } else {
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
                };
                result
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
        OperationKind::UndoCommits => return Err("No unsaved changes to undo.".into()),
        OperationKind::RemoveCommits => {
            let Some(ids) = removal_range(&form.commit_ids, &form.from_id, &form.to_id) else {
                return Err("Choose a valid commit range that leaves at least one commit.".into());
            };
            let ids = ids.to_vec();
            let output = form.remove_commits_output.trim().to_owned();
            let passwords = password_set(&form.archive_passwords, None);
            Box::new(move || {
                let result = operations::remove_archive_commits_with_passwords(
                    &archive,
                    &ids,
                    (!output.is_empty()).then_some(output.as_str()),
                    &passwords,
                )
                .map_err(|error| error.to_string())?;
                let warning = if !result.replay_validated {
                    let _ = events.send(UiEvent::Warning(
                        "Retained encrypted headers prevented validation of the remaining history."
                            .to_owned(),
                    ));
                    "Warning: retained encrypted headers prevented history validation. "
                } else {
                    ""
                };
                Ok(format!(
                    "{warning}Removed {} commit(s){}",
                    ids.len(),
                    if output.is_empty() {
                        String::new()
                    } else {
                        format!(" into {output}")
                    }
                ))
            })
        }
        OperationKind::JoinCommits => {
            let options = form.encoding()?;
            let from = form.from_id;
            let to = form.to_id;
            let output = form.join_output.trim().to_owned();
            Box::new(move || {
                let temporary = if output.is_empty() {
                    None
                } else {
                    if output == archive {
                        return Err("Write to must be a different archive.".to_owned());
                    }
                    let parent = Path::new(&output)
                        .parent()
                        .filter(|parent| !parent.as_os_str().is_empty())
                        .unwrap_or_else(|| Path::new("."));
                    let temporary = tempfile::NamedTempFile::new_in(parent)
                        .map_err(|error| format!("Cannot prepare output {output}: {error}"))?;
                    std::fs::copy(&archive, temporary.path())
                        .map_err(|error| format!("Cannot prepare output {output}: {error}"))?;
                    Some(temporary)
                };
                let target = temporary.as_ref().map_or_else(
                    || archive.clone(),
                    |file| file.path().to_string_lossy().into_owned(),
                );
                operations::join_archive_with_encoding_progress(
                    &target,
                    &from,
                    &to,
                    options,
                    Some(progress.as_ref()),
                )
                .map_err(|error| error.to_string())?;
                if let Some(temporary) = temporary {
                    temporary
                        .persist(&output)
                        .map_err(|error| format!("Cannot save output {output}: {}", error.error))?;
                }
                Ok(if output.is_empty() {
                    format!("Joined commits {from} to {to}")
                } else {
                    format!("Joined commits {from} to {to} into {output}")
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
            let passwords = if form.password_is_set {
                PasswordSet::new(password.clone())
            } else {
                password_set(&form.archive_passwords, None)
            };
            Box::new(move || {
                let report = operations::check_archive_with_recovery_and_passwords_progress(
                    &archive,
                    recover,
                    &passwords,
                    Some(progress.as_ref()),
                )
                .map_err(|error| error.to_string())?;
                Ok(check_summary(&report))
            })
        }
        OperationKind::ListArchive => {
            let to = (!form.to.trim().is_empty()).then(|| form.to.clone());
            let passwords = if form.password_is_set {
                PasswordSet::new(password.clone())
            } else {
                password_set(&form.archive_passwords, None)
            };
            Box::new(move || {
                operations::list_archive_with_passwords(&archive, to.as_deref(), &passwords)
                    .map(|toc| format!("Loaded {} entries from {archive}", toc.len()))
                    .map_err(|error| error.to_string())
            })
        }
        OperationKind::ExtractArchive => {
            let mut filters = form.filters()?;
            if form.extract_selected_only {
                if form.extract_selected_paths.is_empty() {
                    return Err("Select at least one archive entry to extract.".into());
                }
                let selected = form
                    .extract_selected_paths
                    .iter()
                    .map(|path| ArchivePath::new(path.clone()))
                    .collect::<Result<Vec<_>, _>>()?;
                filters.restrict_to_paths(selected);
            }
            let destination = PathBuf::from(form.destination);
            let to = (!form.to.trim().is_empty()).then(|| form.to.clone());
            let dry_run = form.dry_run;
            let collision = collision_policy(form.collision);
            let existing = existing_policy(form.existing);
            let restore_metadata = form.metadata;
            let restore_symlinks = form.restore_symlinks;
            let restore_devices = form.restore_devices;
            let strip_top_level = form.strip_top_level;
            let skip_unselected_frame_validation = form.skip_unselected_frame_validation;
            let import_archive = operations::is_supported_import_archive(Path::new(&archive));
            let passwords = if form.password_is_set {
                PasswordSet::new(password.clone())
            } else {
                password_set(&form.archive_passwords, None)
            };
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
                    operations::create_archive_with_password_progress(
                        &imported_path,
                        &[PathBuf::from(&archive)],
                        false,
                        operations::EncodingOptions::default(),
                        None,
                        Some(progress.as_ref()),
                        false,
                    )
                    .map_err(|error| error.to_string())?;
                    imported_path
                } else {
                    archive.clone()
                };
                let validate_blocks = checksum.recheck(Path::new(&archive_to_extract));
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
                let prepared = if let Some(background) = background {
                    progress.message("Waiting for background validation");
                    match background.wait(Some(progress.as_ref())) {
                        Ok(prepared) if background.unchanged() => Some(prepared),
                        Ok(_) => None,
                        Err(error) if progress.is_cancelled() => return Err(error),
                        Err(error) => {
                            progress
                                .warning(&format!("Background validation unavailable: {error}"));
                            None
                        }
                    }
                } else {
                    None
                };
                let result = if let Some(prepared) = prepared {
                    prepared.extract_with_block_validation(
                        &destination,
                        ExtractionOptions {
                            to: prepared.to(),
                            ..options
                        },
                        Some(progress.as_ref()),
                        &filters,
                        validate_blocks,
                    )
                } else {
                    operations::extract_archive_with_validation_options(
                        &archive_to_extract,
                        &destination,
                        options,
                        &passwords,
                        Some(progress.as_ref()),
                        &filters,
                        operations::ExtractionFrameValidation {
                            skip_unselected: skip_unselected_frame_validation,
                            validate_on_read: validate_blocks,
                        },
                    )
                };
                result
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

fn extract_and_open(
    archive: &str,
    path: &str,
    passwords: &PasswordSet,
    to: Option<&str>,
    prepared: Option<&operations::PreparedExtraction>,
    validate_blocks: bool,
) -> Result<String, String> {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let mut target = std::env::temp_dir().join(format!("zarg-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&target).map_err(|error| error.to_string())?;
    target.push(path.rsplit('/').next().unwrap_or(path));
    if let Some(prepared) = prepared {
        prepared
            .extract_single_file_with_block_validation(path, &target, validate_blocks)
            .map_err(|error| error.to_string())?;
    } else {
        operations::extract_single_file_through_with_block_validation(
            archive,
            path,
            &target,
            passwords,
            to,
            validate_blocks,
        )
        .map_err(|error| error.to_string())?;
    }
    open_with_default_program(&target)?;
    Ok(format!("Opened {path}"))
}

fn open_with_default_program(file: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("explorer.exe");
        command.arg(file);
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

fn format_created(details: Option<&ArchiveEntryDetails>) -> String {
    details
        .and_then(|details| details.created_unix_millis)
        .map(format_unix_millis)
        .unwrap_or_else(|| "—".to_owned())
}

fn format_accessed(details: Option<&ArchiveEntryDetails>) -> String {
    details
        .and_then(|details| details.accessed_unix_millis)
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
        SortBy::MTime => (
            details
                .get(path)
                .and_then(|details| details.modified_unix_millis)
                .map(|date| -i128::from(date)),
            String::new(),
        ),
        SortBy::CTime => (
            details
                .get(path)
                .and_then(|details| details.created_unix_millis)
                .map(|date| -i128::from(date)),
            String::new(),
        ),
        SortBy::ATime => (
            details
                .get(path)
                .and_then(|details| details.accessed_unix_millis)
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
            details
                .get(path)
                .and_then(|details| details.compression_ratio)
                .map(|ratio| (ratio * 1_000_000.0) as i128),
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

fn select_archive_entry(
    visible: &[String],
    selected: &mut BTreeSet<String>,
    anchor: &mut Option<String>,
    current: &mut Option<String>,
    range_base: &mut Option<BTreeSet<String>>,
    clicked: &str,
    modifiers: egui::Modifiers,
) {
    if modifiers.shift {
        let start = anchor
            .as_ref()
            .and_then(|path| visible.iter().position(|item| item == path))
            .or_else(|| {
                current
                    .as_ref()
                    .and_then(|path| visible.iter().position(|item| item == path))
            });
        if let (Some(start), Some(end)) = (start, visible.iter().position(|item| item == clicked)) {
            let base = range_base.get_or_insert_with(|| selected.clone());
            *selected = base.clone();
            selected.extend(visible[start.min(end)..=start.max(end)].iter().cloned());
            *anchor = Some(visible[start].clone());
        } else {
            selected.insert(clicked.to_owned());
            *anchor = Some(clicked.to_owned());
            *range_base = None;
        }
    } else {
        *range_base = None;
        if modifiers.command || modifiers.ctrl {
            if !selected.remove(clicked) {
                selected.insert(clicked.to_owned());
            }
        } else {
            selected.clear();
            selected.insert(clicked.to_owned());
        }
        *anchor = Some(clicked.to_owned());
    }
    *current = Some(clicked.to_owned());
}

fn archive_row_selection(
    response: &egui::Response,
    path: &str,
    visible: &[String],
    selected: &mut BTreeSet<String>,
    position: (&mut Option<String>, &mut Option<String>),
    range_base: &mut Option<BTreeSet<String>>,
    press: &mut Option<(String, egui::Modifiers, bool)>,
) {
    let (anchor, current) = position;
    if response.is_pointer_button_down_on()
        && response.ctx.input(|input| input.pointer.primary_pressed())
    {
        let modifiers = response.ctx.input(|input| input.modifiers);
        let deferred = selected.contains(path) && !modifiers.shift;
        if deferred {
            *current = Some(path.to_owned());
        } else {
            select_archive_entry(
                visible, selected, anchor, current, range_base, path, modifiers,
            );
        }
        *press = Some((path.to_owned(), modifiers, deferred));
    }
    if response.clicked()
        && let Some((pressed, modifiers, deferred)) = press.take()
        && pressed == path
        && deferred
    {
        select_archive_entry(
            visible, selected, anchor, current, range_base, path, modifiers,
        );
    }
}

fn paint_archive_current(ui: &egui::Ui, rect: egui::Rect, current: bool) {
    if current {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            0.0,
            ui.visuals().selection.stroke,
            egui::StrokeKind::Inside,
        );
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

fn removal_range<'a>(commits: &'a [String], from: &str, to: &str) -> Option<&'a [String]> {
    let first = commits.iter().position(|id| id == from)?;
    let last = commits.iter().position(|id| id == to)?;
    if first > last || last - first + 1 == commits.len() {
        return None;
    }
    Some(&commits[first..=last])
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

fn truncate_adding_message(message: &str, max_width: f32, measure: impl Fn(&str) -> f32) -> String {
    let Some(path) = message.strip_prefix("Adding ") else {
        return message.to_owned();
    };
    if measure(message) <= max_width {
        return message.to_owned();
    }

    let path_chars: Vec<char> = path.chars().collect();
    let mut low = 0;
    let mut high = path_chars.len();
    let mut best = "Adding ...".to_owned();
    while low <= high {
        let kept_chars = low + (high - low) / 2;
        let start_chars = kept_chars.div_ceil(2);
        let end_chars = kept_chars / 2;
        let start: String = path_chars.iter().take(start_chars).collect();
        let end: String = path_chars
            .iter()
            .skip(path_chars.len() - end_chars)
            .collect();
        let candidate = format!("Adding {start}...{end}");
        if measure(&candidate) <= max_width {
            best = candidate;
            low = kept_chars + 1;
        } else if kept_chars == 0 {
            break;
        } else {
            high = kept_chars - 1;
        }
    }
    best
}

fn ensure_zar_extension(path: &Path) -> PathBuf {
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zar"))
        || path.file_name().is_none()
    {
        return path.to_path_buf();
    }
    let mut name = path.as_os_str().to_os_string();
    name.push(".zar");
    PathBuf::from(name)
}

fn default_save_as_filename(current_archive: &str, first_entry: Option<&str>) -> Option<String> {
    Path::new(current_archive)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .or_else(|| {
            first_entry
                .and_then(|path| Path::new(path).file_stem())
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
        })
        .map(str::to_owned)
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
mod tests;
