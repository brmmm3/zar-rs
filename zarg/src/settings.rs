use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowGeometry {
    pub size: [f32; 2],
    pub position: Option<[f32; 2]>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settings {
    pub window: Option<WindowGeometry>,
    pub monitor_count: usize,
    pub view: String,
    pub sort: String,
    pub sort_reversed: bool,
    pub column_widths: BTreeMap<String, f32>,
    pub recent_archives: Vec<String>,
}

pub fn monitor_count() -> usize {
    display_info::DisplayInfo::all().map_or(0, |displays| displays.len())
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

    pub fn save(&self) {
        let Some(path) = settings_path() else {
            return;
        };
        if let Some(parent) = path.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            return;
        }
        let _ = std::fs::write(path, self.render());
    }

    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        let mut size: Option<[f32; 2]> = None;
        let mut position: [Option<f32>; 2] = [None, None];
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
                "view" => settings.view = value.to_owned(),
                "sort" => settings.sort = value.to_owned(),
                "sort_reversed" => settings.sort_reversed = value == "true",
                _ => {
                    if let Some(column) = key.strip_prefix("column.")
                        && let Ok(width) = value.parse::<f32>()
                        && width.is_finite()
                        && width > 0.0
                    {
                        settings.column_widths.insert(column.to_owned(), width);
                    }
                }
            }
        }
        settings.window = size
            .filter(|size| size[0] >= 100.0 && size[1] >= 100.0)
            .map(|size| WindowGeometry {
                size,
                position: position[0].zip(position[1]).map(|(x, y)| [x, y]),
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
        text.push_str(&format!("monitors={}\n", self.monitor_count));
        text.push_str(&format!("view={}\nsort={}\n", self.view, self.sort));
        text.push_str(&format!("sort_reversed={}\n", self.sort_reversed));
        for archive in &self.recent_archives {
            text.push_str(&format!("recent={archive}\n"));
        }
        for (column, width) in &self.column_widths {
            text.push_str(&format!("column.{column}={width}\n"));
        }
        text
    }

    /// Saved window geometry is only reused when the monitor setup is unchanged.
    pub fn geometry_for_startup(&self, current_monitors: usize) -> Option<WindowGeometry> {
        (current_monitors > 0 && self.monitor_count == current_monitors)
            .then_some(self.window)
            .flatten()
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
            monitor_count: 2,
            view: "details".to_owned(),
            sort: "date".to_owned(),
            sort_reversed: true,
            column_widths: BTreeMap::from([("details:Name".to_owned(), 321.5)]),
            recent_archives: vec!["/a/one.zar".to_owned(), "/b/two.zar".to_owned()],
        }
    }

    #[test]
    fn settings_round_trip() {
        let settings = sample();
        assert_eq!(Settings::parse(&settings.render()), settings);
    }

    #[test]
    fn geometry_requires_matching_monitor_count() {
        let settings = sample();
        assert!(settings.geometry_for_startup(2).is_some());
        assert!(settings.geometry_for_startup(1).is_none());
        assert!(settings.geometry_for_startup(0).is_none());
    }

    #[test]
    fn garbage_is_ignored() {
        let settings = Settings::parse("nonsense\nwindow.width=abc\ncolumn.x=-3\n");
        assert!(settings.window.is_none());
        assert!(settings.column_widths.is_empty());
    }
}
