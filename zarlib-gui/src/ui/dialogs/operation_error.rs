use super::super::*;

pub(in crate::ui) fn error_dialog_icon() -> egui::IconData {
    static ICON: OnceLock<egui::IconData> = OnceLock::new();
    ICON.get_or_init(|| {
        let icon = image::load_from_memory(include_bytes!("../../../assets/zarg-error.png"))
            .expect("embedded error dialog icon must be a valid PNG")
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
    pub(in crate::ui) fn render_operation_error_dialog(&mut self, context: &egui::Context) {
        if let Some(report) = self.report.clone() {
            let viewport_id = egui::ViewportId::from_hash_of("operation-error-dialog");
            let size_key = "operation-error";
            let size = self.dialog_size(size_key, [520.0, 190.0]);
            let mut actual_size = None;
            let mut open = true;
            let viewport_builder = Self::centered_dialog_viewport_builder(
                context,
                viewport_id,
                "Operation error",
                size,
                false,
            )
            .with_app_id("zarg-error")
            .with_icon(error_dialog_icon());
            context.show_viewport_immediate(viewport_id, viewport_builder, |ui, _class| {
                ui.painter().rect_filled(
                    ui.max_rect(),
                    0.0,
                    ui.visuals().window_fill().to_opaque(),
                );
                actual_size = Self::viewport_inner_size(ui.ctx());
                if ui.ctx().input(|input| input.viewport().close_requested()) {
                    open = false;
                }
                egui::Panel::bottom("operation-error-actions")
                    .frame(egui::Frame::new().inner_margin(3))
                    .show_separator_line(false)
                    .show(ui, |ui| {
                        ui.separator();
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Close").clicked() {
                                open = false;
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
                                let error_icon = icon_texture(ui.ctx(), &self.icons, UiIcon::Error);
                                ui.add(
                                    egui::Image::new(&error_icon)
                                        .fit_to_exact_size(egui::vec2(48.0, 48.0)),
                                );
                                egui::ScrollArea::vertical()
                                    .max_height(message_size.y)
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        ui.add_space(8.0);
                                        ui.add(egui::Label::new(report.as_str()).wrap());
                                    });
                            },
                        );
                    });
                if !open {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            self.record_dialog_size(size_key, actual_size);
            if !open {
                self.report = None;
            }
        }
    }
}
