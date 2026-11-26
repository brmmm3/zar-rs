//! Open a mounted path in Windows Explorer or FreeCommander.
//!
//! Per `zarmount.md`:
//! - "File Explorer" is the default.
//! - If "FreeCommander" is selected in Settings (and installed), a new FreeCommander tab
//!   is opened with the path/drive.

use std::path::Path;
use std::process::Command;

use crate::settings::{FileManager, Settings};

/// Opens the given path in the user's preferred file manager.
///
/// If FreeCommander is configured and available, opens a new tab in FreeCommander.
/// Otherwise, opens the destination in Windows Explorer.
pub fn open_path(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::load();
    if settings.file_manager == FileManager::FreeCommander
        && let Some(fc) = find_freecommander()
    {
        // FreeCommander `/C` passes to running instance, `/T` opens a new tab,
        // `/L` sets the path in the left/source panel.
        Command::new(&fc)
            .arg("/C")
            .arg("/T")
            .arg(format!("/L={}", path.display()))
            .spawn()
            .map_err(|e| format!("failed to launch FreeCommander: {e}"))?;
        return Ok(());
    }

    // Default: Windows Explorer
    Command::new("explorer.exe")
        .arg(path)
        .spawn()
        .map_err(|e| format!("failed to launch Explorer: {e}"))?;
    Ok(())
}

/// Checks if FreeCommander is installed and accessible.
pub fn is_freecommander_available() -> bool {
    find_freecommander().is_some()
}

/// Locate the FreeCommander executable, if installed.
pub fn find_freecommander() -> Option<String> {
    let candidates = [
        // Common install locations.
        r"C:\Program Files\FreeCommander XE\FreeCommander.exe",
        r"C:\Program Files\FreeCommander\FreeCommander.exe",
        r"C:\Program Files (x86)\FreeCommander XE\FreeCommander.exe",
        r"C:\Program Files (x86)\FreeCommander\FreeCommander.exe",
    ];
    for candidate in candidates {
        let p = std::path::Path::new(candidate);
        if p.exists() {
            return Some(candidate.to_string());
        }
    }

    // Search the PATH.
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            for name in ["FreeCommander.exe", "FreeCommanderXE.exe"] {
                let candidate = dir.join(name);
                if candidate.exists() {
                    return Some(candidate.to_string_lossy().into_owned());
                }
            }
        }
    }

    None
}
