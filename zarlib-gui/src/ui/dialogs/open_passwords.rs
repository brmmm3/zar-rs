use super::super::*;

impl OpenPasswordDialog {
    fn copy_first_password(&mut self) {
        copy_first_password(&mut self.rows);
    }

    pub(in crate::ui) fn passwords(&self) -> BTreeMap<[u8; 4], String> {
        entered_passwords(&self.rows)
    }
}

impl UnlockCommitsDialog {
    fn copy_first_password(&mut self) {
        copy_first_password(&mut self.rows);
    }

    fn passwords(&self) -> BTreeMap<[u8; 4], String> {
        entered_passwords(&self.rows)
    }
}

fn copy_first_password(rows: &mut [OpenPasswordRow]) {
    let Some(password) = rows
        .iter()
        .find(|row| row.encrypted)
        .map(|row| row.password.clone())
        .filter(|password| !password.is_empty())
    else {
        return;
    };
    for row in rows.iter_mut().filter(|row| row.encrypted).skip(1) {
        row.password.clone_from(&password);
    }
}

fn entered_passwords(rows: &[OpenPasswordRow]) -> BTreeMap<[u8; 4], String> {
    rows.iter()
        .filter(|row| row.encrypted && !row.password.is_empty())
        .map(|row| (row.id, row.password.clone()))
        .collect()
}

fn verify_unlock_passwords(
    archive: &str,
    passwords: BTreeMap<[u8; 4], String>,
) -> Result<BTreeMap<[u8; 4], String>, String> {
    operations::list_archive_with_details_and_passwords(
        archive,
        None,
        &password_set(&passwords, None),
    )
    .map(|_| passwords)
    .map_err(|error| error.to_string())
}

fn render_rows(ui: &mut egui::Ui, rows: &mut [OpenPasswordRow], busy: bool) -> egui::Response {
    egui::Grid::new("open-password-commits")
        .num_columns(2)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            for row in rows.iter_mut() {
                ui.label(format!("Commit {:08x}", u32::from_le_bytes(row.id)));
                if row.encrypted {
                    ui.add_enabled(
                        !busy,
                        egui::TextEdit::singleline(&mut row.password)
                            .password(true)
                            .hint_text("Password"),
                    );
                } else {
                    ui.weak("Not encrypted");
                }
                ui.end_row();
            }
        });
    ui.add_space(8.0);
    let enabled = !busy
        && rows
            .iter()
            .find(|row| row.encrypted)
            .is_some_and(|row| !row.password.is_empty());
    ui.add_enabled(
        enabled,
        egui::Button::new("Same password for all encrypted commits"),
    )
}

impl ZargApp {
    pub(in crate::ui) fn render_open_password_dialog(&mut self, context: &egui::Context) {
        let Some(dialog) = &mut self.open_password_dialog else {
            return;
        };
        let viewport_id = egui::ViewportId::from_hash_of("open-archive-passwords");
        let mut cancel = false;
        let mut submit = false;
        context.show_viewport_immediate(
            viewport_id,
            Self::centered_dialog_viewport_builder(
                context,
                viewport_id,
                "Open encrypted archive",
                [520.0, 220.0],
                true,
            ),
            |ui, _class| {
                ui.painter().rect_filled(
                    ui.max_rect(),
                    0.0,
                    ui.visuals().window_fill().to_opaque(),
                );
                if ui.ctx().input(|input| input.viewport().close_requested()) {
                    if dialog.busy {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    } else {
                        cancel = true;
                    }

                }
                egui::Panel::bottom("open-password-actions")
                    .frame(egui::Frame::new().inner_margin(6))
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            cancel |= icon_button(ui, &self.icons, UiIcon::Cancel, "Cancel", !dialog.busy).clicked();
                            submit |= icon_button(ui, &self.icons, UiIcon::Confirm, "Open", !dialog.busy).clicked();
                        });
                    });
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.label("Enter passwords for the commits to unlock. Leave a field empty to keep that commit locked.");
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        if render_rows(ui, &mut dialog.rows, dialog.busy).clicked() {
                            dialog.copy_first_password();
                        }
                        if let Some(error) = &dialog.error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                        if dialog.busy {
                            ui.weak("Checking passwords...");
                        }
                    });
                });
                if cancel {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            },
        );
        if cancel {
            if dialog.answers.send(None).is_err() {
                self.status = "Could not cancel opening the archive: worker stopped.".to_owned();
                self.report = Some(self.status.clone());
            }
            self.open_password_dialog = None;
        } else if submit {
            if dialog.answers.send(Some(dialog.passwords())).is_err() {
                dialog.error = Some("Could not open the archive: worker stopped.".to_owned());
            } else {
                dialog.error = None;
                dialog.busy = true;
            }
        }
    }
}

impl ZargApp {
    pub(in crate::ui) fn show_unlock_commits(&mut self) {
        match operations::info_archive(&self.read_archive, false) {
            Ok(commits) => {
                let rows = commits
                    .into_iter()
                    .map(|commit| {
                        let encrypted = commit.header_encrypted || commit.data_encrypted;
                        OpenPasswordRow {
                            id: commit.id,
                            encrypted,
                            password: if encrypted {
                                self.archive_passwords
                                    .get(&commit.id)
                                    .cloned()
                                    .unwrap_or_default()
                            } else {
                                String::new()
                            },
                        }
                    })
                    .collect();
                self.unlock_commits_dialog = Some(UnlockCommitsDialog {
                    rows,
                    result: None,
                    error: None,
                    busy: false,
                });
            }
            Err(error) => {
                self.status = format!("Cannot read archive commits: {error}");
                self.report = Some(self.status.clone());
            }
        }
    }

    pub(in crate::ui) fn render_unlock_commits_dialog(&mut self, context: &egui::Context) {
        let Some(dialog) = &mut self.unlock_commits_dialog else {
            return;
        };
        let result = dialog.result.as_ref().map(Receiver::try_recv);
        match result {
            Some(Ok(result)) => {
                dialog.result = None;
                dialog.busy = false;
                match result {
                    Ok(passwords) => {
                        self.remember_commit_passwords(&passwords);
                        self.status = format!("Unlocked {} commit(s).", passwords.len());
                        self.unlock_commits_dialog = None;
                        return;
                    }
                    Err(error) => dialog.error = Some(error),
                }
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                dialog.result = None;
                dialog.busy = false;
                dialog.error = Some("Unlock worker stopped unexpectedly.".to_owned());
            }
            Some(Err(mpsc::TryRecvError::Empty)) | None => {}
        }

        let mut cancel = false;
        let mut submit = false;
        let viewport_id = egui::ViewportId::from_hash_of("unlock-commits");
        context.show_viewport_immediate(
            viewport_id,
            Self::centered_dialog_viewport_builder(
                context,
                viewport_id,
                "Unlock commits",
                [520.0, 220.0],
                true,
            ),
            |ui, _class| {
                ui.painter().rect_filled(
                    ui.max_rect(),
                    0.0,
                    ui.visuals().window_fill().to_opaque(),
                );
                if ui.ctx().input(|input| input.viewport().close_requested()) {
                    if dialog.busy {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    } else {
                        cancel = true;
                    }
                }
                egui::Panel::bottom("unlock-commits-actions")
                    .frame(egui::Frame::new().inner_margin(6))
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            cancel |= icon_button(
                                ui,
                                &self.icons,
                                UiIcon::Cancel,
                                "Cancel",
                                !dialog.busy,
                            )
                            .clicked();
                            submit |= icon_button(
                                ui,
                                &self.icons,
                                UiIcon::Unlock,
                                "Unlock",
                                !dialog.busy
                                    && dialog.rows.iter().any(|row| {
                                        row.encrypted && !row.password.is_empty()
                                    }),
                            )
                            .clicked();
                        });
                    });
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.label("Enter passwords for the commits to unlock. Leave a field empty to keep that commit locked.");
                    ui.add_space(8.0);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        if render_rows(ui, &mut dialog.rows, dialog.busy).clicked() {
                            dialog.copy_first_password();
                        }
                        if let Some(error) = &dialog.error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                        if dialog.busy {
                            ui.weak("Checking passwords...");
                        }
                    });
                });
                if cancel {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            },
        );
        if cancel {
            self.unlock_commits_dialog = None;
        } else if submit {
            let passwords = dialog.passwords();
            let archive = self.read_archive.clone();
            let (sender, receiver) = mpsc::channel();
            dialog.result = Some(receiver);
            dialog.error = None;
            dialog.busy = true;
            std::thread::spawn(move || {
                let result = verify_unlock_passwords(&archive, passwords);
                let _ = sender.send(result);
            });
        }
        if self.unlock_commits_dialog.is_some() {
            context.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlock_dialog_lists_commits_and_validates_passwords() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("encrypted.zar");
        let input = directory.path().join("input.txt");
        std::fs::write(&input, b"contents").unwrap();
        let archive = archive.to_string_lossy().into_owned();
        operations::create_archive_with_password_progress_and_filters_and_encryption(
            &archive,
            std::slice::from_ref(&input),
            false,
            EncodingOptions::default(),
            Some((
                b"correct password",
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

        let mut app = ZargApp {
            archive: archive.clone(),
            read_archive: archive.clone(),
            ..Default::default()
        };
        app.show_unlock_commits();
        let dialog = app.unlock_commits_dialog.as_ref().unwrap();
        assert_eq!(dialog.rows.len(), 1);
        assert!(dialog.rows[0].encrypted);

        let id = dialog.rows[0].id;
        assert!(
            verify_unlock_passwords(
                &archive,
                BTreeMap::from([(id, "wrong password".to_owned())])
            )
            .is_err()
        );
        assert_eq!(
            verify_unlock_passwords(
                &archive,
                BTreeMap::from([(id, "correct password".to_owned())])
            )
            .unwrap()
            .get(&id)
            .map(String::as_str),
            Some("correct password")
        );
    }

    #[test]
    fn same_password_button_is_enabled_only_by_first_encrypted_field() {
        let context = egui::Context::default();
        let (answers, _) = mpsc::channel();
        let mut dialog = OpenPasswordDialog {
            rows: vec![
                OpenPasswordRow {
                    id: 1u32.to_le_bytes(),
                    encrypted: false,
                    password: String::new(),
                },
                OpenPasswordRow {
                    id: 2u32.to_le_bytes(),
                    encrypted: true,
                    password: String::new(),
                },
                OpenPasswordRow {
                    id: 3u32.to_le_bytes(),
                    encrypted: true,
                    password: "other".to_owned(),
                },
            ],
            answers,
            error: None,
            busy: false,
        };
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 400.0));
        fn frame(
            context: &egui::Context,
            dialog: &mut OpenPasswordDialog,
            screen: egui::Rect,
            events: Vec<egui::Event>,
        ) -> egui::Response {
            let mut response = None;
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        response = Some(render_rows(ui, &mut dialog.rows, dialog.busy));
                        if response.as_ref().is_some_and(egui::Response::clicked) {
                            dialog.copy_first_password();
                        }
                    },
                )
                .drop_without_applying_deltas();
            response.unwrap()
        }
        assert!(!frame(&context, &mut dialog, screen, vec![]).enabled());
        dialog.rows[1].password = "first".to_owned();
        let response = frame(&context, &mut dialog, screen, vec![]);
        assert!(response.enabled());
        let pos = response.rect.center();
        for pressed in [true, false] {
            frame(
                &context,
                &mut dialog,
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
        assert_eq!(dialog.rows[2].password, "first");
        assert!(dialog.rows[0].password.is_empty());
        assert_eq!(dialog.passwords().len(), 2);
    }
}
