# Changelog

Notable changes to ZAR are recorded here. Entries remain under **Unreleased**
until a version is released.

## Unreleased

### Changed

- Retained partial results when cancelling a zarg Find search and added Escape
  as a cancellation shortcut.
- Enabled system clipboard support in edit fields and added Alt+F to open Find
  from the main window.
- Disabled Find searches when the search field is empty.
- Added case, whole-word, regex, and chained-search toggles to the zarg Find
  dialog; Enter in its filename glob now starts a search.
- Grep now filters before scheduling workers, streams results, and reuses
  unlocked headers for repeated searches in zarg and the zarc grep shell.
- Switched zarlib grep to fancy-regex, adding lookaround and backreference
  support. Inline Unicode mode changes such as `(?-u:...)` are no longer
  supported, and invalid UTF-8 bytes cannot be matched individually.

### Added

- Added `/regex`, `/case`, and `/word` search settings to the zarc grep shell.
- Added `zarc grep --shell` for interactive archive path and content searches
  with session command history, adjustable context, chained searches, and
  Escape cancellation.
- Added Ctrl+I to invert the zarg file-list selection and Enter to open the
  first selected entry, alongside existing Ctrl+A and Delete shortcuts.
- Added zarg **Edit > Find**, with themed icons and a virtualized result list,
  to search archive file contents by glob and regex, with optional surrounding
  lines and double-click to open matching files.
- Added zarg Performance settings for archive worker CPU priority and maximum
  CPU count, with live updates for new worker pools.

## 0.17.0 - 2026-11-24

### Added

- Added `ZARLIB_CPU_PRIORITY` to lower the scheduling priority of zarlib worker
  threads on Linux, macOS, and Windows.
- Added `ZARLIB_MAX_CPU_COUNT` to configure parallel compression and extraction
  workers, with negative values reserving cores.
- Added a NixOS package expression for all five binaries and GUI desktop assets.
- Added interoperable single-entry TCP streaming server and client examples
  for the language bindings and the Node.js WASI adapter.
- Added archive grep to zarlib, zarc, and the language bindings, with filename
  glob filters, regex matching, and optional before/after line context. Searches
  run in parallel on native targets and sequentially on WASI.

### Changed

- Updated the `scandir` dependency to 4.0 in zarlib and zcopy.

## 0.16.0 - 2026-11-21

### Fixed

- Unmount `zarmount` cleanly on Ctrl-C, SIGTERM, and SIGHUP.
- Fix GitHub Pages repository links to use the default branch.

### Added

- Added an Arch Linux PKGBUILD for `zarc`, `zarg`, `zcopy`, `zarmcp`, and
  `zarmount`.
- Added cargo-binstall metadata for `zarc`, `zarg`, `zcopy`, `zarmcp`, and
  Linux `zarmount`; release archives now include `zarmcp`.
- Added `zarmount`, a read-only Linux FUSE filesystem for ZAR archives.
- Added Lua 5.4 bindings with archive operations, streaming, examples, and tests.
- Added seekable-archive streaming APIs for creating archives from readers and
  extracting individual files to writers.
- Added streaming APIs, examples, and tests for the C, C++, C#, Go, Java,
  Python, and WASI/Node.js bindings.
- Added TCP streaming examples for individual files and case-sensitive
  selections from a directory of ZAR archives. The client supports archive
  listing, repeated fnmatch patterns, overwrite control, and a zarc-style
  progress bar. The server sends files as they are prepared and logs requests
  unless quiet mode is enabled.
- Added a Linux streaming bandwidth test for archive and extracted-tree
  transfers over isolated, rate-limited loopback, with CPU and RAM sampling.

## 0.15.0 - 2026-11-20

### Added

- Added `zarg --context` actions for creating, adding, opening, extracting, and
  testing archives from file-manager context menus.
- Added resumable archive benchmarks with per-run results in `.benchmark_cache`,
  progress-numbered filenames, and cache cleanup after successful report generation.
- Added benchmark coverage for the Linux 4.0.1 and precompiled Linux 6.1.1
  trees, plus the `duplicates`, `icons`, and `small_archive` datasets.
- Added `--dataset` to run a benchmark for one packaged dataset while preserving
  cached results for the other datasets.
- Added CPU-model and GitHub Actions postfixes to benchmark report and chart names.
- Added benchmark run counters, completion percentages, and periodic elapsed-time
  progress for long-running archive benchmark steps.
- Added `scripts/update_version.py` to update current package versions, lockfiles,
  and release URLs together, with a dry-run mode and unchanged download checksums.
- Added per-command defaults from `zarc.json`, searched in the current folder
  before the per-user configuration directory, and `--no-config` to skip it.
- Added `zarc extract --recheck-block-checksums auto|always|never`, defaulting
  to Auto with the same filesystem detection as `zarg`.

### Changed

- Run each archive benchmark dataset as a separate GitHub Actions job for every
  enabled platform, with optional dataset and OS selectors that default to all
  and a unique artifact per platform and dataset.
- Allow one missing extracted file on Windows benchmark runs while continuing to
  reject extra files, multiple missing files, and size mismatches.
- Include the Linux 4.0.1 dataset name in generated benchmark report and chart
  filenames, like the other datasets.
- Scale saved window dimensions and position by the configured UI scale when
  restoring the startup geometry.
- Reuse validated archive headers and the table of contents throughout extraction
  instead of reparsing headers for reference discovery and source preparation.
  Filter validated TOC entries directly instead of replaying them as new commits.

### Fixed

- Fix archive creation when an input directory contains a child with the same
  name as the input root.
- Accept 7-Zip's errors for a subset of source-tree symlinks while still failing
  other extraction errors.

## 0.14.0 - 2026-11-18

### Added

- Added `CrossCommitReference` and the `cross_commit_references` and
  `cross_commit_references_with_passwords` APIs to inspect references between
  commits in an archive.
- Added Delete-key removal of selected files and folders from the `zarg` main
  window's file tree.
- Added reference listings to `zarc info`, capped at 10 unless `-vv` is used,
  and a counted, collapsible cross-commit reference section to the `zarg` Info
  dialog.
- Added cancellation to archive testing in the `zarg` GUI.

### Changed

- Default `zarg` extraction to automatically rechecking block checksums on
  network and older filesystems, with Always and Never options in Settings.
- Validate dependencies between commits before removal, track file origins and
  source commits in `Toc`, and require passwords for both sides of cross-commit
  references during archive transformations.

### Fixed

- Improve errors for duplicate commit IDs and invalid source commits.
- Enable dragging archive entries out of `zarg` on X11.

## 0.13.0 - 2026-11-15

### Added

- Added APIs to read and update custom archive-level metadata in the Python,
  C, C++, Go, Java FFM/JNI, and C# bindings. Updates append a metadata-only
  commit.

### Fixed

- Fixed macOS JNI native-library linking and added a distribution smoke test.
- Fixed Java native-library artifact layout in CI.

## 0.12.0 - 2026-11-14

### Added

- Added drag-and-drop of files and folders into `zarg` and extraction by
  dragging archive entries out on Windows, macOS, and Wayland.
- Added an option to extract only the entries selected in `zarg`.

### Changed

- Publish v3 and baseline v1 assets to one `x.y.z` release through sequential
  CI and packaging workflows. Baseline wheels are grouped in `-v1` ZIP assets.

## 0.11.0 - 2026-11-12

### Changed

- Normal CI and packaging tag-push releases now use tags matching `*.*.*-v3`;
  baseline x86-64 releases use a separate stable source tag and `-v1` release.
- Renamed the CLI and GUI workspace packages to `zarlib-cli` and
  `zarlib-gui`; their executables remain `zarc` and `zarg`.
- Renamed the `zcopy` workspace package to `zarlib-zcopy`; its executable
  remains `zcopy`.
- Write and read only logical-header version 8. Archives containing earlier
  unreleased logical-header versions 0–7 are no longer supported.
- Renamed the create/add `--dates` option to `--modification-time` (`--mt`)
  and the corresponding public `save_dates` option to
  `save_modification_time`. The unreleased version 8 entry-metadata layout
  changed in place; archives using its earlier layout may be unreadable.

### Added

- Added local winget, Chocolatey, and Scoop package definitions for `zarc`,
  `zarg`, and `zcopy`; their installation behavior differs by package.
- Added a separate `x.y.z-v1` GitHub release for baseline x86-64 binaries
  and Python wheels, with Linux, Windows, and macOS installer packages.
- Added `zarmcp`, a local stdio MCP server for inspecting, reading, and
  managing ZAR archives, including password-encrypted commits and configurable
  read-only or tool-allowlist restrictions.
- Added ISO 9660 image import for create/add, direct extraction, and `zarc convert`,
  including Rock Ridge and Joliet filenames. ISO links and special entries are rejected.
- Added RAR archive import for create/add, direct extraction, and `zarc convert`.
  Encrypted and multi-volume RAR archives, links, and special entries are rejected.
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

- Build x86-64 v3 release binaries and packages with explicit v3 CPU flags,
  preserve the CPU target for static builds, and include `zcopy` in reusable
  Windows binary archives. Distinguish v3 package filenames from v1 packages.
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
