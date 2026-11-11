use super::*;
use dialogs::benchmark::{BenchmarkSettings, run_benchmark};

#[test]
fn delayed_default_password_is_applied_when_ready() {
    let mut app = ZargApp::default();
    let (sender, receiver) = mpsc::channel();
    app.default_password_loading = Some(receiver);

    app.poll_default_password();
    assert!(app.default_password_loading.is_some());

    sender.send(Ok(Some("saved password".to_owned()))).unwrap();
    app.poll_default_password();
    assert_eq!(app.default_password, "saved password");
    assert_eq!(app.default_password_input, "saved password");
    assert!(app.default_password_loading.is_none());
}

#[test]
fn delayed_default_password_does_not_replace_user_input_or_status() {
    for (input, status) in [("new password", None), ("", Some("removed"))] {
        let mut app = ZargApp {
            default_password_input: input.to_owned(),
            default_password_status: status.map(str::to_owned),
            ..Default::default()
        };
        let (sender, receiver) = mpsc::channel();
        app.default_password_loading = Some(receiver);
        sender.send(Ok(Some("old password".to_owned()))).unwrap();

        app.poll_default_password();
        assert!(app.default_password.is_empty());
        assert_eq!(app.default_password_input, input);
        assert_eq!(app.default_password_status.as_deref(), status);
    }
}

#[test]
fn delayed_default_password_reports_store_errors() {
    let mut app = ZargApp::default();
    let (sender, receiver) = mpsc::channel();
    app.default_password_loading = Some(receiver);
    sender.send(Err("store unavailable".to_owned())).unwrap();

    app.poll_default_password();
    assert!(
        app.default_password_status
            .unwrap()
            .contains("store unavailable")
    );
}

#[derive(Debug)]
struct TestDroppedFile(std::path::PathBuf);

impl egui::DroppedFile for TestDroppedFile {
    fn path(&self) -> &std::path::Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|error| error.to_string())
    }
}

#[test]
fn save_as_default_filename_prefers_current_archive_name() {
    assert_eq!(
        default_save_as_filename("/archives/current.zar", Some("entry.txt")),
        Some("current.zar".to_owned())
    );
}

#[test]
fn save_as_default_filename_uses_first_entry_stem_when_archive_is_unnamed() {
    assert_eq!(
        default_save_as_filename("", Some("folder/first-entry.txt")),
        Some("first-entry".to_owned())
    );
}

#[test]
fn long_add_progress_message_keeps_path_start_and_end() {
    let shortened = truncate_adding_message("Adding ABCDEFGHIJKLMNOPQRSTUVWXYZ", 20.0, |text| {
        text.chars().count() as f32
    });

    assert!(shortened.starts_with("Adding ABCDE"));
    assert!(shortened.ends_with("VWXYZ"));
    assert!(shortened.contains("..."));
    assert!(shortened.chars().count() <= 20);
}

#[test]
fn add_status_messages_are_limited_to_four_per_second() {
    let start = Instant::now();
    let mut last_update = None;

    assert!(allow_status_update(&mut last_update, start));
    assert!(!allow_status_update(
        &mut last_update,
        start + Duration::from_millis(249)
    ));
    assert!(allow_status_update(
        &mut last_update,
        start + Duration::from_millis(250)
    ));
    assert!(!allow_status_update(
        &mut last_update,
        start + Duration::from_millis(499)
    ));
}

#[test]
fn dialog_placement_stays_inside_monitor_bounds() {
    let monitor = egui::Rect::from_min_size(egui::pos2(100.0, 50.0), egui::vec2(800.0, 600.0));
    let (inner_size, _, position) = ZargApp::clamp_dialog_placement(
        egui::pos2(890.0, 640.0),
        monitor,
        egui::vec2(500.0, 400.0),
    );
    let outer_rect = egui::Rect::from_min_size(position, inner_size + egui::vec2(16.0, 48.0));

    assert!(outer_rect.min.x >= monitor.min.x + 8.0);
    assert!(outer_rect.min.y >= monitor.min.y + 8.0);
    assert!(outer_rect.max.x <= monitor.max.x - 8.0);
    assert!(outer_rect.max.y <= monitor.max.y - 8.0);

    let (small_inner_size, _, _) =
        ZargApp::clamp_dialog_placement(monitor.center(), monitor, egui::vec2(1200.0, 900.0));
    assert!(small_inner_size.x < 1200.0);
    assert!(small_inner_size.y < 900.0);
}

#[test]
fn dialog_geometry_snaps_to_device_pixels() {
    let pixels_per_point = 1.5;
    let first = ZargApp::snap_to_device_pixel(560.00006, pixels_per_point);
    let second = ZargApp::snap_to_device_pixel(560.1, pixels_per_point);

    assert_eq!(first, second);
    assert_eq!((first * pixels_per_point).round(), first * pixels_per_point);
}

#[test]
fn existing_dialog_builder_does_not_reset_position() {
    let context = egui::Context::default();
    let viewport_id = egui::ViewportId::from_hash_of("settings-dialog");
    let mut input = egui::RawInput::default();
    input
        .viewports
        .insert(viewport_id, egui::ViewportInfo::default());

    let output = context.run_ui(input, |ui| {
        let builder = ZargApp::centered_dialog_viewport_builder(
            ui.ctx(),
            viewport_id,
            "Settings",
            [560.0, 650.0],
            true,
        );
        assert_eq!(builder.position, None);
    });
    output.drop_without_applying_deltas();
}

#[test]
fn window_and_file_list_columns_survive_settings_round_trip() {
    let context = egui::Context::default();
    let mut app = ZargApp {
        geometry_monitor_count: 2,
        ..Default::default()
    };
    let mut input = egui::RawInput::default();
    let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
    viewport.inner_rect = Some(egui::Rect::from_min_size(
        egui::pos2(105.0, 130.0),
        egui::vec2(1000.0, 700.0),
    ));
    viewport.outer_rect = Some(egui::Rect::from_min_size(
        egui::pos2(100.0, 100.0),
        egui::vec2(1010.0, 735.0),
    ));
    let output = context.run_ui(input, |ui| app.track_window_geometry(ui.ctx()));
    output.drop_without_applying_deltas();
    app.store_column_widths("details", &["Name", "Compressed"], &[321.5, 177.0]);
    app.store_column_widths("structured", &["Name", "Size"], &[450.0, 130.0]);
    let saved = app.current_settings_with_monitor_count(2);
    let loaded = crate::settings::Settings::parse(&saved.render());
    assert_eq!(loaded.geometry_for_startup(2), saved.window);
    assert_eq!(
        loaded.geometry_for_startup(1),
        Some(crate::settings::WindowGeometry {
            size: [1000.0, 700.0],
            position: None,
        })
    );
    let mut restored = ZargApp::default();
    restored.apply_settings(&loaded);
    assert_eq!(
        restored.column_widths_for("details", &["Name", "Compressed"], &[1.0, 1.0]),
        [321.5, 177.0]
    );
    assert_eq!(
        restored.column_widths_for("structured", &["Name", "Size"], &[1.0, 1.0]),
        [450.0, 130.0]
    );
    assert_eq!(
        app.current_settings_with_monitor_count(1)
            .window
            .unwrap()
            .position,
        None
    );
}

#[test]
fn maximized_window_does_not_replace_normal_window_geometry() {
    let context = egui::Context::default();
    let geometry = crate::settings::WindowGeometry {
        size: [900.0, 600.0],
        position: Some([10.0, 20.0]),
    };
    let mut app = ZargApp {
        window_geometry: Some(geometry),
        ..Default::default()
    };
    let mut input = egui::RawInput::default();
    let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
    viewport.maximized = Some(true);
    viewport.inner_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(1920.0, 1080.0),
    ));
    let output = context.run_ui(input, |ui| app.track_window_geometry(ui.ctx()));
    output.drop_without_applying_deltas();
    assert_eq!(app.window_geometry, Some(geometry));
}

#[test]
fn window_size_is_tracked_without_a_window_position() {
    let context = egui::Context::default();
    let mut app = ZargApp {
        geometry_monitor_count: 1,
        ..Default::default()
    };
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1234.0, 765.0),
        )),
        ..Default::default()
    };
    assert!(
        input.viewports[&egui::ViewportId::ROOT]
            .inner_rect
            .is_none()
    );
    let output = context.run_ui(input, |ui| app.track_window_geometry(ui.ctx()));
    output.drop_without_applying_deltas();
    assert_eq!(
        app.window_geometry,
        Some(crate::settings::WindowGeometry {
            size: [1234.0, 765.0],
            position: None,
        })
    );
    let saved = app.current_settings_with_monitor_count(1);
    let loaded = crate::settings::Settings::parse(&saved.render());
    assert_eq!(loaded.geometry_for_startup(1), app.window_geometry);

    let input = egui::RawInput {
        screen_rect: None,
        ..Default::default()
    };
    let output = context.run_ui(input, |ui| app.track_window_geometry(ui.ctx()));
    output.drop_without_applying_deltas();
    assert_eq!(app.window_geometry, loaded.window);
}

#[test]
fn window_title_tracks_logical_archive_and_resets() {
    let mut app = ZargApp::default();
    let context = egui::Context::default();
    let title_commands = |app: &mut ZargApp| {
        let output = context.run_ui(egui::RawInput::default(), |ui| {
            app.update_window_title(ui.ctx());
        });
        let commands = output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .filter_map(|command| match command {
                egui::ViewportCommand::Title(title) => Some(title.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        output.drop_without_applying_deltas();
        commands
    };
    assert_eq!(title_commands(&mut app), ["ZAR Archive Manager"]);
    assert!(title_commands(&mut app).is_empty());
    app.archive = "/archives/a file.zar".to_owned();
    app.read_archive = "/tmp/working.zar".to_owned();
    assert_eq!(
        title_commands(&mut app),
        ["ZAR Archive Manager - a file.zar"]
    );
    app.archive = "/archives/import.zip".to_owned();
    assert_eq!(
        title_commands(&mut app),
        ["ZAR Archive Manager - import.zip"]
    );
    app.archive = "/archives/saved.zar".to_owned();
    assert_eq!(
        title_commands(&mut app),
        ["ZAR Archive Manager - saved.zar"]
    );
    app.discard_current_archive();
    assert_eq!(title_commands(&mut app), ["ZAR Archive Manager"]);
    app.read_archive = "/tmp/unsaved.zar".to_owned();
    assert!(title_commands(&mut app).is_empty());
}

#[test]
fn metadata_changes_require_an_edited_pair() {
    let mut form = OperationForm::new(OperationKind::CustomMetadata, "");
    assert!(!form.has_metadata_changes());

    form.meta_new.push(("key".to_owned(), String::new()));
    assert!(form.has_metadata_changes());
    form.meta_new.clear();

    form.meta_items
        .push(("key".to_owned(), "old".to_owned(), "old".to_owned(), false));
    assert!(!form.has_metadata_changes());
    form.meta_items[0].2 = "new".to_owned();
    assert!(form.has_metadata_changes());
    form.meta_items[0].2 = "old".to_owned();
    form.meta_items[0].3 = true;
    assert!(form.has_metadata_changes());
}

#[test]
fn join_requires_to_commit_after_from_commit() {
    let commits = vec!["first".to_owned(), "middle".to_owned(), "last".to_owned()];
    assert!(join_range_is_valid(&commits, "first", "middle"));
    assert!(join_range_is_valid(&commits, "first", "last"));
    assert!(!join_range_is_valid(&commits, "middle", "middle"));
    assert!(!join_range_is_valid(&commits, "last", "first"));
    assert!(!join_range_is_valid(&commits, "missing", "last"));
}

#[test]
fn encryption_transform_ranges_allow_one_or_more_commits_and_reject_bad_ranges() {
    let commits = vec!["first".to_owned(), "middle".to_owned(), "last".to_owned()];
    assert!(commit_range_is_valid(&commits, "first", "first"));
    assert!(commit_range_is_valid(&commits, "first", "last"));
    assert!(!commit_range_is_valid(&commits, "last", "first"));
    assert!(!commit_range_is_valid(&commits, "missing", "last"));
}

#[test]
fn encryption_operation_forms_prefill_commit_range_and_password() {
    let mut app = ZargApp {
        archive: "archive.zar".to_owned(),
        commit_ids: vec!["00000001".to_owned(), "00000002".to_owned()],
        archive_password: Some("existing password".to_owned()),
        ..ZargApp::default()
    };

    app.show_operation(OperationKind::ChangeCommits);

    assert_eq!(app.operation.from_id, "00000001");
    assert_eq!(app.operation.to_id, "00000002");
    assert_eq!(app.operation.existing_password, "existing password");
    assert!(app.operation.transform_strict);
}

#[test]
fn archive_operation_forms_use_saved_defaults_without_enabling_encryption() {
    let settings = crate::settings::Settings {
        default_block_size: "16M".to_owned(),
        default_compression_level: 9,
        default_encryption_method: crate::settings::DefaultEncryptionMethod::ChaCha20Poly1305,
        archive_defaults: crate::settings::ArchiveDefaults {
            metadata: true,
            dates: true,
            creation_time: true,
            access_time: true,
            special_files: true,
            strict: true,
            save_user: true,
            include_files: "*.rs\n*.txt".to_owned(),
            exclude_files: "*.tmp".to_owned(),
            include_folders: "src/**".to_owned(),
            exclude_folders: "target".to_owned(),
        },
        ..crate::settings::Settings::default()
    };
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("archive.zar");
    operations::create_archive(archive.to_str().unwrap(), &[]).unwrap();
    let mut app = ZargApp {
        archive: archive.to_string_lossy().into_owned(),
        ..Default::default()
    };
    app.apply_settings(&settings);

    for kind in [OperationKind::NewArchive, OperationKind::AddToArchive] {
        app.show_operation(kind);
        assert_eq!(app.operation.block_size, "16M");
        assert_eq!(app.operation.compression_level, "9");
        assert_eq!(app.operation.aead, 1);
        assert!(!app.operation.password_is_set);
        assert!(
            app.operation.metadata
                && app.operation.dates
                && app.operation.creation_time
                && app.operation.access_time
                && app.operation.special_files
                && app.operation.strict
        );
        assert_eq!(app.operation.save_user, kind == OperationKind::NewArchive);
        assert_eq!(app.operation.include_files, "*.rs\n*.txt");
        assert_eq!(app.operation.exclude_files, "*.tmp");
        assert_eq!(app.operation.include_folders, "src/**");
        assert_eq!(app.operation.exclude_folders, "target");
        let filters = app.operation.filters().unwrap();
        assert!(filters.includes("src/main.rs", false));
        assert!(!filters.includes("src/cache.tmp", false));
        assert!(!filters.includes("target/main.rs", false));
    }
    let extract = app.new_operation_form(OperationKind::ExtractArchive, "archive.zar");
    assert!(extract.include_files.is_empty());
    assert!(!extract.metadata);
    assert_eq!(
        app.current_settings().archive_defaults,
        settings.archive_defaults
    );
}

#[test]
fn new_archive_seeds_add_settings_and_later_dialogs_remember_edits() {
    let directory = tempfile::tempdir().unwrap();
    let mut app = ZargApp {
        default_block_size: "16M".to_owned(),
        default_compression_level: 9,
        ..Default::default()
    };
    app.archive_defaults.include_files = "*.rs".to_owned();
    app.show_operation(OperationKind::NewArchive);
    assert_eq!(app.operation.block_size, "16M");
    assert_eq!(app.operation.compression_level, "9");
    assert_eq!(app.operation.include_files, "*.rs");

    app.operation.block_size = "4M".to_owned();
    app.operation.compression_level = "5".to_owned();
    app.operation.metadata = true;
    app.operation.dates = true;
    app.operation.special_files = true;
    app.operation.strict = true;
    app.operation.verbose = true;
    app.operation.aead = 1;
    app.operation.encryption_scope = 1;
    app.operation.include_files = "*.txt".to_owned();
    app.operation.exclude_files = "*.tmp".to_owned();
    app.operation.include_folders = "src/**".to_owned();
    app.operation.exclude_folders = "target".to_owned();
    let expected = AddArchiveSettings::from_form(&app.operation);
    let form = std::mem::take(&mut app.operation);
    app.start_operation(form);
    wait_for_worker(&mut app);
    assert!(app.report.is_none(), "{:?}", app.report);

    app.show_operation(OperationKind::AddToArchive);
    assert_eq!(AddArchiveSettings::from_form(&app.operation), expected);
    app.operation.block_size = "2M".to_owned();
    app.operation
        .filelists
        .push(directory.path().join("filters.txt"));
    app.operation
        .input_paths
        .push(directory.path().join("input.txt"));
    app.operation.password_is_set = true;
    app.operation.password = "secret".to_owned();
    app.operation.password_confirm = "secret".to_owned();
    app.operation.commit_unlocks.push(CommitUnlock {
        id: 1u32.to_le_bytes(),
        selected: true,
        password: "old secret".to_owned(),
    });
    let remembered = AddArchiveSettings::from_form(&app.operation);
    app.close_operation();

    app.show_operation(OperationKind::AddToArchive);
    assert_eq!(AddArchiveSettings::from_form(&app.operation), remembered);
    assert!(app.operation.input_paths.is_empty());
    assert!(app.operation.password.is_empty());
    assert!(app.operation.password_confirm.is_empty());
    assert!(app.operation.commit_unlocks.is_empty());

    app.show_operation_now(OperationKind::NewArchive);
    assert_eq!(app.operation.block_size, "16M");
    assert_eq!(app.operation.include_files, "*.rs");
    app.close_operation();
    app.show_operation(OperationKind::AddToArchive);
    assert_eq!(AddArchiveSettings::from_form(&app.operation), remembered);
}

#[test]
fn archive_switches_reset_session_settings_only_on_successful_creation() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.txt");
    let archive = directory.path().join("opened.zar");
    std::fs::write(&input, b"contents").unwrap();
    operations::create_archive(archive.to_str().unwrap(), &[input]).unwrap();

    let mut app = ZargApp {
        default_compression_level: 8,
        ..Default::default()
    };
    let mut first = app.new_operation_form(OperationKind::NewArchive, "");
    first.compression_level = "4".to_owned();
    app.start_operation(first);
    wait_for_worker(&mut app);
    app.show_operation(OperationKind::AddToArchive);
    assert_eq!(app.operation.compression_level, "4");
    app.operation.compression_level = "6".to_owned();
    app.close_operation();

    let mut failed = app.new_operation_form(OperationKind::NewArchive, "");
    failed.block_size = "invalid".to_owned();
    app.start_operation(failed);
    if app.busy {
        wait_for_worker(&mut app);
    }
    app.show_operation(OperationKind::AddToArchive);
    assert_eq!(app.operation.compression_level, "6");

    // A successful new archive starts a fresh session even if another form was open.
    app.close_operation();
    let mut second = app.new_operation_form(OperationKind::NewArchive, "");
    second.compression_level = "2".to_owned();
    app.start_operation(second);
    wait_for_worker(&mut app);
    app.show_operation(OperationKind::AddToArchive);
    assert_eq!(app.operation.compression_level, "2");

    app.open_archive_now(archive, None);
    wait_for_worker(&mut app);
    assert!(app.operation.kind.is_none());
    app.show_operation(OperationKind::AddToArchive);
    assert_eq!(app.operation.compression_level, "8");
    app.operation.compression_level = "3".to_owned();
    app.close_operation();
    app.show_operation(OperationKind::AddToArchive);
    assert_eq!(app.operation.compression_level, "3");
    app.discard_current_archive();
    assert!(app.add_archive_settings.is_none());
}

#[test]
fn context_created_archive_seeds_add_settings_from_global_defaults() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.txt");
    std::fs::write(&input, b"contents").unwrap();
    let settings = crate::settings::Settings {
        default_compression_level: 6,
        ..Default::default()
    };
    let mut app =
        ZargApp::from_context_action(ContextAction::CreateNamed, vec![input], &settings).unwrap();
    wait_for_worker(&mut app);
    assert!(app.report.is_none(), "{:?}", app.report);
    app.show_operation(OperationKind::AddToArchive);
    assert_eq!(app.operation.compression_level, "6");
}

#[test]
fn extract_dialog_detects_encryption_for_main_and_context_actions() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.txt");
    std::fs::write(&input, b"content").unwrap();
    for (name, scope) in [
        ("plain.zar", None),
        ("header.zar", Some(EncryptionScope::HeaderAndData)),
        ("data.zar", Some(EncryptionScope::Data)),
    ] {
        let archive = directory.path().join(name);
        operations::create_archive_with_password_progress_and_filters_and_encryption(
            archive.to_str().unwrap(),
            std::slice::from_ref(&input),
            false,
            EncodingOptions::default(),
            scope.map(|scope| (b"password".as_slice(), ContentAead::Aes256Gcm, scope)),
            None,
            CreateFilterOptions {
                input: CreateInputOptions::default(),
                filters: &FilterSet::default(),
            },
        )
        .unwrap();
        let mut app = ZargApp::default();
        app.open_archive(archive.clone(), scope.map(|_| "password".to_owned()));
        if scope.is_some() {
            wait_for_open_password_dialog(&mut app);
            let dialog = app.open_password_dialog.as_mut().unwrap();
            dialog.answers.send(Some(dialog.passwords())).unwrap();
            dialog.busy = true;
        }
        wait_for_worker(&mut app);
        assert!(app.report.is_none(), "{:?}", app.report);
        app.show_operation(OperationKind::ExtractArchive);
        assert_eq!(app.operation.kind, Some(OperationKind::ExtractArchive));
        assert_eq!(app.operation.extract_has_encrypted_commits, scope.is_some());
        assert_eq!(
            app.operation.destination,
            directory.path().to_string_lossy()
        );
        assert_ne!(
            app.operation.destination,
            Path::new(&app.read_archive)
                .parent()
                .unwrap()
                .to_string_lossy()
        );

        let context_app = ZargApp::from_context_action(
            ContextAction::ExtractTo,
            vec![archive],
            &crate::settings::Settings::default(),
        )
        .unwrap();
        assert_eq!(
            context_app.operation.destination,
            directory.path().to_string_lossy()
        );
        assert_eq!(
            context_app.operation.extract_has_encrypted_commits,
            scope.is_some()
        );
        assert!(!context_app.operation.commit_ids.is_empty());
        assert_eq!(
            context_app.operation.to,
            *context_app.operation.commit_ids.last().unwrap()
        );
    }
}

#[test]
fn extract_destination_defaults_to_archive_parent_or_current_directory() {
    let app = ZargApp::default();
    for (archive, expected) in [
        ("/tmp/my archives/test.zar", "/tmp/my archives"),
        ("relative/test.zar", "relative"),
        ("test.zar", "."),
        ("", "."),
    ] {
        assert_eq!(
            app.new_operation_form(OperationKind::ExtractArchive, archive)
                .destination,
            expected
        );
    }

    let mut unsaved = ZargApp::default();
    unsaved.create_default_archive();
    wait_for_worker(&mut unsaved);
    unsaved.show_operation(OperationKind::ExtractArchive);
    assert_eq!(unsaved.operation.destination, ".");
}

#[test]
fn default_password_enables_dialog_encryption_but_not_startup_archive() {
    let mut app = ZargApp {
        default_password: "default password".into(),
        ..Default::default()
    };
    app.create_default_archive();
    wait_for_worker(&mut app);
    let commits = operations::info_archive(&app.read_archive, false).unwrap();
    assert!(!commits[0].header_encrypted && !commits[0].data_encrypted);

    app.show_operation(OperationKind::AddToArchive);
    assert!(app.operation.password_is_set);
    app.operation.password_is_set = false;
    app.operation.add_archive_tab = dialogs::operation::AddArchiveTab::Encryption;
    egui::__run_test_ui(|ui| app.render_operation_dialog(ui.ctx()));
    assert!(!app.operation.password_is_set);
    app.close_operation();
    app.show_operation(OperationKind::AddToArchive);
    assert!(app.operation.password_is_set);
    app.close_operation();
    app.show_operation_now(OperationKind::NewArchive);
    assert!(app.operation.password_is_set);

    app.default_password.clear();
    app.show_operation_now(OperationKind::NewArchive);
    assert!(!app.operation.password_is_set);
}

#[test]
fn loaded_password_enables_existing_creation_form_without_overwriting_input() {
    let mut app = ZargApp::default();
    app.operation = app.new_operation_form(OperationKind::NewArchive, "");
    app.apply_default_password("saved password".into());
    assert!(app.operation.password_is_set);
    assert_eq!(app.operation.password, "saved password");
    assert_eq!(app.operation.password_confirm, "saved password");
    app.operation.password = "user input".into();
    app.operation.password_is_set = false;
    app.apply_default_password("another password".into());
    assert_eq!(app.operation.password, "user input");
    assert!(!app.operation.password_is_set);
}

#[test]
fn default_password_encrypts_added_commit() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.txt");
    std::fs::write(&input, b"contents").unwrap();
    let mut app = ZargApp {
        default_password: "default password".into(),
        ..Default::default()
    };
    app.create_default_archive();
    wait_for_worker(&mut app);
    app.show_operation(OperationKind::AddToArchive);
    app.operation.input_paths.push(input);
    click_add_archive_action(&mut app);
    wait_for_worker(&mut app);
    assert!(app.report.is_none(), "{:?}", app.report);
    let commits = operations::info_archive(&app.read_archive, false).unwrap();
    assert!(commits.last().unwrap().data_encrypted);
    assert!(app.entries.iter().any(|(path, _)| path == "input.txt"));
}

#[test]
fn new_encrypted_archive_remembers_existing_commit_password() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.txt");
    std::fs::write(&input, b"contents").unwrap();
    let mut app = ZargApp::default();
    let mut form = app.new_operation_form(OperationKind::NewArchive, "");
    form.password_is_set = true;
    form.password = "GoodPassword9!".to_owned();
    form.password_confirm = form.password.clone();
    form.aead = 1;
    app.start_operation(form);
    wait_for_worker(&mut app);
    assert!(app.report.is_none(), "{:?}", app.report);

    app.show_operation(OperationKind::AddToArchive);
    assert!(app.operation.password_is_set);
    assert_eq!(app.operation.aead, 1);
    assert!(app.operation.password.is_empty());
    assert!(app.operation.password_confirm.is_empty());
    app.operation.input_paths.push(input);
    app.operation.password = "GoodPassword9!".to_owned();
    app.operation.password_confirm = app.operation.password.clone();
    assert_eq!(app.operation.commit_unlocks.len(), 1);
    assert!(app.operation.commit_unlocks[0].selected);
    assert_eq!(app.operation.commit_unlocks[0].password, "GoodPassword9!");
    click_add_archive_action(&mut app);
    wait_for_worker(&mut app);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert!(app.entries.iter().any(|(path, _)| path == "input.txt"));
}

#[test]
fn add_unlocks_multiple_encrypted_commits_with_individual_passwords() {
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("archive.zar");
    let first = directory.path().join("first.txt");
    let second = directory.path().join("second.txt");
    let third = directory.path().join("third.txt");
    for path in [&first, &second, &third] {
        std::fs::write(path, path.file_name().unwrap().as_encoded_bytes()).unwrap();
    }
    let archive = archive.to_str().unwrap();
    operations::create_archive_with_password_progress_and_filters_and_encryption(
        archive,
        &[first],
        false,
        EncodingOptions::default(),
        Some((
            b"first password",
            ContentAead::Aes256Gcm,
            EncryptionScope::HeaderAndData,
        )),
        None,
        CreateFilterOptions {
            input: CreateInputOptions::default(),
            filters: &FilterSet::default(),
        },
    )
    .unwrap();
    operations::add_archive_with_encoding_progress_and_filters_and_encryption(
        archive,
        &[second],
        EncodingOptions::default(),
        None,
        AddInputOptions::default(),
        &FilterSet::default(),
        Some((
            b"second password",
            ContentAead::Aes256Gcm,
            EncryptionScope::HeaderAndData,
        )),
        &PasswordSet::new(Some(b"first password".to_vec())),
    )
    .unwrap();
    let mut app = ZargApp {
        archive: archive.to_owned(),
        read_archive: archive.to_owned(),
        ..Default::default()
    };
    app.show_operation(OperationKind::AddToArchive);
    assert_eq!(app.operation.commit_unlocks.len(), 2);
    let ids = operations::info_archive(archive, false).unwrap();
    assert_eq!(
        app.operation
            .commit_unlocks
            .iter()
            .map(|row| row.id)
            .collect::<Vec<_>>(),
        ids.iter().map(|commit| commit.id).collect::<Vec<_>>()
    );
    for (row, password) in app
        .operation
        .commit_unlocks
        .iter_mut()
        .zip(["first password", "second password"])
    {
        row.selected = true;
        row.password = password.to_owned();
    }
    app.operation.input_paths.push(third);
    app.operation.password_is_set = true;
    app.operation.password = "third password".to_owned();
    app.operation.password_confirm = app.operation.password.clone();
    let form = std::mem::take(&mut app.operation);
    app.start_operation(form);
    wait_for_worker(&mut app);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert_eq!(app.commit_ids.len(), 3);
    for name in ["first.txt", "second.txt", "third.txt"] {
        assert!(app.entries.iter().any(|(path, _)| path == name), "{name}");
    }
    app.show_operation(OperationKind::AddToArchive);
    assert_eq!(app.operation.commit_unlocks.len(), 3);
    for (row, password) in app.operation.commit_unlocks.iter().zip([
        "first password",
        "second password",
        "third password",
    ]) {
        assert!(row.selected);
        assert_eq!(row.password, password);
    }
}

#[test]
fn add_form_detects_data_only_encryption_and_reports_inspection_errors() {
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("archive.zar");
    let first = directory.path().join("first.txt");
    let second = directory.path().join("second.txt");
    std::fs::write(&first, b"one").unwrap();
    std::fs::write(&second, b"two").unwrap();
    let path = archive.to_str().unwrap();
    operations::create_archive(path, &[first]).unwrap();
    let mut app = ZargApp {
        archive: path.to_owned(),
        read_archive: path.to_owned(),
        ..Default::default()
    };
    app.show_operation(OperationKind::AddToArchive);
    assert!(app.operation.commit_unlocks.is_empty());
    app.close_operation();

    operations::add_archive_with_encoding_progress_and_filters_and_encryption(
        path,
        &[second],
        EncodingOptions::default(),
        None,
        AddInputOptions::default(),
        &FilterSet::default(),
        Some((b"password", ContentAead::Aes256Gcm, EncryptionScope::Data)),
        &PasswordSet::default(),
    )
    .unwrap();
    app.show_operation(OperationKind::AddToArchive);
    let commits = operations::info_archive(path, false).unwrap();
    assert_eq!(commits.len(), 2);
    assert!(!commits[1].header_encrypted && commits[1].data_encrypted);
    assert_eq!(app.operation.commit_unlocks.len(), 1);
    assert_eq!(app.operation.commit_unlocks[0].id, commits[1].id);
    app.close_operation();
    app.read_archive = directory
        .path()
        .join("missing.zar")
        .to_string_lossy()
        .into_owned();
    app.show_operation(OperationKind::AddToArchive);
    assert!(app.operation.kind.is_none());
    assert!(app.status.contains("Cannot inspect archive commits"));
}

#[test]
fn change_commits_encrypt_job_writes_to_requested_archive() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.txt");
    let archive = directory.path().join("source.zar");
    let output = directory.path().join("encrypted.zar");
    std::fs::write(&source, b"secret contents").unwrap();
    operations::create_archive(archive.to_str().unwrap(), &[source]).unwrap();
    let commits = operations::info_archive(archive.to_str().unwrap(), false).unwrap();
    let commit_id = format!("{:08x}", u32::from_le_bytes(commits[0].id));

    let mut form = OperationForm::new(OperationKind::ChangeCommits, archive.to_str().unwrap());
    form.commit_ids = vec![commit_id.clone()];
    form.from_id.clone_from(&commit_id);
    form.to_id.clone_from(&commit_id);
    form.transform_output = output.to_string_lossy().into_owned();
    form.password = "GoodPassword9!".to_owned();
    form.password_confirm = form.password.clone();
    form.transform_strict = true;
    form.transform_action = 0;
    let (events, _event_rx) = mpsc::channel();
    let (_answers, answer_rx) = mpsc::channel();
    let progress = UiProgress {
        events: events.clone(),
        answers: Arc::new(Mutex::new(answer_rx)),
        cancellation: None,
        limit_status_updates: false,
        last_status_update: Mutex::new(None),
    };
    let job = build_job(OperationKind::ChangeCommits, form, events.clone(), progress).unwrap();
    job().unwrap();

    assert!(!commits[0].header_encrypted);
    let encrypted = operations::info_archive(output.to_str().unwrap(), false).unwrap();
    assert!(encrypted[0].header_encrypted);
    assert!(encrypted[0].data_encrypted);
    let password_set = PasswordSet::new(Some(b"GoodPassword9!".to_vec()));
    operations::list_archive_with_passwords(output.to_str().unwrap(), None, &password_set).unwrap();
    let mut app = ZargApp {
        read_archive: output.to_string_lossy().into_owned(),
        archive_password: Some("GoodPassword9!".to_owned()),
        has_unsaved_changes: true,
        ..Default::default()
    };
    app.show_operation(OperationKind::NewArchive);
    wait_for_worker(&mut app);
    assert!(matches!(
        app.pending_archive_switch,
        Some(ArchiveSwitchAction::New)
    ));
    assert!(app.report.is_none(), "{:?}", app.report);
}

#[test]
fn archive_contents_default_to_normal_view() {
    assert_eq!(ArchiveView::default(), ArchiveView::Normal);
}

#[test]
fn destination_path_requires_existing_directory_or_parent() {
    let directory =
        std::env::temp_dir().join(format!("zarg-destination-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    assert!(valid_destination_path(directory.to_str().unwrap()));
    assert!(valid_destination_path(
        directory.join("new-output").to_str().unwrap()
    ));
    assert!(valid_destination_path(
        directory.join("missing-parent/output").to_str().unwrap()
    ));
    let file = directory.join("existing-file");
    std::fs::write(&file, b"test").unwrap();
    assert!(!valid_destination_path(file.to_str().unwrap()));
    assert!(!valid_destination_path(""));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn named_archive_uses_selected_item_stem_and_parent() {
    let parent = std::env::temp_dir();
    assert_eq!(
        named_archive_path(&parent.join("a file.txt")).unwrap(),
        parent.join("a file.zar")
    );
    assert_eq!(
        named_archive_path(std::path::Path::new("relative-folder")).unwrap(),
        PathBuf::from("relative-folder.zar")
    );
}

#[test]
fn archive_folder_destination_uses_archive_name_and_parent() {
    let parent = std::env::temp_dir();
    assert_eq!(
        archive_folder_destination(&parent.join("foo.zar")).unwrap(),
        parent.join("foo")
    );
    assert_eq!(
        archive_folder_destination(std::path::Path::new("foo.zar")).unwrap(),
        PathBuf::from("foo")
    );
}

#[test]
fn saving_archive_copies_staged_contents_and_replaces_target() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("working.zar");
    let target = directory.path().join("archive.zar");
    std::fs::write(&source, b"updated archive").unwrap();
    std::fs::write(&target, b"old archive").unwrap();

    save_archive_copy(source.to_str().unwrap(), target.to_str().unwrap()).unwrap();

    assert_eq!(std::fs::read(target).unwrap(), b"updated archive");
}

#[test]
fn archive_modifications_are_staged_until_saved() {
    let directory = tempfile::tempdir().unwrap();
    let original_file = directory.path().join("original.txt");
    let added_file = directory.path().join("added.txt");
    let archive = directory.path().join("archive.zar");
    std::fs::write(&original_file, b"original").unwrap();
    std::fs::write(&added_file, b"added").unwrap();
    operations::create_archive(archive.to_str().unwrap(), &[original_file]).unwrap();

    let mut app = ZargApp::default();
    assert!(!app.can_add_to_archive());
    assert!(!app.has_current_archive());
    app.open_archive(archive.clone(), None);
    wait_for_worker(&mut app);
    assert!(app.can_add_to_archive());
    assert!(app.has_current_archive());
    assert_ne!(app.read_archive, archive.to_string_lossy());
    assert!(!app.has_unsaved_changes);

    app.show_operation(OperationKind::AddToArchive);
    app.operation.input_paths.push(added_file);
    click_add_archive_action(&mut app);
    wait_for_worker(&mut app);
    assert!(app.has_unsaved_changes);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert!(app.entries.iter().any(|(path, _)| path == "added.txt"));

    let original_contents =
        operations::list_archive(archive.to_str().unwrap(), None, None).unwrap();
    assert!(
        !original_contents
            .iter()
            .any(|(path, _)| path.as_str() == "added.txt")
    );
    let staged_contents = operations::list_archive(&app.read_archive, None, None).unwrap();
    assert!(
        staged_contents
            .iter()
            .any(|(path, _)| path.as_str() == "added.txt")
    );

    let working = PathBuf::from(&app.read_archive);
    app.save_archive();
    wait_for_worker(&mut app);
    assert!(!app.has_unsaved_changes);
    assert!(!working.exists());
    assert_eq!(app.read_archive, archive.to_string_lossy());
    let saved_contents = operations::list_archive(archive.to_str().unwrap(), None, None).unwrap();
    assert!(
        saved_contents
            .iter()
            .any(|(path, _)| path.as_str() == "added.txt")
    );
}

fn rewrite_fixture() -> (tempfile::TempDir, ZargApp, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("archive.zar");
    let first = directory.path().join("first.txt");
    let second = directory.path().join("second.txt");
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    operations::create_archive(archive.to_str().unwrap(), &[first]).unwrap();
    let mut app = ZargApp::default();
    app.open_archive(archive.clone(), None);
    wait_for_worker(&mut app);
    app.show_operation(OperationKind::AddToArchive);
    app.operation.input_paths.push(second);
    let form = std::mem::take(&mut app.operation);
    app.start_operation(form);
    wait_for_worker(&mut app);
    assert!(app.report.is_none(), "{:?}", app.report);
    (directory, app, archive)
}

fn run_join(app: &mut ZargApp, output: Option<&Path>) {
    app.show_operation(OperationKind::JoinCommits);
    app.operation.from_id = app.commit_ids.first().unwrap().clone();
    app.operation.to_id = app.commit_ids.last().unwrap().clone();
    app.operation.join_output =
        output.map_or_else(String::new, |path| path.to_string_lossy().into_owned());
    let form = std::mem::take(&mut app.operation);
    app.start_operation(form);
    if app.busy {
        wait_for_worker(app);
    }
}

fn run_encrypt(app: &mut ZargApp, output: Option<&Path>) {
    app.show_operation(OperationKind::ChangeCommits);
    app.operation.from_id = app.commit_ids.first().unwrap().clone();
    app.operation.to_id = app.commit_ids.last().unwrap().clone();
    app.operation.transform_output =
        output.map_or_else(String::new, |path| path.to_string_lossy().into_owned());
    app.operation.password = "GoodPassword9!".to_owned();
    app.operation.password_confirm = app.operation.password.clone();
    let form = std::mem::take(&mut app.operation);
    app.start_operation(form);
    if app.busy {
        wait_for_worker(app);
    }
}

fn run_undo(app: &mut ZargApp, steps: usize) {
    app.show_operation(OperationKind::UndoCommits);
    app.operation.undo_commits = steps;
    let form = std::mem::take(&mut app.operation);
    app.start_operation(form);
    if app.busy {
        wait_for_worker(app);
    }
}

fn run_remove_commits(app: &mut ZargApp, from: &str, to: &str, output: Option<&Path>) {
    app.show_operation(OperationKind::RemoveCommits);
    app.operation.from_id = from.to_owned();
    app.operation.to_id = to.to_owned();
    app.operation.remove_commits_output =
        output.map_or_else(String::new, |path| path.to_string_lossy().into_owned());
    let form = std::mem::take(&mut app.operation);
    app.start_operation(form);
    if app.busy {
        wait_for_worker(app);
    }
}

#[test]
fn removing_commits_stages_change_and_undo_stops_at_saved_state() {
    let (directory, mut app, archive) = rewrite_fixture();
    let third = directory.path().join("third.txt");
    std::fs::write(&third, b"third").unwrap();
    app.show_operation(OperationKind::AddToArchive);
    app.operation.input_paths.push(third);
    let form = std::mem::take(&mut app.operation);
    app.start_operation(form);
    wait_for_worker(&mut app);
    assert!(app.save_archive());
    wait_for_worker(&mut app);
    assert!(!app.can_undo());
    let original = std::fs::read(&archive).unwrap();
    let ids = app.commit_ids.clone();
    run_remove_commits(&mut app, &ids[1], &ids[1], None);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert_eq!(app.commit_ids, vec![ids[0].clone(), ids[2].clone()]);
    assert!(app.entries.iter().all(|(path, _)| path != "second.txt"));
    assert!(app.entries.iter().any(|(path, _)| path == "third.txt"));
    assert_eq!(std::fs::read(&archive).unwrap(), original);
    assert!(app.can_undo());
    run_undo(&mut app, 1);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert_eq!(app.commit_ids, ids);
    assert_eq!(std::fs::read(&app.read_archive).unwrap(), original);
    assert!(!app.has_unsaved_changes);
    assert!(!app.can_undo());
    run_undo(&mut app, 1);
    assert!(!app.busy);
    assert_eq!(std::fs::read(&archive).unwrap(), original);
}

#[test]
fn removing_commits_to_output_preserves_current_archive_and_undo_history() {
    let (directory, mut app, _archive) = rewrite_fixture();
    let before = std::fs::read(&app.read_archive).unwrap();
    let before_ids = app.commit_ids.clone();
    let steps = app.maximum_undo_steps();
    let output = directory.path().join("removed.zar");
    run_remove_commits(&mut app, &before_ids[0], &before_ids[0], Some(&output));
    assert!(app.report.is_none(), "{:?}", app.report);
    assert_eq!(std::fs::read(&app.read_archive).unwrap(), before);
    assert_eq!(app.commit_ids, before_ids);
    assert_eq!(app.maximum_undo_steps(), steps);
    let commits = operations::info_archive(output.to_str().unwrap(), false).unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(
        format!("{:08x}", u32::from_le_bytes(commits[0].id)),
        before_ids[1]
    );
}

#[test]
fn removing_commits_warns_when_retained_history_cannot_be_validated() {
    let (_directory, mut app, _archive) = rewrite_fixture();
    let first = app.commit_ids[0].clone();
    let last = app.commit_ids[1].clone();
    app.show_operation(OperationKind::ChangeCommits);
    app.operation.from_id = first.clone();
    app.operation.to_id = first;
    app.operation.password = "FirstPassword9!".to_owned();
    app.operation.password_confirm = app.operation.password.clone();
    let form = std::mem::take(&mut app.operation);
    app.start_operation(form);
    wait_for_worker(&mut app);
    let passwords = app.archive_passwords.clone();
    run_remove_commits(&mut app, &last, &last, None);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert!(app.status.contains("Warning:"));
    assert!(
        app.warnings
            .iter()
            .any(|warning| warning.contains("encrypted headers"))
    );
    assert_eq!(app.archive_passwords, passwords);
    run_undo(&mut app, 1);
    assert_eq!(app.archive_passwords, passwords);
    assert_eq!(app.commit_ids.len(), 2);
}

#[test]
fn non_strict_encryption_preserves_skipped_commit_passwords_and_undo_restores_them() {
    let (_directory, mut app, _archive) = rewrite_fixture();
    let first = app.commit_ids[0].clone();
    let last = app.commit_ids[1].clone();

    app.show_operation(OperationKind::ChangeCommits);
    app.operation.from_id = first.clone();
    app.operation.to_id = first.clone();
    app.operation.password = "FirstPassword9!".to_owned();
    app.operation.password_confirm = app.operation.password.clone();
    let form = std::mem::take(&mut app.operation);
    app.start_operation(form);
    wait_for_worker(&mut app);
    assert!(app.report.is_none(), "{:?}", app.report);
    let first_passwords = app.archive_passwords.clone();
    assert_eq!(first_passwords.len(), 1);

    app.show_operation(OperationKind::ChangeCommits);
    app.operation.from_id = first;
    app.operation.to_id = last;
    app.operation.existing_password.clear();
    app.operation.transform_strict = false;
    app.operation.password = "SecondPassword9!".to_owned();
    app.operation.password_confirm = app.operation.password.clone();
    let form = std::mem::take(&mut app.operation);
    app.start_operation(form);
    wait_for_worker(&mut app);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert_eq!(app.archive_passwords.len(), 2);
    assert!(
        first_passwords
            .iter()
            .all(|(id, password)| { app.archive_passwords.get(id) == Some(password) })
    );
    assert!(
        app.archive_passwords
            .values()
            .any(|password| password == "SecondPassword9!")
    );

    run_undo(&mut app, 1);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert_eq!(app.archive_passwords, first_passwords);
}

#[test]
fn staged_rewrites_undo_multiple_steps_and_restore_previous_changes() {
    let (_directory, mut app, archive) = rewrite_fixture();
    let original = std::fs::read(&archive).unwrap();
    let before_join = std::fs::read(&app.read_archive).unwrap();
    run_join(&mut app, None);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert_eq!(app.commit_ids.len(), 1);
    assert!(app.can_undo());
    run_encrypt(&mut app, None);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert!(app.archive_password.is_some());
    assert_eq!(app.archive_passwords.len(), 1);
    assert_eq!(app.maximum_undo_steps(), 3);
    run_undo(&mut app, 2);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert_eq!(std::fs::read(&app.read_archive).unwrap(), before_join);
    assert_eq!(std::fs::read(&archive).unwrap(), original);
    assert!(app.has_unsaved_changes);
    assert!(app.archive_password.is_none());
    assert!(app.archive_passwords.is_empty());
    assert_eq!(app.commit_ids.len(), 2);
    assert_eq!(app.rewrite_snapshots.len(), 1);
}

#[test]
fn unnamed_archive_rewrite_can_be_undone_from_tools() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.txt");
    let second = directory.path().join("second.txt");
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    let mut app = ZargApp::default();
    let mut create = app.new_operation_form(OperationKind::NewArchive, "");
    create.input_paths.push(first);
    app.start_operation(create);
    wait_for_worker(&mut app);
    app.show_operation(OperationKind::AddToArchive);
    app.operation.input_paths.push(second);
    let form = std::mem::take(&mut app.operation);
    app.start_operation(form);
    wait_for_worker(&mut app);
    run_join(&mut app, None);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert!(app.can_undo());
    run_undo(&mut app, 1);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert_eq!(app.commit_ids.len(), 2);
}

#[test]
fn undo_of_first_rewrite_restores_clean_state() {
    let (_directory, mut app, archive) = rewrite_fixture();
    assert!(app.save_archive());
    wait_for_worker(&mut app);
    let saved = std::fs::read(&archive).unwrap();
    run_join(&mut app, None);
    assert!(app.has_unsaved_changes);
    run_undo(&mut app, 1);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert!(!app.has_unsaved_changes);
    assert_eq!(std::fs::read(&app.read_archive).unwrap(), saved);
    assert_eq!(std::fs::read(&archive).unwrap(), saved);
}

#[test]
fn staged_rewrites_undo_can_continue_into_earlier_commits() {
    let (_directory, mut app, _archive) = rewrite_fixture();
    run_join(&mut app, None);
    assert!(app.report.is_none(), "{:?}", app.report);
    run_undo(&mut app, 2);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert_eq!(app.commit_ids.len(), 1);
    assert!(app.entries.iter().all(|(path, _)| path != "second.txt"));
}

#[test]
fn staged_rewrite_followed_by_add_undoes_in_action_order() {
    let (directory, mut app, _archive) = rewrite_fixture();
    let before_join = std::fs::read(&app.read_archive).unwrap();
    run_join(&mut app, None);
    let joined = std::fs::read(&app.read_archive).unwrap();
    let third = directory.path().join("third.txt");
    std::fs::write(&third, b"third").unwrap();
    app.show_operation(OperationKind::AddToArchive);
    app.operation.input_paths.push(third);
    let form = std::mem::take(&mut app.operation);
    app.start_operation(form);
    wait_for_worker(&mut app);
    assert!(app.report.is_none(), "{:?}", app.report);
    run_undo(&mut app, 1);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert_eq!(std::fs::read(&app.read_archive).unwrap(), joined);
    run_undo(&mut app, 1);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert_eq!(std::fs::read(&app.read_archive).unwrap(), before_join);
}

#[test]
fn explicit_rewrite_outputs_leave_current_archive_untouched() {
    let (directory, mut app, archive) = rewrite_fixture();
    let before = std::fs::read(&app.read_archive).unwrap();
    let output = directory.path().join("joined.zar");
    run_join(&mut app, Some(&output));
    assert!(app.report.is_none(), "{:?}", app.report);
    assert_eq!(
        operations::info_archive(output.to_str().unwrap(), false)
            .unwrap()
            .len(),
        1
    );
    let encrypted = directory.path().join("encrypted.zar");
    run_encrypt(&mut app, Some(&encrypted));
    assert!(app.report.is_none(), "{:?}", app.report);
    assert!(
        operations::info_archive(encrypted.to_str().unwrap(), false).unwrap()[0].header_encrypted
    );
    assert_eq!(app.archive, archive.to_string_lossy());
    assert_eq!(std::fs::read(&app.read_archive).unwrap(), before);
    assert!(app.archive_password.is_none());
    assert!(app.has_unsaved_changes);
    assert_eq!(app.rewrite_snapshots.len(), 1);
    run_join(&mut app, Some(&archive));
    assert!(!app.busy);
    assert!(app.status.contains("different archive"));
}

#[test]
fn failed_explicit_join_preserves_existing_output() {
    let (directory, mut app, _archive) = rewrite_fixture();
    let output = directory.path().join("existing.zar");
    std::fs::write(&output, b"existing output").unwrap();
    app.show_operation(OperationKind::JoinCommits);
    app.operation.from_id = "ffffffff".to_owned();
    app.operation.to_id = app.commit_ids.last().unwrap().clone();
    app.operation.join_output = output.to_string_lossy().into_owned();
    let form = std::mem::take(&mut app.operation);
    app.start_operation(form);
    wait_for_worker(&mut app);
    assert!(app.report.is_some());
    assert_eq!(std::fs::read(output).unwrap(), b"existing output");
}

#[test]
fn save_cleans_rewrite_state_and_next_rewrite_is_staged() {
    let (directory, mut app, archive) = rewrite_fixture();
    run_join(&mut app, None);
    let working = PathBuf::from(&app.read_archive);
    let snapshot = app.rewrite_snapshots[0].directory.path().to_path_buf();
    assert!(!app.save_archive_to(working.to_string_lossy().into_owned()));
    assert!(app.status.contains("outside the temporary archive storage"));
    let missing = directory.path().join("missing").join("failed.zar");
    assert!(app.save_archive_to(missing.to_string_lossy().into_owned()));
    wait_for_worker(&mut app);
    assert!(app.report.is_some());
    assert!(working.exists() && snapshot.exists());
    let saved_as = directory.path().join("saved-as.zar");
    assert!(app.save_archive_to(saved_as.to_string_lossy().into_owned()));
    wait_for_worker(&mut app);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert!(!working.exists() && !snapshot.exists());
    assert!(app.rewrite_snapshots.is_empty());
    assert_eq!(app.read_archive, saved_as.to_string_lossy());
    assert!(!app.has_unsaved_changes);
    let saved = std::fs::read(&saved_as).unwrap();
    run_encrypt(&mut app, None);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert_ne!(app.read_archive, saved_as.to_string_lossy());
    assert_eq!(std::fs::read(&saved_as).unwrap(), saved);
    assert_ne!(std::fs::read(&archive).unwrap(), saved);
}

#[test]
fn commit_selector_tracks_history_and_archive_changes() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.txt");
    let second = directory.path().join("second.txt");
    let third = directory.path().join("third.txt");
    let archive = directory.path().join("archive.zar");
    std::fs::write(&first, b"first revision").unwrap();
    std::fs::write(&second, b"second").unwrap();
    std::fs::write(&third, b"third").unwrap();
    operations::create_archive(archive.to_str().unwrap(), &[first]).unwrap();

    let mut app = ZargApp::default();
    app.open_archive(archive, None);
    wait_for_worker(&mut app);
    let first_id = app.commit_ids[0].clone();
    assert_eq!(app.selected_commit, first_id);
    app.show_operation(OperationKind::AddToArchive);
    app.operation.input_paths.push(second);
    click_add_archive_action(&mut app);
    wait_for_worker(&mut app);
    let second_id = app.commit_ids.last().unwrap().clone();
    assert_ne!(second_id, first_id);
    assert_eq!(app.selected_commit, second_id);

    app.select_commit(first_id.clone());
    wait_for_worker(&mut app);
    assert_eq!(app.selected_commit, first_id);
    assert!(app.entries.iter().any(|(path, _)| path == "first.txt"));
    assert!(!app.entries.iter().any(|(path, _)| path == "second.txt"));
    assert_eq!(app.entry_summary.uncompressed, Some(14));
    assert!(
        app.entry_summary
            .display(app.entries.len(), app.commit_ids.len())
            .ends_with("2 commits")
    );
    app.show_operation_now(OperationKind::ExtractArchive);
    assert_eq!(app.operation.to, first_id);

    let output = directory.path().join("extracted");
    operations::extract_single_file_through(
        &app.read_archive,
        "first.txt",
        &output,
        &PasswordSet::default(),
        Some(&first_id),
    )
    .unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"first revision");
    assert!(
        operations::extract_single_file_through(
            &app.read_archive,
            "second.txt",
            &directory.path().join("missing"),
            &PasswordSet::default(),
            Some(&first_id),
        )
        .is_err()
    );

    app.select_commit(second_id.clone());
    wait_for_worker(&mut app);
    assert_eq!(app.selected_commit, second_id);
    assert!(app.entries.iter().any(|(path, _)| path == "second.txt"));
    app.selected.insert("second.txt".to_owned());
    let working = std::mem::replace(&mut app.read_archive, "/nonexistent/archive.zar".to_owned());
    app.select_commit(first_id.clone());
    wait_for_worker(&mut app);
    assert_eq!(app.selected_commit, second_id);
    assert!(app.entries.iter().any(|(path, _)| path == "second.txt"));
    assert!(app.selected.contains("second.txt"));
    assert!(app.report.is_some());
    app.read_archive = working;

    app.select_commit(first_id.clone());
    wait_for_worker(&mut app);
    app.show_operation(OperationKind::AddToArchive);
    app.operation.input_paths.push(third);
    click_add_archive_action(&mut app);
    wait_for_worker(&mut app);
    assert_eq!(app.selected_commit, *app.commit_ids.last().unwrap());
    assert!(app.entries.iter().any(|(path, _)| path == "second.txt"));
    assert!(app.entries.iter().any(|(path, _)| path == "third.txt"));

    let mut form = app.new_operation_form(OperationKind::RemoveEntries, &app.archive);
    form.remove_text = "first.txt".to_owned();
    app.start_operation(form);
    wait_for_worker(&mut app);
    assert_eq!(app.selected_commit, *app.commit_ids.last().unwrap());
    assert!(!app.entries.iter().any(|(path, _)| path == "first.txt"));
    assert_eq!(app.entry_summary.uncompressed, Some(11));
    app.select_commit(first_id);
    wait_for_worker(&mut app);
    assert!(app.entries.iter().any(|(path, _)| path == "first.txt"));
}

#[test]
fn remove_archive_entries_is_staged_and_updates_ui_without_relisting() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("README");
    let archive = directory.path().join("archive.zar");
    std::fs::write(&input, b"contents").unwrap();
    operations::create_archive(archive.to_str().unwrap(), &[input]).unwrap();

    let mut app = ZargApp::default();
    app.open_archive(archive.clone(), None);
    wait_for_worker(&mut app);
    app.selected.insert("README".to_owned());
    app.selection_anchor = Some("README".to_owned());

    let mut form = app.new_operation_form(OperationKind::RemoveEntries, &app.archive);
    form.remove_text = "README".to_owned();
    app.start_operation(form);
    wait_for_worker(&mut app);

    assert!(app.has_unsaved_changes);
    assert!(app.entries.is_empty());
    assert!(!app.selected.contains("README"));
    assert!(app.selection_anchor.is_none());
    assert_eq!(
        operations::info_archive(&app.read_archive, false)
            .unwrap()
            .len(),
        2
    );
    assert!(
        operations::list_archive(archive.to_str().unwrap(), None, None)
            .unwrap()
            .iter()
            .any(|(path, _)| path.as_str() == "README")
    );
    assert!(
        !operations::list_archive(&app.read_archive, None, None)
            .unwrap()
            .iter()
            .any(|(path, _)| path.as_str() == "README")
    );
}

#[test]
fn new_archive_is_created_in_temporary_storage_until_saved() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.txt");
    let archive = directory.path().join("new.zar");
    std::fs::write(&input, b"new archive").unwrap();

    let mut app = ZargApp::default();
    let mut form = app.new_operation_form(OperationKind::NewArchive, archive.to_str().unwrap());
    form.input_paths.push(input);
    form.save_user = true;
    app.start_operation(form);
    wait_for_worker(&mut app);

    assert!(app.has_unsaved_changes);
    assert!(!archive.exists());
    assert!(app.archive.is_empty());
    assert_ne!(app.read_archive, archive.to_string_lossy());
    assert!(
        operations::info_archive(&app.read_archive, false).unwrap()[0]
            .user
            .is_some()
    );
    app.show_operation(OperationKind::AddToArchive);
    app.operation.compression_level = "7".to_owned();
    app.close_operation();
    assert!(app.save_archive_to(archive.to_string_lossy().into_owned()));
    wait_for_worker(&mut app);
    assert!(archive.exists());
    assert_eq!(app.archive, archive.to_string_lossy());
    assert!(!app.has_unsaved_changes);
    app.show_operation(OperationKind::AddToArchive);
    assert_eq!(app.operation.compression_level, "7");
}

#[test]
fn empty_new_archive_is_created_in_temporary_storage() {
    let mut app = ZargApp::default();
    let form = app.new_operation_form(OperationKind::NewArchive, "");
    app.start_operation(form);
    wait_for_worker(&mut app);

    assert!(app.has_unsaved_changes);
    assert!(app.archive.is_empty());
    assert!(!app.read_archive.is_empty());
    assert!(
        operations::list_archive(&app.read_archive, None, None)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn startup_archive_uses_settings_and_seeds_add_dialog() {
    let settings = crate::settings::Settings {
        default_block_size: "16M".to_owned(),
        default_compression_level: 9,
        default_encryption_method: crate::settings::DefaultEncryptionMethod::ChaCha20Poly1305,
        archive_defaults: crate::settings::ArchiveDefaults {
            metadata: true,
            dates: true,
            creation_time: true,
            access_time: true,
            special_files: true,
            strict: true,
            save_user: true,
            include_files: "*.rs".to_owned(),
            exclude_files: "*.tmp".to_owned(),
            include_folders: "src/**".to_owned(),
            exclude_folders: "target/**".to_owned(),
        },
        ..Default::default()
    };
    let mut app = ZargApp::default();
    app.apply_settings(&settings);
    app.create_default_archive();
    wait_for_worker(&mut app);

    assert!(app.has_current_archive());
    assert!(app.can_add_to_archive());
    assert!(app.has_unsaved_changes);
    assert!(app.archive.is_empty());
    assert!(
        operations::list_archive(&app.read_archive, None, None)
            .unwrap()
            .is_empty()
    );
    assert!(
        operations::info_archive(&app.read_archive, false).unwrap()[0]
            .user
            .is_some()
    );
    assert!(app.report.is_none());

    app.show_operation(OperationKind::AddToArchive);
    assert_eq!(app.operation.block_size, "16M");
    assert_eq!(app.operation.compression_level, "9");
    assert_eq!(app.operation.aead, 1);
    assert!(
        app.operation.metadata
            && app.operation.dates
            && app.operation.creation_time
            && app.operation.access_time
            && app.operation.special_files
            && app.operation.strict
    );
    assert_eq!(app.operation.include_files, "*.rs");
    assert_eq!(app.operation.exclude_files, "*.tmp");
    assert_eq!(app.operation.include_folders, "src/**");
    assert_eq!(app.operation.exclude_folders, "target/**");
    assert!(!app.operation.password_is_set);
}

#[test]
fn startup_archive_reports_invalid_defaults() {
    let mut app = ZargApp {
        default_block_size: "invalid".to_owned(),
        ..Default::default()
    };
    app.create_default_archive();
    assert!(!app.busy);
    assert!(!app.has_current_archive());
    assert!(!app.status.is_empty());
}

#[test]
fn add_to_unsaved_new_archive_uses_working_archive() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("added.txt");
    let other_input = directory.path().join("second.txt");
    std::fs::write(&input, b"added contents").unwrap();
    std::fs::write(&other_input, b"second contents").unwrap();

    let mut app = ZargApp::default();
    assert!(!app.can_add_to_archive());
    app.start_operation(app.new_operation_form(OperationKind::NewArchive, ""));
    assert!(!app.can_add_to_archive());
    wait_for_worker(&mut app);
    assert!(app.can_add_to_archive());
    assert!(app.has_current_archive());

    app.show_operation(OperationKind::AddToArchive);
    assert_eq!(app.operation.archive, app.read_archive);
    app.operation.compression_level = "6".to_owned();
    app.operation.input_paths.extend([input, other_input]);
    click_add_archive_action(&mut app);
    assert!(!app.can_add_to_archive());
    wait_for_worker(&mut app);

    assert!(app.can_add_to_archive());
    assert!(app.archive.is_empty());
    assert!(app.has_unsaved_changes);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert!(app.entries.iter().any(|(path, _)| path == "added.txt"));
    assert!(app.entries.iter().any(|(path, _)| path == "second.txt"));
    app.show_operation(OperationKind::AddToArchive);
    assert_eq!(app.operation.compression_level, "6");
    assert!(app.operation.input_paths.is_empty());
    let visible_paths: Vec<_> = app
        .visible_entry_indices
        .iter()
        .map(|&index| app.entries[index].0.as_str())
        .collect();
    assert!(visible_paths.contains(&"added.txt"));
    assert!(visible_paths.contains(&"second.txt"));
    assert!(
        operations::list_archive(&app.read_archive, None, None)
            .unwrap()
            .iter()
            .any(|(path, _)| path.as_str() == "added.txt")
    );
    assert!(
        operations::list_archive(&app.read_archive, None, None)
            .unwrap()
            .iter()
            .any(|(path, _)| path.as_str() == "second.txt")
    );
    app.imported_archive_dir = Some(tempfile::tempdir().unwrap());
    assert!(!app.can_add_to_archive());
}

#[test]
fn dropping_files_and_folders_prefills_add_form_for_current_archive() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("dropped.txt");
    let folder = directory.path().join("dropped-folder");
    std::fs::write(&file, b"dropped file").unwrap();
    std::fs::create_dir(&folder).unwrap();

    let mut app = ZargApp::default();
    app.create_default_archive();
    wait_for_worker(&mut app);
    assert!(app.can_add_to_archive());

    let context = egui::Context::default();
    let input = egui::RawInput {
        dropped_files: vec![
            std::sync::Arc::new(TestDroppedFile(file.clone())),
            std::sync::Arc::new(TestDroppedFile(folder.clone())),
        ],
        ..Default::default()
    };
    context
        .run_ui(input, |ui| app.handle_dropped_files(ui.ctx()))
        .drop_without_applying_deltas();

    assert_eq!(app.operation.kind, Some(OperationKind::AddToArchive));
    assert_eq!(app.operation.archive, app.read_archive);
    assert_eq!(app.operation.input_paths, vec![file, folder]);
}

fn click_add_archive_action(app: &mut ZargApp) {
    let context = egui::Context::default();
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(500.0, 360.0));
    let mut button_rect = egui::Rect::NOTHING;
    for frame in 0..3 {
        let events = if frame == 0 {
            Vec::new()
        } else {
            vec![
                egui::Event::PointerMoved(button_rect.center()),
                egui::Event::PointerButton {
                    pos: button_rect.center(),
                    button: egui::PointerButton::Primary,
                    pressed: frame == 1,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        let input = egui::RawInput {
            screen_rect: Some(rect),
            events,
            ..Default::default()
        };
        context
            .run_ui(input, |ui| {
                egui::Panel::bottom("operation-actions")
                    .default_size(ui.spacing().interact_size.y.max(16.0) + 24.0)
                    .frame(egui::Frame::new().inner_margin(3))
                    .show_separator_line(false)
                    .show(ui, |ui| {
                        let response = app
                            .render_operation_actions(ui, OperationKind::AddToArchive)
                            .unwrap();
                        assert!(rect.contains_rect(response.rect));
                        assert!(response.enabled());
                        button_rect = response.rect;
                    });
            })
            .drop_without_applying_deltas();
    }
}

#[test]
fn quitting_with_unsaved_changes_prompts_before_exiting() {
    let mut app = ZargApp {
        archive: "current.zar".to_owned(),
        read_archive: "current.zar".to_owned(),
        has_unsaved_changes: true,
        latest_archive_empty: Some(false),
        ..Default::default()
    };

    app.request_quit(&egui::Context::default());

    assert!(app.has_unsaved_changes);
    assert!(!app.exit_requested);
    assert!(matches!(
        app.pending_archive_switch,
        Some(ArchiveSwitchAction::Quit)
    ));
}

#[test]
fn quitting_without_unsaved_changes_requests_exit() {
    let mut app = ZargApp::default();

    app.request_quit(&egui::Context::default());

    assert!(app.exit_requested);
}

#[test]
fn archive_switch_is_deferred_when_changes_are_unsaved() {
    let mut app = ZargApp {
        archive: "current.zar".to_owned(),
        has_unsaved_changes: true,
        ..Default::default()
    };
    let target = PathBuf::from("next.zar");

    app.open_archive(target.clone(), None);

    assert_eq!(app.archive, "current.zar");
    assert!(app.has_unsaved_changes);
    assert!(matches!(
        app.pending_archive_switch,
        Some(ArchiveSwitchAction::OpenPath(path, None)) if path == target
    ));
}

#[test]
fn new_archive_skips_confirmation_only_for_empty_first_commit() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("input.txt");
    std::fs::write(&file, b"contents").unwrap();

    let mut empty = ZargApp::default();
    empty.create_default_archive();
    wait_for_worker(&mut empty);
    empty.show_operation(OperationKind::NewArchive);
    wait_for_worker(&mut empty);
    assert!(empty.pending_archive_switch.is_none());
    assert_eq!(empty.operation.kind, Some(OperationKind::NewArchive));
    assert!(empty.has_current_archive());
    empty.close_operation();

    operations::update_custom_metadata(
        &empty.read_archive,
        &[(
            "note".to_owned(),
            zarlib::MetadataType::String("value".to_owned()),
        )],
        &[],
    )
    .unwrap();
    let mut add = empty.new_operation_form(OperationKind::AddToArchive, &empty.read_archive);
    add.input_paths.push(file.clone());
    empty.start_operation(add);
    wait_for_worker(&mut empty);
    assert!(empty.entries.iter().any(|(path, _)| path == "input.txt"));
    empty.show_operation(OperationKind::NewArchive);
    wait_for_worker(&mut empty);
    assert!(empty.pending_archive_switch.is_none());
    assert_eq!(empty.operation.kind, Some(OperationKind::NewArchive));

    let mut nonempty = ZargApp::default();
    let mut form = nonempty.new_operation_form(OperationKind::NewArchive, "");
    form.input_paths.push(file.clone());
    nonempty.start_operation(form);
    wait_for_worker(&mut nonempty);
    nonempty.show_operation(OperationKind::NewArchive);
    wait_for_worker(&mut nonempty);
    assert!(matches!(
        nonempty.pending_archive_switch,
        Some(ArchiveSwitchAction::New)
    ));
    assert!(nonempty.operation.kind.is_none());
    assert!(nonempty.has_current_archive());

    // An empty latest state no longer requires confirmation.
    nonempty.pending_archive_switch = None;
    nonempty.start_operation({
        let mut form =
            nonempty.new_operation_form(OperationKind::RemoveEntries, &nonempty.read_archive);
        form.remove_text = "input.txt".to_owned();
        form
    });
    wait_for_worker(&mut nonempty);
    assert!(nonempty.entries.is_empty());
    nonempty.show_operation(OperationKind::NewArchive);
    wait_for_worker(&mut nonempty);
    assert!(nonempty.pending_archive_switch.is_none());
    assert_eq!(nonempty.operation.kind, Some(OperationKind::NewArchive));
}

#[test]
fn empty_archives_disable_saving_and_open_without_confirmation() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.txt");
    let destination = directory.path().join("empty.zar");
    let existing = directory.path().join("existing.zar");
    std::fs::write(&input, b"contents").unwrap();
    operations::create_archive(existing.to_str().unwrap(), std::slice::from_ref(&input)).unwrap();

    let mut app = ZargApp::default();
    app.create_default_archive();
    wait_for_worker(&mut app);
    assert_eq!(app.latest_archive_empty, Some(true));
    assert!(!app.can_save_archive_as());
    assert!(!app.save_archive());
    assert!(!app.save_archive_as());
    assert!(!app.save_archive_to(destination.to_string_lossy().into_owned()));
    assert!(!destination.exists());
    app.open_archive(existing, None);
    assert!(app.pending_archive_switch.is_none());
    wait_for_worker(&mut app);
    assert_eq!(app.latest_archive_empty, Some(false));
    assert!(app.can_save_archive_as());
    assert!(app.entries.iter().any(|(path, _)| path == "input.txt"));
}

#[test]
fn archive_emptiness_tracks_latest_revision_not_displayed_history() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.txt");
    std::fs::write(&input, b"contents").unwrap();
    let mut app = ZargApp::default();
    let mut create = app.new_operation_form(OperationKind::NewArchive, "");
    create.input_paths.push(input.clone());
    app.start_operation(create);
    wait_for_worker(&mut app);
    let mut remove = app.new_operation_form(OperationKind::RemoveEntries, &app.read_archive);
    remove.remove_text = "input.txt".into();
    app.start_operation(remove);
    wait_for_worker(&mut app);
    let empty_commit = app.selected_commit.clone();
    assert_eq!(app.latest_archive_empty, Some(true));
    let mut add = app.new_operation_form(OperationKind::AddToArchive, &app.read_archive);
    add.input_paths.push(input);
    app.start_operation(add);
    wait_for_worker(&mut app);
    assert_eq!(app.latest_archive_empty, Some(false));
    assert!(app.can_save_archive_as());
    app.select_commit(empty_commit.clone());
    wait_for_worker(&mut app);
    assert!(
        app.entries.is_empty(),
        "empty_commit={empty_commit:?}, ids={:?}, selected={:?}, report={:?}, entries={:?}",
        app.commit_ids,
        app.selected_commit,
        app.report,
        app.entries.iter().map(|(path, _)| path).collect::<Vec<_>>()
    );
    assert_eq!(app.latest_archive_empty, Some(false));
    assert!(app.can_save_archive_as());
    app.open_archive(directory.path().join("another.zar"), None);
    assert!(matches!(
        app.pending_archive_switch,
        Some(ArchiveSwitchAction::OpenPath(_, None))
    ));

    app.pending_archive_switch = None;
    let latest = app.commit_ids.last().unwrap().clone();
    app.select_commit(latest);
    wait_for_worker(&mut app);
    let mut remove = app.new_operation_form(OperationKind::RemoveEntries, &app.read_archive);
    remove.remove_text = "input.txt".into();
    app.start_operation(remove);
    wait_for_worker(&mut app);
    assert_eq!(app.latest_archive_empty, Some(true));
    assert!(!app.can_save_archive_as());
    app.select_commit(app.commit_ids[0].clone());
    wait_for_worker(&mut app);
    assert!(!app.entries.is_empty());
    assert_eq!(app.latest_archive_empty, Some(true));
    assert!(!app.can_save_archive_as());
}

#[test]
fn new_archive_reports_first_commit_check_errors() {
    let mut app = ZargApp {
        has_unsaved_changes: true,
        read_archive: "/nonexistent/zarg-archive.zar".to_owned(),
        ..Default::default()
    };
    app.show_operation(OperationKind::NewArchive);
    wait_for_worker(&mut app);
    assert!(app.operation.kind.is_none());
    assert!(app.pending_archive_switch.is_none());
    assert!(app.has_unsaved_changes);
    assert!(
        app.report
            .as_deref()
            .unwrap()
            .contains("Cannot check the first archive commit")
    );
}

fn wait_for_worker(app: &mut ZargApp) {
    let context = egui::Context::default();
    for _ in 0..12_000 {
        app.poll_worker(&context);
        if !app.busy {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!(
        "archive worker did not finish in time: progress={}, confirmation={:?}",
        app.progress_message, app.confirmation
    );
}

fn wait_for_open_password_dialog(app: &mut ZargApp) {
    let context = egui::Context::default();
    for _ in 0..12_000 {
        app.poll_worker(&context);
        if app.open_password_dialog.is_some() {
            return;
        }
        assert!(app.busy, "archive worker stopped: {:?}", app.report);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("archive password dialog did not appear");
}

#[test]
fn opening_encrypted_archive_accepts_per_commit_passwords_and_cancel() {
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("multiple.zar");
    let plain = directory.path().join("plain.txt");
    let first = directory.path().join("first.txt");
    let second = directory.path().join("second.txt");
    for path in [&plain, &first, &second] {
        std::fs::write(path, path.file_name().unwrap().as_encoded_bytes()).unwrap();
    }
    let path = archive.to_str().unwrap();
    operations::create_archive(path, &[plain]).unwrap();
    for (input, password, existing) in [
        (first, b"first password".as_slice(), None),
        (
            second,
            b"second password".as_slice(),
            Some(b"first password".to_vec()),
        ),
    ] {
        operations::add_archive_with_encoding_progress_and_filters_and_encryption(
            path,
            &[input],
            EncodingOptions::default(),
            None,
            AddInputOptions::default(),
            &FilterSet::default(),
            Some((
                password,
                ContentAead::Aes256Gcm,
                EncryptionScope::HeaderAndData,
            )),
            &PasswordSet::new(existing),
        )
        .unwrap();
    }

    let mut app = ZargApp::default();
    app.archive = "previous.zar".to_owned();
    app.read_archive = app.archive.clone();
    app.open_archive_now(archive.clone(), None);
    wait_for_open_password_dialog(&mut app);
    let dialog = app.open_password_dialog.as_ref().unwrap();
    assert_eq!(dialog.rows.len(), 3);
    assert!(!dialog.rows[0].encrypted);
    assert!(dialog.rows[1].encrypted && dialog.rows[2].encrypted);
    dialog.answers.send(None).unwrap();
    wait_for_worker(&mut app);
    assert_eq!(app.archive, "previous.zar");
    assert!(app.open_password_dialog.is_none());

    app.open_archive_now(archive.clone(), None);
    wait_for_open_password_dialog(&mut app);
    let dialog = app.open_password_dialog.as_mut().unwrap();
    dialog.rows[1].password = "wrong password".to_owned();
    dialog.answers.send(Some(dialog.passwords())).unwrap();
    dialog.busy = true;
    let context = egui::Context::default();
    for _ in 0..12_000 {
        app.poll_worker(&context);
        if app
            .open_password_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.error.is_some())
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(app.open_password_dialog.as_ref().unwrap().error.is_some());
    assert_eq!(app.archive, "previous.zar");
    let dialog = app.open_password_dialog.as_mut().unwrap();
    dialog.rows[1].password = "first password".to_owned();
    dialog.rows[2].password = "second password".to_owned();
    dialog.answers.send(Some(dialog.passwords())).unwrap();
    dialog.busy = true;
    wait_for_worker(&mut app);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert_eq!(app.archive_passwords.len(), 2);
    app.show_archive_info();
    let context = egui::Context::default();
    for _ in 0..12_000 {
        app.poll_archive_info(&context);
        if app.info_loading.is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let view = app.info_view.as_ref().expect("archive info loaded");
    assert_eq!(app.info_passwords.len(), 2);
    assert!(view.commits[1].commit.details.is_some());
    assert!(view.commits[2].commit.details.is_some());
    app.clear_archive_info();
    for name in ["plain.txt", "first.txt", "second.txt"] {
        assert!(app.entries.iter().any(|(path, _)| path == name), "{name}");
    }
    let first_commit = app.commit_ids[1].clone();
    app.select_commit(first_commit);
    wait_for_worker(&mut app);
    assert!(app.entries.iter().any(|(path, _)| path == "first.txt"));
    assert!(!app.entries.iter().any(|(path, _)| path == "second.txt"));
    let last_commit = app.commit_ids[2].clone();
    app.select_commit(last_commit);
    wait_for_worker(&mut app);
    let output = directory.path().join("extracted");
    std::fs::create_dir(&output).unwrap();
    app.show_operation(OperationKind::ExtractArchive);
    app.operation.destination = output.to_string_lossy().into_owned();
    let form = std::mem::take(&mut app.operation);
    app.start_operation(form);
    wait_for_worker(&mut app);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert!(app.entries.iter().any(|(path, _)| path == "second.txt"));
    assert_eq!(
        std::fs::read(output.join("first.txt")).unwrap(),
        b"first.txt"
    );
    assert_eq!(
        std::fs::read(output.join("second.txt")).unwrap(),
        b"second.txt"
    );
    app.open_archive_now(archive, None);
    wait_for_open_password_dialog(&mut app);
    let dialog = app.open_password_dialog.as_mut().unwrap();
    assert_eq!(dialog.rows[1].password, "first password");
    assert_eq!(dialog.rows[2].password, "second password");
    for row in &mut dialog.rows {
        row.password.clear();
    }
    dialog.answers.send(Some(dialog.passwords())).unwrap();
    dialog.busy = true;
    wait_for_worker(&mut app);
    assert!(app.archive_passwords.is_empty());
    assert!(app.entries.iter().any(|(path, _)| path == "plain.txt"));
    assert!(!app.entries.iter().any(|(path, _)| path == "first.txt"));
    assert!(!app.entries.iter().any(|(path, _)| path == "second.txt"));
    app.show_operation(OperationKind::ListArchive);
    app.operation.to = app.commit_ids[1].clone();
    app.operation.password_is_set = true;
    app.operation.password = "first password".to_owned();
    let form = std::mem::take(&mut app.operation);
    app.start_operation(form);
    wait_for_worker(&mut app);
    assert!(app.report.is_none(), "{:?}", app.report);
    assert_eq!(app.archive_passwords.len(), 1);
    app.show_archive_info();
    for _ in 0..12_000 {
        app.poll_archive_info(&context);
        if app.info_loading.is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let view = app.info_view.as_ref().expect("archive info loaded");
    assert!(view.commits[1].commit.details.is_some());
    assert!(view.commits[2].commit.details.is_none());
}

#[test]
fn create_context_action_prefills_archive_path_from_first_selection() {
    let directory = std::env::temp_dir().join(format!(
        "zarg-create-context-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let first = directory.join("first file.txt");
    let second = directory.join("second.txt");
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();

    let app = ZargApp::from_context_action(
        ContextAction::Create,
        vec![first.clone(), second.clone()],
        &crate::settings::Settings {
            archive_defaults: crate::settings::ArchiveDefaults {
                dates: true,
                include_files: "*.txt".into(),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert!(app.operation.dates);
    assert_eq!(app.operation.include_files, "*.txt");

    assert_eq!(
        app.operation.archive,
        directory
            .join("first file.zar")
            .to_string_lossy()
            .into_owned()
    );
    assert_eq!(app.operation.input_paths, [first, second]);

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn password_requirements_report_missing_rules() {
    assert_eq!(
        password_requirements("short"),
        vec![
            "at least 12 characters",
            "at least one uppercase letter",
            "at least one digit",
            "at least one symbol",
        ]
    );
    assert!(password_requirements("Correct-Horse9").is_empty());
}

#[test]
fn block_sizes_match_cli_limits() {
    assert_eq!(parse_block_size("64K").unwrap(), 64 * 1024);
    assert_eq!(parse_block_size("1M").unwrap(), 1024 * 1024);
    assert_eq!(parse_block_size("auto").unwrap(), 0);
    assert!(parse_block_size("63K").is_err());
    assert!(parse_block_size("33M").is_err());
}

#[test]
fn archive_list_cells_preserve_column_widths() {
    let mut row_width = 0.0;
    egui::__run_test_ui(|ui| {
        ui.horizontal(|ui| {
            left_aligned_cell(ui, 110.0, 20.0, egui::Label::new("1.2 KiB"));
            left_aligned_cell(ui, 110.0, 20.0, egui::Label::new("4.8 KiB"));
            row_width = ui.min_rect().width();
        });
    });
    assert!(row_width >= 220.0, "row width collapsed to {row_width}");
}

#[test]
fn repeated_filter_text_lines_are_parsed() {
    assert_eq!(lines(" *.rs \n\n*.toml"), ["*.rs", "*.toml"]);
}

#[test]
fn archive_list_filter_returns_matching_entry_indexes() {
    let entries = vec![
        ("docs/README.md".to_owned(), EntryData::File(12)),
        ("src/main.rs".to_owned(), EntryData::File(20)),
        ("docs/guide.md".to_owned(), EntryData::File(30)),
    ];
    assert_eq!(
        filtered_entry_indices(&entries, &EntryFilter::new("", false, false).unwrap()),
        [0, 1, 2]
    );
    assert_eq!(
        filtered_entry_indices(&entries, &EntryFilter::new("DOCS/", false, false).unwrap()),
        [0, 2]
    );
}

#[test]
fn file_size_columns_show_compressed_uncompressed_and_ratio_in_order() {
    let details = ArchiveEntryDetails {
        compressed_size: Some(200),
        compression_ratio: Some(0.2),
        ..Default::default()
    };
    assert_eq!(
        file_size_columns(1000, Some(&details)),
        (display_size(200), display_size(1000), "20.0%".to_owned())
    );
    assert_eq!(
        file_size_columns(1000, None),
        ("—".to_owned(), display_size(1000), "—".to_owned())
    );
    let empty = ArchiveEntryDetails {
        compressed_size: Some(0),
        ..Default::default()
    };
    assert_eq!(
        file_size_columns(0, Some(&empty)),
        (display_size(0), display_size(0), "—".to_owned())
    );
    let tiny = ArchiveEntryDetails {
        compressed_size: Some(0),
        compression_ratio: Some(0.1),
        ..Default::default()
    };
    assert_eq!(file_size_columns(1, Some(&tiny)).2, "10.0%");
}

#[test]
fn idle_archive_summary_uses_weighted_file_sizes_and_total_commit_count() {
    let entries = vec![
        ("dir".to_owned(), EntryData::Directory),
        ("dir/a".to_owned(), EntryData::File(1000)),
        ("dir/b".to_owned(), EntryData::File(3000)),
        ("empty".to_owned(), EntryData::File(0)),
    ];
    let details = std::collections::BTreeMap::from([
        (
            "dir/a".to_owned(),
            ArchiveEntryDetails {
                compressed_size: Some(200),
                compression_ratio: Some(0.2),
                ..Default::default()
            },
        ),
        (
            "dir/b".to_owned(),
            ArchiveEntryDetails {
                compressed_size: Some(1500),
                compression_ratio: Some(0.5),
                ..Default::default()
            },
        ),
    ]);
    let summary = EntrySummary::from_entries(&entries, &details);
    assert_eq!(
        summary.display(entries.len(), 3),
        "4 entries | Compressed: 1.66 KiB | Uncompressed: 3.91 KiB | Ratio: 42.5% | 3 commits"
    );
    assert_eq!(
        EntrySummary::from_entries(&[], &details).display(0, 1),
        "0 entries | Compressed: 0 B | Uncompressed: 0 B | Ratio: — | 1 commit"
    );
    let missing = EntrySummary::from_entries(&entries, &Default::default());
    assert_eq!(
        missing.display(entries.len(), 1),
        "4 entries | Compressed: — | Uncompressed: 3.91 KiB | Ratio: — | 1 commit"
    );
    let tiny = vec![("tiny".to_owned(), EntryData::File(1))];
    let tiny_details = std::collections::BTreeMap::from([(
        "tiny".to_owned(),
        ArchiveEntryDetails {
            compressed_size: Some(0),
            compression_ratio: Some(0.1),
            ..Default::default()
        },
    )]);
    assert_eq!(
        EntrySummary::from_entries(&tiny, &tiny_details).ratio,
        Some(0.1)
    );
    let overflow = vec![
        ("a".to_owned(), EntryData::File(u64::MAX)),
        ("b".to_owned(), EntryData::File(1)),
    ];
    assert!(
        EntrySummary::from_entries(&overflow, &Default::default())
            .uncompressed
            .is_none()
    );
}

#[test]
fn archive_entries_sort_by_requested_criterion() {
    let entries = vec![
        ("b.txt".to_owned(), EntryData::File(10)),
        ("a.rs".to_owned(), EntryData::File(100)),
        ("dir".to_owned(), EntryData::Directory),
        ("c.txt".to_owned(), EntryData::File(20)),
    ];
    let details = std::collections::BTreeMap::from([
        (
            "b.txt".to_owned(),
            ArchiveEntryDetails {
                modified_unix_millis: Some(100),
                ..ArchiveEntryDetails::default()
            },
        ),
        (
            "a.rs".to_owned(),
            ArchiveEntryDetails {
                modified_unix_millis: Some(300),
                ..ArchiveEntryDetails::default()
            },
        ),
        (
            "dir".to_owned(),
            ArchiveEntryDetails {
                modified_unix_millis: Some(200),
                ..ArchiveEntryDetails::default()
            },
        ),
    ]);
    let mut indices = (0..entries.len()).collect::<Vec<_>>();

    sort_entry_indices(&mut indices, &entries, &details, SortBy::Name);
    assert_eq!(indices, [1, 0, 3, 2]);
    sort_entry_indices(&mut indices, &entries, &details, SortBy::Date);
    assert_eq!(indices, [1, 2, 0, 3]);
    sort_entry_indices(&mut indices, &entries, &details, SortBy::UncompressedSize);
    assert_eq!(indices, [1, 3, 0, 2]);
    sort_entry_indices(&mut indices, &entries, &details, SortBy::Type);
    assert_eq!(indices, [2, 1, 0, 3]);

    let sizes = std::collections::BTreeMap::from([
        (
            "b.txt".to_owned(),
            ArchiveEntryDetails {
                compressed_size: Some(5),
                compression_ratio: Some(0.5),
                ..ArchiveEntryDetails::default()
            },
        ),
        (
            "a.rs".to_owned(),
            ArchiveEntryDetails {
                compressed_size: Some(20),
                compression_ratio: Some(0.2),
                ..ArchiveEntryDetails::default()
            },
        ),
        (
            "c.txt".to_owned(),
            ArchiveEntryDetails {
                compressed_size: Some(18),
                compression_ratio: Some(0.9),
                ..ArchiveEntryDetails::default()
            },
        ),
    ]);
    sort_entry_indices(&mut indices, &entries, &sizes, SortBy::CompressedSize);
    assert_eq!(indices, [1, 3, 0, 2]);
    // Ratios: a.rs 20%, b.txt 50%, c.txt 90%; the folder has none.
    sort_entry_indices(&mut indices, &entries, &sizes, SortBy::Ratio);
    assert_eq!(indices, [1, 0, 3, 2]);
}

#[test]
fn modified_time_format_uses_utc_and_handles_pre_epoch_values() {
    assert_eq!(format_unix_millis(0), "1970-01-01 00:00 UTC");
    assert_eq!(format_unix_millis(-1), "1969-12-31 23:59 UTC");
}

#[test]
fn structured_rows_build_and_expand_directory_hierarchy() {
    let entries = vec![
        ("folder/a.txt".to_owned(), EntryData::File(12)),
        ("folder/sub/b.txt".to_owned(), EntryData::File(24)),
        ("loose.txt".to_owned(), EntryData::File(6)),
    ];
    let all = [0, 1, 2];
    let details = std::collections::BTreeMap::new();
    let collapsed = structured_rows(
        &entries,
        &all,
        &BTreeSet::new(),
        false,
        &details,
        SortBy::Name,
        false,
    );
    assert_eq!(
        collapsed
            .iter()
            .map(|row| row.path.as_str())
            .collect::<Vec<_>>(),
        ["folder", "loose.txt"]
    );
    let expanded = structured_rows(
        &entries,
        &all,
        &BTreeSet::from(["folder".to_owned()]),
        false,
        &details,
        SortBy::Name,
        false,
    );
    assert_eq!(
        expanded
            .iter()
            .map(|row| (row.path.as_str(), row.depth))
            .collect::<Vec<_>>(),
        [
            ("folder", 0),
            ("folder/a.txt", 1),
            ("folder/sub", 1),
            ("loose.txt", 0)
        ]
    );
}

#[test]
fn structured_filter_reveals_matching_entry_ancestors() {
    let entries = vec![
        ("folder/sub/match.txt".to_owned(), EntryData::File(12)),
        ("folder/other.txt".to_owned(), EntryData::File(24)),
    ];
    let rows = structured_rows(
        &entries,
        &[0],
        &BTreeSet::new(),
        true,
        &std::collections::BTreeMap::new(),
        SortBy::Name,
        false,
    );
    assert_eq!(
        rows.iter().map(|row| row.path.as_str()).collect::<Vec<_>>(),
        ["folder", "folder/sub", "folder/sub/match.txt"]
    );
}

#[test]
fn extended_selection_supports_replace_toggle_and_ranges() {
    let visible = [1, 2, 3, 4, 5];
    let empty = BTreeSet::new();
    let plain = extended_selection(&visible, &empty, &None, &2, egui::Modifiers::NONE);
    assert_eq!(plain.selected, BTreeSet::from([2]));
    let toggled = extended_selection(
        &visible,
        &plain.selected,
        &plain.anchor,
        &4,
        egui::Modifiers {
            ctrl: true,
            ..Default::default()
        },
    );
    assert_eq!(toggled.selected, BTreeSet::from([2, 4]));
    let ranged = extended_selection(
        &visible,
        &toggled.selected,
        &Some(2),
        &5,
        egui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(ranged.selected, BTreeSet::from([2, 3, 4, 5]));
    let additive_range = extended_selection(
        &visible,
        &plain.selected,
        &Some(2),
        &4,
        egui::Modifiers {
            shift: true,
            ctrl: true,
            ..Default::default()
        },
    );
    assert_eq!(additive_range.selected, BTreeSet::from([2, 3, 4]));
}

#[test]
fn file_icons_follow_mime_type_extensions() {
    assert_eq!(file_icon("images/photo.JPEG"), UiIcon::Image);
    assert_eq!(file_icon("music/song.flac"), UiIcon::Audio);
    assert_eq!(file_icon("src/main.rs"), UiIcon::Code);
    assert_eq!(file_icon("archives/backup.ZAR"), UiIcon::ZarArchive);
    assert_eq!(file_icon("archives/backup.zip"), UiIcon::Archive);
    assert_eq!(file_icon("archives/backup.7z"), UiIcon::Archive);
    assert_eq!(file_icon("archives/backup.iso"), UiIcon::Archive);
    assert_eq!(file_icon("archives/backup.tar"), UiIcon::Archive);
    assert_eq!(file_icon("archives/backup.tar.gz"), UiIcon::Archive);
    assert_eq!(file_icon("archives/backup.tgz"), UiIcon::Archive);
    assert_eq!(file_icon("unknown.data"), UiIcon::Generic);
}

#[test]
fn bundled_icons_are_valid_in_both_themes() {
    let icons = [
        UiIcon::Application,
        UiIcon::OpenArchive,
        UiIcon::SaveArchive,
        UiIcon::NewArchive,
        UiIcon::Quit,
        UiIcon::SelectAll,
        UiIcon::ClearSelection,
        UiIcon::Refresh,
        UiIcon::ListContents,
        UiIcon::Tools,
        UiIcon::Configure,
        UiIcon::TestArchive,
        UiIcon::Undo,
        UiIcon::ChangeCommits,
        UiIcon::Benchmark,
        UiIcon::Join,
        UiIcon::About,
        UiIcon::Add,
        UiIcon::AddFolder,
        UiIcon::OnlineHelp,
        UiIcon::Extract,
        UiIcon::Info,
        UiIcon::Remove,
        UiIcon::Browse,
        UiIcon::GoUp,
        UiIcon::ClearInputs,
        UiIcon::Confirm,
        UiIcon::ConfirmApply,
        UiIcon::Cancel,
        UiIcon::AddFilterList,
        UiIcon::Folder,
        UiIcon::Bookmarks,
        UiIcon::Drive,
        UiIcon::Symlink,
        UiIcon::Image,
        UiIcon::Audio,
        UiIcon::Video,
        UiIcon::Archive,
        UiIcon::ZarArchive,
        UiIcon::Pdf,
        UiIcon::Code,
        UiIcon::Python,
        UiIcon::Java,
        UiIcon::Script,
        UiIcon::JavaScript,
        UiIcon::Text,
        UiIcon::Word,
        UiIcon::Spreadsheet,
        UiIcon::Presentation,
        UiIcon::Generic,
        UiIcon::SpecialData,
        UiIcon::Error,
        UiIcon::Question,
    ];
    for theme in [
        crate::settings::IconTheme::Crystal,
        crate::settings::IconTheme::Flat,
    ] {
        for icon in icons {
            image::load_from_memory(icon_bytes(icon, theme))
                .unwrap_or_else(|error| panic!("invalid {theme:?} icon {icon:?}: {error}"));
        }
    }
}

#[test]
fn icon_theme_updates_cached_textures_and_settings() {
    use crate::settings::IconTheme;

    let context = egui::Context::default();
    let mut app = ZargApp::default();
    let original = icon_texture(&context, &app.icons, UiIcon::Folder);
    assert_eq!(
        icon_texture(&context, &app.icons, UiIcon::Folder).id(),
        original.id()
    );
    app.apply_settings(&crate::settings::Settings {
        icon_theme: IconTheme::Flat,
        ..Default::default()
    });
    assert!(app.icons.textures.borrow().is_empty());
    let flat = icon_texture(&context, &app.icons, UiIcon::Folder);
    assert_ne!(original.id(), flat.id());
    assert_ne!(
        icon_bytes(UiIcon::Folder, IconTheme::Crystal),
        icon_bytes(UiIcon::Folder, IconTheme::Flat)
    );
    assert_eq!(app.current_settings().icon_theme, IconTheme::Flat);
    app.icons.set_theme(IconTheme::Flat);
    assert_eq!(
        icon_texture(&context, &app.icons, UiIcon::Folder).id(),
        flat.id()
    );
    app.icons.set_theme(IconTheme::Crystal);
    assert!(app.icons.textures.borrow().is_empty());
}

#[test]
fn error_dialog_window_icon_is_valid() {
    let icon = dialogs::operation_error::error_dialog_icon();
    assert!(icon.width > 0 && icon.height > 0);
    assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
}

#[test]
fn invert_selection_flips_visible_entries_only() {
    let selected = BTreeSet::from(["a".to_owned(), "hidden".to_owned()]);
    let visible = ["a".to_owned(), "b".to_owned(), "c".to_owned()];
    assert_eq!(
        invert_selection(&selected, &visible),
        BTreeSet::from(["b".to_owned(), "c".to_owned(), "hidden".to_owned()])
    );
}

#[test]
fn filter_supports_wildcards_and_regex() {
    let entries = [
        ("docs/Readme.md", EntryData::File(1)),
        ("src/main.rs", EntryData::File(1)),
        ("src/lib.rs", EntryData::File(1)),
    ]
    .map(|(path, data)| (path.to_owned(), data));
    let run = |text: &str, regex: bool| {
        filtered_entry_indices(&entries, &EntryFilter::new(text, regex, false).unwrap())
    };
    assert_eq!(run("*.RS", false), [1, 2]);
    assert_eq!(run("m?in.rs", false), [1]);
    assert_eq!(run("src/*", false), [1, 2]);
    assert_eq!(run(r"^src/(main|lib)\.rs$", true), [1, 2]);
    assert_eq!(run("readme", true), [0]);
    assert!(EntryFilter::new("(", true, false).is_err());
    let case = |text: &str, regex: bool| {
        filtered_entry_indices(&entries, &EntryFilter::new(text, regex, true).unwrap())
    };
    assert!(case("*.RS", false).is_empty());
    assert!(case("README", false).is_empty());
    assert_eq!(case("Readme", false), [0]);
    assert!(case("readme", true).is_empty());
}

#[test]
fn recent_archives_are_unique_newest_first_and_capped() {
    let mut recent = Vec::new();
    for index in 0..12 {
        remember_recent_archive(&mut recent, &format!("/a{index}.zar"));
    }
    remember_recent_archive(&mut recent, "/a5.zar");
    assert_eq!(recent.len(), MAX_RECENT_ARCHIVES);
    assert_eq!(recent[0], "/a5.zar");
    assert_eq!(recent.iter().filter(|path| *path == "/a5.zar").count(), 1);
    assert_eq!(recent[1], "/a11.zar");
}

#[test]
fn benchmark_reports_sizes_and_cleans_up() {
    let dir = std::env::temp_dir().join(format!("zarg-bench-test-{}", std::process::id()));
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("a.txt"), vec![b'a'; 100_000]).unwrap();
    std::fs::write(data.join("b.txt"), b"hello").unwrap();
    let archive = dir.join("in.zar").to_string_lossy().into_owned();
    operations::create_archive(&archive, &[data]).unwrap();
    let result = run_benchmark(
        &archive,
        None,
        &BenchmarkSettings::default(),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(result.files, 2);
    assert_eq!(result.uncompressed, 100_005);
    assert!(result.archive_size > 0 && result.archive_size < result.uncompressed);
    let _ = std::fs::remove_dir_all(&dir);
}
