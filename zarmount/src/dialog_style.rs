use eframe::egui;

const APP: &[u8] = include_bytes!("../assets/zarg-icon.png");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum IconTheme {
    #[default]
    Crystal,
    Flat,
}

impl IconTheme {
    pub(crate) const ALL: [Self; 2] = [Self::Crystal, Self::Flat];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Crystal => "Crystal",
            Self::Flat => "Flat",
        }
    }

    pub(crate) fn as_setting(self) -> &'static str {
        match self {
            Self::Crystal => "crystal",
            Self::Flat => "flat",
        }
    }

    pub(crate) fn from_setting(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "crystal" => Some(Self::Crystal),
            "flat" => Some(Self::Flat),
            _ => None,
        }
    }
}

fn texture(
    ctx: &egui::Context,
    name: &'static str,
    theme: IconTheme,
    bytes: &'static [u8],
) -> egui::TextureHandle {
    let id = egui::Id::unique((name, theme));
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

#[derive(Clone, Copy)]
pub(crate) enum ButtonIcon {
    Close,
    Cancel,
    Ok,
}

pub(crate) fn icon_bytes(icon: ButtonIcon, theme: IconTheme) -> &'static [u8] {
    macro_rules! themed_icon {
        ($path:literal) => {
            match theme {
                IconTheme::Crystal => {
                    include_bytes!(concat!("../assets/crystal/", $path)) as &'static [u8]
                }
                IconTheme::Flat => {
                    include_bytes!(concat!("../assets/flat/", $path)) as &'static [u8]
                }
            }
        };
    }
    match icon {
        ButtonIcon::Close => themed_icon!("actions/dialog-close.png"),
        ButtonIcon::Cancel => themed_icon!("actions/dialog-cancel.png"),
        ButtonIcon::Ok => themed_icon!("actions/dialog-ok.png"),
    }
}

pub(crate) fn set_icon_theme(ctx: &egui::Context, theme: IconTheme) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::unique("zarmount-icon-theme"), theme));
}

pub(crate) fn button(ui: &mut egui::Ui, icon: ButtonIcon, label: &str) -> egui::Response {
    let theme = ui.ctx().data(|data| {
        data.get_temp::<IconTheme>(egui::Id::unique("zarmount-icon-theme"))
            .unwrap_or_default()
    });
    let name = match icon {
        ButtonIcon::Close => "zarmount-dialog-close",
        ButtonIcon::Cancel => "zarmount-dialog-cancel",
        ButtonIcon::Ok => "zarmount-dialog-ok",
    };
    let texture = texture(ui.ctx(), name, theme, icon_bytes(icon, theme));
    ui.add(egui::Button::image_and_text(
        egui::Image::new(&texture).fit_to_exact_size(egui::vec2(16.0, 16.0)),
        label,
    ))
}

pub(crate) fn about(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let icon = texture(ui.ctx(), "zarmount-about-app", IconTheme::default(), APP);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_themes_load_distinct_icons_and_cached_textures() {
        let ctx = egui::Context::default();
        for (name, icon) in [
            ("close", ButtonIcon::Close),
            ("cancel", ButtonIcon::Cancel),
            ("ok", ButtonIcon::Ok),
        ] {
            let crystal = icon_bytes(icon, IconTheme::Crystal);
            let flat = icon_bytes(icon, IconTheme::Flat);
            assert_ne!(crystal, flat);
            assert!(image::load_from_memory(crystal).is_ok());
            assert!(image::load_from_memory(flat).is_ok());
            let original = texture(&ctx, name, IconTheme::Crystal, crystal);
            let changed = texture(&ctx, name, IconTheme::Flat, flat);
            assert_ne!(original.id(), changed.id());
            assert_eq!(
                original.id(),
                texture(&ctx, name, IconTheme::Crystal, crystal).id()
            );
        }
    }
}
