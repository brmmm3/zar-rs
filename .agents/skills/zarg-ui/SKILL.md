---
name: zarg-ui
description: Use for every new UI element in this repository, across all subcrates, bindings, and packages, including dialogs, forms, controls, menus, progress views, and settings. Analyze and apply zarg's existing egui theming, icon themes, layout, interaction, accessibility, and keyboard conventions while preserving the target application's behavior.
---

# Zarg UI conventions

## Required workflow

1. Before adding any UI element, read this skill and the applicable sections of
   [the dialog inventory](references/dialog-inventory.md). This applies across
   the whole repository: all current and future subcrates, bindings and
   packages, including those with their own `AGENTS.md`. For non-egui UI,
   adapt the interaction and visual principles to the platform; do not copy
   Rust APIs blindly.
2. Read the target's existing UI, settings, and relevant zarg reference code.
   The inventory is a map, not a substitute for checking current source.
   Preserve existing user-visible behavior unless the task requires a change.
3. Use the existing target-app theme, icon and layout helpers where available.
   If a needed equivalent is absent, implement the smallest compatible
   integration, including settings and live updates where the feature requires
   them. Do not import a binary crate's private modules or couple binaries to
   another app's asset paths.
4. Implement the whole interaction: state, validation, disabled/busy states,
   feedback, cancellation, focus, shortcuts, persistence, and worker lifecycle
   as applicable. A visual replica with missing behavior is not complete.
5. Test the changed behavior, not just rendering. Cover relevant shortcuts,
   invalid input, enabled states, cancellation and persistence. Check layout
   and readability at supported zoom levels and light/dark themes. Run the
   smallest relevant build and tests, including platform-specific checks where
   possible; report unavailable runtime/platform validation explicitly.

## Visual system

- Zarg's source of truth is
  [`theme.rs`](../../../zarlib-gui/src/theme.rs), not a newly invented palette:
  System, Light, Dark and Catppuccin Latte, Frappe, Macchiato, Mocha.
  System follows the OS; the Unix watcher updates it at runtime. Install the
  theme before the first frame and update it through the existing controller.
- Qt Fusion-inspired, compact native desktop styling: light window `#efefef`,
  dark window `#353535`; thin borders, 2-point widget corners, 3-point window
  corners, restrained shadows. The existing style sets item spacing 6x4,
  button padding 8x3, interaction size 40x22, and solid 12-point scrollbars.
  Reuse semantic `ui.visuals()` colors for text, errors, selection and widget
  states; verify hover, active, focus, disabled and contrast in each theme.
  Do not treat legacy hard-coded warning colors as a new design requirement.
- Appearance settings include Crystal and Flat icon themes and UI zoom options
  1.0, 1.25, 1.5, 1.75, and 2.0. Use the target app's equivalent settings and
  shared icon loading/cache; new themed icons must have both variants and a
  theme change must invalidate or distinguish cached textures. Zarg uses
  [`UiIcon`, `icon_bytes`, `icon_texture`, `icon_button`](../../../zarlib-gui/src/ui.rs)
  and typically 16x16 icons in buttons, 48x48 for prominent message/progress
  illustrations, and 88x88 for the About application icon. Do not hard-code
  one icon theme or re-decode an icon on every frame.

## Dialog and control patterns

- For zarg dialogs, prefer `show_viewport_immediate` and
  `centered_dialog_viewport_builder`: unique viewport IDs, restored per-dialog
  size, monitor-clamped initial placement and focus-on-open without resetting
  an existing window. Record valid sizes through `record_dialog_size` where
  the particular dialog supports it. Native `rfd` dialogs and the in-window
  About and Rename dialogs are deliberate exceptions; do not convert them
  merely to enforce a template.
- Most dialog viewports paint an opaque theme window background, reserve a
  bottom action panel with a separator, then use a central form area. Use
  right-to-left button layout to put the cancel/close button on the right;
  keep actions visible while form content scrolls. Typical panel inner margin
  is 3 points; use larger content margins where the existing dialog does.
  Respect minimum dimensions, localization/text length, scaling, and narrow
  windows. Use `horizontal_wrapped` tabs and distinct scroll-area IDs per tab.
- Use concise labels, meaningful grouping, inline help, and a visible error
  state. Disable actions when their preconditions are unmet; the same predicate
  must gate clicks and keyboard activation. Keep password inputs masked,
  avoid leaking secrets, and follow the target's secret-lifetime conventions.
- Zarg's `close_or_cancel` uses **Alt+C**, only when enabled; `consume_alt_key`
  consumes an exact Alt-only, non-repeating key event. Do not assume Escape
  or Enter applies to every dialog. When adding a shortcut, match the relevant
  dialog's semantics, avoid conflicts with parent or nested dialogs, and test
  focus, modifiers, key repeat, busy and disabled states. Zarg's New/Add/
  Extract actions use Alt+N/A/E, Find uses Alt+F, and focused search fields can
  submit with Enter. Standalone extraction has explicit Escape handling.
- While background work runs, keep its visible status/progress and cancellation
  semantics consistent with its owning workflow. Do not close a running
  operation or drop partial results unless the workflow explicitly does so.
  Confirmations must clearly distinguish accept, reject, and cancel; do not
  infer that every existing close action has identical semantics.

## Validation references

- Headless egui frame and shortcut patterns:
  [`ui/tests.rs`](../../../zarlib-gui/src/ui/tests.rs),
  [`operation_tests.rs`](../../../zarlib-gui/src/ui/dialogs/operation_tests.rs),
  [`settings_tests.rs`](../../../zarlib-gui/src/ui/dialogs/settings_tests.rs).
- Theme and live icon changes:
  [`theme.rs`](../../../zarlib-gui/src/theme.rs) and
  [`settings.rs`](../../../zarlib-gui/src/ui/dialogs/settings.rs).
- For zarg use `cargo test -p zarlib-gui --bin zarg <test-filter>` or the
  relevant dialog tests; for other applications use their own test commands.
  Automated headless tests do not establish native window placement or actual
  platform rendering; verify those separately when needed.
