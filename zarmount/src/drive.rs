//! `subst`-based drive-letter mapping for zarmount.
//!
//! When the user requests a drive letter as the mount target, zarmount
//! projects the archive onto a directory and maps that directory to
//! the requested drive letter with `subst`. This module performs the mapping,
//! its removal, and discovery of unmapped drive letters.

use std::path::{Path, PathBuf};
use std::process::Command;
use windows::Win32::Storage::FileSystem::GetLogicalDrives;

/// Checks if a string represents a Windows drive letter (e.g. `Z:`, `Z:\`, `z:`).
pub fn is_drive_letter(s: &str) -> bool {
    let s = s.trim();
    if (s.len() == 2 && s.as_bytes()[1] == b':')
        || (s.len() == 3 && s.as_bytes()[1] == b':' && s.as_bytes()[2] == b'\\')
    {
        s.as_bytes()[0].is_ascii_alphabetic()
    } else {
        false
    }
}

/// Parses a drive letter from a string if valid, returning an uppercase char (e.g. `'Z'`).
pub fn parse_drive_letter(s: &str) -> Option<char> {
    if is_drive_letter(s) {
        s.trim().chars().next().map(|c| c.to_ascii_uppercase())
    } else {
        None
    }
}

/// Returns a sorted list of all drive letters ('A' through 'Z') that are currently not mapped.
pub fn unmapped_drive_letters() -> Vec<char> {
    let mask = unsafe { GetLogicalDrives() };
    let mut available = Vec::new();
    for i in 0..26 {
        if (mask & (1 << i)) == 0 {
            let letter = (b'A' + i as u8) as char;
            available.push(letter);
        }
    }
    available
}

/// Map a directory to a drive letter using `subst`.
///
/// Returns the drive root (e.g. `Z:\`) on success.
pub fn subst(directory: &Path, letter: char) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let letter = letter.to_ascii_uppercase();
    let drive_arg = format!("{letter}:");
    let drive_root = format!("{letter}:\\");
    let output = Command::new("subst")
        .arg(&drive_arg)
        .arg(directory)
        .output()
        .map_err(|e| format!("failed to run subst: {e}"))?;
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "subst failed (exit {:?}): {}{}",
            output.status.code(),
            stdout.trim(),
            stderr.trim()
        )
        .into());
    }
    Ok(PathBuf::from(drive_root))
}

/// Remove a drive-letter mapping created with `subst`.
pub fn subst_remove(letter: char) -> Result<(), Box<dyn std::error::Error>> {
    let letter = letter.to_ascii_uppercase();
    let drive_arg = format!("{letter}:");
    let output = Command::new("subst")
        .arg(&drive_arg)
        .arg("/d")
        .output()
        .map_err(|e| format!("failed to run subst: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("subst /d failed: {stderr}").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_and_parse_drive_letter() {
        assert!(is_drive_letter("Z:"));
        assert!(is_drive_letter("z:"));
        assert!(is_drive_letter("Z:\\"));
        assert!(is_drive_letter("  C: "));
        assert!(!is_drive_letter("C:\\foo"));
        assert!(!is_drive_letter("1:"));
        assert!(!is_drive_letter("ZZ:"));
        assert!(!is_drive_letter(""));

        assert_eq!(parse_drive_letter("Z:"), Some('Z'));
        assert_eq!(parse_drive_letter("z:"), Some('Z'));
        assert_eq!(parse_drive_letter("z:\\"), Some('Z'));
        assert_eq!(parse_drive_letter("not_a_drive"), None);
    }

    #[test]
    fn test_unmapped_drives_returns_valid_letters() {
        let unmapped = unmapped_drive_letters();
        for &letter in &unmapped {
            assert!(letter.is_ascii_uppercase());
            assert!(letter >= 'A' && letter <= 'Z');
        }
        // At least some drive letters should be unmapped on any test machine
        assert!(!unmapped.is_empty());
    }
}
