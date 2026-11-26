//! Qt6 Fusion and Catppuccin themes for zarmount dialogs, matching `zarg`.
//!
//! Provides the exact same theme options as `zarg` (zarlib-gui):
//! System, Light, Dark, Catppuccin Latte, Catppuccin Frappe, Catppuccin Macchiato, Catppuccin Mocha.

use eframe::egui::{
    self, Color32, CornerRadius, Margin, Shadow, Stroke, Theme, ThemePreference, Visuals, vec2,
};

/// The theme mode to apply (same options as `zarg`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
    CatppuccinLatte,
    CatppuccinFrappe,
    CatppuccinMacchiato,
    CatppuccinMocha,
}

impl ThemeMode {
    pub const ALL: [Self; 7] = [
        Self::System,
        Self::Light,
        Self::Dark,
        Self::CatppuccinLatte,
        Self::CatppuccinFrappe,
        Self::CatppuccinMacchiato,
        Self::CatppuccinMocha,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::CatppuccinLatte => "Catppuccin Latte",
            Self::CatppuccinFrappe => "Catppuccin Frappe",
            Self::CatppuccinMacchiato => "Catppuccin Macchiato",
            Self::CatppuccinMocha => "Catppuccin Mocha",
        }
    }

    pub fn as_setting(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
            Self::CatppuccinLatte => "catppuccin-latte",
            Self::CatppuccinFrappe => "catppuccin-frappe",
            Self::CatppuccinMacchiato => "catppuccin-macchiato",
            Self::CatppuccinMocha => "catppuccin-mocha",
        }
    }

    pub fn from_setting(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "system" => Some(Self::System),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            "catppuccin-latte" | "latte" => Some(Self::CatppuccinLatte),
            "catppuccin-frappe" | "frappe" => Some(Self::CatppuccinFrappe),
            "catppuccin-macchiato" | "macchiato" => Some(Self::CatppuccinMacchiato),
            "catppuccin-mocha" | "mocha" => Some(Self::CatppuccinMocha),
            _ => None,
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

pub fn apply_theme(ctx: &egui::Context, mode: ThemeMode) {
    apply_styles(ctx, mode);
    let preference = match mode {
        ThemeMode::System => ThemePreference::System,
        ThemeMode::Light | ThemeMode::CatppuccinLatte => ThemePreference::from(Theme::Light),
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
pub fn install(ctx: &egui::Context, mode: ThemeMode) {
    // Used on platforms where no system theme can be determined.
    ctx.options_mut(|options| options.fallback_theme = Theme::Light);
    apply_theme(ctx, mode);
}
