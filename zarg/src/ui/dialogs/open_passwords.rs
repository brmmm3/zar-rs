use super::super::*;

impl OpenPasswordDialog {
    fn copy_first_password(&mut self) {
        let Some(password) = self
            .rows
            .iter()
            .find(|row| row.encrypted)
            .map(|row| row.password.clone())
            .filter(|password| !password.is_empty())
        else {
            return;
        };
        for row in self.rows.iter_mut().filter(|row| row.encrypted).skip(1) {
            row.password.clone_from(&password);
        }
    }

    pub(in crate::ui) fn passwords(&self) -> BTreeMap<[u8; 4], String> {
        self.rows
            .iter()
            .filter(|row| row.encrypted && !row.password.is_empty())
            .map(|row| (row.id, row.password.clone()))
            .collect()
    }
}

fn render_rows(ui: &mut egui::Ui, dialog: &mut OpenPasswordDialog) -> egui::Response {
    egui::Grid::new("open-password-commits")
        .num_columns(2)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            for row in &mut dialog.rows {
                ui.label(format!("Commit {:08x}", u32::from_le_bytes(row.id)));
                if row.encrypted {
                    ui.add_enabled(
                        !dialog.busy,
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
    let enabled = !dialog.busy
        && dialog
            .rows
            .iter()
            .find(|row| row.encrypted)
            .is_some_and(|row| !row.password.is_empty());
    let response = ui.add_enabled(
        enabled,
        egui::Button::new("Same password for all encrypted commits"),
    );
    if response.clicked() {
        dialog.copy_first_password();
    }
    response
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
                        render_rows(ui, dialog);
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

#[cfg(test)]
mod tests {
    use super::*;

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
                    |ui| response = Some(render_rows(ui, dialog)),
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
