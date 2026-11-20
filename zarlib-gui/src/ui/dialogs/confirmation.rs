use super::super::*;

enum UnsavedArchiveChoice {
    Save,
    Discard,
    Cancel,
}

fn unsaved_archive_actions(ui: &mut egui::Ui, icons: &IconCache) -> [egui::Response; 3] {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        [
            icon_button(ui, icons, UiIcon::Cancel, "Cancel", true),
            icon_button(ui, icons, UiIcon::Remove, "Discard", true),
            icon_button(ui, icons, UiIcon::SaveArchive, "Save", true),
        ]
    })
    .inner
}

fn confirmation_actions(
    ui: &mut egui::Ui,
    icons: &IconCache,
) -> [(Option<OverwriteDecision>, egui::Response); 5] {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        [
            (None, icon_button(ui, icons, UiIcon::Cancel, "Cancel", true)),
            (
                Some(OverwriteDecision::Never),
                icon_button(ui, icons, UiIcon::Cancel, "No to all", true),
            ),
            (
                Some(OverwriteDecision::Always),
                icon_button(ui, icons, UiIcon::Confirm, "Yes to all", true),
            ),
            (
                Some(OverwriteDecision::No),
                icon_button(ui, icons, UiIcon::Cancel, "No", true),
            ),
            (
                Some(OverwriteDecision::Yes),
                icon_button(ui, icons, UiIcon::Confirm, "Yes", true),
            ),
        ]
    })
    .inner
}

impl ZargApp {
    pub(in crate::ui) fn render_unsaved_archive_dialog(&mut self, context: &egui::Context) {
        if self.pending_archive_switch.is_none() || self.pending_switch_after_save {
            return;
        }

        let viewport_id = egui::ViewportId::from_hash_of("unsaved-archive-dialog");
        let size_key = "unsaved-archive";
        let saved_size = self.dialog_size(size_key, [440.0, 170.0]);
        let size = [saved_size[0].max(440.0), saved_size[1].max(170.0)];
        let mut actual_size = None;
        let mut open = true;
        let mut choice = None;
        context.show_viewport_immediate(
            viewport_id,
            Self::centered_dialog_viewport_builder(
                context,
                viewport_id,
                "Unsaved changes",
                size,
                false,
            ),
            |ui, _class| {
                ui.painter().rect_filled(
                    ui.max_rect(),
                    0.0,
                    ui.visuals().window_fill().to_opaque(),
                );
                actual_size = Self::viewport_inner_size(ui.ctx());
                if ui.ctx().input(|input| input.viewport().close_requested()) {
                    open = false;
                }
                egui::Panel::bottom("unsaved-archive-actions")
                    .default_size(ui.spacing().interact_size.y.max(16.0) + 24.0)
                    .frame(egui::Frame::new().inner_margin(3))
                    .show_separator_line(false)
                    .show(ui, |ui| {
                        ui.separator();
                        let [cancel, discard, save] = unsaved_archive_actions(ui, &self.icons);
                        if cancel.clicked() {
                            choice = Some(UnsavedArchiveChoice::Cancel);
                        }
                        if discard.clicked() {
                            choice = Some(UnsavedArchiveChoice::Discard);
                        }
                        if save.clicked() {
                            choice = Some(UnsavedArchiveChoice::Save);
                        }
                    });
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().inner_margin(3))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let question = icon_texture(ui.ctx(), &self.icons, UiIcon::Question);
                            ui.add(
                                egui::Image::new(&question)
                                    .fit_to_exact_size(egui::vec2(48.0, 48.0)),
                            );
                            ui.add(
                                egui::Label::new(
                                    "Save changes to the current archive before continuing?",
                                )
                                .wrap(),
                            );
                        });
                    });
                if !open || choice.is_some() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            },
        );
        self.record_dialog_size(size_key, actual_size);

        if let Some(choice) = choice {
            match choice {
                UnsavedArchiveChoice::Save => {
                    let started = self.save_archive();
                    self.pending_switch_after_save = started;
                    if !started {
                        self.pending_archive_switch = None;
                    }
                }
                UnsavedArchiveChoice::Discard => {
                    if let Some(action) = self.pending_archive_switch.take() {
                        if !matches!(
                            action,
                            ArchiveSwitchAction::OpenDialog | ArchiveSwitchAction::OpenPath(..)
                        ) {
                            self.discard_current_archive();
                        }
                        self.execute_archive_switch(action);
                    }
                }
                UnsavedArchiveChoice::Cancel => self.pending_archive_switch = None,
            }
        } else if !open {
            self.pending_archive_switch = None;
        }
    }

    pub(in crate::ui) fn render_confirmation_dialog(&mut self, context: &egui::Context) {
        if let Some(prompt) = self.confirmation.clone() {
            let viewport_id = egui::ViewportId::from_hash_of("confirmation-dialog");
            let size_key = "confirmation";
            let saved_size = self.dialog_size(size_key, [500.0, 150.0]);
            let size = [saved_size[0].max(500.0), saved_size[1]];
            let mut actual_size = None;
            let mut open = true;
            context.show_viewport_immediate(
                viewport_id,
                Self::centered_dialog_viewport_builder(
                    context,
                    viewport_id,
                    "Confirmation",
                    size,
                    false,
                ),
                |ui, _class| {
                    ui.painter().rect_filled(
                        ui.max_rect(),
                        0.0,
                        ui.visuals().window_fill().to_opaque(),
                    );
                    actual_size = Self::viewport_inner_size(ui.ctx());
                    if ui.ctx().input(|input| input.viewport().close_requested()) {
                        open = false;
                    }
                    egui::Panel::bottom("confirmation-actions")
                        .frame(egui::Frame::new().inner_margin(3))
                        .show(ui, |ui| {
                            for (decision, response) in confirmation_actions(ui, &self.icons) {
                                if response.clicked() {
                                    self.answer_confirmation(decision);
                                }
                            }
                        });
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().inner_margin(3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let question =
                                    icon_texture(ui.ctx(), &self.icons, UiIcon::Question);
                                ui.add(
                                    egui::Image::new(&question)
                                        .fit_to_exact_size(egui::vec2(48.0, 48.0)),
                                );
                                ui.add(egui::Label::new(&prompt).wrap());
                            });
                        });
                    if !open || self.confirmation.is_none() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                },
            );
            self.record_dialog_size(size_key, actual_size);
            if !open && self.confirmation.is_some() {
                self.answer_confirmation(None);
            }
        }
    }
    pub(super) fn answer_confirmation(&mut self, answer: Option<OverwriteDecision>) {
        if let Some(worker) = &self.worker {
            if answer.is_none()
                && let Some(cancellation) = &worker.cancellation
            {
                cancellation.store(true, Ordering::Relaxed);
            }
            let _ = worker.answers.send(answer);
        }
        self.confirmation = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsaved_archive_actions_fit_in_dialog() {
        let context = egui::Context::default();
        let icons = IconCache::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(440.0, 170.0));
        let input = egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        };
        context
            .run_ui(input, |ui| {
                egui::Panel::bottom("unsaved-test-actions")
                    .default_size(ui.spacing().interact_size.y.max(16.0) + 24.0)
                    .frame(egui::Frame::new().inner_margin(3))
                    .show_separator_line(false)
                    .show(ui, |ui| {
                        ui.separator();
                        for button in unsaved_archive_actions(ui, &icons) {
                            assert!(
                                rect.contains_rect(button.rect),
                                "{:?} outside {rect:?}",
                                button.rect
                            );
                        }
                    });
            })
            .drop_without_applying_deltas();
    }

    #[test]
    fn confirmation_actions_fit_in_dialog() {
        let context = egui::Context::default();
        let icons = IconCache::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(500.0, 150.0));
        let input = egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        };
        context
            .run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    for (_, response) in confirmation_actions(ui, &icons) {
                        assert!(
                            rect.contains_rect(response.rect),
                            "{:?} outside {rect:?}",
                            response.rect
                        );
                    }
                });
            })
            .drop_without_applying_deltas();
    }

    #[test]
    fn confirmation_responses_reach_the_worker() {
        for decision in [
            Some(OverwriteDecision::Yes),
            Some(OverwriteDecision::No),
            Some(OverwriteDecision::Always),
            Some(OverwriteDecision::Never),
            None,
        ] {
            let (events, event_receiver) = mpsc::channel();
            let (answers, answer_receiver) = mpsc::channel();
            let cancellation = Arc::new(AtomicBool::new(false));
            let progress = UiProgress {
                events,
                answers: Arc::new(Mutex::new(answer_receiver)),
                cancellation: Some(Arc::clone(&cancellation)),
                limit_status_updates: false,
                last_status_update: Mutex::new(None),
            };
            assert!(progress.is_interactive());
            let worker = std::thread::spawn(move || {
                progress
                    .confirm_overwrite("Overwrite file? [y/N/Always/Never] ")
                    .map_err(|error| error.to_string())
            });
            let event = event_receiver.recv().unwrap();
            assert!(matches!(event, UiEvent::Confirm(ref prompt) if prompt == "Overwrite file?"));
            let mut app = ZargApp {
                worker: Some(WorkerChannels {
                    events: event_receiver,
                    answers,
                    cancellation: Some(Arc::clone(&cancellation)),
                }),
                confirmation: Some("Overwrite file?".to_owned()),
                ..Default::default()
            };
            app.answer_confirmation(decision);
            assert!(app.confirmation.is_none());
            assert_eq!(
                worker.join().unwrap(),
                decision.ok_or_else(|| "operation cancelled".to_owned())
            );
            assert_eq!(cancellation.load(Ordering::Relaxed), decision.is_none());
        }
    }
}
