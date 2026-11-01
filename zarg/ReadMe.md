# `zarg` — ZAR Archive Manager

`zarg` is the native egui graphical companion to [`zarc`](../zarc/ReadMe.md).
It uses the same `zarlib` operations, so GUI-created archives use the same
format and validation as archives created by the CLI.

## Run

From the workspace root:

```sh
cargo run --release -p zarg
```

The installer file-manager actions use the same startup interface as
`zarg --context ACTION -- PATH...`, where `ACTION` is `create`, `add`,
`create-named`, `open`, `extract-here`, `extract-to`, `extract-to-folder`, or
`test`. The `extract-to-folder` action extracts `foo.zar` into a sibling
directory named `foo` (the archive filename without `.zar`). The normal
no-argument launch still opens the main window. Opening a `.zar` file directly
also opens it in the manager.

The application is a desktop program built with eframe/egui. On Linux, native
file selection uses the XDG desktop portal. The PNG app icon and selected
KDE4 Crystal Diamond icons for buttons and archive entry types are embedded in
the window binary. Icon attribution and license information is in
[`assets/icons/README.md`](./assets/icons/README.md). Wayland requires a
matching desktop entry to show the app icon in the dock or app switcher;
`data/zarg.desktop` provides the entry. After installing the `zarg` executable,
install the launcher and icon with:

```sh
install -Dm644 zarg/data/zarg.desktop ~/.local/share/applications/zarg.desktop
install -Dm644 zarg/assets/zarg-icon.png ~/.local/share/icons/hicolor/256x256/apps/zarg.png
```

The window title-bar icon is platform-controlled and may not be displayed
under Wayland.

## Features

- Open and browse archives in a virtualized list with MIME-style icons,
  compressed/uncompressed sizes, ratio, and saved owner/permission metadata
  when available.
- Switch the archive contents between Normal icon tiles, Compact multi-column
  rows, Details with size, ratio, owner, permissions, and type columns, and a
  Structured expandable directory tree. Structured view shows file sizes,
  folder item counts, and stored modification times. Modified, Owner, and
  Permissions columns appear only when the archive stores that metadata.
- Create and Add can store modification dates with the "Save modification dates"
  option.
- Sort archive contents by name, modification date, compressed size,
  uncompressed size, ratio, or type. Dates and sizes sort
  newest/largest first, ratio sorts lowest (best compression) first, and entries
  without the value come last.
- In Details and Structured views, click a column header to sort by it (click
  again to reverse; a small arrow shows the active order) and drag the column
  borders to resize columns.
- Double-click a file to extract it to a temporary folder and open it with the
  operating system's default program.
- Add, edit and remove custom archive metadata (key/value pairs) from the
  Custom metadata dialog.
- Keep the 10 most recently opened archives available from File → Recent
  archives.
- Show `.zar` files with the dedicated bundled ZAR archive icon.
- Virtualize large archive listings so only visible rows are rendered.
- Create or append archives from selected files and folders with a dedicated options panel for ZAR's Zstandard compression level, block size, metadata/special-file handling, strict input checks, filters and overwrite behavior. Create and Add support AES-256-GCM or ChaCha20-Poly1305 encryption with header/data scope selection; Add can also unlock existing commits.
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
- Use installed file-manager actions to create archives from selected items,
  open `.zar` files, extract, and test archives. “Add to Archive...” opens the
  create form and asks for a destination for a new archive. “Create Archive...”
  pre-fills the destination using the first selected item’s name with a `.zar`
  extension. “Extract to archive folder” extracts into a sibling directory
  named after the archive, without its `.zar` extension.
- Run long operations in a worker thread, with progress, warnings, results, and
  confirmation dialogs reported in the application.

Operations are provided by `zarlib`; encryption, file type and filesystem
safety behavior follows the library. In particular, archive append and
compaction retain the same encrypted-header limitations as `zarc`.
