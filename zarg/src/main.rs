#![windows_subsystem = "windows"]

mod file_dialog;
mod settings;
mod ui;

use std::ffi::OsString;
use std::path::PathBuf;

fn main() -> eframe::Result {
    let saved = settings::Settings::load();
    let mut app = match parse_context_args(std::env::args_os().skip(1)) {
        Ok(Some((action, paths))) => ui::ZargApp::from_context_action(action, paths)
            .unwrap_or_else(|error| {
                eprintln!("zarg: {error}");
                std::process::exit(2);
            }),
        Ok(None) => ui::ZargApp::default(),
        Err(error) => {
            eprintln!("zarg: {error}");
            std::process::exit(2);
        }
    };
    app.apply_settings(&saved);
    let icon = image::load_from_memory(include_bytes!("../assets/zarg-icon.png"))
        .expect("embedded app icon must be a valid PNG")
        .into_rgba8();
    let (width, height) = icon.dimensions();
    let mut viewport = eframe::egui::ViewportBuilder::default().with_inner_size([1000.0, 680.0]);
    if let Some(geometry) = saved.geometry_for_startup(settings::monitor_count()) {
        viewport = viewport.with_inner_size(geometry.size);
        if let Some(position) = geometry.position {
            viewport = viewport.with_position(position);
        }
    }
    let options = eframe::NativeOptions {
        viewport: viewport
            .with_min_inner_size([720.0, 460.0])
            .with_app_id("zarg")
            .with_icon(eframe::egui::IconData {
                rgba: icon.into_raw(),
                width,
                height,
            }),
        ..Default::default()
    };
    eframe::run_native(
        "ZAR Archive Manager",
        options,
        Box::new(move |_creation_context| Ok(Box::new(app))),
    )
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
        "create" => ui::ContextAction::Create,
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
}
