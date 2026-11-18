use std::path::Path;

/// Whether archives on this filesystem should have block checksums rechecked
/// during extraction. Network and legacy filesystems require rechecking.
/// Unknown filesystems and failed detection return an error so callers can
/// report the uncertainty and conservatively enable rechecking.
pub fn extraction_needs_checksum_recheck(archive: &Path) -> std::io::Result<bool> {
    needs_recheck(archive)
}

#[cfg(any(test, target_os = "linux", target_os = "macos", windows))]
fn classify_filesystem(name: &str) -> Option<bool> {
    let name = name.to_ascii_lowercase();
    if name.starts_with("fuse")
        || matches!(
            name.as_str(),
            "nfs"
                | "nfs4"
                | "cifs"
                | "smb3"
                | "smbfs"
                | "9p"
                | "afs"
                | "ceph"
                | "coda"
                | "davfs"
                | "glusterfs"
                | "lustre"
                | "beegfs"
                | "sshfs"
                | "fat"
                | "fat12"
                | "fat16"
                | "fat32"
                | "vfat"
                | "msdos"
                | "exfat"
                | "ext2"
                | "ext3"
                | "hfs"
                | "hfsplus"
        )
    {
        Some(true)
    } else if matches!(
        name.as_str(),
        "apfs"
            | "btrfs"
            | "ext4"
            | "ntfs"
            | "ntfs3"
            | "refs"
            | "xfs"
            | "zfs"
            | "tmpfs"
            | "ramfs"
            | "overlay"
            | "squashfs"
            | "erofs"
    ) {
        Some(false)
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn needs_recheck(path: &Path) -> std::io::Result<bool> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::fs::canonicalize(path)?;
    let mounts = std::fs::read("/proc/self/mountinfo")?;
    let name = filesystem_for_path(path.as_os_str().as_bytes(), &mounts)?;
    classify_filesystem(&name)
        .ok_or_else(|| std::io::Error::other(format!("unrecognized filesystem type {name}")))
}

#[cfg(target_os = "linux")]
fn filesystem_for_path(path: &[u8], mounts: &[u8]) -> std::io::Result<String> {
    let invalid = || std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid mountinfo");
    let mut best: Option<(usize, String)> = None;
    for line in mounts
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let fields: Vec<&[u8]> = line.split(|byte| *byte == b' ').collect();
        let separator = fields
            .iter()
            .position(|field| *field == b"-")
            .ok_or_else(invalid)?;
        if separator < 6 || fields.len() <= separator + 1 {
            return Err(invalid());
        }
        let mut mount = Vec::new();
        let mut bytes = fields[4].iter().copied();
        while let Some(byte) = bytes.next() {
            if byte == b'\\' {
                let a = bytes.next().ok_or_else(invalid)?;
                let b = bytes.next().ok_or_else(invalid)?;
                let c = bytes.next().ok_or_else(invalid)?;
                if !(b'0'..=b'3').contains(&a)
                    || !(b'0'..=b'7').contains(&b)
                    || !(b'0'..=b'7').contains(&c)
                {
                    return Err(invalid());
                }
                mount.push((a - b'0') * 64 + (b - b'0') * 8 + c - b'0');
            } else {
                mount.push(byte);
            }
        }
        let matches = path == mount
            || (path.starts_with(&mount)
                && (mount == b"/" || path.get(mount.len()) == Some(&b'/')));
        if matches
            && best
                .as_ref()
                .is_none_or(|(length, _)| mount.len() >= *length)
        {
            let name = std::str::from_utf8(fields[separator + 1]).map_err(|_| invalid())?;
            best = Some((mount.len(), name.to_owned()));
        }
    }
    best.map(|(_, name)| name)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "archive mount not found"))
}

#[cfg(target_os = "macos")]
fn needs_recheck(path: &Path) -> std::io::Result<bool> {
    use std::ffi::{CStr, CString};
    use std::os::unix::ffi::OsStrExt;
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "path contains NUL"))?;
    let mut info = std::mem::MaybeUninit::<libc::statfs>::uninit();
    if unsafe { libc::statfs(path.as_ptr(), info.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let info = unsafe { info.assume_init() };
    if info.f_flags & libc::MNT_LOCAL as u32 == 0 {
        return Ok(true);
    }
    let name = unsafe { CStr::from_ptr(info.f_fstypename.as_ptr()) }.to_string_lossy();
    classify_filesystem(&name)
        .ok_or_else(|| std::io::Error::other(format!("unrecognized filesystem type {name}")))
}

#[cfg(windows)]
fn needs_recheck(path: &Path) -> std::io::Result<bool> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetVolumeInformationW, GetVolumePathNameW,
    };
    use windows_sys::Win32::System::WindowsProgramming::{
        DRIVE_NO_ROOT_DIR, DRIVE_REMOTE, DRIVE_UNKNOWN,
    };
    let path = std::fs::canonicalize(path)?;
    let path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut root = vec![0u16; 32768];
    if unsafe { GetVolumePathNameW(path.as_ptr(), root.as_mut_ptr(), root.len() as u32) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    match unsafe { GetDriveTypeW(root.as_ptr()) } {
        DRIVE_REMOTE => return Ok(true),
        DRIVE_UNKNOWN | DRIVE_NO_ROOT_DIR => {
            return Err(std::io::Error::other("unknown Windows drive type"));
        }
        _ => {}
    }
    let mut name = [0u16; 256];
    if unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            name.as_mut_ptr(),
            name.len() as u32,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    let end = name.iter().position(|ch| *ch == 0).unwrap_or(name.len());
    let name = String::from_utf16(&name[..end])
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    classify_filesystem(&name)
        .ok_or_else(|| std::io::Error::other(format!("unrecognized filesystem type {name}")))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn needs_recheck(_path: &Path) -> std::io::Result<bool> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "filesystem detection is not supported on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_network_legacy_and_modern_filesystems() {
        for name in ["nfs4", "cifs", "fuse.sshfs", "vfat", "exfat", "FAT32"] {
            assert_eq!(classify_filesystem(name), Some(true), "{name}");
        }
        for name in ["ext4", "btrfs", "APFS", "NTFS"] {
            assert_eq!(classify_filesystem(name), Some(false), "{name}");
        }
        assert_eq!(classify_filesystem("unrecognized"), None);
        assert!(extraction_needs_checksum_recheck(Path::new("/nonexistent-zarg-archive")).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn mount_lookup_uses_resolved_longest_matching_mount_and_decodes_escapes() {
        let mounts = b"1 0 0:1 / / rw - ext4 /dev/a rw\n\
                       2 1 0:2 / /mnt/net rw - nfs server:/share rw\n\
                       3 2 0:3 / /mnt/net/my\\040drive rw - exfat /dev/b rw\n";
        assert_eq!(
            filesystem_for_path(b"/mnt/net/my drive/a.zar", mounts).unwrap(),
            "exfat"
        );
        assert_eq!(
            filesystem_for_path(b"/mnt/net/a.zar", mounts).unwrap(),
            "nfs"
        );
        assert_eq!(
            filesystem_for_path(b"/mnt/network/a.zar", mounts).unwrap(),
            "ext4"
        );
        assert!(filesystem_for_path(b"/a", b"invalid").is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn auto_resolves_symlinks_to_the_archive_filesystem() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("archive.zar");
        let link = directory.path().join("link.zar");
        std::fs::write(&archive, []).unwrap();
        std::os::unix::fs::symlink(&archive, &link).unwrap();
        assert_eq!(
            extraction_needs_checksum_recheck(&archive).map_err(|error| error.to_string()),
            extraction_needs_checksum_recheck(&link).map_err(|error| error.to_string())
        );
    }
}
