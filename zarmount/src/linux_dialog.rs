use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use eframe::egui;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

use crate::dialog_style::{self, ButtonIcon, IconTheme};
use crate::theme::{self, ThemeMode};

#[derive(Serialize, Deserialize)]
pub(crate) struct CommitPassword {
    pub id: [u8; 4],
    pub password: String,
}

impl Drop for CommitPassword {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

enum Dialog {
    Ask { path: String },
    Password { rows: Vec<CommitPassword> },
    Settings { theme: ThemeMode },
    About,
    Error { message: String },
}

struct App {
    dialog: Dialog,
    error: Option<String>,
    icon_theme: IconTheme,
}

fn settings_path() -> io::Result<PathBuf> {
    dirs::config_dir()
        .map(|path| path.join("zarmount/settings.conf"))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "configuration directory unavailable",
            )
        })
}

fn parse_settings(
    text: &str,
    theme: &mut Option<ThemeMode>,
    icon_theme: &mut Option<IconTheme>,
) -> io::Result<()> {
    for line in text.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "theme" if theme.is_none() => {
                    *theme = Some(ThemeMode::from_setting(value).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "invalid theme setting")
                    })?);
                }
                "icon_theme" if icon_theme.is_none() => {
                    *icon_theme = Some(IconTheme::from_setting(value).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "invalid icon theme setting")
                    })?);
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn read_settings() -> io::Result<(ThemeMode, IconTheme)> {
    let paths = [
        settings_path()?,
        dirs::config_dir()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "configuration directory unavailable",
                )
            })?
            .join("zarg/settings.conf"),
    ];
    let mut theme = None;
    let mut icon_theme = None;
    for path in paths {
        match std::fs::read_to_string(path) {
            Ok(text) => parse_settings(&text, &mut theme, &mut icon_theme)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        if theme.is_some() && icon_theme.is_some() {
            break;
        }
    }
    Ok((theme.unwrap_or_default(), icon_theme.unwrap_or_default()))
}

fn save_settings(theme: ThemeMode, icon_theme: IconTheme) -> io::Result<()> {
    let path = settings_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("invalid settings path"))?;
    std::fs::create_dir_all(parent)?;
    std::fs::write(path, render_settings(theme, icon_theme))
}

fn render_settings(theme: ThemeMode, icon_theme: IconTheme) -> String {
    format!(
        "theme={}\nicon_theme={}\n",
        theme.as_setting(),
        icon_theme.as_setting()
    )
}

fn output(value: &impl Serialize) -> Result<(), String> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, value).map_err(|error| error.to_string())?;
    stdout.flush().map_err(|error| error.to_string())
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let context = ui.ctx().clone();
        let (cancel, submit) = ui.input(|input| {
            (
                input.key_pressed(egui::Key::Escape)
                    || (input.modifiers == egui::Modifiers::ALT && input.key_pressed(egui::Key::C)),
                input.modifiers == egui::Modifiers::ALT
                    && input.key_pressed(match self.dialog {
                        Dialog::Ask { .. } => egui::Key::M,
                        Dialog::Password { .. } => egui::Key::O,
                        _ => egui::Key::C,
                    }),
            )
        });
        if cancel {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let mut action = submit;
        let dialog_frame = egui::Frame::new()
            .fill(ui.visuals().window_fill().to_opaque())
            .inner_margin(3);
        if matches!(
            self.dialog,
            Dialog::Ask { .. } | Dialog::Settings { .. } | Dialog::About
        ) {
            egui::Panel::bottom("dialog-actions")
                .frame(dialog_frame)
                .show_separator_line(false)
                .show(ui, |ui| {
                    if !matches!(self.dialog, Dialog::About) {
                        ui.separator();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if matches!(self.dialog, Dialog::Ask { .. }) {
                            if dialog_style::button(ui, ButtonIcon::Cancel, "Cancel").clicked() {
                                context.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            action |= dialog_style::button(ui, ButtonIcon::Ok, "Mount").clicked();
                        } else {
                            action |=
                                dialog_style::button(ui, ButtonIcon::Close, "Close").clicked();
                        }
                    });
                });
        }
        egui::CentralPanel::default()
            .frame(dialog_frame)
            .show(ui, |ui| {
                match &mut self.dialog {
                    Dialog::Ask { path } => {
                        ui.label("Path to mount to:");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut *path)
                                    .desired_width((ui.available_width() - 85.0).max(0.0)),
                            );
                            if ui.button("Browse...").clicked()
                                && let Some(folder) = rfd::FileDialog::new().pick_folder()
                            {
                                *path = folder.to_string_lossy().into_owned();
                            }
                        });
                        if action {
                            if path.trim().is_empty() {
                                self.error = Some("Enter a mount path.".into());
                            } else {
                                match output(path) {
                                    Ok(()) => {
                                        context.send_viewport_cmd(egui::ViewportCommand::Close)
                                    }
                                    Err(error) => self.error = Some(error),
                                }
                            }
                        }
                    }
                    Dialog::Password { rows } => {
                        ui.label("Enter passwords for encrypted commits:");
                        egui::ScrollArea::vertical()
                            .max_height(240.0)
                            .show(ui, |ui| {
                                for row in rows.iter_mut() {
                                    ui.horizontal(|ui| {
                                        ui.label(format!(
                                            "Commit {:08x}",
                                            u32::from_le_bytes(row.id)
                                        ));
                                        ui.add(
                                            egui::TextEdit::singleline(&mut row.password)
                                                .password(true),
                                        );
                                    });
                                }
                            });
                        if ui
                            .button("Same password for all encrypted commits")
                            .clicked()
                            && let Some(first) =
                                rows.first().map(|row| Zeroizing::new(row.password.clone()))
                        {
                            for row in rows.iter_mut().skip(1) {
                                row.password.clone_from(&first);
                            }
                        }
                        ui.horizontal(|ui| {
                            action |= ui.button("Open").clicked();
                            if ui.button("Cancel").clicked() {
                                context.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                        if action {
                            match output(rows) {
                                Ok(()) => context.send_viewport_cmd(egui::ViewportCommand::Close),
                                Err(error) => self.error = Some(error),
                            }
                        }
                    }
                    Dialog::Settings { theme: selected } => {
                        ui.horizontal(|ui| {
                            ui.label("File manager:");
                            ui.label("System default");
                        });
                        ui.horizontal(|ui| {
                            ui.label("Theme color:");
                            egui::ComboBox::from_id_salt("settings-theme")
                                .selected_text(selected.label())
                                .show_ui(ui, |ui| {
                                    for mode in ThemeMode::ALL {
                                        if ui
                                            .selectable_value(selected, mode, mode.label())
                                            .changed()
                                        {
                                            theme::apply_theme(&context, *selected);
                                            if let Err(error) =
                                                save_settings(*selected, self.icon_theme)
                                            {
                                                self.error = Some(error.to_string());
                                            }
                                        }
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label("Icon theme:");
                            egui::ComboBox::from_id_salt("settings-icon-theme")
                                .selected_text(self.icon_theme.label())
                                .show_ui(ui, |ui| {
                                    for icon_theme in IconTheme::ALL {
                                        if ui
                                            .selectable_value(
                                                &mut self.icon_theme,
                                                icon_theme,
                                                icon_theme.label(),
                                            )
                                            .changed()
                                        {
                                            dialog_style::set_icon_theme(&context, self.icon_theme);
                                            if let Err(error) =
                                                save_settings(*selected, self.icon_theme)
                                            {
                                                self.error = Some(error.to_string());
                                            }
                                        }
                                    }
                                });
                        });
                        ui.weak("System follows your operating system's light/dark preference.");
                        if action {
                            match save_settings(*selected, self.icon_theme) {
                                Ok(()) => context.send_viewport_cmd(egui::ViewportCommand::Close),
                                Err(error) => self.error = Some(error.to_string()),
                            }
                        }
                    }
                    Dialog::About => {
                        dialog_style::about(ui);
                        if action {
                            context.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    Dialog::Error { message } => {
                        ui.heading("Operation error");
                        ui.label(message.as_str());
                        if ui.button("Close").clicked() || action {
                            context.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                }
                if let Some(error) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
            });
    }
}

pub(crate) fn run(kind: &str, argument: Option<&str>) -> ExitCode {
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let (theme, icon_theme) = read_settings()?;
        let dialog = match kind {
            "ask" => Dialog::Ask {
                path: argument.ok_or("missing mount path")?.to_owned(),
            },
            "password" => {
                let archive = argument.ok_or("missing archive")?;
                let commits = zarlib::operations::info_archive(archive, false)?;
                Dialog::Password {
                    rows: commits
                        .into_iter()
                        .filter(|commit| commit.header_encrypted || commit.data_encrypted)
                        .map(|commit| CommitPassword {
                            id: commit.id,
                            password: String::new(),
                        })
                        .collect(),
                }
            }
            "settings" => Dialog::Settings { theme },
            "about" => Dialog::About,
            "error" => Dialog::Error {
                message: argument.ok_or("missing error message")?.to_owned(),
            },
            _ => return Err("unknown dialog".into()),
        };
        let title = match kind {
            "ask" => "Mount options",
            "password" => "Open encrypted archive",
            "settings" => "Settings",
            "error" => "Operation error",
            _ => "About zarmount",
        };
        let size = match kind {
            "about" => [440.0, 220.0],
            "settings" => [440.0, 190.0],
            "ask" => [480.0, 130.0],
            _ => [520.0, 240.0],
        };
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title(title)
                .with_inner_size(size)
                .with_app_id("zarmount"),
            ..Default::default()
        };
        eframe::run_native(
            title,
            options,
            Box::new(move |creation| {
                theme::install(&creation.egui_ctx, theme);
                dialog_style::set_icon_theme(&creation.egui_ctx, icon_theme);
                Ok(Box::new(App {
                    dialog,
                    error: None,
                    icon_theme,
                }))
            }),
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("zarmount: dialog failed: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_theme_settings_fall_back_per_field() {
        let mut theme = None;
        let mut icons = None;
        parse_settings("theme=dark\n", &mut theme, &mut icons).unwrap();
        parse_settings("theme=light\nicon_theme=flat\n", &mut theme, &mut icons).unwrap();
        assert_eq!(theme, Some(ThemeMode::Dark));
        assert_eq!(icons, Some(IconTheme::Flat));
        let saved = render_settings(theme.unwrap(), icons.unwrap());
        let mut saved_theme = None;
        let mut saved_icons = None;
        parse_settings(&saved, &mut saved_theme, &mut saved_icons).unwrap();
        assert_eq!((saved_theme, saved_icons), (theme, icons));
        assert!(parse_settings("icon_theme=invalid\n", &mut None, &mut None).is_err());
    }
}

pub(crate) fn ask(kind: &str, arg: &Path) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    let result = Command::new(crate::SELF_EXECUTABLE)
        .arg("--linux-dialog")
        .arg(kind)
        .arg(arg)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .output()
        .map_err(|error| format!("cannot start {kind} dialog: {error}"))?;
    if !result.status.success() {
        return Err(format!(
            "{kind} dialog failed: {}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    if result.stdout.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Zeroizing::new(result.stdout)))
    }
}

pub(crate) fn show(kind: &str) -> Result<(), String> {
    spawn_dialog(kind, None)
}

pub(crate) fn show_error(message: &str) -> Result<(), String> {
    spawn_dialog("error", Some(message))
}

fn spawn_dialog(kind: &str, argument: Option<&str>) -> Result<(), String> {
    let mut command = Command::new(crate::SELF_EXECUTABLE);
    command.arg("--linux-dialog").arg(kind);
    if let Some(argument) = argument {
        command.arg(argument);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|error| format!("cannot start {kind} dialog: {error}"))?;
    let kind = kind.to_owned();
    std::thread::spawn(move || match child.wait() {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("zarmount: {kind} dialog failed: {status}"),
        Err(error) => eprintln!("zarmount: cannot wait for {kind} dialog: {error}"),
    });
    Ok(())
}
