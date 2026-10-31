# Changelog

Notable changes to ZAR are recorded here. Entries remain under **Unreleased**
until a version is released.

## Unreleased

### Added

- Added `wasizarlib`, a server-side WASI Preview 2 component with a WIT
  interface backed by the existing JSON operation API.
- Added CI and release artifacts for `wasizarlib`.
- Added cross-platform CI builds for `zarc` and `zarg`, including Linux ARM,
  Linux static, and Windows ARM targets.
- Added OS-specific Java JAR builds for Linux, macOS, and Windows.
- Added Windows x64 and ARM64 C# NuGet packages.
- Added Python CI coverage for CPython 3.15 and free-threaded 3.15t.
- Added PyPI and TestPyPI publishing workflows for `pyzarlib`.

### Changed

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
