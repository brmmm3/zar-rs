# AGENTS.md (zarg - Graphical Archive Manager)

`zarg` is the cross-platform GUI archive manager for the ZAR system, built with `eframe` (`egui`). It supports visual archive inspection, staging changes, commit history browsing, and standalone context menu action dialogs.

## Build & Test Instructions
- **Build**: `cargo build --bin zarg` (or `--release`)
- **Run tests**: `cargo test -p zarlib-gui --bin zarg`
- **Lint**: `cargo clippy -p zarlib-gui`
- **Format**: `cargo fmt`

## Invocation Modes

### 1. Normal GUI Mode
- `cargo run --bin zarg [-- <archive.zar>]`
- Opens the full main window (menus, toolbar, entry tables, and status bar).
- Restores geometry from settings when available.

### 2. Context Action Modes (Shell Integration)
- `--context create` (or `--context create-archive`) `-- <paths...>`:
  - Opens the main window with the **New archive** dialog centered and layered on top (`AlwaysOnTop`).
  - Pre-fills input paths and suggests a target archive name based on the first item.
- `--context extract-here -- <archive.zar>`:
  - Bypasses the main window completely.
  - If the archive contains encrypted commits, shows the **Password Required** dialog first.
  - Switches to a centered progress dialog with progress bar and **Cancel** button.
  - Automatically closes and exits `zarg` upon completion or cancellation.
- `--context extract-to -- <archive.zar>`:
  - Bypasses the main window completely.
  - Displays a centered standalone **Extract** dialog (Destination folder picker, options, and encryption settings).
  - Clicking "Extract" transitions to the progress dialog and extracts the files.
  - Cancelling or finishing the operation exits `zarg`.
- `--context extract-to-folder -- <archive.zar>`:
  - Extracts the archive into a sibling folder named after the archive.
- `--context test -- <archive.zar>`:
  - Opens the test archive dialog directly.

## Architecture & Code Structure
- **App State**: `ZargApp` (`ui.rs`) implements `eframe::App` and manages the main window, background worker thread channels (`mpsc`), selection, and staging caches.
- **Dialogs (`ui/dialogs/`)**:
  - `operation.rs`: New, Add, Extract, Join, Remove, and standalone extract dialogs.
  - `open_passwords.rs`: Unlock commits, password prompts, and `extract-here` password dialog.
  - `confirmation.rs`: Overwrite prompts and unsaved changes confirmation.
  - `archive_info.rs`, `benchmark.rs`, `find.rs`, `settings.rs`, `operation_error.rs`.
- **Theming**: `theme.rs` provides Light, Dark, Catppuccin flavors (Latte, Frappé, Macchiato, Mocha), and System auto-detection.
- **Window Positioning**: `centered_dialog_viewport_builder` and `center_extract_here_window` utilize `display-info` to center dialog viewports on the active monitor.
- **Windows Integration (`windows_move.rs`)**: Win32 window subclassing for smooth dragging (bypassed in test environments via `cfg!(test)`).

## UI Testing Patterns
- Test headless UI frames using `context.run_ui(raw_input, |ui| ...)` followed by `.drop_without_applying_deltas()`.
- Use mock frames via `eframe::Frame::_new_kittest()`.
- Background operations communicate via channels; use `wait_for_worker(&mut app)` in tests to step background tasks to completion.
