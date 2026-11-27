# AGENTS.md (zarmount - Virtual Filesystem Mount Tool)

`zarmount` exposes ZAR archives as read-only virtual filesystems without extracting them to disk. It displays the latest commit by default, or an earlier state selected with `--to <commit_id>`.

## Build & Test Instructions
- **Build**: `cargo build --bin zarmount` (or `--release`)
- **Run tests**: `cargo test -p zarmount`
- **Lint**: `cargo clippy -p zarmount`
- **Format**: `cargo fmt`

## Platform Implementations

### 1. Linux (FUSE 3)
- Utilizes `/dev/fuse` and `fusermount3`.
- By default runs as a per-user daemon with system tray integration (StatusNotifierItem).
- Foregound mode: `zarmount --foreground <archive.zar> <mountpoint>`.
- Supports automated temporary directory creation and desktop folder opening via `xdg-open`.

### 2. Windows (ProjFS - Projected File System)
- Requires the Windows Optional Feature `Client-ProjFS`.
- Mounts directly to an empty target directory in the foreground.
- Symbolic links inside archives are followed internally up to 64 levels.
- Supports UAC elevation helpers for administrative mounts (`elevate.rs`).

## Key CLI Options
- `zarmount <archive.zar> [mountpoint]`: Mount archive at mountpoint (creates temporary directory if omitted).
- `--ask`: Displays a directory picker dialog to select the mount target.
- `--to <commit_id>`: Mounts an earlier historical commit state.
- `--foreground`: Runs the mount process in the foreground.
