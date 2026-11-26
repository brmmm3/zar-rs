#![cfg_attr(all(target_os = "windows", not(test)), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
mod mount;

#[cfg(target_os = "linux")]
mod linux_daemon;

#[cfg(target_os = "linux")]
mod linux_dialog;

// Keep re-execution working when a running binary is replaced by a rebuild.
#[cfg(target_os = "linux")]
const SELF_EXECUTABLE: &str = "/proc/self/exe";

#[cfg(target_os = "windows")]
mod mount_windows;

#[cfg(target_os = "windows")]
mod elevate;

#[cfg(target_os = "windows")]
mod pipe;

#[cfg(target_os = "windows")]
mod drive;

#[cfg(target_os = "windows")]
mod open;

#[cfg(target_os = "windows")]
mod mounter;

#[cfg(target_os = "windows")]
mod tray;

#[cfg(target_os = "windows")]
mod daemon;

#[cfg(target_os = "windows")]
mod dialog;

#[cfg(any(target_os = "windows", target_os = "linux"))]
mod theme;

#[cfg(any(target_os = "windows", target_os = "linux"))]
mod dialog_style;

#[cfg(target_os = "windows")]
mod settings;

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    linux_daemon::main()
}

#[cfg(target_os = "windows")]
fn main() -> std::process::ExitCode {
    use std::os::windows::process::CommandExt;
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Internal modes (invoked by the daemon or the client).
    if args.iter().any(|a| a == "--enable-projfs") {
        unsafe {
            let _ = windows::Win32::System::Console::FreeConsole();
        }
        let status = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Enable-WindowsOptionalFeature -Online -FeatureName Client-ProjFS -NoRestart",
            ])
            .creation_flags(0x08000000)
            .status();
        return match status {
            Ok(s) if s.success() => std::process::ExitCode::SUCCESS,
            _ => std::process::ExitCode::FAILURE,
        };
    }
    if args.iter().any(|a| a == "--daemon") {
        unsafe {
            let _ = windows::Win32::System::Console::FreeConsole();
        }
        return daemon::run();
    }
    if let Some(pos) = args.iter().position(|a| a == "--worker") {
        // --worker <pipe_name>
        unsafe {
            let _ = windows::Win32::System::Console::FreeConsole();
        }
        if args.len() >= pos + 2 {
            return mounter::run_worker(&args[pos + 1]);
        }
        eprintln!("zarmount: --worker requires <pipe_name>");
        return std::process::ExitCode::FAILURE;
    }
    if let Some(pos) = args.iter().position(|a| a == "--ask-dialog") {
        // --ask-dialog <temp_path> <out_file>
        if args.len() >= pos + 3 {
            return dialog::run_ask_dialog(
                std::path::Path::new(&args[pos + 1]),
                std::path::Path::new(&args[pos + 2]),
            );
        }
        eprintln!("zarmount: --ask-dialog requires <temp_path> <out_file>");
        return std::process::ExitCode::FAILURE;
    }
    if let Some(pos) = args.iter().position(|a| a == "--projfs-dialog") {
        // --projfs-dialog <out_file>
        if args.len() >= pos + 2 {
            return dialog::run_projfs_dialog(std::path::Path::new(&args[pos + 1]));
        }
        eprintln!("zarmount: --projfs-dialog requires <out_file>");
        return std::process::ExitCode::FAILURE;
    }
    if let Some(pos) = args.iter().position(|a| a == "--map-drive-dialog") {
        // --map-drive-dialog <out_file>
        if args.len() >= pos + 2 {
            return dialog::run_map_drive_dialog(std::path::Path::new(&args[pos + 1]));
        }
        eprintln!("zarmount: --map-drive-dialog requires <out_file>");
        return std::process::ExitCode::FAILURE;
    }
    if args.iter().any(|a| a == "--settings-dialog") {
        return dialog::run_settings_dialog();
    }
    if args.iter().any(|a| a == "--about-dialog") {
        return dialog::run_about_dialog();
    }
    if let Some(pos) = args.iter().position(|a| a == "--password-dialog") {
        // --password-dialog <archive> <out_file>
        if args.len() >= pos + 3 {
            return dialog::run_password_dialog(
                &args[pos + 1],
                std::path::Path::new(&args[pos + 2]),
            );
        }
        eprintln!("zarmount: --password-dialog requires <archive> <out_file>");
        return std::process::ExitCode::FAILURE;
    }
    if let Some(pos) = args.iter().position(|a| a == "--error-dialog") {
        // --error-dialog <message>
        if args.len() >= pos + 2 {
            return dialog::run_error_dialog(&args[pos + 1]);
        }
        eprintln!("zarmount: --error-dialog requires <message>");
        return std::process::ExitCode::FAILURE;
    }

    // CLI client mode: attach to parent console so output prints to terminal.
    attach_console();

    run_client(&args)
}

#[cfg(target_os = "windows")]
fn attach_console() {
    use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
        SetStdHandle,
    };
    use windows::core::PCWSTR;

    // If stdout is already redirected (e.g. pipe or file), keep it!
    if let Ok(h) = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }
        && !h.is_invalid()
        && !h.0.is_null()
    {
        return;
    }

    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS).is_ok() } {
        let conout: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
        if let Ok(handle) = unsafe {
            CreateFileW(
                PCWSTR(conout.as_ptr()),
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        } {
            unsafe {
                let _ = SetStdHandle(STD_OUTPUT_HANDLE, handle);
                let _ = SetStdHandle(STD_ERROR_HANDLE, handle);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn run_client(args: &[String]) -> std::process::ExitCode {
    use clap::Parser;

    let argv: Vec<&str> = std::iter::once("zarmount")
        .chain(args.iter().map(|s| s.as_str()))
        .collect();
    let options = match mount_windows::Options::try_parse_from(argv) {
        Ok(o) => o,
        Err(e) => {
            e.exit();
        }
    };

    let archive =
        std::fs::canonicalize(&options.archive).unwrap_or_else(|_| options.archive.clone());

    // If --ask was passed, open the ask dialog to let user choose path and drive letter
    let command = if options.ask {
        let temp_prefill = std::env::temp_dir().join(format!("zarmount_{}", std::process::id()));
        let out_file =
            std::env::temp_dir().join(format!("zarmount_ask_{}.txt", std::process::id()));
        let chosen = dialog::spawn_ask_dialog(&temp_prefill, &out_file);
        match chosen {
            Some((path, Some(drive))) => {
                format!("mount {} {} {}:", archive.display(), path.display(), drive)
            }
            Some((path, None)) => {
                format!("mount {} {}", archive.display(), path.display())
            }
            None => {
                // User cancelled or closed dialog
                return std::process::ExitCode::SUCCESS;
            }
        }
    } else {
        match (&options.target1, &options.target2) {
            (Some(t1), Some(t2)) => format!("mount {} {} {}", archive.display(), t1, t2),
            (Some(t1), None) => format!("mount {} {}", archive.display(), t1),
            (None, Some(t2)) => format!("mount {} {}", archive.display(), t2),
            (None, None) => format!("mount {}", archive.display()),
        }
    };

    let mut response: Option<String> = None;
    let mut daemon_spawned = false;
    for attempt in 0..200 {
        match pipe::call(pipe::PIPE_NAME, &command) {
            Ok(r) => {
                response = Some(r);
                break;
            }
            Err(_) => {
                if !daemon_spawned && attempt >= 2 {
                    daemon::spawn_daemon();
                    daemon_spawned = true;
                }
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
        }
    }

    match response {
        Some(r) if r.trim() == "ok" => {
            println!("zarmount: mounted {}", options.archive.display());
            std::process::ExitCode::SUCCESS
        }
        Some(r) => {
            eprintln!("zarmount: {r}");
            std::process::ExitCode::FAILURE
        }
        None => {
            eprintln!("zarmount: daemon not available");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn main() {
    eprintln!("zarmount currently supports Linux and Windows only");
    std::process::exit(1);
}
