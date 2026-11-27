#![cfg_attr(not(test), windows_subsystem = "windows")]

mod extraction_checksum;
mod password_store;
mod settings;
mod theme;
mod ui;
#[cfg(target_os = "linux")]
mod wayland_drag;
#[cfg(windows)]
mod windows_move;
#[cfg(target_os = "linux")]
mod x11_drag;

use std::ffi::OsString;
use std::path::PathBuf;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

const WINDOW_TITLE: &str = "ZAR Archive Manager";

fn main() -> eframe::Result {
    let saved = settings::Settings::load();
    saved.performance.apply();
    let mut create_default_archive = false;
    let mut app = match parse_context_args(std::env::args_os().skip(1)) {
        Ok(Some((action, paths))) => ui::ZargApp::from_context_action(action, paths, &saved)
            .unwrap_or_else(|error| {
                eprintln!("zarg: {error}");
                std::process::exit(2);
            }),
        Ok(None) => {
            let mut app = ui::ZargApp::default();
            app.apply_settings(&saved);
            create_default_archive = true;
            app
        }
        Err(error) => {
            eprintln!("zarg: {error}");
            std::process::exit(2);
        }
    };
    if create_default_archive {
        app.create_default_archive();
    }
    let icon = image::load_from_memory(include_bytes!("../assets/zarg-icon.png"))
        .expect("embedded app icon must be a valid PNG")
        .into_rgba8();
    let (width, height) = icon.dimensions();
    let (window_title, viewport) = if app.is_convert_to_zar_mode() {
        let size = [460.0, 160.0];
        let mut builder = eframe::egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_min_inner_size([400.0, 140.0])
            .with_resizable(false)
            .with_active(true)
            .with_app_id("zarg")
            .with_icon(eframe::egui::IconData {
                rgba: icon.into_raw(),
                width,
                height,
            });
        if let Some(position) = centered_extract_here_position(size, saved.ui_scale) {
            builder = builder.with_position(position);
        }
        ("Convert to ZAR", builder)
    } else if app.is_extract_to_mode() {
        let size = [520.0, 300.0];
        let mut builder = eframe::egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_min_inner_size([500.0, 280.0])
            .with_resizable(true)
            .with_active(true)
            .with_app_id("zarg")
            .with_icon(eframe::egui::IconData {
                rgba: icon.into_raw(),
                width,
                height,
            });
        if let Some(position) = centered_extract_here_position(size, saved.ui_scale) {
            builder = builder.with_position(position);
        }
        ("Extract", builder)
    } else if app.is_extract_here_mode() {
        let size = if app.has_encrypted_commits() {
            [480.0, 200.0]
        } else {
            [460.0, 160.0]
        };
        let mut builder = eframe::egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_min_inner_size([400.0, 150.0])
            .with_resizable(false)
            .with_active(true)
            .with_app_id("zarg")
            .with_icon(eframe::egui::IconData {
                rgba: icon.into_raw(),
                width,
                height,
            });
        if let Some(position) = centered_extract_here_position(size, saved.ui_scale) {
            builder = builder.with_position(position);
        }
        (
            if app.has_encrypted_commits() {
                "Password Required"
            } else {
                "Extract Archive"
            },
            builder,
        )
    } else {
        let mut viewport =
            eframe::egui::ViewportBuilder::default().with_inner_size([1200.0, 680.0]);
        if let Some(geometry) = saved.geometry_for_startup(settings::monitor_count()) {
            let geometry = startup_geometry_at_default_zoom(geometry, saved.ui_scale);
            viewport = viewport.with_inner_size(geometry.size);
            if let Some(position) = geometry.position {
                viewport = viewport.with_position(position);
            }
        }
        (
            WINDOW_TITLE,
            viewport
                .with_active(true)
                .with_min_inner_size([720.0, 460.0])
                .with_app_id("zarg")
                .with_icon(eframe::egui::IconData {
                    rgba: icon.into_raw(),
                    width,
                    height,
                }),
        )
    };
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let theme_mode = saved.theme_mode;
    let ui_scale = saved.ui_scale;
    eframe::run_native(
        window_title,
        options,
        Box::new(move |creation_context| {
            #[cfg(windows)]
            windows_move::install(creation_context);
            creation_context.egui_ctx.set_zoom_factor(ui_scale);
            let controller = theme::install(&creation_context.egui_ctx, theme_mode);
            let mut app = app;
            app.set_theme_controller(controller);
            app.load_default_password(&creation_context.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}

fn startup_geometry_at_default_zoom(
    geometry: settings::WindowGeometry,
    saved_ui_scale: f32,
) -> settings::WindowGeometry {
    settings::WindowGeometry {
        size: [
            geometry.size[0] * saved_ui_scale,
            geometry.size[1] * saved_ui_scale,
        ],
        position: geometry
            .position
            .map(|position| [position[0] * saved_ui_scale, position[1] * saved_ui_scale]),
    }
}

#[cfg(not(all(target_os = "linux", target_env = "musl")))]
fn centered_extract_here_position(size: [f32; 2], ui_scale: f32) -> Option<[f32; 2]> {
    let displays = display_info::DisplayInfo::all().ok()?;
    let display = displays
        .iter()
        .find(|d| d.is_primary)
        .or_else(|| displays.first())?;
    let scale = if display.scale_factor > 0.0 {
        display.scale_factor
    } else {
        1.0
    };
    let width_points = display.width as f32 / scale;
    let height_points = display.height as f32 / scale;
    let x_points = display.x as f32 / scale;
    let y_points = display.y as f32 / scale;
    let center_x = x_points + (width_points - size[0] * ui_scale) * 0.5;
    let center_y = y_points + (height_points - size[1] * ui_scale) * 0.5;
    Some([center_x.max(x_points), center_y.max(y_points)])
}

#[cfg(all(target_os = "linux", target_env = "musl"))]
fn centered_extract_here_position(_size: [f32; 2], _ui_scale: f32) -> Option<[f32; 2]> {
    None
}

fn parse_context_args(
    args: impl IntoIterator<Item = OsString>,
) -> Result<Option<(ui::ContextAction, Vec<PathBuf>)>, String> {
    let mut args = args.into_iter();
    let Some(flag) = args.next() else {
        return Ok(None);
    };
    if flag != "--context" {
        if flag.to_string_lossy().starts_with('-') {
            return Err("unsupported argument; expected --context ACTION [-- PATH ...]".to_owned());
        }
        let path = PathBuf::from(flag);
        if args.next().is_some() {
            return Err("opening an archive accepts one path".to_owned());
        }
        return Ok(Some((ui::ContextAction::Open, vec![path])));
    }
    let action = args
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| "missing context action".to_owned())?;
    let action = match action.as_str() {
        "create" | "create-archive" => ui::ContextAction::Create,
        "convert-to-zar" => ui::ContextAction::ConvertToZar,
        "add" => ui::ContextAction::Add,
        "create-named" => ui::ContextAction::CreateNamed,
        "open" => ui::ContextAction::Open,
        "extract-here" => ui::ContextAction::ExtractHere,
        "extract-to" => ui::ContextAction::ExtractTo,
        "extract-to-folder" => ui::ContextAction::ExtractToFolder,
        "test" => ui::ContextAction::Test,
        _ => return Err(format!("unknown context action: {action}")),
    };
    let paths = args.collect::<Vec<_>>();
    let paths = paths
        .strip_prefix(&[OsString::from("--")])
        .unwrap_or(&paths)
        .iter()
        .cloned()
        .map(PathBuf::from)
        .collect();
    Ok(Some((action, paths)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_geometry_accounts_for_saved_ui_scale() {
        let geometry = settings::WindowGeometry {
            size: [800.0, 600.0],
            position: Some([20.0, 30.0]),
        };

        assert_eq!(
            startup_geometry_at_default_zoom(geometry, 1.5),
            settings::WindowGeometry {
                size: [1200.0, 900.0],
                position: Some([30.0, 45.0]),
            }
        );
    }

    fn os_args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn context_arguments_preserve_spaces_and_multiple_paths() {
        let parsed = parse_context_args(os_args(&[
            "--context",
            "create",
            "--",
            "/tmp/first file",
            "/tmp/second folder",
        ]))
        .unwrap();

        assert_eq!(
            parsed,
            Some((
                ui::ContextAction::Create,
                vec![
                    PathBuf::from("/tmp/first file"),
                    PathBuf::from("/tmp/second folder")
                ]
            ))
        );

        let parsed_alias = parse_context_args(os_args(&[
            "--context",
            "create-archive",
            "--",
            "/tmp/first file",
        ]))
        .unwrap();
        assert_eq!(
            parsed_alias,
            Some((
                ui::ContextAction::Create,
                vec![PathBuf::from("/tmp/first file")]
            ))
        );

        let parsed_convert = parse_context_args(os_args(&[
            "--context",
            "convert-to-zar",
            "--",
            "/tmp/first archive.zip",
        ]))
        .unwrap();
        assert_eq!(
            parsed_convert,
            Some((
                ui::ContextAction::ConvertToZar,
                vec![PathBuf::from("/tmp/first archive.zip")]
            ))
        );
    }

    #[test]
    fn no_arguments_keeps_normal_gui_startup() {
        assert!(
            parse_context_args(Vec::<OsString>::new())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn archive_file_argument_opens_that_archive() {
        assert_eq!(
            parse_context_args(os_args(&["/tmp/archive with spaces.zar"])).unwrap(),
            Some((
                ui::ContextAction::Open,
                vec![PathBuf::from("/tmp/archive with spaces.zar")]
            ))
        );
    }

    #[test]
    fn extract_to_folder_context_action_is_recognized() {
        assert_eq!(
            parse_context_args(os_args(&[
                "--context",
                "extract-to-folder",
                "--",
                "/tmp/foo.zar",
            ]))
            .unwrap(),
            Some((
                ui::ContextAction::ExtractToFolder,
                vec![PathBuf::from("/tmp/foo.zar")]
            ))
        );
    }

    #[test]
    fn unknown_context_action_is_rejected() {
        assert!(parse_context_args(os_args(&["--context", "unknown"])).is_err());
    }

    #[test]
    fn centered_extract_here_position_returns_position_if_displays_available() {
        let size = [460.0, 160.0];
        let pos = centered_extract_here_position(size, 1.0);
        #[cfg(not(all(target_os = "linux", target_env = "musl")))]
        if display_info::DisplayInfo::all().is_ok() {
            assert!(pos.is_some());
        }
    }
}
