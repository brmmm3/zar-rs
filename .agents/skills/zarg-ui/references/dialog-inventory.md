# Zarg dialog inventory

This is a source-based map of the current dialogs and their interaction
patterns. Recheck the linked implementation before applying a detail; these are
observations, not a requirement to reproduce every existing inconsistency.

## Shared appearance and interaction

- [`theme.rs`](../../../../zarlib-gui/src/theme.rs) sets Fusion-inspired
  colors, spacing and widget states for both light and dark visuals. System
  detects the desktop theme, including a Unix watcher. Catppuccin Latte is
  light; Frappe, Macchiato and Mocha are dark.
- [`settings.rs`](../../../../zarlib-gui/src/settings.rs) defines theme,
  Crystal/Flat icons and 1.0-2.0 UI scale. The
  [Settings dialog](../../../../zarlib-gui/src/ui/dialogs/settings.rs) applies
  theme and scale changes and clears the icon cache when the icon theme changes.
- [`ui.rs`](../../../../zarlib-gui/src/ui.rs) contains the `UiIcon` mapping,
  icon cache, 16-point icon buttons, exact Alt-only shortcut consumption,
  `close_or_cancel`, viewport geometry/focus helpers and root shortcut
  handling. `close_or_cancel` supplies Alt+C, not universal Escape.
- Most dialogs use a centered, always-on-top immediate child viewport with an
  opaque themed background, persistent size where supported, a bottom action
  panel, and scrolling central content. About and Rename are egui windows in
  the parent; `rfd` pickers and the native replace confirmation are platform
  dialogs. Individual dialogs also use different margins and close policies.

## Archive operation dialogs

[`operation.rs`](../../../../zarlib-gui/src/ui/dialogs/operation.rs) owns
`OperationKind`, per-kind size keys, forms and action buttons. New, Add and
Extract have tabs, scrollable content and sticky bottom actions. Cancel uses
Alt+C; New, Add and Extract also expose Alt+N, Alt+A and Alt+E respectively,
subject to the same enablement and nested-dialog gating as their buttons.

| Dialog / form | Layout and behavior |
|---|---|
| New archive | General, Advanced, Encryption, Filters tabs; input paths, compression, metadata/options, password confirmation and filter patterns. New action requires matching passwords and a nonempty password when encryption is enabled. |
| Add to archive | Files, Compression, Advanced, Unlock, Encryption, Filters tabs; add/remove input paths, per-commit unlock fields and encryption settings. Add requires inputs, valid encryption and passwords for selected locked commits. |
| Extract | General, Advanced, Encryption, Filter tabs; destination, commit, selection, collision/existing-file policies, restoration and validation options. Extract requires a valid archive and destination. |
| Join commits | Inclusive commit range, compression, block size and output path; only valid ranges enable Join. |
| Remove commits | Inclusive range and output path; at least one commit must remain. Remove requires a valid range. |
| Change commits | Encrypt, decrypt or re-encrypt an inclusive range, with existing/new password, encryption algorithm and scope, and output options; Apply validates the range and required passwords. |
| Undo | Fixed-size form for unsaved-session steps; requires a valid nonzero number of available steps. |
| Custom metadata | Edit/remove existing entries and add new key/value entries; reserved keys and value types are explained; Apply requires a valid archive and changes. |
| Test archive | Fixed-size form for recovery and password options; Test requires an existing archive. |
| List archive | Commit selection, password and filtering controls; List starts the corresponding operation. |
| Remove entries | An `OperationKind`, but `render_operation_dialog` deliberately does not render a dialog for it; removal is handled by the main archive workflow. |

Filter editors use a two-column include/exclude layout for file and folder
patterns. Encryption forms mask passwords, offer show-password controls and
report mismatches. Unlock indicators distinguish encrypted locked/unlocked
commits. Inspect the specific form for all fields and validation rules before
reusing it.

## Other dialogs

| Dialog | Source, layout, functionality and shortcuts |
|---|---|
| Settings | [settings.rs](../../../../zarlib-gui/src/ui/dialogs/settings.rs): resizable viewport; Appearance, Compression, Performance, Advanced, Encryption and Filters tabs; scrollable forms and sticky Close/Alt+C. Changes are applied to app state; password-safe actions report their own status. |
| Archive information | [archive_info.rs](../../../../zarlib-gui/src/ui/dialogs/archive_info.rs): scrollable summary, metadata, references and expandable commits; Close/Alt+C. Locked commits offer unlock/hide controls. Nested Unlock commit asks for a masked password and offers Unlock, Unlock all and Cancel/Alt+C, with busy and error feedback. |
| Open encrypted archive | [open_passwords.rs](../../../../zarlib-gui/src/ui/dialogs/open_passwords.rs): per-commit masked fields, copy-first-password control, Open and Cancel/Alt+C; busy state disables actions and intercepts window close. Empty fields leave commits locked. |
| Unlock commits | Same file: per-commit passwords and copy control; asynchronous verification, inline errors and busy state. Unlock requires an entered encrypted-commit password; Cancel/Alt+C is disabled while busy. Successful passwords are remembered. |
| Password Required, standalone extract | Same file: 48-point lock icon, single password field or scrollable commit rows; Enter submits when allowed, Escape or Cancel/Alt+C cancels; verification is asynchronous and success transitions to progress. |
| Extract here / convert progress | [operation.rs](../../../../zarlib-gui/src/ui/dialogs/operation.rs): standalone window with 48-point icon, heading, truncated status with tooltip, progress bar and Cancel. Escape or Alt+C requests cancellation and exit; extract progress only displays a percentage when known. |
| Extract to, standalone | [operation.rs](../../../../zarlib-gui/src/ui/dialogs/operation.rs) and [main.rs](../../../../zarlib-gui/src/main.rs): extract tabs and normal validation/actions in a separate startup window. Escape cancels and exits; Alt+E extracts. |
| Find | [find.rs](../../../../zarlib-gui/src/ui/dialogs/find.rs): glob, text/regex, case/word/chain toggles, context-line counts, scrollable grouped results and status. Alt+F starts only when enabled; Enter in either search field can submit. While running, Alt+C or Escape requests cancellation; partial results remain available. Idle Close uses Alt+C. |
| Benchmark | [benchmark.rs](../../../../zarlib-gui/src/ui/dialogs/benchmark.rs): compression and metadata controls, Start, Cancel while running, Close while idle, spinner and result/error summary. Alt+C follows the enabled cancel/close action; window-close requests are blocked during a run. |
| Unsaved changes | [confirmation.rs](../../../../zarlib-gui/src/ui/dialogs/confirmation.rs): question icon and Save, Discard, Cancel/Alt+C; the result determines the pending archive switch. |
| Overwrite confirmation | Same file: question icon and Yes, No, Yes to all, No to all, Cancel/Alt+C; forwards the decision to the worker. Window close cancels. |
| Operation error | [operation_error.rs](../../../../zarlib-gui/src/ui/dialogs/operation_error.rs): 48-point error icon, wrapped scrollable message and Close/Alt+C; closing clears the report. |
| File/folder selection | [file_picker.rs](../../../../zarlib-gui/src/ui/dialogs/file_picker.rs) and [render.rs](../../../../zarlib-gui/src/ui/dialogs/file_picker/render.rs): address/navigation, places/bookmarks sidebar, sortable multi-select file table, filter, Clear, Apply and Cancel/Alt+C. Enter in the address field navigates; Ctrl/Shift modify selection. The current OS-close path and Cancel path differ in how selections are handled; inspect them rather than assuming equivalent cancellation. |
| About | [about.rs](../../../../zarlib-gui/src/ui/dialogs/about.rs): centered fixed egui window, 88-point app icon, product/version, author, license and icon attribution; Close/Alt+C receives initial focus. |
| Rename | [ui.rs](../../../../zarlib-gui/src/ui.rs): in-window focused name field and Rename/Cancel. Enter accepts; Escape or Alt+C cancels; name is validated as an archive path. |
| Native pickers and replacement prompt | [ui.rs](../../../../zarlib-gui/src/ui.rs) and [operation.rs](../../../../zarlib-gui/src/ui/dialogs/operation.rs): `rfd` archive open/save, destination and filter-list dialogs, plus a native Yes/No replacement confirmation in Save As. Their visual treatment and shortcuts follow the platform, not the egui dialog theme. |

The root window also handles Alt+S/N/O/A/E/I/J/F for settings and archive
actions and Ctrl+S/Ctrl+Shift+S for save/save as, with action-specific
availability. Archive-list navigation has separate focus and selection rules.
Do not install root shortcuts inside a child dialog without checking conflicts.
