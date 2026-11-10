use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowGeometry {
    pub size: [f32; 2],
    pub position: Option<[f32; 2]>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IconTheme {
    #[default]
    Crystal,
    Flat,
}

impl IconTheme {
    pub fn label(self) -> &'static str {
        match self {
            Self::Crystal => "Crystal",
            Self::Flat => "Flat",
        }
    }

    fn as_setting(self) -> &'static str {
        match self {
            Self::Crystal => "crystal",
            Self::Flat => "flat",
        }
    }

    fn from_setting(value: &str) -> Option<Self> {
        match value {
            "crystal" => Some(Self::Crystal),
            "flat" => Some(Self::Flat),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DefaultEncryptionMethod {
    #[default]
    Aes256Gcm,
    ChaCha20Poly1305,
}

impl DefaultEncryptionMethod {
    pub fn label(self) -> &'static str {
        match self {
            Self::Aes256Gcm => "AES-256-GCM",
            Self::ChaCha20Poly1305 => "ChaCha20-Poly1305",
        }
    }

    fn as_setting(self) -> &'static str {
        match self {
            Self::Aes256Gcm => "aes-256-gcm",
            Self::ChaCha20Poly1305 => "chacha20-poly1305",
        }
    }

    fn from_setting(value: &str) -> Option<Self> {
        match value {
            "aes-256-gcm" => Some(Self::Aes256Gcm),
            "chacha20-poly1305" => Some(Self::ChaCha20Poly1305),
            _ => None,
        }
    }
}

impl ThemeMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn as_setting(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn from_setting(value: &str) -> Option<Self> {
        match value {
            "system" => Some(Self::System),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub window: Option<WindowGeometry>,
    pub dialog_sizes: BTreeMap<String, [f32; 2]>,
    pub monitor_count: usize,
    pub view: String,
    pub sort: String,
    pub sort_reversed: bool,
    pub ui_scale: f32,
    pub theme_mode: ThemeMode,
    pub icon_theme: IconTheme,
    pub default_block_size: String,
    pub default_compression_level: i32,
    pub default_encryption_method: DefaultEncryptionMethod,
    pub archive_defaults: ArchiveDefaults,
    pub column_widths: BTreeMap<String, f32>,
    pub recent_archives: Vec<String>,
    pub bookmarks: Vec<PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArchiveDefaults {
    pub metadata: bool,
    pub dates: bool,
    pub special_files: bool,
    pub strict: bool,
    pub save_user: bool,
    pub include_files: String,
    pub exclude_files: String,
    pub include_folders: String,
    pub exclude_folders: String,
}

pub const BLOCK_SIZE_OPTIONS: [&str; 6] = ["1M", "2M", "4M", "8M", "16M", "32M"];

pub const UI_SCALE_OPTIONS: [(f32, &str); 5] = [
    (1.0, "1.0"),
    (1.25, "1.25"),
    (1.5, "1.5"),
    (1.75, "1.75"),
    (2.0, "2.0"),
];

impl Default for Settings {
    fn default() -> Self {
        Self {
            window: None,
            dialog_sizes: BTreeMap::new(),
            monitor_count: 0,
            view: String::new(),
            sort: String::new(),
            sort_reversed: false,
            ui_scale: 1.0,
            theme_mode: ThemeMode::System,
            icon_theme: IconTheme::Crystal,
            default_block_size: "8M".to_owned(),
            default_compression_level: 3,
            default_encryption_method: DefaultEncryptionMethod::Aes256Gcm,
            archive_defaults: ArchiveDefaults::default(),
            column_widths: BTreeMap::new(),
            recent_archives: Vec::new(),
            bookmarks: Vec::new(),
        }
    }
}

#[cfg(not(all(target_os = "linux", target_env = "musl")))]
pub fn monitor_count() -> usize {
    display_info::DisplayInfo::all().map_or(0, |displays| displays.len())
}

#[cfg(all(target_os = "linux", target_env = "musl"))]
pub fn monitor_count() -> usize {
    0
}

fn settings_path() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    }?;
    Some(base.join("zarg").join("settings.conf"))
}

impl Settings {
    pub fn load() -> Self {
        settings_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = settings_path().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "configuration directory unavailable",
            )
        })?;
        self.save_to(&path)
    }

    fn save_to(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.render())
    }

    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        let mut size: Option<[f32; 2]> = None;
        let mut position: [Option<f32>; 2] = [None, None];
        let mut dialog_sizes: BTreeMap<String, [Option<f32>; 2]> = BTreeMap::new();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            match key {
                "window.width" => {
                    size = Some([value.parse().unwrap_or(0.0), size.map_or(0.0, |s| s[1])])
                }
                "window.height" => {
                    size = Some([size.map_or(0.0, |s| s[0]), value.parse().unwrap_or(0.0)])
                }
                "window.x" => position[0] = value.parse().ok(),
                "window.y" => position[1] = value.parse().ok(),
                "monitors" => settings.monitor_count = value.parse().unwrap_or(0),
                "recent" if !value.is_empty() => settings.recent_archives.push(value.to_owned()),
                "bookmark" if !value.is_empty() => {
                    settings.bookmarks.push(PathBuf::from(value));
                }
                "view" => settings.view = value.to_owned(),
                "sort" => settings.sort = value.to_owned(),
                "sort_reversed" => settings.sort_reversed = value == "true",
                "ui_scale" => {
                    if let Ok(scale) = value.parse::<f32>()
                        && UI_SCALE_OPTIONS.iter().any(|(option, _)| *option == scale)
                    {
                        settings.ui_scale = scale;
                    }
                }
                "large_controls" => {
                    settings.ui_scale = if value == "true" { 1.25 } else { 1.0 };
                }
                "theme" => {
                    if let Some(theme_mode) = ThemeMode::from_setting(value) {
                        settings.theme_mode = theme_mode;
                    }
                }
                "icon_theme" => {
                    if let Some(icon_theme) = IconTheme::from_setting(value) {
                        settings.icon_theme = icon_theme;
                    }
                }
                "default_block_size" if BLOCK_SIZE_OPTIONS.contains(&value) => {
                    settings.default_block_size = value.to_owned();
                }
                "default_compression_level" => {
                    if let Ok(level) = value.parse::<i32>()
                        && (-7..=22).contains(&level)
                    {
                        settings.default_compression_level = level;
                    }
                }
                "default_encryption_method" => {
                    if let Some(method) = DefaultEncryptionMethod::from_setting(value) {
                        settings.default_encryption_method = method;
                    }
                }
                "default_metadata" => settings.archive_defaults.metadata = value == "true",
                "default_dates" => settings.archive_defaults.dates = value == "true",
                "default_special_files" => {
                    settings.archive_defaults.special_files = value == "true"
                }
                "default_strict" => settings.archive_defaults.strict = value == "true",
                "default_save_user" => settings.archive_defaults.save_user = value == "true",
                "default_include_file" => {
                    Self::append_filter(&mut settings.archive_defaults.include_files, value)
                }
                "default_exclude_file" => {
                    Self::append_filter(&mut settings.archive_defaults.exclude_files, value)
                }
                "default_include_folder" => {
                    Self::append_filter(&mut settings.archive_defaults.include_folders, value)
                }
                "default_exclude_folder" => {
                    Self::append_filter(&mut settings.archive_defaults.exclude_folders, value)
                }
                _ => {
                    if let Some((dialog, dimension)) = key
                        .strip_prefix("dialog.")
                        .and_then(|key| key.rsplit_once('.'))
                    {
                        let index = match dimension {
                            "width" => Some(0),
                            "height" => Some(1),
                            _ => None,
                        };
                        if let Some(index) = index
                            && let Ok(value) = value.parse::<f32>()
                            && value.is_finite()
                            && value >= 100.0
                        {
                            dialog_sizes
                                .entry(dialog.to_owned())
                                .or_insert([None, None])[index] = Some(value);
                        }
                    } else if let Some(column) = key.strip_prefix("column.")
                        && let Ok(width) = value.parse::<f32>()
                        && width.is_finite()
                        && width > 0.0
                    {
                        settings.column_widths.insert(column.to_owned(), width);
                    }
                }
            }
        }
        settings.dialog_sizes = dialog_sizes
            .into_iter()
            .filter_map(|(dialog, size)| Some((dialog, [size[0]?, size[1]?])))
            .collect();
        settings.window = size
            .filter(|size| {
                size.iter()
                    .all(|value| value.is_finite() && *value >= 100.0)
            })
            .map(|size| WindowGeometry {
                size,
                position: position[0]
                    .zip(position[1])
                    .filter(|(x, y)| x.is_finite() && y.is_finite())
                    .map(|(x, y)| [x, y]),
            });
        settings
    }

    pub fn render(&self) -> String {
        let mut text = String::new();
        if let Some(window) = &self.window {
            text.push_str(&format!(
                "window.width={}\nwindow.height={}\n",
                window.size[0], window.size[1]
            ));
            if let Some([x, y]) = window.position {
                text.push_str(&format!("window.x={x}\nwindow.y={y}\n"));
            }
        }
        for (dialog, size) in &self.dialog_sizes {
            text.push_str(&format!(
                "dialog.{dialog}.width={}\ndialog.{dialog}.height={}\n",
                size[0], size[1]
            ));
        }
        text.push_str(&format!("monitors={}\n", self.monitor_count));
        text.push_str(&format!("view={}\nsort={}\n", self.view, self.sort));
        text.push_str(&format!("sort_reversed={}\n", self.sort_reversed));
        text.push_str(&format!("ui_scale={}\n", self.ui_scale));
        text.push_str(&format!("theme={}\n", self.theme_mode.as_setting()));
        text.push_str(&format!("icon_theme={}\n", self.icon_theme.as_setting()));
        text.push_str(&format!("default_block_size={}\n", self.default_block_size));
        text.push_str(&format!(
            "default_compression_level={}\n",
            self.default_compression_level
        ));
        text.push_str(&format!(
            "default_encryption_method={}\n",
            self.default_encryption_method.as_setting()
        ));
        let defaults = &self.archive_defaults;
        text.push_str(&format!(
            "default_metadata={}\ndefault_dates={}\ndefault_special_files={}\ndefault_strict={}\ndefault_save_user={}\n",
            defaults.metadata, defaults.dates, defaults.special_files, defaults.strict, defaults.save_user
        ));
        for (key, patterns) in [
            ("default_include_file", &defaults.include_files),
            ("default_exclude_file", &defaults.exclude_files),
            ("default_include_folder", &defaults.include_folders),
            ("default_exclude_folder", &defaults.exclude_folders),
        ] {
            for pattern in patterns
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
            {
                text.push_str(&format!("{key}={pattern}\n"));
            }
        }
        for archive in &self.recent_archives {
            text.push_str(&format!("recent={archive}\n"));
        }
        for bookmark in &self.bookmarks {
            text.push_str(&format!("bookmark={}\n", bookmark.display()));
        }
        for (column, width) in &self.column_widths {
            text.push_str(&format!("column.{column}={width}\n"));
        }
        text
    }

    fn append_filter(target: &mut String, value: &str) {
        if !value.is_empty() {
            if !target.is_empty() {
                target.push('\n');
            }
            target.push_str(value);
        }
    }

    /// Keep the saved size even when the previous position may no longer be visible.
    pub fn geometry_for_startup(&self, current_monitors: usize) -> Option<WindowGeometry> {
        self.window.map(|mut geometry| {
            if current_monitors == 0 || self.monitor_count != current_monitors {
                geometry.position = None;
            }
            geometry
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Settings {
        Settings {
            window: Some(WindowGeometry {
                size: [1100.0, 700.0],
                position: Some([40.0, 25.0]),
            }),
            dialog_sizes: BTreeMap::from([
                ("benchmark".to_owned(), [560.0, 220.0]),
                ("operation-create".to_owned(), [568.0, 332.0]),
            ]),
            monitor_count: 2,
            view: "details".to_owned(),
            sort: "date".to_owned(),
            sort_reversed: true,
            ui_scale: 1.75,
            theme_mode: ThemeMode::Dark,
            icon_theme: IconTheme::Flat,
            default_block_size: "16M".to_owned(),
            default_compression_level: 12,
            default_encryption_method: DefaultEncryptionMethod::ChaCha20Poly1305,
            archive_defaults: ArchiveDefaults {
                metadata: true,
                dates: true,
                special_files: true,
                strict: true,
                save_user: true,
                include_files: "*.rs\nname=*.txt".to_owned(),
                exclude_files: "*.tmp".to_owned(),
                include_folders: "src/**".to_owned(),
                exclude_folders: "target/**".to_owned(),
            },
            column_widths: BTreeMap::from([("details:Name".to_owned(), 321.5)]),
            recent_archives: vec!["/a/one.zar".to_owned(), "/b/two.zar".to_owned()],
            bookmarks: vec![
                PathBuf::from("/projects"),
                PathBuf::from("/data/archive tools"),
            ],
        }
    }

    #[test]
    fn settings_round_trip() {
        let settings = sample();
        assert_eq!(Settings::parse(&settings.render()), settings);
    }

    #[test]
    fn settings_save_to_disk_and_report_failures() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config").join("settings.conf");
        let settings = sample();
        settings.save_to(&path).unwrap();
        assert_eq!(
            Settings::parse(&std::fs::read_to_string(path).unwrap()),
            settings
        );
        assert!(settings.save_to(directory.path()).is_err());
    }

    #[test]
    fn ui_scale_defaults_to_one_and_loads_supported_values() {
        assert_eq!(Settings::default().ui_scale, 1.0);
        for (scale, _) in UI_SCALE_OPTIONS {
            assert_eq!(
                Settings::parse(&format!("ui_scale={scale}\n")).ui_scale,
                scale
            );
        }
        assert_eq!(Settings::parse("ui_scale=1.3\n").ui_scale, 1.0);
    }

    #[test]
    fn ui_scale_migrates_legacy_large_controls_setting() {
        assert_eq!(Settings::parse("large_controls=true\n").ui_scale, 1.25);
        assert_eq!(Settings::parse("large_controls=false\n").ui_scale, 1.0);
    }

    #[test]
    fn theme_mode_defaults_to_system_and_loads_supported_values() {
        assert_eq!(Settings::default().theme_mode, ThemeMode::System);
        for (setting, mode) in [
            ("system", ThemeMode::System),
            ("light", ThemeMode::Light),
            ("dark", ThemeMode::Dark),
        ] {
            assert_eq!(
                Settings::parse(&format!("theme={setting}\n")).theme_mode,
                mode
            );
        }

        assert_eq!(
            Settings::parse("theme=unknown\n").theme_mode,
            ThemeMode::System
        );
    }

    #[test]
    fn icon_theme_defaults_to_crystal_and_round_trips() {
        assert_eq!(Settings::default().icon_theme, IconTheme::Crystal);
        for (value, theme) in [("crystal", IconTheme::Crystal), ("flat", IconTheme::Flat)] {
            let settings = Settings::parse(&format!("icon_theme={value}\n"));
            assert_eq!(settings.icon_theme, theme);
            assert!(settings.render().contains(&format!("icon_theme={value}\n")));
        }
        assert_eq!(
            Settings::parse("icon_theme=unknown\n").icon_theme,
            IconTheme::Crystal
        );
    }

    #[test]
    fn operation_defaults_are_validated_and_persisted() {
        let defaults = Settings::default();
        assert_eq!(defaults.archive_defaults, ArchiveDefaults::default());
        assert_eq!(
            Settings::parse("default_block_size=16M\n").archive_defaults,
            ArchiveDefaults::default()
        );
        assert_eq!(defaults.default_block_size, "8M");
        assert_eq!(defaults.default_compression_level, 3);
        assert_eq!(
            defaults.default_encryption_method,
            DefaultEncryptionMethod::Aes256Gcm
        );

        let parsed = Settings::parse(
            "default_block_size=16M\ndefault_compression_level=-7\ndefault_encryption_method=chacha20-poly1305\n",
        );
        assert_eq!(parsed.default_block_size, "16M");
        assert_eq!(parsed.default_compression_level, -7);
        assert_eq!(
            parsed.default_encryption_method,
            DefaultEncryptionMethod::ChaCha20Poly1305
        );
        let parsed = Settings::parse(
            "default_block_size=3M\ndefault_compression_level=23\ndefault_encryption_method=unknown\n",
        );
        assert_eq!(parsed.default_block_size, "8M");
        assert_eq!(parsed.default_compression_level, 3);
        assert_eq!(
            parsed.default_encryption_method,
            DefaultEncryptionMethod::Aes256Gcm
        );

        let settings = Settings {
            default_block_size: "4M".to_owned(),
            default_compression_level: 7,
            default_encryption_method: DefaultEncryptionMethod::ChaCha20Poly1305,
            ..Settings::default()
        };
        assert_eq!(Settings::parse(&settings.render()), settings);
    }

    #[test]
    fn geometry_preserves_size_but_requires_matching_monitor_count_for_position() {
        let settings = sample();
        assert_eq!(settings.geometry_for_startup(2), settings.window);
        for count in [0, 1, 3] {
            assert_eq!(
                settings.geometry_for_startup(count),
                Some(WindowGeometry {
                    size: [1100.0, 700.0],
                    position: None
                })
            );
        }
        assert_eq!(Settings::default().geometry_for_startup(1), None);
    }

    #[test]
    fn garbage_is_ignored() {
        let settings = Settings::parse("nonsense\nwindow.width=abc\ncolumn.x=-3\n");
        assert!(settings.window.is_none());
        assert!(settings.column_widths.is_empty());
        assert!(
            Settings::parse("window.width=inf\nwindow.height=500\n")
                .window
                .is_none()
        );
        assert_eq!(
            Settings::parse("window.width=800\nwindow.height=600\nwindow.x=NaN\nwindow.y=1\n")
                .window
                .unwrap()
                .position,
            None
        );
    }
}
