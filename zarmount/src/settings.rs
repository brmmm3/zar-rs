//! Persistent settings for zarmount.
//!
//! Stores user preferences such as the preferred file manager (File Explorer or
//! FreeCommander) and GUI theme, stored in `%APPDATA%\zarmount\settings.conf`.

use std::fs;
use std::path::PathBuf;

use crate::dialog_style::IconTheme;
use crate::open::is_freecommander_available;
use crate::theme::ThemeMode;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileManager {
    #[default]
    FileExplorer,
    FreeCommander,
}

impl FileManager {
    pub const ALL: [Self; 2] = [Self::FileExplorer, Self::FreeCommander];

    pub fn label(self) -> &'static str {
        match self {
            Self::FileExplorer => "File Explorer",
            Self::FreeCommander => "FreeCommander",
        }
    }

    pub fn as_setting(self) -> &'static str {
        match self {
            Self::FileExplorer => "explorer",
            Self::FreeCommander => "freecommander",
        }
    }

    pub fn from_setting(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "explorer" | "file explorer" | "file-explorer" => Some(Self::FileExplorer),
            "freecommander" | "free-commander" | "fc" => Some(Self::FreeCommander),
            _ => None,
        }
    }

    pub fn is_available(self) -> bool {
        match self {
            Self::FileExplorer => true,
            Self::FreeCommander => is_freecommander_available(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub file_manager: FileManager,
    pub theme_mode: ThemeMode,
    pub icon_theme: IconTheme,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            file_manager: FileManager::FileExplorer,
            theme_mode: ThemeMode::System,
            icon_theme: IconTheme::Crystal,
        }
    }
}

pub fn settings_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("zarmount").join("settings.conf"))
}

pub fn zarg_settings_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("zarg").join("settings.conf"))
}

impl Settings {
    pub fn load() -> Self {
        if let Some(path) = settings_path()
            && let Ok(text) = fs::read_to_string(&path)
        {
            let mut settings = Self::parse(&text);
            if !text.lines().any(|line| {
                line.split_once('=')
                    .is_some_and(|(key, _)| key.trim() == "icon_theme")
            }) && let Some(zarg_path) = zarg_settings_path()
                && let Ok(zarg_text) = fs::read_to_string(zarg_path)
            {
                settings.icon_theme = Self::parse(&zarg_text).icon_theme;
            }
            return settings;
        }
        // Fall back to reading theme from zarg settings if available
        let mut settings = Self::default();
        if let Some(zarg_path) = zarg_settings_path()
            && let Ok(text) = fs::read_to_string(zarg_path)
        {
            for line in text.lines() {
                let trimmed = line.trim();
                if let Some((key, val)) = trimmed.split_once('=')
                    && key.trim() == "theme"
                    && let Some(mode) = ThemeMode::from_setting(val.trim())
                {
                    settings.theme_mode = mode;
                }
                if let Some((key, val)) = trimmed.split_once('=')
                    && key.trim() == "icon_theme"
                    && let Some(icon_theme) = IconTheme::from_setting(val)
                {
                    settings.icon_theme = icon_theme;
                }
            }
        }
        settings
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = settings_path().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "configuration directory unavailable",
            )
        })?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, self.render())
    }

    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some((key, val)) = trimmed.split_once('=') {
                let key = key.trim();
                let val = val.trim();
                match key {
                    "file_manager" => {
                        if let Some(fm) = FileManager::from_setting(val) {
                            settings.file_manager = fm;
                        }
                    }
                    "theme" => {
                        if let Some(mode) = ThemeMode::from_setting(val) {
                            settings.theme_mode = mode;
                        }
                    }
                    "icon_theme" => {
                        if let Some(icon_theme) = IconTheme::from_setting(val) {
                            settings.icon_theme = icon_theme;
                        }
                    }
                    _ => {}
                }
            }
        }
        settings
    }

    pub fn render(&self) -> String {
        format!(
            "file_manager={}\ntheme={}\nicon_theme={}\n",
            self.file_manager.as_setting(),
            self.theme_mode.as_setting(),
            self.icon_theme.as_setting()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_round_trip() {
        let default_settings = Settings::default();
        assert_eq!(default_settings.file_manager, FileManager::FileExplorer);
        assert_eq!(default_settings.theme_mode, ThemeMode::System);

        let rendered = default_settings.render();
        let parsed = Settings::parse(&rendered);
        assert_eq!(default_settings, parsed);
    }

    #[test]
    fn parse_custom_settings() {
        let text = "file_manager=freecommander\ntheme=dark\nicon_theme=flat\n";
        let parsed = Settings::parse(text);
        assert_eq!(parsed.file_manager, FileManager::FreeCommander);
        assert_eq!(parsed.theme_mode, ThemeMode::Dark);
        assert_eq!(parsed.icon_theme, IconTheme::Flat);
        assert_eq!(Settings::parse(&parsed.render()), parsed);
    }

    #[test]
    fn parse_file_manager_aliases() {
        assert_eq!(
            FileManager::from_setting("explorer"),
            Some(FileManager::FileExplorer)
        );
        assert_eq!(
            FileManager::from_setting("freecommander"),
            Some(FileManager::FreeCommander)
        );
        assert_eq!(
            FileManager::from_setting("fc"),
            Some(FileManager::FreeCommander)
        );
        assert_eq!(FileManager::from_setting("unknown"), None);
    }
}
