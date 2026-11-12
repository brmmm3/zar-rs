use super::super::*;

impl ZargApp {
    pub(in crate::ui) fn render_about_dialog(&mut self, context: &egui::Context) {
        if self.show_about {
            let mut close_about = false;
            let mut open = true;
            let size = self.dialog_size("about", [440.0, 220.0]);
            let icons = &self.icons;
            egui::Window::new("About zarg")
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .constrain_to(context.content_rect())
                .fixed_size(size)
                .resizable(false)
                .collapsible(false)
                .order(egui::Order::Foreground)
                .frame(egui::Frame::window(&context.style_of(context.theme())).inner_margin(3))
                .open(&mut open)
                .show(context, |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            let icon = icon_texture(ui.ctx(), &self.icons, UiIcon::Application);
                            ui.add(
                                egui::Image::new(&icon).fit_to_exact_size(egui::vec2(88.0, 88.0)),
                            );
                            ui.vertical(|ui| {
                                ui.heading("ZAR Archive Manager");
                                ui.label("A graphical archive manager for the ZAR format.");
                                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                                ui.hyperlink_to(
                                    "GitHub repository",
                                    "https://github.com/brmmm3/zar-rs-inter",
                                );
                            });
                        });
                        ui.separator();
                        ui.label(format!("Author: {}", env!("CARGO_PKG_AUTHORS")));
                        ui.label(format!("License: {}", env!("CARGO_PKG_LICENSE")));
                        ui.separator();
                        ui.label("Icons: KDE4 Crystal Diamond 1.1 by Paolino (Paolo Campitelli)");
                        ui.horizontal(|ui| {
                            ui.label("Licensed under the");
                            ui.hyperlink_to(
                                "GNU LGPL 3.0",
                                "https://www.gnu.org/licenses/lgpl-3.0.html",
                            );
                            ui.label("-");
                            ui.hyperlink_to(
                                "paolocampitelli.com",
                                "https://www.paolocampitelli.com/",
                            );
                        });
                        ui.add_space(ui.available_height().max(0.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_button(ui, icons, UiIcon::Cancel, "Close", true).clicked() {
                                close_about = true;
                            }
                        });
                    });
                });
            if !open || close_about {
                self.show_about = false;
            }
        }
    }
}
