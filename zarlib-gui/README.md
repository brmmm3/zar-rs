# `zarg` — ZAR Archive Manager

`zarg` is the native egui graphical companion to [`zarc`](../zarlib-cli/README.md).
It uses the same `zarlib` operations, so GUI-created archives use the same
format and validation as archives created by the CLI.

## Run

From the workspace root:

```sh
cargo run --release -p zarlib-gui
```

For a release build using mimalloc, run `cargo build --release -p zarlib-gui --features mimalloc`.

The installer file-manager actions use the same startup interface as
`zarg --context ACTION -- PATH...`, where `ACTION` is `create`, `add`,
`create-named`, `open`, `extract-here`, `extract-to`, `extract-to-folder`, or
`test`. The `extract-to-folder` action extracts `foo.zar` into a sibling
directory named `foo` (the archive filename without `.zar`). The normal
no-argument launch opens the main window with a new, empty, unsaved archive
using the configured defaults. Opening a `.zar` file directly opens that
archive in the manager instead.
Save and Save as are unavailable while the latest archive state is empty, and
switching away from an empty archive does not prompt about unsaved changes.

The application is a desktop program built with eframe/egui. It uses a Qt6
Fusion appearance and follows the desktop's light or dark mode: Windows and
macOS report the theme through winit, while on Linux it is read from the XDG
settings portal (falling back to `gsettings`, KDE `kdeglobals`, GTK settings,
and `GTK_THEME`) and re-checked every few seconds, so switching the system
theme updates the running app. If no theme can be determined, light is used. On
Linux, native
file selection uses the XDG desktop portal. The PNG app icon and selected
Crystal and Flat icons for buttons and archive entry types are embedded in
the window binary. Select an icon theme in Settings; Crystal is the default.
Icon attribution and license information is in
[`assets/crystal/README.md`](./assets/crystal/README.md). Wayland requires a
matching desktop entry to show the app icon in the dock or app switcher;
`data/zarg.desktop` provides the entry. After installing the `zarg` executable,
install the launcher and icon with:

```sh
install -Dm644 zarlib-gui/data/zarg.desktop ~/.local/share/applications/zarg.desktop
install -Dm644 zarlib-gui/assets/zarg-icon.png ~/.local/share/icons/hicolor/256x256/apps/zarg.png
install -Dm644 zarlib-gui/data/zarg-error.desktop ~/.local/share/applications/zarg-error.desktop
install -Dm644 zarlib-gui/assets/zarg-error.png ~/.local/share/icons/hicolor/64x64/apps/zarg-error.png
```

The error dialog uses the `zarg-error` desktop identity so Wayland compositors
such as KDE Plasma can resolve its window and taskbar icon from the matching
desktop entry.

## Features

- Open and browse archives in a virtualized list with MIME-style icons,
  compressed/uncompressed sizes, ratio, and saved owner/permission metadata
  when available.
- Show the open archive's filename in the main window title.
- Select a commit beside the entry count to browse the archive through that
  commit; the latest commit is selected by default. Extraction and opening a
  file use the displayed revision. Removing entries requires the latest view;
  archive changes return the view to the latest commit.
- The idle status bar shows the selected revision's entry count, estimated
  compressed and uncompressed file sizes, and aggregate compression ratio,
  alongside the total number of commits in the archive. Unavailable estimates
  are shown as a dash; filters do not change these totals.
- Switch the archive contents between Normal icon tiles, Compact multi-column
  rows, Details with size, ratio, owner, permissions, and type columns, and a
  Structured expandable directory tree. Structured view shows file sizes and
  folder item counts. In Details and Structured views, Modified, Created,
  Accessed, Owner, and Permissions columns appear only when the displayed
  archive revision contains that metadata. In Details view, time columns follow
  the Type column.
  Compressed sizes are estimates based on the average compression ratio of
  their blocks; packed files use their shared block's ratio. Encrypted block
  ratios are unavailable unless the commit can be unlocked.
- Create and Add can optionally store modification, creation, and access times.
- Sort archive contents by name, modification, creation, or access time,
  compressed size, uncompressed size, ratio, or type. Time-based sort options
  appear only when the displayed archive revision contains that timestamp.
  When a saved time sort is unavailable, the list temporarily sorts by name.
  By default, times and sizes sort newest/largest first, ratio sorts lowest
  (best compression) first, and entries without the value come last. Reversing
  a sort reverses that order.
- In Details and Structured views, click a column header to sort by it (click
  again to reverse; a small arrow shows the active order) and drag the column
  borders to resize columns. Column widths and the main window's normal size
  and position are saved on exit. The position is ignored if the monitor count
  changed or cannot be determined; some window systems do not permit apps to
  restore window positions.
- Double-click a file to extract it to a temporary folder and open it with the
  operating system's default program.
- Add, edit and remove custom archive metadata (key/value pairs) from the
  Custom metadata dialog.
- Keep the 10 most recently opened archives available from File → Recent
  archives.
- Show `.zar` files with the dedicated bundled ZAR archive icon.
- Virtualize large archive listings so only visible rows are rendered.
- Create or append archives from selected files and folders with a dedicated
  options panel for ZAR's Zstandard compression level, block size,
  metadata/special-file handling, strict input checks, filters and overwrite
  behavior. Create and Add support AES-256-GCM or ChaCha20-Poly1305 encryption
  with header/data scope selection; Add can also unlock existing commits. A
  saved default password preselects encryption in these dialogs, which can still
  be unchecked.
- Include a `.zip`, `.7z`, `.tar`, `.tar.gz`, or `.tgz` file among Create or Add
  inputs to import its contents using the paths stored inside the archive.
- Open those formats directly for read-only browsing and extraction; zarg
  creates a temporary ZAR snapshot and leaves the source archive unchanged.
- Remove selected archive entries or paths.
- Join a contiguous range of commits, or remove an inclusive range through
  Tools → Remove commits. At least one commit must remain; removing commits
  can change the resulting history. When retained encrypted headers prevent
  validation, zarg reports a warning.
- Edit → Undo reverses unsaved session changes in action order, without
  removing saved commit history. Saving clears the undo history.
- Encrypt, decrypt, or change encryption for commits through a unified dialog
  (Tools → Change commits). Choose an inclusive commit range and optionally
  write the result to a different archive without changing the open archive.
  Join and Remove commits also support a separate output archive. Leave "Write to" empty
  for an in-place staged change; an output path must differ from the open
  archive. Encryption offers AES-256-GCM or
  ChaCha20-Poly1305, with header/data scope selection.
- Test archives, optionally recovering past damaged framing.
- View commit information, including stored and uncompressed sizes, with
  optional verbose statistics or JSON output.
- List the latest archive state or select a commit and unlock encrypted
  headers with a password.
- Opening an archive with encrypted commits prompts for each commit's password.
  Leave a password blank to keep that commit locked, or copy the first
  password to all encrypted commits. Cancel leaves the current archive open.
  Successfully unlocked commit passwords are remembered in memory for the
  application session and reused in the Info and Add dialogs. Reopening the
  archive prefills its password fields; passwords are not saved to disk.
- Extract with collision and existing-file policies, optional metadata,
  symlink/device restoration, dry run, and commit selection. The Extract
  dialog has General, Advanced, and Filter tabs, plus an Encryption tab when
  the archive contains encrypted commits. Its default destination is the
  archive's containing folder, or the current directory for an unsaved archive.
- Configure block size, compression, strict input handling, special files,
  overwrite, and the CLI's fnmatch include/exclude filters and filter lists.
- Set defaults for metadata, dates, special files, strict input validation,
  username recording, and include/exclude patterns in Settings. These defaults
  prefill New archive, whose options are grouped into General, Advanced,
  Encryption, and Filters tabs. The first Add dialog for a new archive uses
  the New archive dialog's settings; for an opened archive it uses Settings defaults. Later
  Add dialogs remember their settings until another archive is created or opened
  or the application exits, including when the dialog is canceled. Selected
  input files and new-commit encryption passwords are not remembered.
- Settings are grouped into Appearance, Compression, Advanced, Encryption,
  and Filters tabs.
- New archive prompts to save unsaved changes only when the latest state is
  nonempty and the current archive's first commit contains entries.
  Add to archive groups its file list, compression, advanced, unlock,
  encryption, and filter options into separate tabs.
  The Unlock tab appears only for archives with encrypted commits and lets
  you select each commit and enter its password separately.
  Username recording applies only to new archives. Extraction filters remain
  independent.
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

To measure compression and extraction of a saved archive with different
settings, see
[Benchmark an archive in zarg](../doc/benchmarks/README.md#benchmark-an-archive-in-zarg).
