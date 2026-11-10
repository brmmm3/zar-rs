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
fn saved_default_password_prefills_new_and_add_forms_and_enables_encryption() {
    let app = ZargApp {
        default_password: "from-password-safe".into(),
        ..Default::default()
    };

    for kind in [OperationKind::NewArchive, OperationKind::AddToArchive] {
        let form = app.new_operation_form(kind, "archive.zar");
        assert_eq!(form.password, "from-password-safe");
        assert_eq!(form.password_confirm, "from-password-safe");
        assert!(form.password_is_set);
    }
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
}

#[test]
fn extract_tabs_render_requested_fields_without_nested_sections() {
    fn collect_text(shape: &egui::Shape, texts: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => texts.push(text.galley.job.text.clone()),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_text(shape, texts);
                }
            }
            _ => {}
        }
    }

    let context = egui::Context::default();
    let mut app = ZargApp {
        operation: form(OperationKind::ExtractArchive, "archive.zar"),
        ..Default::default()
    };
    app.operation.extract_has_encrypted_commits = true;
    for (tab, labels) in [
        (
            ExtractTab::General,
            &[
                "Destination:",
                "Last commit",
                "Case collision policy",
                "Existing file policy",
            ][..],
        ),
        (
            ExtractTab::Advanced,
            &[
                "Dry run",
                "Restore metadata",
                "Restore symbolic links",
                "Restore device nodes",
                "Skip top-level folder (extract its contents directly)",
            ][..],
        ),
        (ExtractTab::Encryption, &["Unlock encrypted archive"][..]),
        (
            ExtractTab::Filter,
            &[
                "Include files",
                "Exclude files",
                "Include folders",
                "Exclude folders",
            ][..],
        ),
    ] {
        app.operation.extract_tab = tab;
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 900.0),
                )),
                ..Default::default()
            },
            |ui| app.render_extract_tab_contents(ui),
        );
        let mut texts = Vec::new();
        for shape in &output.shapes {
            collect_text(&shape.shape, &mut texts);
        }
        output.drop_without_applying_deltas();
        for label in labels {
            assert!(texts.iter().any(|text| text == label), "{tab:?}: {label}");
        }
    }
}

#[test]
fn extract_tabs_preserve_options_and_password() {
    let context = egui::Context::default();
    let mut app = ZargApp {
        operation: form(OperationKind::ExtractArchive, "archive.zar"),
        ..Default::default()
    };
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(700.0, 700.0));

    fn frame(
        context: &egui::Context,
        app: &mut ZargApp,
        screen: egui::Rect,
        events: Vec<egui::Event>,
    ) -> (Vec<egui::Rect>, Vec<egui::Rect>, bool) {
        let mut result = None;
        context
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let tabs = app
                        .render_extract_tabs(ui)
                        .into_iter()
                        .map(|r| r.rect)
                        .collect();
                    let (fields, password_visible) = match app.operation.extract_tab {
                        ExtractTab::Advanced => (
                            app.render_extract_advanced(ui)
                                .into_iter()
                                .map(|r| r.rect)
                                .collect(),
                            false,
                        ),
                        ExtractTab::Encryption => {
                            let (unlock, password) = app.render_extract_encryption(ui);
                            (vec![unlock.rect], password.is_some())
                        }
                        _ => {
                            app.render_extract_tab_contents(ui);
                            (Vec::new(), false)
                        }
                    };
                    result = Some((tabs, fields, password_visible));
                },
            )
            .drop_without_applying_deltas();
        result.unwrap()
    }

    fn click(context: &egui::Context, app: &mut ZargApp, screen: egui::Rect, pos: egui::Pos2) {
        for pressed in [true, false] {
            frame(
                context,
                app,
                screen,
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

    let (tabs, _, _) = frame(&context, &mut app, screen, vec![]);
    assert_eq!(tabs.len(), 3);
    app.operation.extract_has_encrypted_commits = true;
    let (tabs, _, _) = frame(&context, &mut app, screen, vec![]);
    assert_eq!(tabs.len(), 4);
    click(&context, &mut app, screen, tabs[1].center());
    assert_eq!(app.operation.extract_tab, ExtractTab::Advanced);
    for index in 0..5 {
        let (_, options, _) = frame(&context, &mut app, screen, vec![]);
        click(&context, &mut app, screen, options[index].center());
    }
    assert!(
        app.operation.dry_run
            && app.operation.metadata
            && app.operation.restore_symlinks
            && app.operation.restore_devices
            && app.operation.strip_top_level
    );
    let (tabs, _, _) = frame(&context, &mut app, screen, vec![]);
    click(&context, &mut app, screen, tabs[2].center());
    assert_eq!(app.operation.extract_tab, ExtractTab::Encryption);
    let (_, unlock, password_visible) = frame(&context, &mut app, screen, vec![]);
    assert!(!password_visible);
    click(&context, &mut app, screen, unlock[0].center());
    app.operation.password = "secret".to_owned();
    let (tabs, _, password_visible) = frame(&context, &mut app, screen, vec![]);
    assert!(password_visible);
    click(&context, &mut app, screen, tabs[3].center());
    assert_eq!(app.operation.extract_tab, ExtractTab::Filter);
    app.operation.include_files = "*.rs".into();
    let (tabs, _, _) = frame(&context, &mut app, screen, vec![]);
    click(&context, &mut app, screen, tabs[0].center());
    assert_eq!(app.operation.extract_tab, ExtractTab::General);
    assert_eq!(app.operation.include_files, "*.rs");
    assert!(app.operation.password_is_set);
    assert_eq!(app.operation.password, "secret");
    assert!(
        app.operation.dry_run
            && app.operation.metadata
            && app.operation.restore_symlinks
            && app.operation.restore_devices
            && app.operation.strip_top_level
    );
}

#[test]
fn fields_cover_join_and_undo_ranges() {
    let context = egui::Context::default();
    let mut app = ZargApp {
        operation: form(OperationKind::JoinCommits, "archive.zar"),
        ..Default::default()
    };
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
        for tab in NewArchiveTab::ALL {
            app.operation.new_archive_tab = tab;
            run_frame(&context, |ui| app.render_new_archive_tab_contents(ui));
        }
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
        app.render_compression_options(ui);
        app.render_encryption_options(ui);
        app.render_filter_fields_contents(ui);
    });
    assert_eq!(app.operation.filelists, [PathBuf::from("filters.txt")]);
    app.operation.kind = Some(OperationKind::ListArchive);
    run_frame(&context, |ui| app.render_filter_fields_contents(ui));
}

#[test]
fn new_archive_advanced_scroll_area_spans_available_width() {
    let context = egui::Context::default();
    context.all_styles_mut(|style| style.animation_time = 0.0);
    let mut app = ZargApp {
        operation: form(OperationKind::NewArchive, ""),
        ..Default::default()
    };
    app.operation.new_archive_tab = NewArchiveTab::Advanced;

    for (width, height) in [(500.0, 100.0), (700.0, 100.0), (500.0, 400.0)] {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        for frame in 0..3 {
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        ..Default::default()
                    },
                    |ui| {
                        let right = ui.available_rect_before_wrap().right();
                        let output = app.render_new_archive_scroll_area(ui);
                        if height == 100.0 {
                            assert!(output.content_size.y > output.inner_rect.height());
                            if frame == 2 {
                                let scrollbar = ui.ctx().read_response(output.id.with(1)).unwrap();
                                let expected = right - ui.spacing().scroll.bar_outer_margin;
                                assert!(
                                    (scrollbar.rect.right() - expected).abs() <= 1.0,
                                    "scrollbar ends at {}, expected {expected}",
                                    scrollbar.rect.right()
                                );
                            }
                        }
                        assert!(
                            (ui.min_rect().right() - right).abs() <= 1.0,
                            "scroll area ends at {}, expected {right}",
                            ui.min_rect().right()
                        );
                    },
                )
                .drop_without_applying_deltas();
        }
    }
}

#[test]
fn new_archive_tabs_switch_and_preserve_values_and_validation() {
    let context = egui::Context::default();
    let mut app = ZargApp {
        operation: form(OperationKind::NewArchive, ""),
        ..Default::default()
    };
    app.operation.compression_level = "9".into();
    app.operation.metadata = true;
    app.operation.dates = true;
    app.operation.special_files = true;
    app.operation.strict = true;
    app.operation.verbose = true;
    app.operation.save_user = true;
    app.operation.include_files = "*.rs".into();
    app.operation.password_is_set = true;
    app.operation.password = "secret".into();
    app.operation.password_confirm = "different".into();

    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(700.0, 700.0));
    for (index, expected) in [
        (1, NewArchiveTab::Advanced),
        (2, NewArchiveTab::Encryption),
        (3, NewArchiveTab::Filters),
        (0, NewArchiveTab::General),
    ] {
        let mut position = egui::Pos2::ZERO;
        for frame in 0..4 {
            let events = if frame < 2 {
                Vec::new()
            } else {
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 2,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            };
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        position = app.render_new_archive_tabs(ui)[index].rect.center();
                        app.render_new_archive_tab_contents(ui);
                    },
                )
                .drop_without_applying_deltas();
        }
        assert_eq!(app.operation.new_archive_tab, expected);
        assert_eq!(app.operation.compression_level, "9");
        assert!(app.operation.metadata);
        assert!(app.operation.dates);
        assert!(app.operation.special_files);
        assert!(app.operation.strict);
        assert!(app.operation.verbose);
        assert!(app.operation.save_user);
        assert_eq!(app.operation.include_files, "*.rs");
        assert_eq!(app.operation.password, "secret");
        assert!(!action_enabled(&mut app, OperationKind::NewArchive));
    }
    app.operation.password_confirm = "secret".into();
    assert!(action_enabled(&mut app, OperationKind::NewArchive));
    app.show_operation_now(OperationKind::NewArchive);
    assert_eq!(app.operation.new_archive_tab, NewArchiveTab::General);
}

#[test]
fn add_archive_unlock_tab_and_rows_follow_encrypted_commits() {
    let context = egui::Context::default();
    let mut app = ZargApp {
        operation: form(OperationKind::AddToArchive, "archive.zar"),
        ..Default::default()
    };
    run_frame(&context, |ui| {
        assert_eq!(app.render_add_archive_tabs(ui).len(), 5)
    });
    app.operation.input_paths.push("input".into());
    app.operation.commit_unlocks = vec![
        CommitUnlock {
            id: 1u32.to_le_bytes(),
            ..Default::default()
        },
        CommitUnlock {
            id: 2u32.to_le_bytes(),
            ..Default::default()
        },
    ];
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(700.0, 400.0));
    let mut checkbox = egui::Pos2::ZERO;
    for frame in 0..4 {
        let events = if frame < 2 {
            vec![]
        } else {
            vec![
                egui::Event::PointerMoved(checkbox),
                egui::Event::PointerButton {
                    pos: checkbox,
                    button: egui::PointerButton::Primary,
                    pressed: frame == 2,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        context
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..Default::default()
                },
                |ui| {
                    assert_eq!(app.render_add_archive_tabs(ui).len(), 6);
                    let rows = app.render_add_archive_unlock(ui);
                    assert_eq!(rows.len(), 2);
                    checkbox = rows[0].0.rect.center();
                    assert!(rows[0].1.rect.right() < rows[0].2.rect.left());
                    assert!(rows[0].2.rect.top() < rows[1].2.rect.top());
                },
            )
            .drop_without_applying_deltas();
    }
    assert!(app.operation.commit_unlocks[0].selected);
    assert!(!app.operation.commit_unlocks[1].selected);
    assert!(!action_enabled(&mut app, OperationKind::AddToArchive));
    app.operation.commit_unlocks[0].password = "first".into();
    assert!(action_enabled(&mut app, OperationKind::AddToArchive));
    let passwords = app.operation.unlock_passwords().unwrap();
    assert_eq!(
        passwords.password_for(&1u32.to_le_bytes()),
        Some(b"first".as_slice())
    );
    assert_eq!(passwords.password_for(&2u32.to_le_bytes()), None);
}

#[test]
fn add_archive_tabs_switch_and_preserve_values() {
    let context = egui::Context::default();
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("archive.zar");
    operations::create_archive(archive.to_str().unwrap(), &[]).unwrap();
    let mut app = ZargApp {
        operation: form(OperationKind::AddToArchive, "archive.zar"),
        ..Default::default()
    };
    app.operation.input_paths.push("input.txt".into());
    app.operation.compression_level = "9".into();
    app.operation.metadata = true;
    app.operation.dates = true;
    app.operation.special_files = true;
    app.operation.strict = true;
    app.operation.verbose = true;
    app.operation.include_files = "*.rs".into();
    app.operation.commit_unlocks.push(CommitUnlock {
        id: 1u32.to_le_bytes(),
        selected: true,
        password: "old".into(),
    });
    app.operation.password_is_set = true;
    app.operation.password = "new".into();
    app.operation.password_confirm = "different".into();

    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(700.0, 700.0));
    for (index, expected) in AddArchiveTab::ALL
        .into_iter()
        .enumerate()
        .skip(1)
        .chain([(0, AddArchiveTab::Files)])
    {
        let mut position = egui::Pos2::ZERO;
        for frame in 0..4 {
            let events = if frame < 2 {
                Vec::new()
            } else {
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 2,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            };
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        position = app.render_add_archive_tabs(ui)[index].rect.center();
                        app.render_add_archive_tab_contents(ui);
                    },
                )
                .drop_without_applying_deltas();
        }
        assert_eq!(app.operation.add_archive_tab, expected);
        assert_eq!(app.operation.input_paths, [PathBuf::from("input.txt")]);
        assert_eq!(app.operation.compression_level, "9");
        assert!(app.operation.metadata);
        assert!(app.operation.dates);
        assert!(app.operation.special_files);
        assert!(app.operation.strict);
        assert!(app.operation.verbose);
        assert_eq!(app.operation.include_files, "*.rs");
        assert_eq!(app.operation.commit_unlocks[0].password, "old");
        assert_eq!(app.operation.password, "new");
        assert!(!action_enabled(&mut app, OperationKind::AddToArchive));
    }
    app.operation.password_confirm = "new".into();
    assert!(action_enabled(&mut app, OperationKind::AddToArchive));
    app.close_operation();
    app.archive = archive.to_string_lossy().into_owned();
    app.show_operation_now(OperationKind::AddToArchive);
    assert_eq!(app.operation.add_archive_tab, AddArchiveTab::Files);
    assert_eq!(app.operation.compression_level, "9");
    assert!(
        app.operation.metadata
            && app.operation.dates
            && app.operation.special_files
            && app.operation.strict
            && app.operation.verbose
    );
}

#[test]
fn add_archive_files_list_fills_space_and_buttons_work() {
    for height in [360.0, 600.0] {
        let context = egui::Context::default();
        let mut app = ZargApp {
            operation: form(OperationKind::AddToArchive, "archive.zar"),
            ..Default::default()
        };
        app.operation.input_paths = (0..30)
            .map(|i| PathBuf::from(format!("file-{i}.txt")))
            .collect();
        app.operation
            .selected_input_paths
            .insert(PathBuf::from("file-0.txt"));
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(500.0, height));
        let mut position = egui::Pos2::ZERO;
        for frame in 0..4 {
            let events = if frame < 2 {
                Vec::new()
            } else {
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 2,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            };
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            let available = ui.available_rect_before_wrap();
                            let (list, buttons) = app.render_add_archive_files(ui);
                            assert!((list.top() - available.top()).abs() < 1.0);
                            assert!((buttons[0].rect.bottom() - available.bottom()).abs() < 2.0);
                            assert!(list.bottom() <= buttons[0].rect.top());
                            assert!(list.height() > height - 60.0);
                            position = buttons[1].rect.center();
                        });
                    },
                )
                .drop_without_applying_deltas();
        }
        assert_eq!(app.operation.input_paths.len(), 29);
        assert!(
            !app.operation
                .input_paths
                .contains(&PathBuf::from("file-0.txt"))
        );
        assert!(app.operation.selected_input_paths.is_empty());
        for frame in 0..4 {
            let events = if frame < 2 {
                Vec::new()
            } else {
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: frame == 2,
                        modifiers: egui::Modifiers::NONE,
                    },
                ]
            };
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
                            let (_, buttons) = app.render_add_archive_files(ui);
                            assert!(!buttons[1].enabled());
                            position = buttons[2].rect.center();
                        });
                    },
                )
                .drop_without_applying_deltas();
        }
        assert!(app.operation.input_paths.is_empty());
        assert!(!action_enabled(&mut app, OperationKind::AddToArchive));
    }
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
fn undo_action_allows_a_staged_rewrite_with_one_commit() {
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("archive.zar");
    std::fs::write(&archive, b"archive").unwrap();
    let mut app = ZargApp {
        operation: form(OperationKind::UndoCommits, archive.to_str().unwrap()),
        ..Default::default()
    };
    app.operation.commit_ids = vec!["first".into()];
    app.rewrite_snapshots.push(RewriteSnapshot {
        directory: tempfile::tempdir().unwrap(),
        dirty: false,
        password: None,
        commit_count: 2,
    });
    app.operation.undo_commits = 1;
    assert!(action_enabled(&mut app, OperationKind::UndoCommits));
    app.operation.undo_commits = 2;
    assert!(action_enabled(&mut app, OperationKind::UndoCommits));
    app.operation.undo_commits = 3;
    assert!(!action_enabled(&mut app, OperationKind::UndoCommits));
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
    app.operation.password = "secret".into();
    app.operation.password_confirm = "different".into();
    assert!(!action_enabled(&mut app, OperationKind::AddToArchive));
    app.operation.password.clear();
    app.operation.password_confirm.clear();
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
    app.operation.password = "secret".into();
    app.operation.password_confirm = "different".into();
    assert!(!action_enabled(&mut app, OperationKind::NewArchive));
    app.operation.password_confirm = "secret".into();
    assert!(action_enabled(&mut app, OperationKind::NewArchive));
    app.operation.password_is_set = true;
    app.operation.password.clear();
    app.operation.password_confirm.clear();
    assert!(!action_enabled(&mut app, OperationKind::NewArchive));
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
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("archive.zar");
    operations::create_archive(archive.to_str().unwrap(), &[]).unwrap();
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
    app.archive = archive.to_string_lossy().into_owned();
    app.show_operation_now(OperationKind::AddToArchive);
    assert_eq!(app.operation.compression_level, "7");
    assert!(app.operation.input_paths.is_empty());
    assert!(app.operation.password.is_empty());
}

#[test]
fn closing_operation_viewport_remembers_add_settings() {
    let context = egui::Context::default();
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("archive.zar");
    operations::create_archive(archive.to_str().unwrap(), &[]).unwrap();
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
    app.archive = archive.to_string_lossy().into_owned();
    app.show_operation_now(OperationKind::AddToArchive);
    assert_eq!(app.operation.compression_level, "5");
}
