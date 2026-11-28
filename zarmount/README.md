# zarmount

`zarmount` exposes a ZAR archive as a read-only virtual filesystem.
It shows the latest commit by default, or an earlier state selected with
`--to`. Directories, regular files, empty files, and symbolic links are
available without extracting the archive to disk.

On **Linux** it uses **FUSE**; on **Windows** it uses **ProjFS** (Projected
File System).

---

## Linux (FUSE)

### Install and mount

Install FUSE 3 and its `fusermount3` helper, and ensure the current user can
access `/dev/fuse`. The default mode also requires a desktop D-Bus session,
a StatusNotifierItem-compatible system tray, a private `XDG_RUNTIME_DIR`, and
`xdg-open` to open the system's default file manager:

```sh
cargo binstall zarmount
# Mount without extracting, and open the default file manager
zarmount archive.zar /tmp/zar-mount
# Or use an automatically created temporary mount directory
zarmount archive.zar
# Choose a mount directory in a dialog
zarmount --ask archive.zar
```

The first invocation starts a per-user daemon. Subsequent invocations add
mounts to its tray menu, where each mount can be opened or unmounted. Exit
unmounts all archives and stops the daemon. Automatically created mount
directories are removed after unmounting. Externally unmounted sessions are
also cleaned up. Daemon errors are logged to
`$XDG_RUNTIME_DIR/zarmount/daemon.log`. If an unmount fails, the daemon
reports the error and retains the mount rather than claiming success; use
`fusermount3 -u` to recover it if necessary, then restart the daemon.

For terminal-only use, supply `--foreground` and an existing empty directory.
Press Ctrl-C to unmount and exit, or unmount from another terminal:

```sh
mkdir -p /tmp/zar-mount
zarmount --foreground archive.zar /tmp/zar-mount
ls -la /tmp/zar-mount
fusermount3 -u /tmp/zar-mount
```

SIGTERM and SIGHUP also trigger a graceful unmount. If the process is killed
forcibly or crashes, recover a disconnected mountpoint with
`fusermount3 -u /tmp/zar-mount`.

Linux binary release archives also contain `zarmount`. The mountpoint must be
empty (daemon mode creates it if necessary), and the archive must not be inside
it. The archive path must be UTF-8. Only the mounting user can access the
filesystem; there is no `allow_other` option. Drive-letter mapping is
Windows-only.

---

## Windows (ProjFS)

### Prerequisites

ProjFS must be enabled on your Windows machine. If not installed `zarmount` enable it, but needs admin rights.
Or run as Administrator manually:

```powershell
Enable-WindowsOptionalFeature -Online -FeatureName Client-ProjFS -NoRestart
```

A reboot may be required.

### Mount

```powershell
# Mount archive.zar to an empty directory
mkdir C:\zar-mount
zarmount archive.zar C:\zar-mount
```

The mount runs in the foreground. Press Ctrl+C to unmount. The mountpoint
must be an empty directory (it will be created if it does not exist).

### Limitations (Windows)

- The filesystem is read-only; all cancelable notifications are rejected.
- Archives containing FIFOs or device nodes are rejected.
- Symbolic links in archives are **resolved internally** to their target
  (regular file or directory) since ProjFS does not expose symlinks to the
  filesystem. If the target does not exist in the archive, the symlink is
  omitted. Symlink chains are followed up to 64 levels; cycles are detected
  and the symlink is omitted.

---

## Common options

The tray Settings dialog offers the same color themes as zarg and a choice of
Crystal or Flat icons. The choice is saved for future dialogs. When no zarmount
icon theme has been saved, it uses zarg's icon theme if available, otherwise
Crystal.

To select an earlier archive state, pass an eight-digit commit ID:

```sh
zarmount --to 00000001 archive.zar /tmp/zar-mount   # Linux
zarmount --to 00000001 archive.zar C:\zar-mount       # Windows
```

## Encrypted archives

Supply a default password file and, if necessary, per-commit password files:

```sh
zarmount --password-file /secure/default-password \
  --password-for 00000002=/secure/commit-2-password archive.zar /tmp/zar-mount
```

Each file contains password bytes, optionally followed by a single newline
(LF or CRLF). Empty passwords are rejected. Restrict the files' permissions;
never put password contents in command arguments. A commit-specific password
overrides the default. Missing or incorrect credentials cause the mount to
fail rather than silently presenting an incomplete archive. In Linux daemon
mode, if no password files are supplied, encrypted archives prompt for
commit-specific passwords.

## Behavior and limitations

- The archive must not be modified, replaced, or removed while mounted.
  Detected changes cause subsequent file reads to fail with an I/O error;
  unmount and mount again to see a new state.
- On Linux the filesystem is mounted read-only, `nodev`, `nosuid`, and
  `noexec`. Files and directories are presented as mode `0444` and `0555`,
  owned by the mounting user. Archived permissions, ownership, and timestamps
  are not restored; timestamps are reported as the Unix epoch.
- On Windows the filesystem is read-only via ProjFS notification cancellation.
- Symbolic-link targets are presented as stored on Linux. Absolute or
  parent-relative links can resolve outside the mount; use care with untrusted
  archives. Archives containing FIFOs or device nodes are rejected.
- Mount preparation validates the selected archive state. Reads decode only
  the required blocks, verify their checksums and authentication, and retain
  at most one decoded block in memory. Partial reads cannot verify a whole-file
  checksum. The operating system's page cache is bypassed.
- This implementation supports Linux (FUSE) and Windows (ProjFS). It does not
  mount ZIP, 7z, TAR, or other import formats directly.

## Tests

```sh
cargo test -p zarmount
cargo test -p zarmount kernel_mount_is_read_only_and_serves_random_reads -- --ignored
cargo test -p zarmount --test fuse_cli -- --ignored
cargo test -p zarmount --test linux_tray -- --ignored
```

The ignored tests perform actual mounts and require an accessible FUSE device
and helper. The tray test also requires `dbus-run-session` and `gdbus`.
