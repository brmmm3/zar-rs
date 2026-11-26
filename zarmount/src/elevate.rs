//! Windows elevation support for zarmount.
//!
//! ProjFS requires administrator privileges. This module checks whether the
//! current process is elevated, checks/enables the Client-ProjFS optional feature,
//! and launches elevated worker processes via `ShellExecuteExW` with `runas`.

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, INFINITE, OpenProcessToken, WaitForSingleObject,
};
use windows::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
use windows::core::{HSTRING, PCWSTR};

const CREATE_NO_WINDOW: u32 = 0x08000000;

/// The daemon's hidden window handle. Set by the daemon after creating its
/// tray window so that `ShellExecuteExW` with "runas" can display the UAC
/// prompt as a child of this window. Without a parent HWND, the UAC prompt
/// may not appear when the daemon has no visible window.
static DAEMON_HWND: AtomicUsize = AtomicUsize::new(0);

/// Set the daemon's hidden window handle for UAC prompts.
pub fn set_daemon_hwnd(hwnd: HWND) {
    DAEMON_HWND.store(hwnd.0 as usize, Ordering::Relaxed);
}

/// Returns `true` if the current process is running with elevated
/// (administrator) privileges.
#[allow(dead_code)]
pub fn is_elevated() -> bool {
    let mut token_handle = Default::default();
    unsafe {
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token_handle).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut return_length = 0u32;
        let ok = GetTokenInformation(
            token_handle,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut return_length,
        );
        let _ = CloseHandle(token_handle);
        ok.is_ok() && elevation.TokenIsElevated != 0
    }
}

/// Checks if Windows Projected File System (Client-ProjFS) is enabled.
///
/// Executes `Get-CimInstance -ClassName Win32_OptionalFeature -Filter "Name = 'Client-ProjFS'"`
/// via PowerShell without displaying any console window.
pub fn is_projfs_enabled() -> bool {
    let script = r#"try {
        $f = Get-CimInstance -ClassName Win32_OptionalFeature -Filter "Name = 'Client-ProjFS'"
        if ($f -and $f.InstallState -eq 1) { exit 0 } else { exit 1 }
    } catch {
        exit 2
    }"#;

    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    match output {
        Ok(out) => out.status.code() == Some(0),
        Err(_) => false,
    }
}

/// Enables the Windows Projected File System (Client-ProjFS) feature with elevation.
///
/// Runs `Enable-WindowsOptionalFeature -Online -FeatureName Client-ProjFS -NoRestart`
/// with administrator privileges without displaying any console window.
pub fn enable_projfs_elevated() -> Result<(), Box<dyn std::error::Error>> {
    let exit_code = run_elevated_self(&["--enable-projfs".to_string()])?;
    if exit_code == 0 {
        Ok(())
    } else {
        Err(format!("Enable-WindowsOptionalFeature failed with exit code {exit_code}").into())
    }
}

/// Build a command-line string from arguments, quoting each one.
/// Returns `None` when there are no arguments (so `lpParameters` can be null).
fn build_command_line(args: &[String]) -> Option<Vec<u16>> {
    if args.is_empty() {
        return None;
    }
    let mut result: Vec<u16> = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            result.push(b' ' as u16);
        }
        result.push(b'"' as u16);
        for c in arg.encode_utf16() {
            result.push(c);
        }
        result.push(b'"' as u16);
    }
    result.push(0);
    Some(result)
}

/// Relaunches the current executable elevated with specific arguments and waits for it to exit.
pub fn run_elevated_self(args: &[String]) -> Result<u32, Box<dyn std::error::Error>> {
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path.parent().unwrap_or(Path::new("."));
    let arguments = build_command_line(args);

    let verb = HSTRING::from("runas");
    let file = HSTRING::from(&exe_path.to_string_lossy().to_string());
    let directory = HSTRING::from(&exe_dir.to_string_lossy().to_string());

    let parent_hwnd = HWND(DAEMON_HWND.load(Ordering::Relaxed) as *mut _);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        hwnd: parent_hwnd,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: arguments
            .as_ref()
            .map(|a| PCWSTR(a.as_ptr()))
            .unwrap_or(PCWSTR::null()),
        lpDirectory: PCWSTR(directory.as_ptr()),
        nShow: SW_HIDE.0,
        ..SHELLEXECUTEINFOW::default()
    };

    unsafe { ShellExecuteExW(&mut info) }
        .map_err(|e| format!("ShellExecuteExW (runas) failed: {e}"))?;

    let process = info.hProcess;
    if process.is_invalid() {
        return Err("elevated process handle is invalid".into());
    }
    unsafe { WaitForSingleObject(process, INFINITE) };

    let mut exit_code = 0u32;
    unsafe { GetExitCodeProcess(process, &mut exit_code) }
        .map_err(|e| format!("GetExitCodeProcess failed: {e}"))?;
    unsafe { CloseHandle(process) }.map_err(|e| format!("CloseHandle failed: {e}"))?;

    Ok(exit_code)
}

/// Relaunches the current executable with administrator privileges via UAC.
///
/// Returns the exit code of the elevated child process on success.
#[allow(dead_code)]
pub fn relaunch_elevated() -> Result<u32, Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    run_elevated_self(&args)
}

/// Spawns a new elevated process (via UAC) without waiting for it to exit.
///
/// Returns the process handle of the elevated child on success.
pub fn spawn_elevated(
    args: &[String],
) -> Result<windows::Win32::Foundation::HANDLE, Box<dyn std::error::Error>> {
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path.parent().unwrap_or(Path::new("."));
    let arguments = build_command_line(args);

    let verb = HSTRING::from("runas");
    let file = HSTRING::from(&exe_path.to_string_lossy().to_string());
    let directory = HSTRING::from(&exe_dir.to_string_lossy().to_string());

    let parent_hwnd = HWND(DAEMON_HWND.load(Ordering::Relaxed) as *mut _);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        hwnd: parent_hwnd,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: arguments
            .as_ref()
            .map(|a| PCWSTR(a.as_ptr()))
            .unwrap_or(PCWSTR::null()),
        lpDirectory: PCWSTR(directory.as_ptr()),
        nShow: SW_HIDE.0,
        ..SHELLEXECUTEINFOW::default()
    };

    unsafe { ShellExecuteExW(&mut info) }
        .map_err(|e| format!("ShellExecuteExW (runas) failed: {e}"))?;

    let process = info.hProcess;
    if process.is_invalid() {
        return Err("elevated process handle is invalid".into());
    }
    Ok(process)
}
