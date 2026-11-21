# zarmount

`zarmount` exposes a ZAR archive as a read-only Linux FUSE filesystem.
It shows the latest commit by default, or an earlier state selected with
`--to`. Directories, regular files, empty files, and symbolic links are
available without extracting the archive to disk.

## Build and mount

Install FUSE 3 and its `fusermount3` helper, and ensure the current user can
access `/dev/fuse`. Build with the Rust toolchain:

```sh
cargo build --release -p zarmount
mkdir -p /tmp/zar-mount
target/release/zarmount archive.zar /tmp/zar-mount
```

The mount runs in the foreground. Press Ctrl-C to unmount and exit, or in
another terminal browse or read files and unmount it:

```sh
ls -la /tmp/zar-mount
fusermount3 -u /tmp/zar-mount
```

SIGTERM and SIGHUP also trigger a graceful unmount. If the process is killed
forcibly or crashes, recover a disconnected mountpoint with
`fusermount3 -u /tmp/zar-mount`.

Linux binary release archives also contain `zarmount`. The mountpoint must be
an existing empty directory, and the archive must not be inside it. The
archive path must be UTF-8. Only the mounting user can access the filesystem;
there is no `allow_other` option.

To select an earlier archive state, pass an eight-digit commit ID:

```sh
target/release/zarmount --to 00000001 archive.zar /tmp/zar-mount
```

## Encrypted archives

Supply a default password file and, if necessary, per-commit password files:

```sh
target/release/zarmount --password-file /secure/default-password \
  --password-for 00000002=/secure/commit-2-password archive.zar /tmp/zar-mount
```

Each file contains password bytes, optionally followed by a single newline
(LF or CRLF). Empty passwords are rejected. Restrict the files' permissions;
never put password contents in command arguments. A commit-specific password
overrides the default. Missing or incorrect credentials cause the mount to
fail rather than silently presenting an incomplete archive.

## Behavior and limitations

- The archive must not be modified, replaced, or removed while mounted.
  Detected changes cause subsequent file reads to fail with an I/O error;
  unmount and mount again to see a new state.
- The filesystem is mounted read-only, `nodev`, `nosuid`, and `noexec`.
  Files and directories are presented as mode `0444` and `0555`, owned by
  the mounting user. Archived permissions, ownership, and timestamps are not
  restored; timestamps are reported as the Unix epoch.
- Symbolic-link targets are presented as stored. Absolute or parent-relative
  links can resolve outside the mount; use care with untrusted archives.
  Archives containing FIFOs or device nodes are rejected.
- Mount preparation validates the selected archive state. Reads decode only
  the required blocks, verify their checksums and authentication, and retain
  at most one decoded block in memory. Partial reads cannot verify a whole-file
  checksum. The operating system's FUSE page cache is bypassed.
- This implementation supports Linux only. It does not mount ZIP, 7z, TAR,
  or other import formats directly.

## Tests

```sh
cargo test -p zarmount
cargo test -p zarmount kernel_mount_is_read_only_and_serves_random_reads -- --ignored
cargo test -p zarmount --test fuse_cli -- --ignored
```

The second command performs an actual mount and requires an accessible FUSE
device and helper. Its temporary mount is removed when the test ends.
