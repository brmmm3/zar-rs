//! Elevated worker process for ProjFS mount and unmount operations.
//!
//! Per `zarmount.md`:
//! The daemon creates an elevated process for mount/unmount operations.
//! When exiting, all archives are unmounted and the elevated process terminates.
//!
//! The elevated worker listens on a dedicated named pipe connected to the daemon.
//! It maintains a map of active ProjFS virtual filesystem instances.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use windows_projfs::ProjectedFileSystem;
use zarlib::operations::PasswordSet;

use crate::dialog;
use crate::mount_windows;
use crate::pipe;

struct MountInstance {
    mountpoint: PathBuf,
    pfs: ProjectedFileSystem,
    is_temp: bool,
}

/// Run the elevated worker process.
///
/// # Arguments
/// * `pipe_name` – named pipe to communicate with the daemon.
pub fn run_worker(pipe_name: &str) -> ExitCode {
    let mut pipe_handle: Option<windows::Win32::Foundation::HANDLE> = None;
    for _ in 0..120 {
        if let Ok(h) = pipe::connect_pipe(pipe_name) {
            pipe_handle = Some(h);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let pipe_handle = match pipe_handle {
        Some(h) => h,
        None => {
            eprintln!("zarmount: elevated worker: failed to connect to pipe {pipe_name}");
            return ExitCode::FAILURE;
        }
    };

    let mut mounts: HashMap<u64, MountInstance> = HashMap::new();

    loop {
        let command = match pipe::read_line(pipe_handle) {
            Ok(line) if line.is_empty() => {
                // Client disconnected (pipe closed)
                break;
            }
            Ok(line) => line,
            Err(_) => {
                // Read error or broken pipe
                break;
            }
        };

        let mut parts = command.splitn(5, ' ');
        let verb = parts.next().unwrap_or("");

        match verb {
            "mount" => {
                // mount <id> <archive> <mountpoint> [password_file]
                let id_str = parts.next().unwrap_or("");
                let archive_str = parts.next().unwrap_or("");
                let mountpoint_str = parts.next().unwrap_or("");
                let password_file = parts.next().filter(|s| !s.is_empty()).map(Path::new);

                let id: u64 = match id_str.parse() {
                    Ok(id) => id,
                    Err(_) => {
                        let _ = pipe::write_line(pipe_handle, "error: invalid mount id");
                        continue;
                    }
                };

                let archive = PathBuf::from(archive_str);
                let mountpoint = PathBuf::from(mountpoint_str);

                let passwords = match password_file.and_then(|p| dialog::read_passwords(p).ok()) {
                    Some(set) => set,
                    None => PasswordSet::new(None),
                };

                match mount_windows::mount_archive(&archive, &mountpoint, None, &passwords) {
                    Ok(pfs) => {
                        let is_temp = mountpoint_str.contains("zarmount_");
                        mounts.insert(
                            id,
                            MountInstance {
                                mountpoint,
                                pfs,
                                is_temp,
                            },
                        );
                        let _ = pipe::write_line(pipe_handle, "ok");
                    }
                    Err(e) => {
                        let err_msg = format!("error: {e}");
                        let _ = pipe::write_line(pipe_handle, &err_msg);
                    }
                }
            }
            "unmount" => {
                // unmount <id>
                let id_str = parts.next().unwrap_or("");
                let id: u64 = match id_str.parse() {
                    Ok(id) => id,
                    Err(_) => {
                        let _ = pipe::write_line(pipe_handle, "error: invalid mount id");
                        continue;
                    }
                };

                if let Some(instance) = mounts.remove(&id) {
                    let mountpoint = instance.mountpoint;
                    let is_temp = instance.is_temp;
                    drop(instance.pfs);
                    if is_temp {
                        let _ = fs::remove_dir_all(&mountpoint);
                    }
                    let _ = pipe::write_line(pipe_handle, "ok");
                } else {
                    let _ = pipe::write_line(pipe_handle, "error: mount not found");
                }
            }
            "exit" => {
                // Unmount everything and exit
                let _ = pipe::write_line(pipe_handle, "ok");
                break;
            }
            _ => {
                let _ = pipe::write_line(pipe_handle, "error: unknown worker command");
            }
        }
    }

    // Cleanup: unmount all remaining projections
    for (_, instance) in mounts.drain() {
        let mountpoint = instance.mountpoint;
        let is_temp = instance.is_temp;
        drop(instance.pfs);
        if is_temp {
            let _ = fs::remove_dir_all(&mountpoint);
        }
    }

    unsafe {
        let _ = windows::Win32::Foundation::CloseHandle(pipe_handle);
    }

    ExitCode::SUCCESS
}
