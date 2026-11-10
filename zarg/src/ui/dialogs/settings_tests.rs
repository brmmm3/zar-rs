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
                "Preserve platform-specific metadata",
                "Save modification dates",
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
