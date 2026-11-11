# Changelog

Notable changes to ZAR are recorded here. Entries remain under **Unreleased**
until a version is released.

## Unreleased

### Changed

- Write and read only logical-header version 8. Archives containing earlier
  unreleased logical-header versions 0–7 are no longer supported.
- Renamed the create/add `--dates` option to `--modification-time` (`--mt`)
  and the corresponding public `save_dates` option to
  `save_modification_time`. The unreleased version 8 entry-metadata layout
  changed in place; archives using its earlier layout may be unreadable.

### Added

- Added opt-in creation- and access-time recording for create/add via
  `--creation-time` (`--ct`), `--access-time` (`--at`), and binding options
  `save_creation_time` and `save_access_time`. Extraction does not restore
  timestamps; recording fails when a requested timestamp is unavailable.
- `zarg` now has a Qt6 Fusion appearance and follows the system light/dark
  theme, including runtime changes. On Linux the theme is detected through the
  XDG settings portal with desktop-specific fallbacks, since winit does not
  report it there.
- Added undo support to remove a positive number of trailing commits while
  retaining the first commit. The operation is available in `zarc`, `zarg`,
  Python, C/C++, Go, Java FFM/JNI, C#, and WASI interfaces.

### Fixed

- Small files are now packed together into configured-size blocks. Logical
  header version 8 records each packed file's offset, preserving extraction and
  join behavior.
- The `zarg` operation dialogs (Create, Add, Extract, ...) keep their action
  buttons pinned to the bottom, so Create and Cancel stay visible when the
  dialog is small; only the form fields scroll.

## 0.9.0 - 2026-11-11

### Added

- Added `.zip`, `.7z`, `.tar`, `.tar.gz`, and `.tgz` input support for create
  and add operations across the CLI, GUI, and language bindings. Entries retain
  their internal paths; safe `.` components are normalized, while traversal,
  links, and special entries are rejected.

### Changed

- Bumped the project version to 0.9.0.

## 0.8.0 - 2026-11-01

### Added

- Added custom commit-level metadata: `zarc meta` lists, sets
  (`--set KEY=VALUE`) and removes (`--remove KEY`) keys, each change stored as a
  header-only commit. Join carries the effective metadata into the merged
  commit; keys starting with `zar.` are reserved. `zarc info --meta` prints the
  pairs.
- Added `zarc extract --strip-top-level` and the matching `zarg` option and
  `zarlib` `ExtractionOptions::strip_top_level` to extract without the archive's
  top-level folder.
- Added fast single-file extraction in `zarlib`, used by `zarg` double-click
  open.
- `zarg`: graphical Info window, Custom metadata dialog, Benchmark dialog with
  cancel, About dialog, Recent archives menu (last 10, stale paths pruned in the
  background), filter field with regex and case toggles, Invert Selection,
  resizable/sortable columns with horizontal scrolling, and persisted settings
  and window geometry.
- `zarg` Join dialog uses commit comboboxes (through never precedes from) and
  can write the result to another file.
- Added opt-in per-entry modification timestamps (`zarc create/add --dates`, a
  `zarg` checkbox, and `save_dates` in the bindings) stored in logical-header
  version 7, plus sorting by name, date, compressed size, uncompressed size,
  ratio, and type in `zarg`.
- Commit headers now store the compressed size of each file group (as
  `zar.group.compressed_size.<group>` header metadata), so `zarc info -v` reads
  only the headers. Archives without these entries fall back to scanning the
  data section.
- Added `wasizarlib`, a server-side WASI Preview 2 component with a WIT
  interface backed by the existing JSON operation API.
- Added Node.js support for `wasizarlib` using Jco and the WASI Preview 2 shim.
- Added CI and release artifacts for `wasizarlib`.
- Added cross-platform CI builds for `zarc` and `zarg`, including Linux ARM,
  Linux static, and Windows ARM targets.
- Added OS-specific Java JAR builds for Linux, macOS, and Windows.
- Added Windows x64 and ARM64 C# NuGet packages.
- Added Python CI coverage for CPython 3.15 and free-threaded 3.15t.
- Added PyPI and TestPyPI publishing workflows for `pyzarlib`.

### Changed

- Removed the `zarg` Favorites and Archive menus.
- Bumped the project version to 0.8.0.
- Updated the Java bindings to target Java 25.
- Added WASI-specific directory traversal and sequential archive/extraction
  processing for runtimes without worker threads.

### Fixed

- Kept extraction dry runs from creating their destination directories and
  displayed paths relative to the requested destination in the Python CLI.
- Activated a virtual environment before running `maturin develop` in CI.
- Made the C integration workflow derive commit IDs from archive metadata
  instead of relying on fixed IDs.
- Addressed Clippy warnings in the archive-processing code.
