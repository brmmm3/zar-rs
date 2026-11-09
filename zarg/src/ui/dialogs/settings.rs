use super::super::*;

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
        let size = self.dialog_size(size_key, [560.0, 650.0]);
        let mut actual_size = None;
        let mut open = true;
        let mut selected_theme = self.theme_mode;
        let mut selected_scale = self.ui_scale;
        let mut selected_block_size = self.default_block_size.clone();
        let mut selected_compression_level = self.default_compression_level;
        let mut selected_encryption_method = self.default_encryption_method;
        let mut archive_defaults = self.archive_defaults.clone();
        let mut scale_changed = false;
        context.show_viewport_immediate(
            viewport_id,
            Self::centered_dialog_viewport_builder(context, "Settings", size, false)
                .with_app_id("zarg-settings")
                .with_icon(settings_dialog_icon()),
            |ui, _class| {
                ui.painter().rect_filled(
                    ui.max_rect(),
                    0.0,
                    ui.visuals().window_fill().to_opaque(),
                );
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
                            if icon_button(ui, &self.icons, UiIcon::Cancel, "Close", true).clicked()
                            {
                                open = false;
                            }
                        });
                    });
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().inner_margin(3))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        ui.heading("Appearance");
                        egui::ComboBox::from_label("Theme color")
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
                        ui.weak("System follows your operating system's light/dark preference.");
                        egui::ComboBox::from_label("Text and button size")
                            .selected_text(
                                crate::settings::UI_SCALE_OPTIONS
                                    .iter()
                                    .find(|(scale, _)| *scale == selected_scale)
                                    .map_or("1.0", |(_, label)| *label),
                            )
                            .show_ui(ui, |ui| {
                                for (scale, label) in crate::settings::UI_SCALE_OPTIONS {
                                    scale_changed |= ui
                                        .selectable_value(&mut selected_scale, scale, label)
                                        .changed();
                                }
                            });
                        ui.weak("Scales text, buttons, and icons for high-resolution displays.");
                        ui.separator();
                        ui.heading("Archive defaults");
                        egui::ComboBox::from_label("Default block size")
                            .selected_text(format!(
                                "{} MB",
                                selected_block_size.trim_end_matches(['M', 'm'])
                            ))
                            .show_ui(ui, |ui| {
                                for size in crate::settings::BLOCK_SIZE_OPTIONS {
                                    ui.selectable_value(
                                        &mut selected_block_size,
                                        size.to_owned(),
                                        format!("{} MB", size.trim_end_matches('M')),
                                    );
                                }
                            });
                        egui::ComboBox::from_label("Default compression level")
                            .selected_text(selected_compression_level.to_string())
                            .show_ui(ui, |ui| {
                                for level in -7..=22 {
                                    ui.selectable_value(
                                        &mut selected_compression_level,
                                        level,
                                        level.to_string(),
                                    );
                                }
                            });
                        egui::ComboBox::from_label("Default encryption method")
                            .selected_text(selected_encryption_method.label())
                            .show_ui(ui, |ui| {
                                for method in [
                                    crate::settings::DefaultEncryptionMethod::Aes256Gcm,
                                    crate::settings::DefaultEncryptionMethod::ChaCha20Poly1305,
                                ] {
                                    ui.selectable_value(
                                        &mut selected_encryption_method,
                                        method,
                                        method.label(),
                                    );
                                }
                            });
                        ui.separator();
                        ui.weak("Defaults for New archive and Add to archive; each operation can override them.");
                        ui.checkbox(&mut archive_defaults.metadata, "Preserve platform-specific metadata");
                        ui.checkbox(&mut archive_defaults.dates, "Save modification dates");
                        ui.checkbox(&mut archive_defaults.special_files, "Include named pipes and device nodes");
                        ui.checkbox(&mut archive_defaults.strict, "Strict input validation");
                        ui.checkbox(&mut archive_defaults.save_user, "Save current OS username (new archives)");
                        ui.separator();
                        ui.label("Default include / exclude filters");
                        super::operation::render_filter_patterns(ui, &mut archive_defaults.include_files,
                            &mut archive_defaults.exclude_files, &mut archive_defaults.include_folders,
                            &mut archive_defaults.exclude_folders);
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
    }
}
