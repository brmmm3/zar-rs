//! The zarmount daemon.
//!
//! Per `zarmount.md`:
//! - Starts a daemon which creates a system tray and an elevated process for
//!   mount/unmount operations.
//! - Listens on `\\.\pipe\zarmount` for client requests.
//! - Manages mounts (ProjFS projections and drive letter mappings).
//! - Opens File Explorer (or FreeCommander in a new tab if selected in Settings)
//!   with the destination after mount.
//! - System tray menu with Open, Map drive letter, Unmount, Settings, About, Exit.
//! - Clean termination of both daemon and elevated process on exit.

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Threading::WaitForSingleObject;
use windows::Win32::UI::WindowsAndMessaging::PostQuitMessage;

use crate::dialog;
use crate::drive;
use crate::elevate;
use crate::open;
use crate::pipe;
use crate::tray;
use zarlib::operations;

/// `CREATE_NO_WINDOW` — do not create a console window for the daemon.
const CREATE_NO_WINDOW: u32 = 0x08000000;
/// `CREATE_NEW_PROCESS_GROUP` — put the child in its own process group.
const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

struct Mount {
    id: u64,
    archive: PathBuf,
    mountpoint: PathBuf,
    open_path: PathBuf,
    label: String,
    drive_letter: Option<char>,
}

type Mounts = Arc<Mutex<Vec<Mount>>>;

struct WorkerClient {
    pipe_handle: HANDLE,
    process_handle: HANDLE,
}

unsafe impl Send for WorkerClient {}
unsafe impl Sync for WorkerClient {}

impl WorkerClient {
    fn call(&self, command: &str) -> Result<String, String> {
        pipe::write_line(self.pipe_handle, command).map_err(|e| e.to_string())?;
        pipe::read_line(self.pipe_handle).map_err(|e| e.to_string())
    }

    fn exit_and_wait(&self) {
        let _ = pipe::write_line(self.pipe_handle, "exit");
        unsafe {
            let _ = CloseHandle(self.pipe_handle);
            let _ = WaitForSingleObject(self.process_handle, 5000);
            let _ = CloseHandle(self.process_handle);
        }
    }
}

type Worker = Arc<Mutex<Option<WorkerClient>>>;

static NEXT_MOUNT_ID: AtomicU64 = AtomicU64::new(1);

/// Run the daemon (tray icon + pipe server + elevated worker).
pub fn run() -> ExitCode {
    let mounts: Mounts = Arc::new(Mutex::new(Vec::new()));
    let worker: Worker = Arc::new(Mutex::new(None));

    let tray_mounts = mounts.clone();
    let tray_cmd_mounts = mounts.clone();
    let tray_cmd_worker = worker.clone();

    let on_ready_mounts = mounts.clone();
    let on_ready_worker = worker.clone();

    tray::run(
        move || {
            let m = tray_mounts.lock().unwrap();
            m.iter()
                .map(|mount| tray::MenuEntry {
                    label: mount.label.clone(),
                    has_drive_letter: mount.drive_letter.is_some(),
                })
                .collect()
        },
        move |cmd_id| handle_tray_command(cmd_id, &tray_cmd_mounts, &tray_cmd_worker),
        move || {
            let m = on_ready_mounts.clone();
            let w = on_ready_worker.clone();
            std::thread::spawn(move || start_services(m, w));
        },
    )
}

/// Spawns the elevated worker and starts the client pipe server.
fn start_services(mounts: Mounts, worker: Worker) {
    // Check if ProjFS is enabled; if disabled, ask user to enable it, and exit if cancelled.
    if !elevate::is_projfs_enabled() {
        if !dialog::spawn_projfs_dialog() {
            unsafe {
                PostQuitMessage(0);
            }
            return;
        }

        if let Err(e) = elevate::enable_projfs_elevated() {
            eprintln!("zarmount: failed to enable ProjFS: {e}");
            dialog::spawn_error_dialog(&format!(
                "Failed to enable Windows Projected File System (ProjFS):\n{e}\n\nProjFS is required to mount ZAR archives."
            ));
            unsafe {
                PostQuitMessage(1);
            }
            return;
        }
    }

    let worker_pipe_name = format!(r"\\.\pipe\zarmount_worker_{}", std::process::id());

    // 1. Create the worker named pipe server with permissive security attributes
    let pipe_handle = match pipe::create_worker_server_pipe(&worker_pipe_name) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("zarmount: failed to create worker pipe: {e}");
            dialog::spawn_error_dialog(&format!("Failed to create worker pipe:\n{e}"));
            unsafe {
                PostQuitMessage(1);
            }
            return;
        }
    };

    // 2. Launch elevated worker via UAC
    let process_handle =
        match elevate::spawn_elevated(&["--worker".to_string(), worker_pipe_name.clone()]) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("zarmount: failed to spawn elevated worker: {e}");
                dialog::spawn_error_dialog(&format!(
                    "Elevation failed:\n{e}\n\nAdministrator privileges are required for ProjFS."
                ));
                unsafe {
                    let _ = CloseHandle(pipe_handle);
                    PostQuitMessage(1);
                }
                return;
            }
        };

    // 3. Wait for the elevated worker to connect (or detect premature process termination)
    if let Err(e) = pipe::wait_for_worker_connection(pipe_handle, process_handle, 60_000) {
        eprintln!("zarmount: {e}");
        dialog::spawn_error_dialog(&format!("{e}.\nOperation cancelled."));
        unsafe {
            let _ = CloseHandle(pipe_handle);
            let _ = CloseHandle(process_handle);
            PostQuitMessage(1);
        }
        return;
    }

    *worker.lock().unwrap() = Some(WorkerClient {
        pipe_handle,
        process_handle,
    });

    // 4. Start client pipe server
    pipe_server(mounts, worker);
}

/// Pipe server loop for client commands on `\\.\pipe\zarmount`.
fn pipe_server(mounts: Mounts, worker: Worker) {
    loop {
        let handle = match pipe::accept_client(pipe::PIPE_NAME) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("zarmount: daemon: failed to accept pipe client: {e}");
                continue;
            }
        };

        let command = match pipe::read_line(handle) {
            Ok(cmd) => cmd,
            Err(e) => {
                eprintln!("zarmount: daemon: read error: {e}");
                unsafe {
                    let _ = CloseHandle(handle);
                }
                continue;
            }
        };

        let response = handle_client_command(&command, &mounts, &worker);
        let _ = pipe::write_line(handle, &response);
        unsafe {
            let _ = CloseHandle(handle);
        }

        if command.trim() == "exit" {
            unsafe {
                PostQuitMessage(0);
            }
            break;
        }
    }
}

/// Handle a command line request from the client.
fn handle_client_command(command: &str, mounts: &Mounts, worker: &Worker) -> String {
    let mut parts = command.splitn(4, ' ');
    let verb = parts.next().unwrap_or("");
    match verb {
        "mount" => {
            let archive = parts.next().unwrap_or("");
            if archive.is_empty() {
                return "error: no archive specified".into();
            }
            let target1 = parts.next().map(|s| s.to_string());
            let target2 = parts.next().map(|s| s.to_string());
            mount_archive(archive, target1, target2, mounts, worker)
        }
        "exit" => {
            unmount_all(mounts, worker);
            exit_worker(worker);
            "ok".into()
        }
        _ => format!("error: unknown command: {command}"),
    }
}

/// Spawns mount in the elevated worker and sets up drive letter and file manager.
fn mount_archive(
    archive: &str,
    target1: Option<String>,
    target2: Option<String>,
    mounts: &Mounts,
    worker: &Worker,
) -> String {
    let archive_path = PathBuf::from(archive);
    if !archive_path.exists() {
        let message = format!("Archive not found:\n{archive}");
        dialog::spawn_error_dialog(&message);
        return format!("error: {message}");
    }

    let id = NEXT_MOUNT_ID.fetch_add(1, Ordering::Relaxed);

    // Resolve mountpoint directory and optional drive letter
    let (mountpoint, drive_letter) = match resolve_targets(target1, target2, id) {
        Ok(res) => res,
        Err(e) => {
            dialog::spawn_error_dialog(&e);
            return format!("error: {e}");
        }
    };

    if let Err(e) = fs::create_dir_all(&mountpoint) {
        let message = format!("Cannot create mount directory:\n{e}");
        dialog::spawn_error_dialog(&message);
        return format!("error: {message}");
    }

    // Collect passwords if archive has encrypted commits
    let password_file = match collect_passwords(&archive_path, id) {
        Ok(p) => p,
        Err(e) => {
            let _ = fs::remove_dir_all(&mountpoint);
            return format!("error: {e}");
        }
    };

    // Ask elevated worker to perform ProjFS mount
    let worker_guard = worker.lock().unwrap();
    let worker_client = match worker_guard.as_ref() {
        Some(c) => c,
        None => {
            let _ = fs::remove_dir_all(&mountpoint);
            return "error: elevated worker is not running".into();
        }
    };

    let pw_arg = password_file
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();

    let cmd = format!(
        "mount {} {} {} {}",
        id,
        archive_path.to_string_lossy(),
        mountpoint.to_string_lossy(),
        pw_arg
    );

    let resp = match worker_client.call(&cmd) {
        Ok(r) => r,
        Err(e) => {
            let _ = fs::remove_dir_all(&mountpoint);
            dialog::spawn_error_dialog(&format!("Elevated mount failed: {e}"));
            return format!("error: {e}");
        }
    };

    if resp.trim() != "ok" {
        let _ = fs::remove_dir_all(&mountpoint);
        dialog::spawn_error_dialog(&format!("Mount failed:\n{resp}"));
        return resp;
    }

    // Clean up temporary password file
    if let Some(pw) = &password_file {
        let _ = fs::remove_file(pw);
    }

    // Map drive letter if requested
    let open_path = if let Some(letter) = drive_letter {
        match drive::subst(&mountpoint, letter) {
            Ok(root) => root,
            Err(e) => {
                let _ = worker_client.call(&format!("unmount {id}"));
                let message = format!("Failed to map drive letter {letter}: {e}");
                dialog::spawn_error_dialog(&message);
                return format!("error: {message}");
            }
        }
    } else {
        mountpoint.clone()
    };

    // Per zarmount.md: "In all cases After mount/drive letter mapping the file explorer is opened with the destination."
    let _ = open::open_path(&open_path);

    let filename = archive_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let label = match drive_letter {
        Some(letter) => format!("{}:\\ ({})", letter, filename),
        None => format!("{} ({})", filename, mountpoint.display()),
    };

    mounts.lock().unwrap().push(Mount {
        id,
        archive: archive_path,
        mountpoint,
        open_path,
        label,
        drive_letter,
    });

    "ok".into()
}

/// Resolves path and drive letter from target arguments.
pub fn resolve_targets(
    target1: Option<String>,
    target2: Option<String>,
    id: u64,
) -> Result<(PathBuf, Option<char>), String> {
    match (target1, target2) {
        (Some(t1), Some(t2)) => {
            if drive::is_drive_letter(&t2) {
                let drive_char = drive::parse_drive_letter(&t2);
                Ok((PathBuf::from(t1), drive_char))
            } else if drive::is_drive_letter(&t1) {
                let drive_char = drive::parse_drive_letter(&t1);
                Ok((PathBuf::from(t2), drive_char))
            } else {
                Err(format!(
                    "Cannot specify two directory paths: '{t1}' and '{t2}'. One must be a drive letter."
                ))
            }
        }
        (Some(t1), None) => {
            if drive::is_drive_letter(&t1) {
                let drive_char = drive::parse_drive_letter(&t1);
                let temp_dir = std::env::temp_dir().join(format!("zarmount_{id}"));
                Ok((temp_dir, drive_char))
            } else {
                Ok((PathBuf::from(t1), None))
            }
        }
        (None, None) => {
            let temp_dir = std::env::temp_dir().join(format!("zarmount_{id}"));
            Ok((temp_dir, None))
        }
        (None, Some(t2)) => {
            if drive::is_drive_letter(&t2) {
                let drive_char = drive::parse_drive_letter(&t2);
                let temp_dir = std::env::temp_dir().join(format!("zarmount_{id}"));
                Ok((temp_dir, drive_char))
            } else {
                Ok((PathBuf::from(t2), None))
            }
        }
    }
}

/// Prompts for passwords via password dialog if needed.
fn collect_passwords(archive: &Path, id: u64) -> Result<Option<PathBuf>, String> {
    let archive_str = archive.to_string_lossy();
    let commits = match operations::info_archive(&archive_str, false) {
        Ok(c) => c,
        Err(e) => return Err(format!("cannot read archive: {e}")),
    };

    let has_encryption = commits
        .iter()
        .any(|c| c.header_encrypted || c.data_encrypted);
    if !has_encryption {
        return Ok(None);
    }

    let out_file = std::env::temp_dir().join(format!("zarmount_pw_{id}.bin"));
    if !dialog::spawn_password_dialog(&archive_str, &out_file) {
        return Err("Password entry was cancelled".into());
    }

    Ok(Some(out_file))
}

/// Unmounts all active mounts.
fn unmount_all(mounts: &Mounts, worker: &Worker) {
    let mut list = mounts.lock().unwrap();
    let worker_guard = worker.lock().unwrap();
    for mount in list.drain(..) {
        if let Some(letter) = mount.drive_letter {
            let _ = drive::subst_remove(letter);
        }
        if let Some(w) = worker_guard.as_ref() {
            let _ = w.call(&format!("unmount {}", mount.id));
        }
    }
}

/// Unmounts a single mount by index.
fn unmount_one(index: usize, mounts: &Mounts, worker: &Worker) {
    let mut list = mounts.lock().unwrap();
    if index < list.len() {
        let mount = list.remove(index);
        if let Some(letter) = mount.drive_letter {
            let _ = drive::subst_remove(letter);
        }
        let worker_guard = worker.lock().unwrap();
        if let Some(w) = worker_guard.as_ref() {
            let _ = w.call(&format!("unmount {}", mount.id));
        }
    }
}

/// Exits the elevated worker process.
fn exit_worker(worker: &Worker) {
    let mut worker_guard = worker.lock().unwrap();
    if let Some(w) = worker_guard.take() {
        w.exit_and_wait();
    }
}

/// Handles commands from the tray menu.
fn handle_tray_command(cmd_id: u32, mounts: &Mounts, worker: &Worker) {
    if cmd_id == tray::cmd::SETTINGS {
        dialog::spawn_settings_dialog();
        return;
    }
    if cmd_id == tray::cmd::ABOUT {
        dialog::spawn_about_dialog();
        return;
    }
    if cmd_id == tray::cmd::EXIT {
        unmount_all(mounts, worker);
        exit_worker(worker);
        unsafe {
            PostQuitMessage(0);
        }
        return;
    }

    // Open item
    if (tray::cmd::OPEN_BASE..tray::cmd::MAP_BASE).contains(&cmd_id) {
        let index = ((cmd_id - tray::cmd::OPEN_BASE) / 10) as usize;
        let list = mounts.lock().unwrap();
        if let Some(mount) = list.get(index) {
            let _ = open::open_path(&mount.open_path);
        }
        return;
    }

    // Map drive letter item
    if (tray::cmd::MAP_BASE..tray::cmd::UNMOUNT_BASE).contains(&cmd_id) {
        let index = ((cmd_id - tray::cmd::MAP_BASE) / 10) as usize;
        let mut list = mounts.lock().unwrap();
        if let Some(mount) = list.get_mut(index)
            && mount.drive_letter.is_none()
        {
            let out_file = std::env::temp_dir().join(format!("zarmount_map_{}.txt", mount.id));
            if let Some(letter) = dialog::spawn_map_drive_dialog(&out_file) {
                match drive::subst(&mount.mountpoint, letter) {
                    Ok(root) => {
                        mount.drive_letter = Some(letter);
                        mount.open_path = root.clone();
                        let filename = mount
                            .archive
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy();
                        mount.label = format!("{}:\\ ({})", letter, filename);
                        let _ = open::open_path(&root);
                    }
                    Err(e) => {
                        dialog::spawn_error_dialog(&format!(
                            "Failed to map drive letter {letter}: {e}"
                        ));
                    }
                }
            }
        }
        return;
    }

    // Unmount item
    if cmd_id >= tray::cmd::UNMOUNT_BASE {
        let index = ((cmd_id - tray::cmd::UNMOUNT_BASE) / 10) as usize;
        unmount_one(index, mounts, worker);
    }
}

/// Spawns the daemon in a new process if not already running.
pub fn spawn_daemon() -> bool {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let result = Command::new(&exe)
        .arg("--daemon")
        .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP)
        .spawn();
    result.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_targets() {
        // Case 1: no targets
        let (p1, d1) = resolve_targets(None, None, 1).unwrap();
        assert!(p1.to_string_lossy().contains("zarmount_1"));
        assert_eq!(d1, None);

        // Case 2: only path
        let (p2, d2) = resolve_targets(Some("C:\\mount".into()), None, 2).unwrap();
        assert_eq!(p2, PathBuf::from("C:\\mount"));
        assert_eq!(d2, None);

        // Case 3: only drive letter
        let (p3, d3) = resolve_targets(Some("Z:".into()), None, 3).unwrap();
        assert!(p3.to_string_lossy().contains("zarmount_3"));
        assert_eq!(d3, Some('Z'));

        // Case 4: path and drive letter
        let (p4, d4) = resolve_targets(Some("C:\\mount".into()), Some("Z:".into()), 4).unwrap();
        assert_eq!(p4, PathBuf::from("C:\\mount"));
        assert_eq!(d4, Some('Z'));

        // Case 5: reversed drive letter and path
        let (p5, d5) = resolve_targets(Some("Z:".into()), Some("C:\\mount".into()), 5).unwrap();
        assert_eq!(p5, PathBuf::from("C:\\mount"));
        assert_eq!(d5, Some('Z'));

        // Error: two non-drive paths
        assert!(resolve_targets(Some("C:\\a".into()), Some("C:\\b".into()), 6).is_err());
    }
}
