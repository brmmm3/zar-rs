# `zarg` — ZAR Archive Manager

`zarg` is the native egui graphical companion to [`zarc`](../zarc/ReadMe.md).
It uses the same `zarlib` operations, so GUI-created archives use the same
format and validation as archives created by the CLI.

## Run

From the workspace root:

```sh
cargo run --release -p zarg
```

The application is a desktop program built with eframe/egui. On Linux, native
file selection uses the XDG desktop portal. The PNG icon is embedded in the
window binary. Wayland requires a matching desktop entry to show the app icon
in the dock or app switcher; `data/zarg.desktop` provides the entry. After
installing the `zarg` executable, install the launcher and icon with:

```sh
install -Dm644 zarg/data/zarg.desktop ~/.local/share/applications/zarg.desktop
install -Dm644 zarg/assets/zarg-icon.png ~/.local/share/icons/hicolor/256x256/apps/zarg.png
```

The window title-bar icon is platform-controlled and may not be displayed
under Wayland.

## Features

- Open and browse archives with file type and logical size.
- Create or append archives from selected files and folders.
- Remove selected archive entries or paths.
- Join a contiguous range of commits.
- Test archives, optionally recovering past damaged framing.
- View commit information, including stored and uncompressed sizes, with
  optional verbose statistics or JSON output.
- List the latest archive state or select a commit and unlock encrypted
  headers with a password.
- Extract with collision and existing-file policies, optional metadata,
  symlink/device restoration, dry run, and commit selection.
- Configure block size, compression, strict input handling, special files,
  overwrite, and the CLI's fnmatch include/exclude filters and filter lists.
- Run long operations in a worker thread, with progress, warnings, results, and
  confirmation dialogs reported in the application.

Operations are provided by `zarlib`; encryption, file type and filesystem
safety behavior follows the library. In particular, archive append and
compaction retain the same encrypted-header limitations as `zarc`.
