//! Qt6 Fusion look for the egui UI, following the desktop's light or dark mode.
//!
//! winit reports the system theme on Windows and macOS only. On Linux/BSD the
//! theme is detected here (XDG settings portal, then desktop-specific
//! settings) and re-checked periodically so the app follows runtime changes.

use eframe::egui::{
    self, Color32, CornerRadius, Margin, Shadow, Stroke, Theme, ThemePreference, Visuals, vec2,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use crate::settings::ThemeMode;

#[derive(Clone)]
pub struct ThemeController {
    mode: Arc<AtomicU8>,
    context: egui::Context,
}

impl ThemeController {
    pub fn set_mode(&self, mode: ThemeMode) {
        self.mode.store(mode.as_byte(), Ordering::Relaxed);
        apply_theme(&self.context, mode);
    }
}

impl ThemeMode {
    fn as_byte(self) -> u8 {
        match self {
            Self::System => 0,
            Self::Light => 1,
            Self::Dark => 2,
            Self::CatppuccinLatte => 3,
            Self::CatppuccinFrappe => 4,
            Self::CatppuccinMacchiato => 5,
            Self::CatppuccinMocha => 6,
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    fn from_byte(value: u8) -> Self {
        match value {
            1 => Self::Light,
            2 => Self::Dark,
            3 => Self::CatppuccinLatte,
            4 => Self::CatppuccinFrappe,
            5 => Self::CatppuccinMacchiato,
            6 => Self::CatppuccinMocha,
            _ => Self::System,
        }
    }
}

/// Colors of the Qt6 Fusion palette for one theme.
struct Palette {
    window: Color32,
    base: Color32,
    alternate_base: Color32,
    code: Color32,
    text: Color32,
    button: Color32,
    // Fill of checkboxes, slider rails and scroll handles; must contrast with `base`.
    control: Color32,
    button_hovered: Color32,
    button_pressed: Color32,
    button_open: Color32,
    highlight: Color32,
    highlighted_text: Color32,
    link: Color32,
    outline: Color32,
    separator: Color32,
    hover_tint: Color32,
    pressed_tint: Color32,
}

fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Light => Palette {
            window: Color32::from_rgb(239, 239, 239),
            base: Color32::WHITE,
            alternate_base: Color32::from_rgb(247, 247, 247),
            code: Color32::from_rgb(247, 247, 247),
            text: Color32::BLACK,
            button: Color32::from_rgb(244, 244, 244),
            control: Color32::from_rgb(208, 208, 208),
            button_hovered: Color32::from_rgb(251, 251, 251),
            button_pressed: Color32::from_rgb(218, 218, 218),
            button_open: Color32::from_rgb(226, 226, 226),
            highlight: Color32::from_rgb(48, 140, 198),
            highlighted_text: Color32::WHITE,
            link: Color32::from_rgb(0, 0, 255),
            outline: Color32::from_rgb(171, 171, 171),
            separator: Color32::from_rgb(199, 199, 199),
            hover_tint: Color32::from_rgb(222, 236, 246),
            pressed_tint: Color32::from_rgb(200, 220, 236),
        },
        Theme::Dark => Palette {
            window: Color32::from_rgb(53, 53, 53),
            base: Color32::from_rgb(35, 35, 35),
            alternate_base: Color32::from_rgb(45, 45, 45),
            code: Color32::from_rgb(45, 45, 45),
            text: Color32::WHITE,
            button: Color32::from_rgb(62, 62, 62),
            control: Color32::from_rgb(62, 62, 62),
            button_hovered: Color32::from_rgb(72, 72, 72),
            button_pressed: Color32::from_rgb(44, 44, 44),
            button_open: Color32::from_rgb(50, 50, 50),
            highlight: Color32::from_rgb(42, 130, 218),
            highlighted_text: Color32::WHITE,
            link: Color32::from_rgb(80, 160, 240),
            outline: Color32::from_rgb(28, 28, 28),
            separator: Color32::from_rgb(74, 74, 74),
            hover_tint: Color32::from_rgb(55, 70, 88),
            pressed_tint: Color32::from_rgb(45, 78, 112),
        },
    }
}

fn color([red, green, blue, alpha]: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(red, green, blue, alpha)
}

fn catppuccin_palette(theme: catppuccin_egui::Theme) -> Palette {
    Palette {
        window: color(theme.base.to_array()),
        base: color(theme.crust.to_array()),
        alternate_base: color(theme.surface0.to_array()),
        code: color(theme.mantle.to_array()),
        text: color(theme.text.to_array()),
        button: color(theme.surface0.to_array()),
        control: color(theme.surface0.to_array()),
        button_hovered: color(theme.surface2.to_array()),
        button_pressed: color(theme.surface1.to_array()),
        button_open: color(theme.surface0.to_array()),
        highlight: color(theme.blue.to_array()),
        highlighted_text: color(theme.text.to_array()),
        link: color(theme.rosewater.to_array()),
        outline: color(theme.overlay1.to_array()),
        separator: color(theme.overlay0.to_array()),
        hover_tint: color(theme.surface2.to_array()),
        pressed_tint: color(theme.surface1.to_array()),
    }
}

fn catppuccin_theme(mode: ThemeMode) -> Option<(Theme, catppuccin_egui::Theme)> {
    match mode {
        ThemeMode::CatppuccinLatte => Some((Theme::Light, catppuccin_egui::LATTE)),
        ThemeMode::CatppuccinFrappe => Some((Theme::Dark, catppuccin_egui::FRAPPE)),
        ThemeMode::CatppuccinMacchiato => Some((Theme::Dark, catppuccin_egui::MACCHIATO)),
        ThemeMode::CatppuccinMocha => Some((Theme::Dark, catppuccin_egui::MOCHA)),
        _ => None,
    }
}

fn visuals_with_palette(theme: Theme, colors: Palette) -> Visuals {
    let mut visuals = match theme {
        Theme::Light => Visuals::light(),
        Theme::Dark => Visuals::dark(),
    };
    let radius = CornerRadius::same(2);
    let text = Stroke::new(1.0, colors.text);
    let outline = Stroke::new(1.0, colors.outline);

    visuals.override_text_color = None;
    visuals.panel_fill = colors.window;
    visuals.window_fill = colors.window;
    visuals.extreme_bg_color = colors.base;
    visuals.faint_bg_color = colors.alternate_base;
    visuals.code_bg_color = colors.code;
    visuals.hyperlink_color = colors.link;
    visuals.window_stroke = outline;
    visuals.window_corner_radius = CornerRadius::same(3);
    visuals.menu_corner_radius = radius;
    visuals.window_shadow = Shadow {
        offset: [0, 3],
        blur: 10,
        spread: 0,
        color: Color32::from_black_alpha(70),
    };
    visuals.popup_shadow = visuals.window_shadow;
    visuals.selection.bg_fill = colors.highlight;
    visuals.selection.stroke = Stroke::new(1.0, colors.highlighted_text);
    visuals.slider_trailing_fill = true;

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = colors.window;
    widgets.noninteractive.weak_bg_fill = colors.window;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, colors.separator);
    widgets.noninteractive.fg_stroke = text;

    widgets.inactive.bg_fill = colors.control;
    widgets.inactive.weak_bg_fill = colors.button;
    widgets.inactive.bg_stroke = outline;
    widgets.inactive.fg_stroke = text;

    widgets.hovered.bg_fill = colors.hover_tint;
    widgets.hovered.weak_bg_fill = colors.button_hovered;
    widgets.hovered.bg_stroke = outline;
    widgets.hovered.fg_stroke = text;

    widgets.active.bg_fill = colors.pressed_tint;
    widgets.active.weak_bg_fill = colors.button_pressed;
    widgets.active.bg_stroke = Stroke::new(1.0, colors.highlight);
    widgets.active.fg_stroke = text;

    widgets.open.bg_fill = colors.button_open;
    widgets.open.weak_bg_fill = colors.button_open;
    widgets.open.bg_stroke = outline;
    widgets.open.fg_stroke = text;

    for widget in [
        &mut widgets.noninteractive,
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
        &mut widgets.open,
    ] {
        widget.corner_radius = radius;
        widget.expansion = 0.0;
    }
    visuals
}

fn visuals(theme: Theme) -> Visuals {
    visuals_with_palette(theme, palette(theme))
}

fn apply_styles(ctx: &egui::Context, mode: ThemeMode) {
    let catppuccin = catppuccin_theme(mode);
    for theme in [Theme::Light, Theme::Dark] {
        let theme_visuals = catppuccin
            .filter(|(catppuccin_base, _)| *catppuccin_base == theme)
            .map_or_else(
                || visuals(theme),
                |(_, catppuccin)| visuals_with_palette(theme, catppuccin_palette(catppuccin)),
            );
        ctx.set_visuals_of(theme, theme_visuals);
        ctx.style_mut_of(theme, |style| {
            let spacing = &mut style.spacing;
            spacing.item_spacing = vec2(6.0, 4.0);
            spacing.button_padding = vec2(8.0, 3.0);
            spacing.interact_size = vec2(40.0, 22.0);
            spacing.window_margin = Margin::same(8);
            spacing.menu_margin = Margin::same(3);

            let scroll = &mut spacing.scroll;
            *scroll = egui::style::ScrollStyle::solid();
            scroll.bar_width = 12.0;
            scroll.bar_inner_margin = 2.0;
            scroll.handle_min_length = 24.0;
            // The handle takes the widget `bg_fill` instead of the text color.
            scroll.foreground_color = false;
        });
    }
}

pub(crate) fn apply_theme(ctx: &egui::Context, mode: ThemeMode) {
    apply_styles(ctx, mode);
    let preference = match mode {
        ThemeMode::System => {
            detect_system_theme().map_or(ThemePreference::System, ThemePreference::from)
        }
        ThemeMode::Light => ThemePreference::from(Theme::Light),
        ThemeMode::CatppuccinLatte => ThemePreference::from(Theme::Light),
        ThemeMode::Dark
        | ThemeMode::CatppuccinFrappe
        | ThemeMode::CatppuccinMacchiato
        | ThemeMode::CatppuccinMocha => ThemePreference::from(Theme::Dark),
    };
    ctx.set_theme(preference);
}

/// Installs the Fusion styles and selects the requested theme preference.
///
/// Must be called before the first frame so the window never flashes in the
/// wrong theme.
pub fn install(ctx: &egui::Context, mode: ThemeMode) -> ThemeController {
    // Used on platforms where no system theme can be determined.
    ctx.options_mut(|options| options.fallback_theme = Theme::Light);
    apply_theme(ctx, mode);
    let mode_state = Arc::new(AtomicU8::new(mode.as_byte()));
    #[cfg(all(unix, not(target_os = "macos")))]
    spawn_watcher(ctx.clone(), mode_state.clone(), detect_system_theme());
    ThemeController {
        mode: mode_state,
        context: ctx.clone(),
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn detect_system_theme() -> Option<Theme> {
    desktop::detect()
}

// winit reports the system theme itself on Windows and macOS.
#[cfg(not(all(unix, not(target_os = "macos"))))]
fn detect_system_theme() -> Option<Theme> {
    None
}

#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_watcher(ctx: egui::Context, mode: Arc<AtomicU8>, initial: Option<Theme>) {
    let _ = std::thread::Builder::new()
        .name("zarg-theme".to_owned())
        .spawn(move || {
            let mut current = initial;
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                if ThemeMode::from_byte(mode.load(Ordering::Relaxed)) != ThemeMode::System {
                    continue;
                }
                if let Some(theme) = detect_system_theme()
                    && current != Some(theme)
                {
                    current = Some(theme);
                    ctx.set_theme(theme);
                    ctx.request_repaint();
                }
            }
        });
}

// Parsers are compiled everywhere so they stay unit-testable.
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
mod desktop {
    use super::Theme;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    pub fn detect() -> Option<Theme> {
        portal_theme()
            .or_else(gsettings_theme)
            .or_else(kde_theme)
            .or_else(gtk_settings_theme)
            .or_else(gtk_theme_variable)
    }

    fn command_output(program: &str, args: &[&str]) -> Option<String> {
        let output = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn portal_theme() -> Option<Theme> {
        let output = command_output(
            "gdbus",
            &[
                "call",
                "--session",
                "--timeout",
                "2",
                "--dest",
                "org.freedesktop.portal.Desktop",
                "--object-path",
                "/org/freedesktop/portal/desktop",
                "--method",
                "org.freedesktop.portal.Settings.Read",
                "org.freedesktop.appearance",
                "color-scheme",
            ],
        )?;
        parse_portal_color_scheme(&output)
    }

    fn gsettings_theme() -> Option<Theme> {
        let output = command_output(
            "gsettings",
            &["get", "org.gnome.desktop.interface", "color-scheme"],
        )?;
        parse_gsettings_color_scheme(&output)
    }

    fn kde_theme() -> Option<Theme> {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok()?;
        if !desktop.to_ascii_uppercase().contains("KDE") {
            return None;
        }
        let path: PathBuf = dirs::config_dir()?.join("kdeglobals");
        parse_kde_window_background(&std::fs::read_to_string(path).ok()?)
    }

    fn gtk_settings_theme() -> Option<Theme> {
        let path = dirs::config_dir()?.join("gtk-3.0").join("settings.ini");
        parse_gtk_settings(&std::fs::read_to_string(path).ok()?)
    }

    fn gtk_theme_variable() -> Option<Theme> {
        let name = std::env::var("GTK_THEME").ok()?;
        Some(theme_from_name(&name))
    }

    fn theme_from_name(name: &str) -> Theme {
        if name.to_ascii_lowercase().contains("dark") {
            Theme::Dark
        } else {
            Theme::Light
        }
    }

    /// Parses `gdbus` output such as `(<<uint32 1>>,)`: 1 is dark, 2 is light,
    /// and 0 means the desktop has no preference.
    pub(super) fn parse_portal_color_scheme(output: &str) -> Option<Theme> {
        let value = output.split("uint32").nth(1)?.trim_start();
        let digits: String = value.chars().take_while(char::is_ascii_digit).collect();
        match digits.as_str() {
            "1" => Some(Theme::Dark),
            "2" => Some(Theme::Light),
            _ => None,
        }
    }

    pub(super) fn parse_gsettings_color_scheme(output: &str) -> Option<Theme> {
        match output.trim().trim_matches('\'') {
            "prefer-dark" => Some(Theme::Dark),
            "prefer-light" => Some(Theme::Light),
            _ => None,
        }
    }

    /// Chooses the theme from the luminance of KDE's window background color.
    pub(super) fn parse_kde_window_background(kdeglobals: &str) -> Option<Theme> {
        let mut in_window_colors = false;
        for line in kdeglobals.lines().map(str::trim) {
            if line.starts_with('[') {
                in_window_colors = line == "[Colors:Window]";
            } else if in_window_colors && let Some(value) = line.strip_prefix("BackgroundNormal=") {
                let channels: Vec<f32> = value
                    .split(',')
                    .filter_map(|channel| channel.trim().parse().ok())
                    .collect();
                if let [red, green, blue] = channels[..] {
                    let luminance = 0.299 * red + 0.587 * green + 0.114 * blue;
                    return Some(if luminance < 128.0 {
                        Theme::Dark
                    } else {
                        Theme::Light
                    });
                }
            }
        }
        None
    }

    pub(super) fn parse_gtk_settings(settings: &str) -> Option<Theme> {
        let mut result = None;
        for line in settings.lines().map(str::trim) {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "gtk-application-prefer-dark-theme" => {
                    if value == "1" || value.eq_ignore_ascii_case("true") {
                        return Some(Theme::Dark);
                    }
                    result = Some(Theme::Light);
                }
                "gtk-theme-name" if theme_from_name(value) == Theme::Dark => {
                    return Some(Theme::Dark);
                }
                "gtk-theme-name" => result = result.or(Some(Theme::Light)),
                _ => {}
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::desktop::*;
    use super::*;

    #[test]
    fn portal_color_scheme_maps_dark_light_and_no_preference() {
        assert_eq!(
            parse_portal_color_scheme("(<<uint32 1>>,)\n"),
            Some(Theme::Dark)
        );
        assert_eq!(
            parse_portal_color_scheme("(<<uint32 2>>,)"),
            Some(Theme::Light)
        );
        assert_eq!(parse_portal_color_scheme("(<<uint32 0>>,)"), None);
        assert_eq!(parse_portal_color_scheme("unexpected"), None);
    }

    #[test]
    fn gsettings_color_scheme_is_parsed() {
        assert_eq!(
            parse_gsettings_color_scheme("'prefer-dark'\n"),
            Some(Theme::Dark)
        );
        assert_eq!(
            parse_gsettings_color_scheme("'prefer-light'"),
            Some(Theme::Light)
        );
        assert_eq!(parse_gsettings_color_scheme("'default'"), None);
    }

    #[test]
    fn kde_window_background_luminance_selects_theme() {
        let dark = "[Colors:Button]\nBackgroundNormal=250,250,250\n[Colors:Window]\nBackgroundNormal=41,44,48\n";
        let light = "[Colors:Window]\nBackgroundNormal=239, 240, 241\n";
        assert_eq!(parse_kde_window_background(dark), Some(Theme::Dark));
        assert_eq!(parse_kde_window_background(light), Some(Theme::Light));
        assert_eq!(parse_kde_window_background("[General]\n"), None);
    }

    #[test]
    fn gtk_settings_detect_dark_preference_and_theme_name() {
        assert_eq!(
            parse_gtk_settings("[Settings]\ngtk-application-prefer-dark-theme=1\n"),
            Some(Theme::Dark)
        );
        assert_eq!(
            parse_gtk_settings("[Settings]\ngtk-theme-name=Adwaita-dark\n"),
            Some(Theme::Dark)
        );
        assert_eq!(
            parse_gtk_settings("[Settings]\ngtk-theme-name=Adwaita\n"),
            Some(Theme::Light)
        );
        assert_eq!(parse_gtk_settings("[Settings]\n"), None);
    }

    #[test]
    fn fusion_visuals_follow_the_theme() {
        let light = visuals(Theme::Light);
        let dark = visuals(Theme::Dark);
        assert!(!light.dark_mode);
        assert!(dark.dark_mode);
        assert_eq!(light.panel_fill, Color32::from_rgb(239, 239, 239));
        assert_eq!(light.selection.bg_fill, Color32::from_rgb(48, 140, 198));
        assert_eq!(dark.selection.bg_fill, Color32::from_rgb(42, 130, 218));
        assert_eq!(dark.widgets.inactive.corner_radius, CornerRadius::same(2));
        assert_eq!(dark.widgets.hovered.expansion, 0.0);
    }

    #[test]
    fn catppuccin_flavors_apply_their_palette_and_color_mode() {
        for (mode, theme, base_theme) in [
            (
                ThemeMode::CatppuccinLatte,
                catppuccin_egui::LATTE,
                Theme::Light,
            ),
            (
                ThemeMode::CatppuccinFrappe,
                catppuccin_egui::FRAPPE,
                Theme::Dark,
            ),
            (
                ThemeMode::CatppuccinMacchiato,
                catppuccin_egui::MACCHIATO,
                Theme::Dark,
            ),
            (
                ThemeMode::CatppuccinMocha,
                catppuccin_egui::MOCHA,
                Theme::Dark,
            ),
        ] {
            let (selected_base, selected_theme) = catppuccin_theme(mode).unwrap();
            assert_eq!(selected_base, base_theme);
            assert_eq!(selected_theme, theme);

            let visuals = visuals_with_palette(base_theme, catppuccin_palette(theme));
            assert_eq!(visuals.dark_mode, base_theme == Theme::Dark);
            assert_eq!(visuals.panel_fill, color(theme.base.to_array()));
            assert_eq!(visuals.faint_bg_color, color(theme.surface0.to_array()));
            assert_eq!(visuals.code_bg_color, color(theme.mantle.to_array()));
            assert_eq!(visuals.selection.bg_fill, color(theme.blue.to_array()));
            assert_eq!(visuals.selection.stroke.color, color(theme.text.to_array()));
            assert_eq!(visuals.hyperlink_color, color(theme.rosewater.to_array()));

            let ctx = egui::Context::default();
            apply_theme(&ctx, mode);
            ctx.run_ui(egui::RawInput::default(), |_| {})
                .drop_without_applying_deltas();
            assert_eq!(ctx.theme(), base_theme);
            assert_eq!(
                ctx.global_style().visuals.panel_fill,
                color(theme.base.to_array())
            );
        }
    }

    #[test]
    fn install_selects_the_detected_theme_and_styles_both_themes() {
        let ctx = egui::Context::default();
        let controller = install(&ctx, ThemeMode::System);
        for theme in [Theme::Light, Theme::Dark] {
            ctx.set_theme(theme);
            ctx.run_ui(egui::RawInput::default(), |_| {})
                .drop_without_applying_deltas();
            assert_eq!(ctx.theme(), theme);
            assert_eq!(
                ctx.global_style().visuals.selection.bg_fill,
                palette(theme).highlight
            );
        }
        controller.set_mode(ThemeMode::Dark);
        assert_eq!(ctx.theme(), Theme::Dark);
        controller.set_mode(ThemeMode::Light);
        assert_eq!(ctx.theme(), Theme::Light);
    }
}
