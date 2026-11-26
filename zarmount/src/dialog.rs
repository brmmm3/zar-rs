//! egui dialogs for zarmount, styled to match `zarg` (zarlib-gui).
//!
//! Per `zarmount.md`:
//! - `--ask` dialog with mount path (edit field + browse button prefilled with temp folder),
//!   combobox of unmapped drive letters + empty default, and Mount (Alt+M) / Cancel (Alt+C, ESC).
//! - Map drive letter dialog from tray.
//! - Settings dialog with File Explorer / FreeCommander (if available) and theme options.
//! - About dialog matching zarg's about panel.
//! - Password dialog for encrypted archives.
//! - Error dialog for reporting failures.

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::OnceLock;

use eframe::egui;
use zarlib::operations::{self, PasswordSet};

use crate::drive;
use crate::settings::{FileManager, Settings};
use crate::theme::{self, ThemeMode};

/// `CREATE_NO_WINDOW` — do not create a console window for the child dialog.
const CREATE_NO_WINDOW: u32 = 0x08000000;

// Bundled icons (shared with zarg).
const ICON_CANCEL: &[u8] = include_bytes!("../../zarlib-gui/assets/flat/actions/dialog-cancel.png");
const ICON_OK: &[u8] = include_bytes!("../../zarlib-gui/assets/flat/actions/dialog-ok.png");
const ICON_CLOSE: &[u8] = include_bytes!("../../zarlib-gui/assets/flat/actions/dialog-close.png");
const ICON_ERROR_LARGE: &[u8] = include_bytes!("../../zarlib-gui/assets/zarg-error.png");
const ICON_APP: &[u8] = include_bytes!("../../zarlib-gui/assets/application-x-zar.png");

fn load_icon(ctx: &egui::Context, name: &str, bytes: &'static [u8]) -> egui::TextureHandle {
    let image = image::load_from_memory(bytes)
        .expect("bundled icon must be a valid image")
        .into_rgba8();
    let [width, height] = [image.width() as usize, image.height() as usize];
    ctx.load_texture(
        name,
        egui::ColorImage::from_rgba_unmultiplied([width, height], image.as_raw()),
        egui::TextureOptions::LINEAR,
    )
}

fn icon_button(
    ui: &mut egui::Ui,
    icon_name: &str,
    icon_bytes: &'static [u8],
    label: impl Into<egui::WidgetText>,
) -> egui::Response {
    let texture = load_icon(ui.ctx(), icon_name, icon_bytes);
    ui.add(egui::Button::image_and_text(
        egui::Image::new(&texture).fit_to_exact_size(egui::vec2(16.0, 16.0)),
        label,
    ))
}

fn app_icon() -> egui::IconData {
    static ICON: OnceLock<egui::IconData> = OnceLock::new();
    ICON.get_or_init(|| {
        let icon = image::load_from_memory(ICON_APP)
            .expect("embedded app icon must be a valid PNG")
            .into_rgba8();
        let (width, height) = icon.dimensions();
        egui::IconData {
            rgba: icon.into_raw(),
            width,
            height,
        }
    })
    .clone()
}

// ---------------------------------------------------------------------------
// 1. Ask Dialog (`zarmount archive.zar --ask`)
// ---------------------------------------------------------------------------

struct AskDialogApp {
    mount_path: String,
    selected_drive: String,
    unmapped_drives: Vec<char>,
    out_file: PathBuf,
}

impl eframe::App for AskDialogApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Keyboard shortcuts: Alt+M for Mount, ESC or Alt+C for Cancel
        let mut do_mount = false;
        let mut do_cancel = false;

        ui.input(|i| {
            if i.key_pressed(egui::Key::Escape)
                || (i.modifiers.alt && i.key_pressed(egui::Key::C))
            {
                do_cancel = true;
            }
            if i.modifiers.alt && i.key_pressed(egui::Key::M) {
                do_mount = true;
            }
        });

        egui::Panel::bottom("ask-dialog-actions")
            .frame(egui::Frame::new().inner_margin(6))
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, "zarmount-cancel", ICON_CANCEL, "Cancel").clicked()
                        || do_cancel
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if icon_button(ui, "zarmount-mount", ICON_OK, "Mount").clicked()
                        || do_mount
                    {
                        let content = format!("{}\n{}\n", self.mount_path.trim(), self.selected_drive.trim());
                        let _ = fs::write(&self.out_file, content);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(4.0);
            ui.label("Path to mount to:");
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.mount_path)
                        .desired_width(ui.available_width() - 85.0),
                );
                if ui.button("Browse...").clicked()
                    && let Some(folder) = rfd::FileDialog::new().pick_folder()
                {
                    self.mount_path = folder.to_string_lossy().into_owned();
                }
            });

            ui.add_space(12.0);
            ui.label("Drive letter (optional):");
            let display_drive = if self.selected_drive.is_empty() {
                "(None)".to_string()
            } else {
                format!("{}:", self.selected_drive)
            };

            egui::ComboBox::from_id_salt("ask-drive-combo")
                .selected_text(display_drive)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.selected_drive, String::new(), "(None)");
                    for &letter in &self.unmapped_drives {
                        ui.selectable_value(
                            &mut self.selected_drive,
                            letter.to_string(),
                            format!("{letter}:"),
                        );
                    }
                });
        });
    }
}

pub fn run_ask_dialog(temp_path: &Path, out_file: &Path) -> ExitCode {
    let unmapped_drives = drive::unmapped_drive_letters();
    let app = AskDialogApp {
        mount_path: temp_path.to_string_lossy().into_owned(),
        selected_drive: String::new(),
        unmapped_drives,
        out_file: out_file.to_path_buf(),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Mount options")
            .with_inner_size([480.0, 180.0])
            .with_resizable(false)
            .with_app_id("zarmount-ask")
            .with_icon(app_icon()),
        ..Default::default()
    };
    let settings = Settings::load();
    let result = eframe::run_native(
        "Mount options",
        options,
        Box::new(move |creation_context| {
            theme::install(&creation_context.egui_ctx, settings.theme_mode);
            Ok(Box::new(app))
        }),
    );
    match result {
        Ok(_) if out_file.exists() => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

pub fn spawn_ask_dialog(temp_path: &Path, out_file: &Path) -> Option<(PathBuf, Option<char>)> {
    let exe = std::env::current_exe().ok()?;
    let _ = fs::remove_file(out_file);
    let status = Command::new(&exe)
        .args([
            "--ask-dialog",
            &temp_path.to_string_lossy(),
            &out_file.to_string_lossy(),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .ok()?;
    if status.success() && out_file.exists() {
        let content = fs::read_to_string(out_file).ok()?;
        let _ = fs::remove_file(out_file);
        let mut lines = content.lines();
        let path_str = lines.next()?.trim().to_string();
        let drive_char = lines
            .next()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .and_then(|s| s.chars().next())
            .map(|c| c.to_ascii_uppercase());
        if !path_str.is_empty() {
            return Some((PathBuf::from(path_str), drive_char));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 2. Map Drive Letter Dialog (Tray menu action)
// ---------------------------------------------------------------------------

struct MapDriveDialogApp {
    selected_drive: String,
    unmapped_drives: Vec<char>,
    out_file: PathBuf,
}

impl eframe::App for MapDriveDialogApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        let mut do_map = false;
        let mut do_cancel = false;

        ui.input(|i| {
            if i.key_pressed(egui::Key::Escape)
                || (i.modifiers.alt && i.key_pressed(egui::Key::C))
            {
                do_cancel = true;
            }
            if i.modifiers.alt && i.key_pressed(egui::Key::M) {
                do_map = true;
            }
        });

        egui::Panel::bottom("map-drive-actions")
            .frame(egui::Frame::new().inner_margin(6))
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, "zarmount-cancel", ICON_CANCEL, "Cancel").clicked()
                        || do_cancel
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if (icon_button(ui, "zarmount-map", ICON_OK, "Map").clicked() || do_map)
                        && !self.selected_drive.is_empty()
                    {
                        let _ = fs::write(&self.out_file, format!("{}\n", self.selected_drive.trim()));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(8.0);
            ui.label("Select drive letter to map:");
            let display_drive = if self.selected_drive.is_empty() {
                "Select a drive letter...".to_string()
            } else {
                format!("{}:", self.selected_drive)
            };

            egui::ComboBox::from_id_salt("map-drive-combo")
                .selected_text(display_drive)
                .show_ui(ui, |ui| {
                    for &letter in &self.unmapped_drives {
                        ui.selectable_value(
                            &mut self.selected_drive,
                            letter.to_string(),
                            format!("{letter}:"),
                        );
                    }
                });
        });
    }
}

pub fn run_map_drive_dialog(out_file: &Path) -> ExitCode {
    let unmapped_drives = drive::unmapped_drive_letters();
    let initial_drive = unmapped_drives
        .first()
        .map(|c| c.to_string())
        .unwrap_or_default();
    let app = MapDriveDialogApp {
        selected_drive: initial_drive,
        unmapped_drives,
        out_file: out_file.to_path_buf(),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Map drive letter")
            .with_inner_size([360.0, 140.0])
            .with_resizable(false)
            .with_app_id("zarmount-map-drive")
            .with_icon(app_icon()),
        ..Default::default()
    };
    let settings = Settings::load();
    let result = eframe::run_native(
        "Map drive letter",
        options,
        Box::new(move |creation_context| {
            theme::install(&creation_context.egui_ctx, settings.theme_mode);
            Ok(Box::new(app))
        }),
    );
    match result {
        Ok(_) if out_file.exists() => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

pub fn spawn_map_drive_dialog(out_file: &Path) -> Option<char> {
    let exe = std::env::current_exe().ok()?;
    let _ = fs::remove_file(out_file);
    let status = Command::new(&exe)
        .args(["--map-drive-dialog", &out_file.to_string_lossy()])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .ok()?;
    if status.success() && out_file.exists() {
        let content = fs::read_to_string(out_file).ok()?;
        let _ = fs::remove_file(out_file);
        return content.trim().chars().next().map(|c| c.to_ascii_uppercase());
    }
    None
}

// ---------------------------------------------------------------------------
// 3. ProjFS Confirmation Dialog (when ProjFS is disabled on daemon startup)
// ---------------------------------------------------------------------------

struct ProjFsDialogApp {
    out_file: PathBuf,
}

impl eframe::App for ProjFsDialogApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        let mut do_enable = false;
        let mut do_cancel = false;

        ui.input(|i| {
            if i.key_pressed(egui::Key::Escape)
                || (i.modifiers.alt && i.key_pressed(egui::Key::C))
            {
                do_cancel = true;
            }
            if i.key_pressed(egui::Key::Enter)
                || (i.modifiers.alt && i.key_pressed(egui::Key::E))
            {
                do_enable = true;
            }
        });

        egui::Panel::bottom("projfs-dialog-actions")
            .frame(egui::Frame::new().inner_margin(6))
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, "zarmount-cancel", ICON_CANCEL, "Cancel").clicked()
                        || do_cancel
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if icon_button(ui, "zarmount-enable", ICON_OK, "Enable").clicked()
                        || do_enable
                    {
                        let _ = fs::write(&self.out_file, "enable\n");
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let app_icon_texture = load_icon(ui.ctx(), "zarmount-app-icon", ICON_APP);
                ui.add(
                    egui::Image::new(&app_icon_texture)
                        .fit_to_exact_size(egui::vec2(48.0, 48.0)),
                );
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.heading("Windows Projected File System (ProjFS)");
                    ui.add_space(4.0);
                    ui.label(
                        "Windows Projected File System (ProjFS) is currently not enabled.",
                    );
                    ui.label(
                        "ProjFS is required to mount ZAR archives as a virtual filesystem.",
                    );
                    ui.add_space(4.0);
                    ui.label("Would you like to enable ProjFS now?");
                });
            });
        });
    }
}

pub fn run_projfs_dialog(out_file: &Path) -> ExitCode {
    let app = ProjFsDialogApp {
        out_file: out_file.to_path_buf(),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Enable ProjFS")
            .with_inner_size([480.0, 160.0])
            .with_resizable(false)
            .with_app_id("zarmount-projfs")
            .with_icon(app_icon()),
        ..Default::default()
    };
    let settings = Settings::load();
    let result = eframe::run_native(
        "Enable ProjFS",
        options,
        Box::new(move |creation_context| {
            theme::install(&creation_context.egui_ctx, settings.theme_mode);
            Ok(Box::new(app))
        }),
    );
    match result {
        Ok(_) if out_file.exists() => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

pub fn spawn_projfs_dialog() -> bool {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let out_file =
        std::env::temp_dir().join(format!("zarmount_projfs_{}.txt", std::process::id()));
    let _ = fs::remove_file(&out_file);
    let status = Command::new(&exe)
        .args(["--projfs-dialog", &out_file.to_string_lossy()])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    let approved = match status {
        Ok(s) if s.success() && out_file.exists() => {
            let content = fs::read_to_string(&out_file).unwrap_or_default();
            content.trim() == "enable"
        }
        _ => false,
    };
    let _ = fs::remove_file(&out_file);
    approved
}

// ---------------------------------------------------------------------------
// 4. Settings Dialog
// ---------------------------------------------------------------------------

struct SettingsDialogApp {
    settings: Settings,
}

impl eframe::App for SettingsDialogApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        let mut do_close = false;
        ui.input(|i| {
            if i.key_pressed(egui::Key::Escape)
                || (i.modifiers.alt && i.key_pressed(egui::Key::C))
            {
                do_close = true;
            }
        });

        egui::Panel::bottom("settings-dialog-actions")
            .frame(egui::Frame::new().inner_margin(6))
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, "zarmount-close", ICON_CLOSE, "Close").clicked()
                        || do_close
                    {
                        let _ = self.settings.save();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(6.0);
            ui.heading("Settings");
            ui.add_space(8.0);

            // File manager combobox
            ui.label("File manager:");
            let mut fm_changed = false;
            egui::ComboBox::from_id_salt("settings-file-manager")
                .selected_text(self.settings.file_manager.label())
                .show_ui(ui, |ui| {
                    for fm in FileManager::ALL {
                        if fm.is_available() {
                            fm_changed |= ui
                                .selectable_value(
                                    &mut self.settings.file_manager,
                                    fm,
                                    fm.label(),
                                )
                                .changed();
                        }
                    }
                });

            ui.add_space(10.0);

            // Theme combobox (same options as zarg)
            ui.label("Theme:");
            let mut theme_changed = false;
            egui::ComboBox::from_id_salt("settings-theme")
                .selected_text(self.settings.theme_mode.label())
                .show_ui(ui, |ui| {
                    for mode in ThemeMode::ALL {
                        theme_changed |= ui
                            .selectable_value(&mut self.settings.theme_mode, mode, mode.label())
                            .changed();
                    }
                });

            if theme_changed {
                theme::apply_theme(&ctx, self.settings.theme_mode);
                let _ = self.settings.save();
            }
            if fm_changed {
                let _ = self.settings.save();
            }
        });
    }
}

pub fn run_settings_dialog() -> ExitCode {
    let settings = Settings::load();
    let current_theme = settings.theme_mode;
    let app = SettingsDialogApp { settings };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Settings")
            .with_inner_size([380.0, 200.0])
            .with_resizable(false)
            .with_app_id("zarmount-settings")
            .with_icon(app_icon()),
        ..Default::default()
    };
    let result = eframe::run_native(
        "Settings",
        options,
        Box::new(move |creation_context| {
            theme::install(&creation_context.egui_ctx, current_theme);
            Ok(Box::new(app))
        }),
    );
    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

pub fn spawn_settings_dialog() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(&exe)
            .arg("--settings-dialog")
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}

// ---------------------------------------------------------------------------
// 4. About Dialog (matches zarg)
// ---------------------------------------------------------------------------

struct AboutDialogApp;

impl eframe::App for AboutDialogApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        let mut do_close = false;
        ui.input(|i| {
            if i.key_pressed(egui::Key::Escape)
                || (i.modifiers.alt && i.key_pressed(egui::Key::C))
            {
                do_close = true;
            }
        });

        egui::Panel::bottom("about-actions")
            .frame(egui::Frame::new().inner_margin(6))
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, "zarmount-close", ICON_CLOSE, "Close").clicked()
                        || do_close
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    let app_icon_texture = load_icon(ui.ctx(), "zarmount-app-icon", ICON_APP);
                    ui.add(
                        egui::Image::new(&app_icon_texture)
                            .fit_to_exact_size(egui::vec2(72.0, 72.0)),
                    );
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.heading("ZAR Archive Manager");
                        ui.label("A graphical archive manager and mount filesystem for ZAR.");
                        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                        ui.hyperlink_to("GitHub repository", "https://github.com/brmmm3/zar-rs");
                    });
                });
                ui.separator();
                ui.label(format!("Author: {}", env!("CARGO_PKG_AUTHORS")));
                ui.label(format!("License: {}", env!("CARGO_PKG_LICENSE")));
                ui.separator();
                ui.label("Icons: KDE4 Crystal Diamond 1.1 by Paolino (Paolo Campitelli)");
                ui.horizontal(|ui| {
                    ui.label("Licensed under the");
                    ui.hyperlink_to("GNU LGPL 3.0", "https://www.gnu.org/licenses/lgpl-3.0.html");
                    ui.label("-");
                    ui.hyperlink_to("paolocampitelli.com", "https://www.paolocampitelli.com/");
                });
            });
        });
    }
}

pub fn run_about_dialog() -> ExitCode {
    let app = AboutDialogApp;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("About zarmount")
            .with_inner_size([460.0, 240.0])
            .with_resizable(false)
            .with_app_id("zarmount-about")
            .with_icon(app_icon()),
        ..Default::default()
    };
    let settings = Settings::load();
    let result = eframe::run_native(
        "About zarmount",
        options,
        Box::new(move |creation_context| {
            theme::install(&creation_context.egui_ctx, settings.theme_mode);
            Ok(Box::new(app))
        }),
    );
    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

pub fn spawn_about_dialog() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(&exe)
            .arg("--about-dialog")
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}

// ---------------------------------------------------------------------------
// 5. Password dialog
// ---------------------------------------------------------------------------

struct OpenPasswordRow {
    id: [u8; 4],
    encrypted: bool,
    password: String,
}

fn copy_first_password(rows: &mut [OpenPasswordRow]) {
    let Some(password) = rows
        .iter()
        .find(|row| row.encrypted)
        .map(|row| row.password.clone())
        .filter(|password| !password.is_empty())
    else {
        return;
    };
    for row in rows.iter_mut().filter(|row| row.encrypted).skip(1) {
        row.password.clone_from(&password);
    }
}

fn render_rows(ui: &mut egui::Ui, rows: &mut [OpenPasswordRow]) -> egui::Response {
    egui::Grid::new("open-password-commits")
        .num_columns(2)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            for row in rows.iter_mut() {
                ui.label(format!("Commit {:08x}", u32::from_le_bytes(row.id)));
                if row.encrypted {
                    ui.add(
                        egui::TextEdit::singleline(&mut row.password)
                            .password(true)
                            .hint_text("Password"),
                    );
                } else {
                    ui.weak("Not encrypted");
                }
                ui.end_row();
            }
        });
    ui.add_space(8.0);
    let enabled = rows
        .iter()
        .find(|row| row.encrypted)
        .is_some_and(|row| !row.password.is_empty());
    ui.add_enabled(
        enabled,
        egui::Button::new("Same password for all encrypted commits"),
    )
}

struct PasswordDialogApp {
    rows: Vec<OpenPasswordRow>,
    out_file: PathBuf,
    error: Option<String>,
}

impl eframe::App for PasswordDialogApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let mut do_cancel = false;
        let mut do_open = false;

        ui.input(|i| {
            if i.key_pressed(egui::Key::Escape)
                || (i.modifiers.alt && i.key_pressed(egui::Key::C))
            {
                do_cancel = true;
            }
            if i.modifiers.alt && i.key_pressed(egui::Key::O) {
                do_open = true;
            }
        });

        egui::Panel::bottom("open-password-actions")
            .frame(egui::Frame::new().inner_margin(6))
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, "zarmount-cancel", ICON_CANCEL, "Cancel").clicked()
                        || do_cancel
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if icon_button(ui, "zarmount-ok", ICON_OK, "Open").clicked() || do_open {
                        let passwords: Vec<([u8; 4], String)> = self
                            .rows
                            .iter()
                            .filter(|row| row.encrypted && !row.password.is_empty())
                            .map(|row| (row.id, row.password.clone()))
                            .collect();
                        if let Err(e) = write_passwords(&self.out_file, &passwords) {
                            self.error = Some(format!("Could not save passwords: {e}"));
                        } else {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(
                "Enter passwords for the commits to unlock. Leave a field empty to keep that commit locked.",
            );
            ui.add_space(8.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                if render_rows(ui, &mut self.rows).clicked() {
                    copy_first_password(&mut self.rows);
                }
                if let Some(error) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            });
        });
    }
}

pub fn run_password_dialog(archive: &str, out_file: &Path) -> ExitCode {
    let commits = match operations::info_archive(archive, false) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("zarmount: cannot read archive: {e}");
            return ExitCode::FAILURE;
        }
    };
    let rows = commits
        .into_iter()
        .map(|commit| OpenPasswordRow {
            id: commit.id,
            encrypted: commit.header_encrypted || commit.data_encrypted,
            password: String::new(),
        })
        .collect();
    let app = PasswordDialogApp {
        rows,
        out_file: out_file.to_path_buf(),
        error: None,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Open encrypted archive")
            .with_inner_size([520.0, 220.0])
            .with_resizable(false)
            .with_app_id("zarmount-password")
            .with_icon(app_icon()),
        ..Default::default()
    };
    let settings = Settings::load();
    let result = eframe::run_native(
        "Open encrypted archive",
        options,
        Box::new(move |creation_context| {
            theme::install(&creation_context.egui_ctx, settings.theme_mode);
            Ok(Box::new(app))
        }),
    );
    match result {
        Ok(_) if out_file.exists() => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}

// ---------------------------------------------------------------------------
// 6. Error dialog
// ---------------------------------------------------------------------------

struct ErrorDialogApp {
    message: String,
}

impl eframe::App for ErrorDialogApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let mut do_close = false;
        ui.input(|i| {
            if i.key_pressed(egui::Key::Escape)
                || (i.modifiers.alt && i.key_pressed(egui::Key::C))
            {
                do_close = true;
            }
        });

        egui::Panel::bottom("operation-error-actions")
            .frame(egui::Frame::new().inner_margin(3))
            .show_separator_line(false)
            .show(ui, |ui| {
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, "zarmount-close", ICON_CLOSE, "Close").clicked()
                        || do_close
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(3))
            .show(ui, |ui| {
                let message_size = ui.available_size();
                ui.allocate_ui_with_layout(
                    message_size,
                    egui::Layout::left_to_right(egui::Align::Min),
                    |ui| {
                        ui.set_min_size(message_size);
                        let error_icon =
                            load_icon(ui.ctx(), "zarmount-error-large", ICON_ERROR_LARGE);
                        ui.add(
                            egui::Image::new(&error_icon).fit_to_exact_size(egui::vec2(48.0, 48.0)),
                        );
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            egui::ScrollArea::vertical()
                                .max_height(ui.available_height())
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.add_space(16.0);
                                    ui.add(egui::Label::new(self.message.as_str()).wrap());
                                });
                        });
                    },
                );
            });
    }
}

pub fn run_error_dialog(message: &str) -> ExitCode {
    let app = ErrorDialogApp {
        message: message.to_string(),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Operation error")
            .with_inner_size([520.0, 190.0])
            .with_resizable(false)
            .with_app_id("zarmount-error")
            .with_icon(app_icon()),
        ..Default::default()
    };
    let settings = Settings::load();
    let result = eframe::run_native(
        "Operation error",
        options,
        Box::new(move |creation_context| {
            theme::install(&creation_context.egui_ctx, settings.theme_mode);
            Ok(Box::new(app))
        }),
    );
    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("zarmount: error dialog failed: {e}");
            ExitCode::FAILURE
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn spawn_password_dialog(archive: &str, out_file: &Path) -> bool {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let result = Command::new(&exe)
        .args(["--password-dialog", archive, &out_file.to_string_lossy()])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    match result {
        Ok(status) => status.success() && out_file.exists(),
        Err(_) => false,
    }
}

pub fn spawn_error_dialog(message: &str) {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };
    let _ = Command::new(&exe)
        .args(["--error-dialog", message])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

pub fn write_passwords(path: &Path, passwords: &[([u8; 4], String)]) -> std::io::Result<()> {
    let mut buf = Vec::new();
    for (id, password) in passwords {
        buf.extend_from_slice(id);
        buf.extend_from_slice(&(password.len() as u32).to_le_bytes());
        buf.extend_from_slice(password.as_bytes());
    }
    fs::write(path, buf)
}

pub fn read_passwords(path: &Path) -> std::io::Result<PasswordSet> {
    let data = fs::read(path)?;
    let mut set = PasswordSet::new(None);
    let mut i = 0;
    while i + 8 <= data.len() {
        let id: [u8; 4] = data[i..i + 4].try_into().unwrap();
        let len = u32::from_le_bytes(data[i + 4..i + 8].try_into().unwrap()) as usize;
        i += 8;
        if i + len > data.len() {
            break;
        }
        let password = String::from_utf8_lossy(&data[i..i + len]).into_owned();
        i += len;
        if !password.is_empty() {
            set.insert_for_commit(id, password.into_bytes());
        }
    }
    Ok(set)
}
