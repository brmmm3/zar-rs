use eframe::egui;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::time::SystemTime;

use crate::ui::{UiIcon, file_icon, icon_texture};

/// A custom file/folder picker dialog built with egui.
///
/// Looks similar to native file dialogs on Windows/KDE:
/// - Navigation tree on the left (quick-access places)
/// - File list on the right with name, size, date, type columns
/// - Address bar, filter field, OK/Cancel buttons
/// - Multi-select with Ctrl/Shift
pub struct FileDialog {
    /// Whether the dialog is open.
    pub open: bool,
    /// The current directory being browsed.
    current_dir: PathBuf,
    /// Entries in the current directory.
    entries: Vec<DirEntry>,
    /// Indices of entries matching the filter.
    visible_indices: Vec<usize>,
    /// Selected visible indices.
    selected: BTreeSet<usize>,
    /// Selection anchor for Shift+click.
    selection_anchor: Option<usize>,
    /// Filter text.
    filter: String,
    /// The address bar text.
    address: String,
    /// Error message.
    error: Option<String>,
    /// Quick-access places.
    places: Vec<Place>,
    /// Sort column.
    sort_by: FileSortBy,
    /// Sort direction (true = reversed).
    sort_reversed: bool,
    /// Column widths.
    col_name_w: f32,
    col_size_w: f32,
    col_date_w: f32,
    col_type_w: f32,
}

#[derive(Clone)]
struct DirEntry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    size: u64,
    modified: Option<i64>,
}

#[derive(Clone)]
struct Place {
    label: String,
    path: PathBuf,
    is_drive: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FileSortBy {
    Name,
    Size,
    Date,
    Type,
}

impl FileDialog {
    pub fn new() -> Self {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let mut dlg = Self {
            open: false,
            current_dir: current_dir.clone(),
            entries: Vec::new(),
            visible_indices: Vec::new(),
            selected: BTreeSet::new(),
            selection_anchor: None,
            filter: String::new(),
            address: current_dir.to_string_lossy().into_owned(),
            error: None,
            places: Self::default_places(),
            sort_by: FileSortBy::Name,
            sort_reversed: false,
            col_name_w: 240.0,
            col_size_w: 100.0,
            col_date_w: 160.0,
            col_type_w: 120.0,
        };
        dlg.refresh_entries();
        dlg
    }

    fn default_places() -> Vec<Place> {
        let mut places = Vec::new();

        if let Some(home) = dirs::home_dir() {
            places.push(Place {
                label: "Home".into(),
                path: home,
                is_drive: false,
            });
        }
        if let Some(desktop) = dirs::desktop_dir() {
            places.push(Place {
                label: "Desktop".into(),
                path: desktop,
                is_drive: false,
            });
        }
        if let Some(docs) = dirs::document_dir() {
            places.push(Place {
                label: "Documents".into(),
                path: docs,
                is_drive: false,
            });
        }
        if let Some(downloads) = dirs::download_dir() {
            places.push(Place {
                label: "Downloads".into(),
                path: downloads,
                is_drive: false,
            });
        }

        #[cfg(windows)]
        {
            for letter in 'A'..='Z' {
                let root = format!("{letter}:\\");
                if std::path::Path::new(&root).exists() {
                    places.push(Place {
                        label: format!("{letter}:"),
                        path: PathBuf::from(&root),
                        is_drive: true,
                    });
                }
            }
        }

        #[cfg(not(windows))]
        {
            places.push(Place {
                label: "Root".into(),
                path: PathBuf::from("/"),
                is_drive: false,
            });
        }

        places
    }

    /// Open the dialog for selecting files and folders.
    pub fn open_for_selection(&mut self) {
        self.selected.clear();
        self.selection_anchor = None;
        self.filter.clear();
        self.error = None;
        self.open = true;
    }

    /// Returns the selected paths.
    pub fn selected_paths(&self) -> Vec<PathBuf> {
        self.selected
            .iter()
            .filter_map(|&vi| self.visible_indices.get(vi))
            .map(|&ei| self.entries[ei].path.clone())
            .collect()
    }

    fn refresh_entries(&mut self) {
        self.entries.clear();
        self.visible_indices.clear();
        self.selected.clear();
        self.selection_anchor = None;
        self.error = None;

        match std::fs::read_dir(&self.current_dir) {
            Ok(reader) => {
                let mut entries: Vec<DirEntry> = reader
                    .filter_map(|entry| {
                        let entry = entry.ok()?;
                        let name = entry.file_name().to_string_lossy().into_owned();
                        let path = entry.path();
                        let meta = std::fs::symlink_metadata(&path).ok()?;
                        let is_dir = meta.is_dir();
                        let size = if is_dir { 0 } else { meta.len() };
                        let modified = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                            .map(|d| d.as_millis() as i64);
                        Some(DirEntry {
                            name,
                            path,
                            is_dir,
                            size,
                            modified,
                        })
                    })
                    .collect();

                entries.sort_by(|a, b| {
                    a.is_dir
                        .cmp(&b.is_dir)
                        .reverse()
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });

                self.entries = entries;
            }
            Err(e) => {
                self.error = Some(format!("Cannot open directory: {e}"));
            }
        }

        self.apply_filter_and_sort();
    }

    fn apply_filter_and_sort(&mut self) {
        let filter = if self.filter.is_empty() {
            None
        } else {
            Some(self.filter.to_lowercase())
        };

        let mut indices: Vec<usize> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                filter
                    .as_ref()
                    .is_none_or(|f| e.name.to_lowercase().contains(f))
            })
            .map(|(i, _)| i)
            .collect();

        let reversed = self.sort_reversed;
        indices.sort_by(|&a, &b| {
            let ea = &self.entries[a];
            let eb = &self.entries[b];
            let cmp = match self.sort_by {
                FileSortBy::Name => ea
                    .is_dir
                    .cmp(&eb.is_dir)
                    .reverse()
                    .then_with(|| ea.name.to_lowercase().cmp(&eb.name.to_lowercase())),
                FileSortBy::Size => ea.is_dir.cmp(&eb.is_dir).reverse().then_with(|| {
                    ea.size
                        .cmp(&eb.size)
                        .then_with(|| ea.name.to_lowercase().cmp(&eb.name.to_lowercase()))
                }),
                FileSortBy::Date => ea.is_dir.cmp(&eb.is_dir).reverse().then_with(|| {
                    ea.modified
                        .cmp(&eb.modified)
                        .reverse()
                        .then_with(|| ea.name.to_lowercase().cmp(&eb.name.to_lowercase()))
                }),
                FileSortBy::Type => ea.is_dir.cmp(&eb.is_dir).reverse().then_with(|| {
                    file_extension(&ea.name)
                        .cmp(&file_extension(&eb.name))
                        .then_with(|| ea.name.to_lowercase().cmp(&eb.name.to_lowercase()))
                }),
            };
            if reversed { cmp.reverse() } else { cmp }
        });

        self.visible_indices = indices;
    }

    /// Render the dialog. Returns `true` if the user confirmed (OK).
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        icon_cache: &RefCell<HashMap<UiIcon, egui::TextureHandle>>,
    ) -> bool {
        let mut confirmed = false;

        // --- Top bar: navigate up + address + go + refresh ---
        ui.horizontal(|ui| {
            if ui
                .button("Up")
                .on_hover_text("Go to parent folder")
                .clicked()
                && let Some(parent) = self.current_dir.parent()
            {
                self.current_dir = parent.to_path_buf();
                self.address = self.current_dir.to_string_lossy().into_owned();
                self.refresh_entries();
            }
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.address)
                    .desired_width(ui.available_width() - 120.0)
                    .hint_text("Path"),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let path = PathBuf::from(self.address.trim());
                if path.exists() && path.is_dir() {
                    self.current_dir = path;
                    self.refresh_entries();
                } else {
                    self.error = Some("Invalid directory path".into());
                }
            }
            if ui.button("Go").clicked() {
                let path = PathBuf::from(self.address.trim());
                if path.exists() && path.is_dir() {
                    self.current_dir = path;
                    self.refresh_entries();
                } else {
                    self.error = Some("Invalid directory path".into());
                }
            }
            if ui.button("Refresh").clicked() {
                self.refresh_entries();
            }
        });

        // Error message
        if let Some(ref error) = self.error.clone() {
            ui.colored_label(egui::Color32::RED, error);
        }

        // --- Main area: places sidebar + file list ---
        let main_height = (ui.available_height() - 40.0).max(100.0);
        let main_size = egui::vec2(ui.available_width(), main_height);
        ui.allocate_ui_with_layout(
            main_size,
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                // Left sidebar: places
                ui.vertical(|ui| {
                    ui.label("Places");
                    let places = self.places.clone();
                    let current_dir = self.current_dir.clone();
                    egui::ScrollArea::vertical()
                        .id_salt("places")
                        .max_height(400.0)
                        .show(ui, |ui| {
                            for place in &places {
                                let selected = current_dir == place.path;
                                let clicked = ui
                                    .horizontal(|ui| {
                                        let icon_kind = if place.is_drive {
                                            UiIcon::Drive
                                        } else {
                                            UiIcon::Folder
                                        };
                                        let icon = icon_texture(ui.ctx(), icon_cache, icon_kind);
                                        ui.add(
                                            egui::Image::new(&icon)
                                                .fit_to_exact_size(egui::vec2(16.0, 16.0)),
                                        );
                                        ui.selectable_label(selected, &place.label).clicked()
                                    })
                                    .inner;
                                if clicked {
                                    self.current_dir = place.path.clone();
                                    self.address = self.current_dir.to_string_lossy().into_owned();
                                    self.refresh_entries();
                                }
                            }
                        });
                });

                ui.separator();

                // Right side: file list
                let list_area_size = ui.available_size();
                ui.allocate_ui_with_layout(
                    list_area_size,
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        // Column headers
                        let total_w =
                            self.col_name_w + self.col_size_w + self.col_date_w + self.col_type_w;

                        ui.horizontal(|ui| {
                            ui.set_min_width(total_w);
                            let sort = &self.sort_by;
                            let rev = self.sort_reversed;

                            let mut clicked = None;
                            header_cell(
                                ui,
                                "Name",
                                self.col_name_w,
                                *sort == FileSortBy::Name,
                                rev,
                                &mut clicked,
                            );
                            header_cell(
                                ui,
                                "Size",
                                self.col_size_w,
                                *sort == FileSortBy::Size,
                                rev,
                                &mut clicked,
                            );
                            header_cell(
                                ui,
                                "Date modified",
                                self.col_date_w,
                                *sort == FileSortBy::Date,
                                rev,
                                &mut clicked,
                            );
                            header_cell(
                                ui,
                                "Type",
                                self.col_type_w,
                                *sort == FileSortBy::Type,
                                rev,
                                &mut clicked,
                            );

                            if let Some(col) = clicked {
                                if col == self.sort_by {
                                    self.sort_reversed = !self.sort_reversed;
                                } else {
                                    self.sort_by = col;
                                    self.sort_reversed = false;
                                }
                                self.apply_filter_and_sort();
                            }
                        });

                        // File list
                        let row_height = 20.0;
                        let list_height = ui.available_height().max(100.0);
                        let num_rows = self.visible_indices.len();

                        let entries = self.entries.clone();
                        egui::ScrollArea::vertical()
                            .id_salt("file-list")
                            .max_height(list_height)
                            .min_scrolled_height(list_height)
                            .show_rows(ui, row_height, num_rows, |ui, row_range| {
                                for row in row_range {
                                    if row >= self.visible_indices.len() {
                                        break;
                                    }
                                    let ei = self.visible_indices[row];
                                    let entry = &entries[ei];
                                    let selected = self.selected.contains(&row);

                                    let (rect, response) = ui.allocate_exact_size(
                                        egui::vec2(total_w, row_height),
                                        egui::Sense::click(),
                                    );

                                    if selected {
                                        ui.painter().rect_filled(
                                            rect,
                                            2.0,
                                            ui.visuals().selection.bg_fill,
                                        );
                                    } else if response.hovered() {
                                        ui.painter().rect_filled(
                                            rect,
                                            2.0,
                                            ui.visuals().widgets.hovered.bg_fill,
                                        );
                                    }

                                    if response.double_clicked() && entry.is_dir {
                                        self.current_dir = entry.path.clone();
                                        self.address =
                                            self.current_dir.to_string_lossy().into_owned();
                                        self.refresh_entries();
                                    }

                                    if response.clicked() {
                                        let modifiers = ui.input(|i| i.modifiers);
                                        let additive = modifiers.command || modifiers.ctrl;
                                        let shift = modifiers.shift;

                                        if shift {
                                            let anchor = self
                                                .selection_anchor
                                                .filter(|&a| a < num_rows)
                                                .unwrap_or(row);
                                            if !additive {
                                                self.selected.clear();
                                            }
                                            let start = anchor.min(row);
                                            let end = anchor.max(row);
                                            for r in start..=end {
                                                self.selected.insert(r);
                                            }
                                            self.selection_anchor = Some(anchor);
                                        } else if additive {
                                            if !self.selected.remove(&row) {
                                                self.selected.insert(row);
                                            }
                                            self.selection_anchor = Some(row);
                                        } else {
                                            self.selected.clear();
                                            self.selected.insert(row);
                                            self.selection_anchor = Some(row);
                                        }
                                    }

                                    // Draw columns
                                    let name_text = entry.name.clone();
                                    let icon_kind = if entry.is_dir {
                                        UiIcon::Folder
                                    } else {
                                        file_icon(&entry.name)
                                    };
                                    let icon = icon_texture(ui.ctx(), icon_cache, icon_kind);
                                    let size_text = if entry.is_dir {
                                        String::new()
                                    } else {
                                        human_size(entry.size)
                                    };
                                    let date_text = entry
                                        .modified
                                        .map(crate::ui::format_unix_millis)
                                        .unwrap_or_default();
                                    let type_text = if entry.is_dir {
                                        "Folder".into()
                                    } else {
                                        file_type_label(&entry.name)
                                    };

                                    let text_color = ui.visuals().text_color();
                                    let font_id = egui::TextStyle::Body.resolve(ui.style());

                                    let mut cx = rect.left() + 4.0;

                                    // Name
                                    let icon_rect = egui::Rect::from_min_size(
                                        egui::pos2(cx, rect.center().y - 8.0),
                                        egui::vec2(16.0, 16.0),
                                    );
                                    ui.painter().image(
                                        icon.id(),
                                        icon_rect,
                                        egui::Rect::from_min_max(
                                            egui::pos2(0.0, 0.0),
                                            egui::pos2(1.0, 1.0),
                                        ),
                                        egui::Color32::WHITE,
                                    );
                                    cx += 20.0;
                                    let name_galley = ui.painter().layout_no_wrap(
                                        name_text,
                                        font_id.clone(),
                                        text_color,
                                    );
                                    let clip_rect = egui::Rect::from_min_size(
                                        egui::pos2(cx, rect.top()),
                                        egui::vec2(self.col_name_w - 28.0, row_height),
                                    );
                                    ui.painter().with_clip_rect(clip_rect).galley(
                                        egui::pos2(
                                            cx,
                                            rect.center().y - name_galley.size().y / 2.0,
                                        ),
                                        name_galley,
                                        text_color,
                                    );
                                    cx += self.col_name_w;

                                    // Size (right-aligned)
                                    let size_galley = ui.painter().layout_no_wrap(
                                        size_text,
                                        font_id.clone(),
                                        text_color,
                                    );
                                    let clip_rect = egui::Rect::from_min_size(
                                        egui::pos2(cx, rect.top()),
                                        egui::vec2(self.col_size_w - 8.0, row_height),
                                    );
                                    ui.painter().with_clip_rect(clip_rect).galley(
                                        egui::pos2(
                                            cx + self.col_size_w - size_galley.size().x - 8.0,
                                            rect.center().y - size_galley.size().y / 2.0,
                                        ),
                                        size_galley,
                                        text_color,
                                    );
                                    cx += self.col_size_w;

                                    // Date
                                    let date_galley = ui.painter().layout_no_wrap(
                                        date_text,
                                        font_id.clone(),
                                        text_color,
                                    );
                                    let clip_rect = egui::Rect::from_min_size(
                                        egui::pos2(cx, rect.top()),
                                        egui::vec2(self.col_date_w - 8.0, row_height),
                                    );
                                    ui.painter().with_clip_rect(clip_rect).galley(
                                        egui::pos2(
                                            cx,
                                            rect.center().y - date_galley.size().y / 2.0,
                                        ),
                                        date_galley,
                                        text_color,
                                    );
                                    cx += self.col_date_w;

                                    // Type
                                    let type_galley =
                                        ui.painter().layout_no_wrap(type_text, font_id, text_color);
                                    let clip_rect = egui::Rect::from_min_size(
                                        egui::pos2(cx, rect.top()),
                                        egui::vec2(self.col_type_w - 8.0, row_height),
                                    );
                                    ui.painter().with_clip_rect(clip_rect).galley(
                                        egui::pos2(
                                            cx,
                                            rect.center().y - type_galley.size().y / 2.0,
                                        ),
                                        type_galley,
                                        text_color,
                                    );
                                }
                            });
                    },
                );
            },
        );

        // --- Bottom: filter + buttons ---
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Filter:");
            let button_font = egui::TextStyle::Button.resolve(ui.style());
            let button_padding = ui.spacing().button_padding.x;
            let minimum_button_width = ui.spacing().interact_size.x;
            let action_width = ["Clear", "Apply", "Cancel"]
                .into_iter()
                .map(|label| {
                    let galley = ui.painter().layout_no_wrap(
                        label.to_owned(),
                        button_font.clone(),
                        ui.visuals().text_color(),
                    );
                    (galley.size().x + button_padding * 2.0).max(minimum_button_width)
                })
                .sum::<f32>();
            let spacing = ui.spacing().item_spacing.x;
            let filter_width = (ui.available_width() - action_width - spacing * 4.0).max(40.0);
            let filter_resp = ui.add_sized(
                egui::vec2(filter_width, ui.spacing().interact_size.y),
                egui::TextEdit::singleline(&mut self.filter).hint_text("Filter files..."),
            );
            if filter_resp.changed() {
                self.apply_filter_and_sort();
            }
            if ui.button("Clear").clicked() {
                self.filter.clear();
                self.apply_filter_and_sort();
            }
            if ui.button("Apply").clicked() {
                confirmed = true;
                self.open = false;
            }
            if ui.button("Cancel").clicked() {
                self.open = false;
            }
        });

        confirmed
    }
}

fn header_cell(
    ui: &mut egui::Ui,
    label: &str,
    width: f32,
    active: bool,
    reversed: bool,
    clicked: &mut Option<FileSortBy>,
) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width, ui.spacing().interact_size.y),
        egui::Sense::click(),
    );
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
    }
    if response.clicked() {
        *clicked = Some(match label {
            "Size" => FileSortBy::Size,
            "Date modified" => FileSortBy::Date,
            "Type" => FileSortBy::Type,
            _ => FileSortBy::Name,
        });
    }
    let font_id = egui::TextStyle::Body.resolve(ui.style());
    let color = ui.visuals().strong_text_color();
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font_id, color);
    let text_pos = egui::pos2(rect.left() + 4.0, rect.center().y - galley.size().y / 2.0);
    let indicator_x = text_pos.x + galley.size().x + 7.0;
    ui.painter().galley(text_pos, galley, color);
    if active {
        let center_y = rect.center().y;
        let points = if reversed {
            vec![
                egui::pos2(indicator_x, center_y + 2.0),
                egui::pos2(indicator_x + 8.0, center_y + 2.0),
                egui::pos2(indicator_x + 4.0, center_y - 3.0),
            ]
        } else {
            vec![
                egui::pos2(indicator_x, center_y - 2.0),
                egui::pos2(indicator_x + 8.0, center_y - 2.0),
                egui::pos2(indicator_x + 4.0, center_y + 3.0),
            ]
        };
        ui.painter().add(egui::Shape::convex_polygon(
            points,
            color,
            egui::Stroke::NONE,
        ));
    }
}

fn human_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{:.1} {}", size, UNITS[unit])
    }
}

fn file_extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_lowercase())
        .unwrap_or_default()
}

fn file_type_label(name: &str) -> String {
    if let Some((_, ext)) = name.rsplit_once('.') {
        match ext.to_lowercase().as_str() {
            "txt" | "md" | "log" | "rtf" => "Text document".into(),
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" => "Image".into(),
            "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" => "Audio".into(),
            "mp4" | "mkv" | "mov" | "avi" | "webm" => "Video".into(),
            "pdf" => "PDF document".into(),
            "zip" | "7z" | "rar" | "tar" | "gz" | "bz2" | "xz" => "Archive".into(),
            "zar" => "ZAR archive".into(),
            "exe" | "dll" => "Application".into(),
            "rs" | "py" | "js" | "ts" | "c" | "cpp" | "h" | "java" | "go" | "cs" => {
                "Source code".into()
            }
            _ => format!("{ext} file"),
        }
    } else {
        "File".into()
    }
}
