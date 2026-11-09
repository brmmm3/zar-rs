use super::*;

fn run_frame(context: &egui::Context, render: impl FnMut(&mut egui::Ui)) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1400.0, 1400.0),
        )),
        ..Default::default()
    };
    context.run_ui(input, render).drop_without_applying_deltas();
}

fn form(kind: OperationKind, archive: &str) -> OperationForm {
    let mut form = OperationForm::new(kind, archive);
    form.commit_ids = vec!["first".into(), "middle".into(), "last".into()];
    form.from_id = "first".into();
    form.to_id = "last".into();
    form.to = "last".into();
    form
}

#[test]
fn operation_kinds_have_distinct_titles_and_dialog_size_keys() {
    let kinds = [
        OperationKind::NewArchive,
        OperationKind::AddToArchive,
        OperationKind::RemoveEntries,
        OperationKind::JoinCommits,
        OperationKind::UndoCommits,
        OperationKind::ChangeCommits,
        OperationKind::CustomMetadata,
        OperationKind::TestArchive,
        OperationKind::ListArchive,
        OperationKind::ExtractArchive,
    ];
    let titles: std::collections::BTreeSet<_> = kinds.iter().map(|kind| kind.title()).collect();
    let keys: std::collections::BTreeSet<_> =
        kinds.iter().map(|kind| kind.dialog_size_key()).collect();
    assert_eq!(titles.len(), kinds.len());
    assert_eq!(keys.len(), kinds.len());
    for kind in kinds {
        assert_eq!(kind.needs_archive(), kind != OperationKind::NewArchive);
    }
}

#[test]
fn all_operation_dialogs_render_with_representative_state() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.txt");
    std::fs::write(&input, b"content").unwrap();
    for kind in [
        OperationKind::NewArchive,
        OperationKind::AddToArchive,
        OperationKind::JoinCommits,
        OperationKind::UndoCommits,
        OperationKind::ChangeCommits,
        OperationKind::CustomMetadata,
        OperationKind::TestArchive,
        OperationKind::ListArchive,
        OperationKind::ExtractArchive,
    ] {
        let context = egui::Context::default();
        let mut app = ZargApp {
            operation: form(kind, input.to_str().unwrap()),
            ..Default::default()
        };
        app.operation.input_paths = vec![directory.path().to_owned(), input.clone()];
        app.operation.destination = directory.path().to_string_lossy().into_owned();
        app.operation.password_is_set = true;
        app.operation.password = "weak".into();
        app.operation.password_confirm = "different".into();
        app.operation.meta_items = vec![
            ("existing".into(), "old".into(), "value".into(), false),
            ("removed".into(), "old".into(), "value".into(), true),
        ];
        app.operation.meta_new = vec![("new".into(), "value".into())];
        run_frame(&context, |ui| app.render_operation_dialog(ui.ctx()));
        assert_eq!(app.operation.kind, Some(kind));
        assert!(!app.busy);
    }
}

#[test]
fn fields_cover_encryption_and_commit_range_states() {
    let context = egui::Context::default();
    let mut app = ZargApp::default();
    for action in 0..=2 {
        app.operation = form(OperationKind::ChangeCommits, "archive.zar");
        app.operation.transform_action = action;
        app.operation.password = "weak".into();
        run_frame(&context, |ui| {
            app.render_operation_fields(ui, OperationKind::ChangeCommits);
        });
        assert_eq!(app.operation.transform_action, action);
    }

    app.operation = form(OperationKind::JoinCommits, "archive.zar");
    app.operation.from_id = "last".into();
    app.operation.to_id = "first".into();
    run_frame(&context, |ui| {
        app.render_operation_fields(ui, OperationKind::JoinCommits);
    });
    assert_eq!(app.operation.to_id, "last");

    app.operation = form(OperationKind::UndoCommits, "archive.zar");
    app.operation.undo_commits = 100;
    run_frame(&context, |ui| {
        app.render_operation_fields(ui, OperationKind::UndoCommits);
    });
    assert_eq!(app.operation.undo_commits, 2);
    app.operation.commit_ids.clear();
    run_frame(&context, |ui| {
        app.render_operation_fields(ui, OperationKind::UndoCommits);
    });
    assert_eq!(app.operation.undo_commits, 1);
}

#[test]
fn creation_options_preserve_filter_and_encryption_values() {
    let context = egui::Context::default();
    let mut app = ZargApp {
        operation: form(OperationKind::NewArchive, ""),
        ..Default::default()
    };
    app.operation.include_files = "*.rs".into();
    app.operation.exclude_files = "*.tmp".into();
    app.operation.include_folders = "src/**".into();
    app.operation.exclude_folders = "target/**".into();
    for (enabled, password, confirmation) in [
        (false, "", ""),
        (true, "weak", "different"),
        (true, "GoodPassword9!", "GoodPassword9!"),
    ] {
        app.operation.password_is_set = enabled;
        app.operation.password = password.into();
        app.operation.password_confirm = confirmation.into();
        run_frame(&context, |ui| {
            app.render_create_options(ui, OperationKind::NewArchive);
        });
        assert_eq!(app.operation.password_is_set, enabled);
        assert_eq!(app.operation.password, password);
        assert_eq!(app.operation.password_confirm, confirmation);
    }
    assert_eq!(app.operation.include_files, "*.rs");
    assert_eq!(app.operation.exclude_files, "*.tmp");
    assert_eq!(app.operation.include_folders, "src/**");
    assert_eq!(app.operation.exclude_folders, "target/**");

    app.operation.kind = Some(OperationKind::AddToArchive);
    app.operation.filelists.push("filters.txt".into());
    run_frame(&context, |ui| {
        app.render_compression_options(ui, OperationKind::AddToArchive);
        app.render_encryption_options(ui);
        app.render_filter_fields_contents(ui);
    });
    assert_eq!(app.operation.filelists, [PathBuf::from("filters.txt")]);
    app.operation.kind = Some(OperationKind::ListArchive);
    run_frame(&context, |ui| app.render_filter_fields_contents(ui));
}

fn action_enabled(app: &mut ZargApp, kind: OperationKind) -> bool {
    let context = egui::Context::default();
    let mut enabled = false;
    run_frame(&context, |ui| {
        enabled = app.render_operation_actions(ui, kind).unwrap().enabled();
    });
    enabled
}

#[test]
fn action_buttons_validate_inputs_and_passwords() {
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("archive.zar");
    std::fs::write(&archive, b"archive").unwrap();
    let archive = archive.to_str().unwrap();
    let mut app = ZargApp {
        operation: form(OperationKind::AddToArchive, archive),
        ..Default::default()
    };
    assert!(!action_enabled(&mut app, OperationKind::AddToArchive));
    app.operation.input_paths.push("input".into());
    assert!(action_enabled(&mut app, OperationKind::AddToArchive));
    app.operation.password_is_set = true;
    assert!(!action_enabled(&mut app, OperationKind::AddToArchive));
    app.operation.password = "secret".into();
    app.operation.password_confirm = "different".into();
    assert!(!action_enabled(&mut app, OperationKind::AddToArchive));
    app.operation.password_confirm = "secret".into();
    assert!(action_enabled(&mut app, OperationKind::AddToArchive));

    app.operation = form(OperationKind::ExtractArchive, archive);
    app.operation.destination = directory.path().to_string_lossy().into_owned();
    assert!(action_enabled(&mut app, OperationKind::ExtractArchive));
    app.operation.destination.clear();
    assert!(!action_enabled(&mut app, OperationKind::ExtractArchive));
    app.operation = form(OperationKind::TestArchive, archive);
    assert!(action_enabled(&mut app, OperationKind::TestArchive));
    app.operation.archive.clear();
    assert!(!action_enabled(&mut app, OperationKind::TestArchive));

    app.operation = form(OperationKind::CustomMetadata, archive);
    assert!(!action_enabled(&mut app, OperationKind::CustomMetadata));
    app.operation.meta_new.push(("key".into(), "value".into()));
    assert!(action_enabled(&mut app, OperationKind::CustomMetadata));
    app.operation = form(OperationKind::UndoCommits, archive);
    app.operation.undo_commits = 1;
    assert!(action_enabled(&mut app, OperationKind::UndoCommits));
    app.operation.undo_commits = 3;
    assert!(!action_enabled(&mut app, OperationKind::UndoCommits));

    app.operation = form(OperationKind::JoinCommits, archive);
    assert!(action_enabled(&mut app, OperationKind::JoinCommits));
    app.operation.to_id = "first".into();
    assert!(!action_enabled(&mut app, OperationKind::JoinCommits));
    app.operation = form(OperationKind::ChangeCommits, archive);
    assert!(!action_enabled(&mut app, OperationKind::ChangeCommits));
    app.operation.password = "secret".into();
    app.operation.password_confirm = "secret".into();
    assert!(action_enabled(&mut app, OperationKind::ChangeCommits));
    app.operation.transform_action = 1;
    app.operation.transform_strict = true;
    assert!(!action_enabled(&mut app, OperationKind::ChangeCommits));
    app.operation.existing_password = "old".into();
    assert!(action_enabled(&mut app, OperationKind::ChangeCommits));
    app.operation = form(OperationKind::NewArchive, "");
    assert!(action_enabled(&mut app, OperationKind::NewArchive));
    app.operation = form(OperationKind::ListArchive, archive);
    assert!(action_enabled(&mut app, OperationKind::ListArchive));
    let context = egui::Context::default();
    run_frame(&context, |ui| {
        assert!(
            app.render_operation_actions(ui, OperationKind::RemoveEntries)
                .is_none()
        );
    });
}

#[test]
fn cancel_button_remembers_add_settings_without_inputs_or_password() {
    let context = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(500.0, 360.0));
    let mut app = ZargApp {
        operation: form(OperationKind::AddToArchive, "archive.zar"),
        ..Default::default()
    };
    app.operation.compression_level = "7".into();
    app.operation.input_paths.push("input".into());
    app.operation.password = "secret".into();
    let mut cancel_position = egui::Pos2::ZERO;
    for frame in 0..4 {
        let events = if frame < 2 {
            Vec::new()
        } else {
            vec![
                egui::Event::PointerMoved(cancel_position),
                egui::Event::PointerButton {
                    pos: cancel_position,
                    button: egui::PointerButton::Primary,
                    pressed: frame == 2,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        let input = egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..Default::default()
        };
        context
            .run_ui(input, |ui| {
                egui::Panel::bottom("operation-actions")
                    .default_size(ui.spacing().interact_size.y.max(16.0) + 24.0)
                    .show(ui, |ui| {
                        let right = ui.max_rect().right();
                        let response = app
                            .render_operation_actions(ui, OperationKind::AddToArchive)
                            .unwrap();
                        cancel_position = egui::pos2(
                            (right + response.rect.right()) / 2.0,
                            response.rect.center().y,
                        );
                    });
            })
            .drop_without_applying_deltas();
    }
    assert!(app.operation.kind.is_none(), "Cancel was not clicked");
    app.archive = "archive.zar".into();
    app.show_operation_now(OperationKind::AddToArchive);
    assert_eq!(app.operation.compression_level, "7");
    assert!(app.operation.input_paths.is_empty());
    assert!(app.operation.password.is_empty());
}

#[test]
fn closing_operation_viewport_remembers_add_settings() {
    let context = egui::Context::default();
    let mut app = ZargApp {
        operation: form(OperationKind::AddToArchive, "archive.zar"),
        ..Default::default()
    };
    app.operation.compression_level = "5".into();
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1000.0, 1000.0),
        )),
        ..Default::default()
    };
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    context
        .run_ui(input, |ui| app.render_operation_dialog(ui.ctx()))
        .drop_without_applying_deltas();
    assert!(app.operation.kind.is_none());
    app.archive = "archive.zar".into();
    app.show_operation_now(OperationKind::AddToArchive);
    assert_eq!(app.operation.compression_level, "5");
}
