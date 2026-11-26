use eframe::egui;

const APP: &[u8] = include_bytes!("../../zarlib-gui/assets/zarg-icon.png");
const CLOSE: &[u8] = include_bytes!("../../zarlib-gui/assets/crystal/actions/dialog-close.png");
const CANCEL: &[u8] = include_bytes!("../../zarlib-gui/assets/crystal/actions/dialog-cancel.png");
const OK: &[u8] = include_bytes!("../../zarlib-gui/assets/crystal/actions/dialog-ok.png");

fn texture(ctx: &egui::Context, name: &'static str, bytes: &'static [u8]) -> egui::TextureHandle {
    let id = egui::Id::unique(name);
    if let Some(texture) = ctx.data_mut(|data| data.get_temp::<egui::TextureHandle>(id)) {
        return texture;
    }
    let image = image::load_from_memory(bytes)
        .expect("bundled icon must be valid")
        .into_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let texture = ctx.load_texture(
        name,
        egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
    texture
}

pub(crate) enum ButtonIcon {
    Close,
    Cancel,
    Ok,
}

pub(crate) fn button(ui: &mut egui::Ui, icon: ButtonIcon, label: &str) -> egui::Response {
    let (name, bytes) = match icon {
        ButtonIcon::Close => ("zarmount-dialog-close", CLOSE),
        ButtonIcon::Cancel => ("zarmount-dialog-cancel", CANCEL),
        ButtonIcon::Ok => ("zarmount-dialog-ok", OK),
    };
    let texture = texture(ui.ctx(), name, bytes);
    ui.add(egui::Button::image_and_text(
        egui::Image::new(&texture).fit_to_exact_size(egui::vec2(16.0, 16.0)),
        label,
    ))
}

pub(crate) fn about(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let icon = texture(ui.ctx(), "zarmount-about-app", APP);
        ui.add(egui::Image::new(&icon).fit_to_exact_size(egui::vec2(88.0, 88.0)));
        ui.vertical(|ui| {
            ui.heading("ZAR Archive Manager");
            ui.label("A graphical archive manager for the ZAR format.");
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
}
