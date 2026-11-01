# Changelog

Notable changes to ZAR are recorded here. Entries remain under **Unreleased**
until a version is released.

## Unreleased

## 0.9.0 - 2026-??-??

### Added

- Added `.zip`, `.7z`, `.tar` and `.tar.gz` input support for create and add
  operations across the CLI, GUI, and language bindings. Entries retain their
  internal paths; safe `.` components are normalized, while traversal, links,
  and special entries are rejected.

### Changed

- Bumped the project version to 0.9.0.

## 0.8.0 - 2026-11-01

### Added

- Added custom commit-level metadata: `zarc meta` lists, sets (`--set KEY=VALUE`)
  and removes (`--remove KEY`) keys, each change stored as a header-only commit.
  Join carries the effective metadata into the merged commit; keys starting with
  `zar.` are reserved. `zarc info --meta` prints the pairs.
- Added `zarc extract --strip-top-level` and the matching `zarg` option and
  `zarlib` `ExtractionOptions::strip_top_level` to extract without the archive's
  top-level folder.
- Added fast single-file extraction in `zarlib`, used by `zarg` double-click open.
- `zarg`: graphical Info window, Custom metadata dialog, Benchmark dialog with
  cancel, About dialog, Recent archives menu (last 10, stale paths pruned in the
  background), filter field with regex and case toggles, Invert Selection,
  resizable/sortable columns with horizontal scrolling, and persisted settings
  and window geometry.
- `zarg` Join dialog uses commit comboboxes (through never precedes from) and can
  write the result to another file.
- Added opt-in per-entry modification timestamps (`zarc create/add --dates`,
  a `zarg` checkbox, and `save_dates` in the bindings) stored in
  logical-header version 7, plus sorting by name, date, compressed size, uncompressed size, ratio, and type in
  `zarg`.
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
