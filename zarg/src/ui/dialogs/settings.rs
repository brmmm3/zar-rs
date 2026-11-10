use super::super::*;

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) enum SettingsTab {
    #[default]
    Appearance,
    Compression,
    Advanced,
    Encryption,
    Filters,
}

impl SettingsTab {
    const ALL: [Self; 5] = [
        Self::Appearance,
        Self::Compression,
        Self::Advanced,
        Self::Encryption,
        Self::Filters,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Compression => "Compression",
            Self::Advanced => "Advanced",
            Self::Encryption => "Encryption",
            Self::Filters => "Filters",
        }
    }
}

fn settings_dialog_icon() -> egui::IconData {
    static ICON: OnceLock<egui::IconData> = OnceLock::new();
    ICON.get_or_init(|| {
        let icon = image::load_from_memory(include_bytes!("../../../assets/application-x-zar.png"))
            .expect("embedded application icon must be a valid PNG")
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

impl ZargApp {
    pub(in crate::ui) fn render_settings_dialog(&mut self, context: &egui::Context) {
        if !self.show_settings {
            return;
        }

        let viewport_id = egui::ViewportId::from_hash_of("settings-dialog");
        let size_key = "settings";
        let size = self.dialog_size(size_key, [420.0, 260.0]);
        let mut actual_size = None;
        let mut open = true;
        let mut selected_theme = self.theme_mode;
        let mut selected_icon_theme = self.icons.theme.get();
        let mut selected_scale = self.ui_scale;
        let mut selected_block_size = self.default_block_size.clone();
        let mut selected_compression_level = self.default_compression_level;
        let mut selected_encryption_method = self.default_encryption_method;
        let mut archive_defaults = self.archive_defaults.clone();
        let mut scale_changed = false;
        context.show_viewport_immediate(
            viewport_id,
            Self::centered_dialog_viewport_builder(context, viewport_id, "Settings", size, true)
                .with_app_id("zarg-settings")
                .with_icon(settings_dialog_icon()),
            |ui, _class| {
                ui.painter().rect_filled(ui.max_rect(), 0.0, ui.visuals().window_fill().to_opaque());
                actual_size = Self::viewport_inner_size(ui.ctx());
                if ui.ctx().input(|input| input.viewport().close_requested()) {
                    open = false;
                }
                egui::Panel::bottom("settings-actions")
                    .frame(egui::Frame::new().inner_margin(3))
                    .show_separator_line(false)
                    .show(ui, |ui| {
                        ui.separator();
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_button(ui, &self.icons, UiIcon::Cancel, "Close", true).clicked() {
                                open = false;
                            }
                        });
                    });
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().inner_margin(3))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for tab in SettingsTab::ALL {
                                ui.selectable_value(&mut self.settings_tab, tab, tab.label());
                            }
                        });
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .id_salt(("settings-tab", self.settings_tab.label()))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                if self.settings_tab == SettingsTab::Appearance {
                                    Self::render_combo_row(ui, "Theme color", |ui| {
                                        egui::ComboBox::from_id_salt("settings-theme")
                                            .selected_text(selected_theme.label())
                                            .show_ui(ui, |ui| {
                                                for mode in [
                                                    crate::settings::ThemeMode::System,
                                                    crate::settings::ThemeMode::Light,
                                                    crate::settings::ThemeMode::Dark,
                                                ] {
                                                    ui.selectable_value(&mut selected_theme, mode, mode.label());
                                                }
                                            });
                                    });
                                    ui.weak("System follows your operating system's light/dark preference.");
                                    Self::render_combo_row(ui, "Icon theme", |ui| {
                                        egui::ComboBox::from_id_salt("settings-icon-theme")
                                            .selected_text(selected_icon_theme.label())
                                            .show_ui(ui, |ui| {
                                                for theme in [crate::settings::IconTheme::Crystal, crate::settings::IconTheme::Flat] {
                                                    ui.selectable_value(&mut selected_icon_theme, theme, theme.label());
                                                }
                                            });
                                    });
                                    Self::render_combo_row(ui, "Text and button size", |ui| {
                                        egui::ComboBox::from_id_salt("settings-ui-scale")
                                            .selected_text(
                                                crate::settings::UI_SCALE_OPTIONS
                                                    .iter()
                                                    .find(|(scale, _)| *scale == selected_scale)
                                                    .map_or("1.0", |(_, label)| *label),
                                            )
                                            .show_ui(ui, |ui| {
                                                for (scale, label) in crate::settings::UI_SCALE_OPTIONS {
                                                    scale_changed |= ui.selectable_value(&mut selected_scale, scale, label).changed();
                                                }
                                            });
                                    });
                                    ui.weak("Scales text, buttons, and icons for high-resolution displays.");
                                }
                                if self.settings_tab == SettingsTab::Compression {
                                    ui.weak("Defaults for New archive and Add to archive; each operation can override them.");
                                    Self::render_combo_row(ui, "Default compression level", |ui| {
                                        egui::ComboBox::from_id_salt("settings-compression-level")
                                            .selected_text(selected_compression_level.to_string())
                                            .show_ui(ui, |ui| {
                                                for level in -7..=22 {
                                                    ui.selectable_value(&mut selected_compression_level, level, level.to_string());
                                                }
                                            });
                                    });
                                    Self::render_combo_row(ui, "Default block size", |ui| {
                                        egui::ComboBox::from_id_salt("settings-block-size")
                                            .selected_text(format!("{} MB", selected_block_size.trim_end_matches(['M', 'm'])))
                                            .show_ui(ui, |ui| {
                                                for size in crate::settings::BLOCK_SIZE_OPTIONS {
                                                    ui.selectable_value(
                                                        &mut selected_block_size,
                                                        size.to_owned(),
                                                        format!("{} MB", size.trim_end_matches('M')),
                                                    );
                                                }
                                            });
                                    });
                                }
                                if self.settings_tab == SettingsTab::Encryption {
                                    Self::render_combo_row(ui, "Default encryption method", |ui| {
                                        egui::ComboBox::from_id_salt("settings-encryption-method")
                                            .selected_text(selected_encryption_method.label())
                                            .show_ui(ui, |ui| {
                                                for method in [
                                                    crate::settings::DefaultEncryptionMethod::Aes256Gcm,
                                                    crate::settings::DefaultEncryptionMethod::ChaCha20Poly1305,
                                                ] {
                                                    ui.selectable_value(&mut selected_encryption_method, method, method.label());
                                                }
                                            });
                                    });
                                    ui.separator();
                                    ui.label("Default password");
                                    ui.weak("Stored in the operating system password safe.");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.default_password_input)
                                            .desired_width(ui.available_width())
                                            .password(!self.show_default_password)
                                            .hint_text("Default encryption password"),
                                    );
                                    ui.checkbox(&mut self.show_default_password, "Show password");
                                    ui.horizontal(|ui| {
                                        if ui
                                            .add_enabled(
                                                !self.default_password_input.is_empty(),
                                                egui::Button::new("Save to password safe"),
                                            )
                                            .clicked()
                                        {
                                            self.default_password_status =
                                                Some(match crate::password_store::save_default_password(&self.default_password_input) {
                                                    Ok(()) => {
                                                        self.default_password.clone_from(&self.default_password_input);
                                                        "Default password saved".to_owned()
                                                    }
                                                    Err(error) => {
                                                        format!("Could not save default password: {error}")
                                                    }
                                                });
                                        }
                                        if ui.button("Remove saved password").clicked() {
                                            self.default_password_status = Some(match crate::password_store::remove_default_password() {
                                                Ok(()) => {
                                                    self.default_password.clear();
                                                    self.default_password_input.clear();
                                                    "Default password removed".to_owned()
                                                }
                                                Err(error) => {
                                                    format!("Could not remove default password: {error}")
                                                }
                                            });
                                        }
                                    });
                                    if let Some(status) = &self.default_password_status {
                                        ui.label(status);
                                    }
                                }
                                if self.settings_tab == SettingsTab::Advanced {
                                    ui.weak("Defaults for New archive and Add to archive; each operation can override them.");
                                    ui.checkbox(&mut archive_defaults.metadata, "Preserve platform-specific metadata");
                                    ui.checkbox(&mut archive_defaults.dates, "Save modification dates");
                                    ui.checkbox(&mut archive_defaults.special_files, "Include named pipes and device nodes");
                                    ui.checkbox(&mut archive_defaults.strict, "Strict input validation");
                                    ui.checkbox(&mut archive_defaults.save_user, "Save current OS username (new archives)");
                                }
                                if self.settings_tab == SettingsTab::Filters {
                                    ui.label("Default include / exclude filters");
                                    super::operation::render_filter_patterns(
                                        ui,
                                        &mut archive_defaults.include_files,
                                        &mut archive_defaults.exclude_files,
                                        &mut archive_defaults.include_folders,
                                        &mut archive_defaults.exclude_folders,
                                    );
                                }
                            });
                    });
                if !open {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            },
        );
        self.record_dialog_size(size_key, actual_size);
        self.show_settings = open;
        self.default_block_size = selected_block_size;
        self.default_compression_level = selected_compression_level;
        self.default_encryption_method = selected_encryption_method;
        self.archive_defaults = archive_defaults;
        if scale_changed {
            self.ui_scale = selected_scale;
            self.apply_control_size(context);
        }
        if selected_theme != self.theme_mode {
            self.set_theme_mode(context, selected_theme);
        }
        if selected_icon_theme != self.icons.theme.get() {
            self.icons.set_theme(selected_icon_theme);
            context.request_repaint();
        }
    }
}
