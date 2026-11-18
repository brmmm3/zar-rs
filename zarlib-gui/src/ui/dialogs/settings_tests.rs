use super::*;

fn text_in_shape(shape: &egui::Shape, texts: &mut Vec<String>) {
    match shape {
        egui::Shape::Text(text) => texts.push(text.galley.job.text.clone()),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                text_in_shape(shape, texts);
            }
        }
        _ => {}
    }
}

#[test]
fn settings_tabs_show_only_their_own_fields_and_preserve_values() {
    let context = egui::Context::default();
    let mut app = ZargApp {
        show_settings: true,
        default_block_size: "16M".into(),
        default_compression_level: 9,
        default_password_input: "unsaved password".into(),
        ..Default::default()
    };
    app.archive_defaults.metadata = true;
    app.archive_defaults.include_files = "*.rs".into();
    app.dialog_sizes.insert("settings".into(), [800.0, 800.0]);

    let cases = [
        (
            SettingsTab::Appearance,
            &["Theme color", "Icon theme", "Text and button size"][..],
        ),
        (
            SettingsTab::Compression,
            &["Default compression level", "Default block size"][..],
        ),
        (
            SettingsTab::Advanced,
            &[
                "Validate selected blocks before extraction (slow)",
                "Auto recheck block checksums during extraction",
                "Preserve platform-specific metadata",
                "Save modification times",
                "Include named pipes and device nodes",
                "Strict input validation",
                "Save current OS username (new archives)",
            ][..],
        ),
        (
            SettingsTab::Encryption,
            &[
                "Default encryption method",
                "Show password",
                "Save to password safe",
                "Remove saved password",
            ][..],
        ),
        (
            SettingsTab::Filters,
            &["Default include / exclude filters"][..],
        ),
    ];
    for (tab, expected_labels) in cases {
        app.settings_tab = tab;
        context
            .run_ui(egui::RawInput::default(), |ui| {
                app.render_settings_dialog(ui.ctx())
            })
            .drop_without_applying_deltas();
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| app.render_settings_dialog(ui.ctx()),
        );
        let mut texts = Vec::new();
        for shape in &output.shapes {
            text_in_shape(&shape.shape, &mut texts);
        }

        output.drop_without_applying_deltas();
        for expected in expected_labels {
            assert!(
                texts.iter().any(|text| text == expected),
                "{tab:?}: {expected}"
            );
        }
        for (other, labels) in cases {
            if other != tab {
                for label in labels {
                    assert!(!texts.iter().any(|text| text == label), "{tab:?}: {label}");
                }
            }
        }
        for tab in SettingsTab::ALL {
            assert!(texts.iter().any(|text| text == tab.label()));
        }
        assert_eq!(app.default_block_size, "16M");
        assert_eq!(app.default_compression_level, 9);
        assert!(app.archive_defaults.metadata);
        assert_eq!(app.archive_defaults.include_files, "*.rs");
        assert_eq!(app.default_password_input, "unsaved password");
    }
}

#[test]
fn extraction_checksum_combobox_switches_modes() {
    fn frame(
        context: &egui::Context,
        app: &mut ZargApp,
        events: Vec<egui::Event>,
    ) -> Vec<egui::Shape> {
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.render_settings_dialog(ui.ctx()),
        );
        let shapes = output
            .shapes
            .iter()
            .map(|shape| shape.shape.clone())
            .collect();
        output.drop_without_applying_deltas();
        shapes
    }

    fn text_position(shapes: &[egui::Shape], label: &str) -> Option<egui::Pos2> {
        for shape in shapes {
            match shape {
                egui::Shape::Text(text) if text.galley.job.text == label => {
                    return Some(text.pos + text.galley.size() / 2.0);
                }
                egui::Shape::Vec(children) => {
                    if let Some(position) = text_position(children, label) {
                        return Some(position);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn click(context: &egui::Context, app: &mut ZargApp, pos: egui::Pos2) {
        for pressed in [true, false] {
            frame(
                context,
                app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }

    let context = egui::Context::default();
    let mut app = ZargApp {
        show_settings: true,
        settings_tab: SettingsTab::Advanced,
        ..Default::default()
    };
    app.dialog_sizes.insert("settings".into(), [800.0, 800.0]);
    frame(&context, &mut app, vec![]);
    let shapes = frame(&context, &mut app, vec![]);
    let auto = text_position(&shapes, crate::settings::ExtractionChecksum::Auto.label()).unwrap();
    click(&context, &mut app, auto);
    let shapes = frame(&context, &mut app, vec![]);
    let always =
        text_position(&shapes, crate::settings::ExtractionChecksum::Always.label()).unwrap();
    click(&context, &mut app, always);
    assert_eq!(
        app.validate_blocks_on_extraction,
        crate::settings::ExtractionChecksum::Always
    );
    let shapes = frame(&context, &mut app, vec![]);
    let selected =
        text_position(&shapes, crate::settings::ExtractionChecksum::Always.label()).unwrap();
    click(&context, &mut app, selected);
    let shapes = frame(&context, &mut app, vec![]);
    let auto = text_position(&shapes, crate::settings::ExtractionChecksum::Auto.label()).unwrap();
    click(&context, &mut app, auto);
    assert_eq!(
        app.validate_blocks_on_extraction,
        crate::settings::ExtractionChecksum::Auto
    );
    let shapes = frame(&context, &mut app, vec![]);
    let selected =
        text_position(&shapes, crate::settings::ExtractionChecksum::Auto.label()).unwrap();
    click(&context, &mut app, selected);
    let shapes = frame(&context, &mut app, vec![]);
    let never = text_position(&shapes, crate::settings::ExtractionChecksum::Never.label()).unwrap();
    click(&context, &mut app, never);
    assert_eq!(
        app.validate_blocks_on_extraction,
        crate::settings::ExtractionChecksum::Never
    );
}
