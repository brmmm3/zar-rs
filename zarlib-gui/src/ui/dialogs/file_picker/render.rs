use super::super::super::*;

impl ZargApp {
    pub(in crate::ui) fn render_file_picker_dialog(&mut self, context: &egui::Context) {
        // Custom file dialog (separate OS window via multi-viewport)
        if self.file_dialog.open {
            let viewport_id = egui::ViewportId::from_hash_of("file-dialog");
            let size_key = "file-selection";
            let size = self.dialog_size(size_key, [850.0, 400.0]);
            let mut actual_size = None;
            let mut confirmed = false;
            let mut closed = false;

            context.show_viewport_immediate(
                viewport_id,
                Self::centered_dialog_viewport_builder(
                    context,
                    viewport_id,
                    "Select files and folders",
                    size,
                    true,
                ),
                |ui, _class| {
                    ui.painter().rect_filled(
                        ui.max_rect(),
                        0.0,
                        ui.visuals().window_fill().to_opaque(),
                    );
                    actual_size = Self::viewport_inner_size(ui.ctx());
                    // Check if the OS close button was clicked
                    if ui.ctx().input(|i| i.viewport().close_requested()) {
                        closed = true;
                    }

                    egui::Frame::new().inner_margin(3).show(ui, |ui| {
                        confirmed = self.file_dialog.ui(ui, &self.icons);
                    });

                    if confirmed || closed {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                },
            );
            self.record_dialog_size(size_key, actual_size);

            if confirmed || closed {
                self.file_dialog.open = false;
                let paths = self.file_dialog.selected_paths();
                self.operation.input_paths.extend(paths);
            }
        }
    }
}
