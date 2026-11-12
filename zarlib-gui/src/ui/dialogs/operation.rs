use super::super::*;

#[cfg(test)]
#[path = "operation_tests.rs"]
mod tests;

fn commit_indicator_icon(
    id: &str,
    encrypted_commit_ids: &BTreeSet<String>,
    passwords: &BTreeMap<[u8; 4], String>,
) -> Option<UiIcon> {
    if !encrypted_commit_ids.contains(id) {
        return None;
    }
    let unlocked = u32::from_str_radix(id, 16)
        .ok()
        .map(u32::to_le_bytes)
        .is_some_and(|commit_id| passwords.contains_key(&commit_id));
    Some(if unlocked {
        UiIcon::Unlock
    } else {
        UiIcon::Lock
    })
}

pub(in crate::ui) fn locked_commit_count(
    encrypted_commit_ids: &BTreeSet<String>,
    passwords: &BTreeMap<[u8; 4], String>,
) -> usize {
    encrypted_commit_ids
        .iter()
        .filter(|id| {
            u32::from_str_radix(id, 16)
                .ok()
                .map(u32::to_le_bytes)
                .is_none_or(|commit_id| !passwords.contains_key(&commit_id))
        })
        .count()
}

pub(in crate::ui) fn render_commit_indicator(
    ui: &mut egui::Ui,
    icons: &IconCache,
    id: &str,
    encrypted_commit_ids: &BTreeSet<String>,
    passwords: &BTreeMap<[u8; 4], String>,
) {
    if let Some(icon) = commit_indicator_icon(id, encrypted_commit_ids, passwords) {
        let texture = icon_texture(ui.ctx(), icons, icon);
        ui.add(egui::Image::new(&texture).fit_to_exact_size(egui::vec2(16.0, 16.0)));
    }
}

pub(in crate::ui) fn commit_selectable_option(
    ui: &mut egui::Ui,
    icons: &IconCache,
    id: &str,
    label: &str,
    selected: bool,
    encrypted_commit_ids: &BTreeSet<String>,
    passwords: &BTreeMap<[u8; 4], String>,
) -> egui::Response {
    match commit_indicator_icon(id, encrypted_commit_ids, passwords) {
        Some(icon) => {
            let texture = icon_texture(ui.ctx(), icons, icon);
            ui.add(
                egui::Button::image_and_text(
                    egui::Image::new(&texture).fit_to_exact_size(egui::vec2(16.0, 16.0)),
                    label,
                )
                .selected(selected)
                .frame_when_inactive(selected),
            )
        }
        None => ui.selectable_label(selected, label),
    }
}

pub(super) fn render_filter_patterns(
    ui: &mut egui::Ui,
    include_files: &mut String,
    exclude_files: &mut String,
    include_folders: &mut String,
    exclude_folders: &mut String,
) {
    ui.label("File and folder rules use fnmatch syntax; one pattern per line.");
    ui.columns(2, |columns| {
        columns[0].label("Include files");
        columns[1].label("Exclude files");
    });
    ui.columns(2, |columns| {
        columns[0].add(
            egui::TextEdit::multiline(include_files)
                .desired_rows(2)
                .hint_text("*.rs"),
        );
        columns[1].add(
            egui::TextEdit::multiline(exclude_files)
                .desired_rows(2)
                .hint_text("*.tmp"),
        );
    });
    ui.columns(2, |columns| {
        columns[0].label("Include folders");
        columns[1].label("Exclude folders");
    });
    ui.columns(2, |columns| {
        columns[0].add(
            egui::TextEdit::multiline(include_folders)
                .desired_rows(2)
                .hint_text("src/**"),
        );
        columns[1].add(
            egui::TextEdit::multiline(exclude_folders)
                .desired_rows(2)
                .hint_text("target/**"),
        );
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum OperationKind {
    NewArchive,
    AddToArchive,
    RemoveEntries,
    JoinCommits,
    RemoveCommits,
    UndoCommits,
    ChangeCommits,
    CustomMetadata,
    TestArchive,
    ListArchive,
    ExtractArchive,
}

impl OperationKind {
    pub(in crate::ui) fn title(self) -> &'static str {
        match self {
            Self::NewArchive => "New archive",
            Self::AddToArchive => "Add to archive",
            Self::RemoveEntries => "Remove entries",
            Self::JoinCommits => "Join commits",
            Self::RemoveCommits => "Remove commits",
            Self::UndoCommits => "Undo",
            Self::ChangeCommits => "Change commits",
            Self::CustomMetadata => "Custom metadata",
            Self::TestArchive => "Test archive",
            Self::ListArchive => "List archive",
            Self::ExtractArchive => "Extract",
        }
    }

    pub(super) fn dialog_size_key(self) -> &'static str {
        match self {
            Self::NewArchive => "operation-create",
            Self::AddToArchive => "operation-add",
            Self::RemoveEntries => "operation-remove",
            Self::JoinCommits => "operation-join",
            Self::RemoveCommits => "operation-remove-commits",
            Self::UndoCommits => "operation-undo",
            Self::ChangeCommits => "operation-change-commits",
            Self::CustomMetadata => "operation-metadata",
            Self::TestArchive => "operation-check",
            Self::ListArchive => "operation-list",
            Self::ExtractArchive => "operation-extract",
        }
    }

    pub(in crate::ui) fn needs_archive(self) -> bool {
        self != Self::NewArchive
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) enum NewArchiveTab {
    #[default]
    General,
    Advanced,
    Encryption,
    Filters,
}

impl NewArchiveTab {
    const ALL: [Self; 4] = [
        Self::General,
        Self::Advanced,
        Self::Encryption,
        Self::Filters,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Advanced => "Advanced",
            Self::Encryption => "Encryption",
            Self::Filters => "Filters",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) enum AddArchiveTab {
    #[default]
    Files,
    Compression,
    Advanced,
    Unlock,
    Encryption,
    Filters,
}

impl AddArchiveTab {
    const ALL: [Self; 6] = [
        Self::Files,
        Self::Compression,
        Self::Advanced,
        Self::Unlock,
        Self::Encryption,
        Self::Filters,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Files => "Files",
            Self::Compression => "Compression",
            Self::Advanced => "Advanced",
            Self::Unlock => "Unlock",
            Self::Encryption => "Encryption",
            Self::Filters => "Filters",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) enum ExtractTab {
    #[default]
    General,
    Advanced,
    Encryption,
    Filter,
}

impl ExtractTab {
    const ALL: [Self; 4] = [
        Self::General,
        Self::Advanced,
        Self::Encryption,
        Self::Filter,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Advanced => "Advanced",
            Self::Encryption => "Encryption",
            Self::Filter => "Filter",
        }
    }
}

impl ZargApp {
    pub(in crate::ui) fn render_operation_dialog(&mut self, context: &egui::Context) {
        let Some(kind) = self.operation.kind else {
            return;
        };
        if kind == OperationKind::RemoveEntries {
            return;
        }
        let viewport_id =
            egui::ViewportId::from_hash_of(format!("operation-dialog:{}", kind.title()));
        let mut open = true;
        let default_size = if kind == OperationKind::NewArchive {
            [500.0, 300.0]
        } else if kind == OperationKind::AddToArchive {
            [500.0, 360.0]
        } else if kind == OperationKind::TestArchive {
            [440.0, 100.0]
        } else if kind == OperationKind::CustomMetadata {
            [440.0, 190.0]
        } else if matches!(
            kind,
            OperationKind::JoinCommits | OperationKind::RemoveCommits
        ) {
            [440.0, 180.0]
        } else if kind == OperationKind::ExtractArchive {
            [500.0, 230.0]
        } else if kind == OperationKind::UndoCommits {
            [500.0, 100.0]
        } else if kind == OperationKind::ChangeCommits {
            [500.0, 440.0]
        } else {
            [500.0, 500.0]
        };
        let size_key = kind.dialog_size_key();
        let fixed_size = matches!(
            kind,
            OperationKind::TestArchive | OperationKind::UndoCommits
        );
        let saved_size = self.dialog_size(size_key, default_size);
        #[allow(clippy::if_same_then_else)]
        let size = if fixed_size {
            default_size
        } else if kind == OperationKind::JoinCommits && saved_size == [440.0, 130.0] {
            // Replace the previous default without discarding custom sizes.
            default_size
        } else if kind == OperationKind::CustomMetadata
            && matches!(saved_size, [440.0, 180.0] | [440.0, 210.0])
        {
            default_size
        } else if kind == OperationKind::ChangeCommits && saved_size == [560.0, 400.0] {
            default_size
        } else {
            saved_size
        };
        let mut actual_size = None;
        context.show_viewport_immediate(
            viewport_id,
            Self::centered_dialog_viewport_builder(
                context,
                viewport_id,
                kind.title(),
                size,
                !fixed_size,
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
                if kind == OperationKind::NewArchive {
                    egui::Panel::bottom("operation-actions")
                        .frame(egui::Frame::new().inner_margin(3))
                        .show_separator_line(false)
                        .show(ui, |ui| self.render_operation_actions(ui, kind));
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().inner_margin(8))
                        .show(ui, |ui| {
                            self.render_new_archive_tabs(ui);
                            ui.separator();
                            self.render_new_archive_scroll_area(ui);
                        });
                } else if kind == OperationKind::AddToArchive {
                    egui::Panel::bottom("operation-actions")
                        .frame(egui::Frame::new().inner_margin(3))
                        .show_separator_line(false)
                        .show(ui, |ui| self.render_operation_actions(ui, kind));
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().inner_margin(3))
                        .show(ui, |ui| {
                            self.render_add_archive_tabs(ui);
                            ui.separator();
                            if self.operation.add_archive_tab == AddArchiveTab::Files {
                                self.render_add_archive_files(ui);
                            } else {
                                egui::ScrollArea::vertical()
                                    .id_salt((
                                        "add-archive-form",
                                        self.operation.add_archive_tab.label(),
                                    ))
                                    .scroll_bar_rect(ui.max_rect())
                                    .show(ui, |ui| self.render_add_archive_tab_contents(ui));
                            }
                        });
                } else if kind == OperationKind::ExtractArchive {
                    egui::Panel::bottom("operation-actions")
                        .frame(egui::Frame::new().inner_margin(3))
                        .show_separator_line(false)
                        .show(ui, |ui| self.render_operation_actions(ui, kind));
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().inner_margin(3))
                        .show(ui, |ui| {
                            self.render_extract_tabs(ui);
                            ui.separator();
                            egui::ScrollArea::vertical()
                                .id_salt(("extract-form", self.operation.extract_tab.label()))
                                .scroll_bar_rect(ui.max_rect())
                                .show(ui, |ui| self.render_extract_tab_contents(ui));
                        });
                } else {
                    // Keep the action buttons visible while the remaining form scrolls.
                    egui::Panel::bottom("operation-actions")
                        .frame(egui::Frame::new().inner_margin(3))
                        .show_separator_line(false)
                        .show(ui, |ui| self.render_operation_actions(ui, kind));
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().inner_margin(3))
                        .show(ui, |ui| {
                            if fixed_size {
                                self.render_operation_fields(ui, kind);
                            } else {
                                egui::ScrollArea::vertical()
                                    .max_height(560.0)
                                    .scroll_bar_rect(ui.max_rect())
                                    .show(ui, |ui| {
                                        self.render_operation_fields(ui, kind);
                                    });
                            }
                        });
                }
                if !open || self.operation.kind.is_none() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            },
        );
        self.record_dialog_size(size_key, actual_size);
        if !open && self.operation.kind == Some(kind) {
            self.close_operation();
        }
    }
    pub(super) fn render_operation_fields(&mut self, ui: &mut egui::Ui, kind: OperationKind) {
        self.render_operation_header(ui, kind);
        match kind {
            OperationKind::NewArchive => {
                self.render_new_archive_tabs(ui);
                self.render_new_archive_tab_contents(ui);
            }
            OperationKind::AddToArchive => {
                self.render_add_archive_tabs(ui);
                if self.operation.add_archive_tab == AddArchiveTab::Files {
                    self.render_add_archive_files(ui);
                } else {
                    self.render_add_archive_tab_contents(ui);
                }
            }
            OperationKind::RemoveEntries => {}
            OperationKind::UndoCommits => {
                let maximum = self.maximum_undo_steps();
                let range_maximum = maximum.max(1);
                self.operation.undo_commits = self.operation.undo_commits.clamp(1, range_maximum);
                ui.horizontal(|ui| {
                    ui.label("Steps to undo:");
                    ui.add_enabled(
                        maximum > 0,
                        egui::DragValue::new(&mut self.operation.undo_commits)
                            .range(1..=range_maximum),
                    );
                });
                ui.label("Only unsaved changes from this session can be undone.");
            }
            OperationKind::RemoveCommits => {
                ui.label("Remove the selected inclusive range of commits. At least one commit must remain. Remaining history may change; encrypted headers cannot be validated.");
                Self::render_combo_row(ui, "First commit", |ui| {
                    render_commit_indicator(
                        ui,
                        &self.icons,
                        &self.operation.from_id,
                        &self.operation.encrypted_commit_ids,
                        &self.operation.archive_passwords,
                    );
                    egui::ComboBox::from_id_salt("remove-first-commit")
                        .selected_text(&self.operation.from_id)
                        .show_ui(ui, |ui| {
                            for id in &self.operation.commit_ids {
                                let response = commit_selectable_option(
                                    ui,
                                    &self.icons,
                                    id,
                                    id,
                                    self.operation.from_id == *id,
                                    &self.operation.encrypted_commit_ids,
                                    &self.operation.archive_passwords,
                                );
                                if response.clicked() {
                                    self.operation.from_id.clone_from(id);
                                }
                            }
                        });
                });
                let first = self
                    .operation
                    .commit_ids
                    .iter()
                    .position(|id| id == &self.operation.from_id)
                    .unwrap_or(0);
                if self
                    .operation
                    .commit_ids
                    .iter()
                    .position(|id| id == &self.operation.to_id)
                    .is_some_and(|last| last < first)
                {
                    self.operation.to_id = self.operation.from_id.clone();
                }
                Self::render_combo_row(ui, "Last commit", |ui| {
                    render_commit_indicator(
                        ui,
                        &self.icons,
                        &self.operation.to_id,
                        &self.operation.encrypted_commit_ids,
                        &self.operation.archive_passwords,
                    );
                    egui::ComboBox::from_id_salt("remove-last-commit")
                        .selected_text(&self.operation.to_id)
                        .show_ui(ui, |ui| {
                            for id in self.operation.commit_ids.iter().skip(first) {
                                let response = commit_selectable_option(
                                    ui,
                                    &self.icons,
                                    id,
                                    id,
                                    self.operation.to_id == *id,
                                    &self.operation.encrypted_commit_ids,
                                    &self.operation.archive_passwords,
                                );
                                if response.clicked() {
                                    self.operation.to_id.clone_from(id);
                                }
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Write to");
                    ui.allocate_ui_with_layout(
                        ui.available_size(),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if icon_button(ui, &self.icons, UiIcon::Browse, "Browse...", true)
                                .clicked()
                                && let Some(path) = rfd::FileDialog::new().save_file()
                            {
                                self.operation.remove_commits_output =
                                    path.to_string_lossy().into_owned();
                            }
                            ui.add(
                                egui::TextEdit::singleline(
                                    &mut self.operation.remove_commits_output,
                                )
                                .desired_width(ui.available_width())
                                .hint_text("Optional; empty stages changes in the current archive"),
                            );
                        },
                    );
                });
            }
            OperationKind::ChangeCommits => {
                ui.label("Choose an action and inclusive commit range.");
                Self::render_combo_row(ui, "Action:", |ui| {
                    egui::ComboBox::from_id_salt("change-commits-action")
                        .selected_text(
                            ["Encrypt", "Decrypt", "Change encryption"]
                                [self.operation.transform_action.min(2)],
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.operation.transform_action, 0, "Encrypt");
                            ui.selectable_value(&mut self.operation.transform_action, 1, "Decrypt");
                            ui.selectable_value(
                                &mut self.operation.transform_action,
                                2,
                                "Change encryption",
                            );
                        });
                });
                ui.separator();
                ui.label("Transform encryption for the selected inclusive commit range.");
                Self::render_combo_row(ui, "From commit:", |ui| {
                    render_commit_indicator(
                        ui,
                        &self.icons,
                        &self.operation.from_id,
                        &self.operation.encrypted_commit_ids,
                        &self.operation.archive_passwords,
                    );
                    egui::ComboBox::from_id_salt("change-commits-from")
                        .selected_text(&self.operation.from_id)
                        .show_ui(ui, |ui| {
                            for id in &self.operation.commit_ids {
                                let response = commit_selectable_option(
                                    ui,
                                    &self.icons,
                                    id,
                                    id,
                                    self.operation.from_id == *id,
                                    &self.operation.encrypted_commit_ids,
                                    &self.operation.archive_passwords,
                                );
                                if response.clicked() {
                                    self.operation.from_id.clone_from(id);
                                }
                            }
                        });
                });
                Self::render_combo_row(ui, "To commit:", |ui| {
                    render_commit_indicator(
                        ui,
                        &self.icons,
                        &self.operation.to_id,
                        &self.operation.encrypted_commit_ids,
                        &self.operation.archive_passwords,
                    );
                    egui::ComboBox::from_id_salt("change-commits-to")
                        .selected_text(&self.operation.to_id)
                        .show_ui(ui, |ui| {
                            let start = self
                                .operation
                                .commit_ids
                                .iter()
                                .position(|id| id == &self.operation.from_id)
                                .unwrap_or(0);
                            for id in self.operation.commit_ids.iter().skip(start) {
                                let response = commit_selectable_option(
                                    ui,
                                    &self.icons,
                                    id,
                                    id,
                                    self.operation.to_id == *id,
                                    &self.operation.encrypted_commit_ids,
                                    &self.operation.archive_passwords,
                                );
                                if response.clicked() {
                                    self.operation.to_id.clone_from(id);
                                }
                            }
                        });
                });
                ui.label(if self.operation.transform_action == 0 {
                    "Existing password (required only for encrypted commits in range):"
                } else {
                    "Current password (used to unlock selected encrypted commits):"
                });
                ui.add(
                    egui::TextEdit::singleline(&mut self.operation.existing_password)
                        .password(!self.operation.show_password)
                        .hint_text("Current password"),
                );
                if self.operation.transform_action != 1 {
                    ui.separator();
                    ui.label("New encryption:");
                    Self::render_combo_row(ui, "Encryption method:", |ui| {
                        egui::ComboBox::from_id_salt("change-commits-method")
                            .selected_text(
                                ["AES-256-GCM", "ChaCha20-Poly1305"][self.operation.aead.min(1)],
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.operation.aead, 0, "AES-256-GCM");
                                ui.selectable_value(
                                    &mut self.operation.aead,
                                    1,
                                    "ChaCha20-Poly1305",
                                );
                            });
                    });
                    Self::render_combo_row(ui, "Encryption scope:", |ui| {
                        egui::ComboBox::from_id_salt("change-commits-scope")
                            .selected_text(
                                ["Header and data", "Data only", "Header only"]
                                    [self.operation.encryption_scope.min(2)],
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.operation.encryption_scope,
                                    0,
                                    "Header and data",
                                );
                                ui.selectable_value(
                                    &mut self.operation.encryption_scope,
                                    1,
                                    "Data only",
                                );
                                ui.selectable_value(
                                    &mut self.operation.encryption_scope,
                                    2,
                                    "Header only",
                                );
                            });
                    });
                    ui.add(
                        egui::TextEdit::singleline(&mut self.operation.password)
                            .password(!self.operation.show_password)
                            .hint_text("New password"),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.operation.password_confirm)
                            .password(!self.operation.show_password)
                            .hint_text("Confirm new password"),
                    );
                    ui.checkbox(&mut self.operation.show_password, "Show passwords");
                    if !self.operation.password.is_empty() {
                        let missing = password_requirements(&self.operation.password);
                        if !missing.is_empty() {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                format!("For stronger security, consider: {}", missing.join(", ")),
                            );
                        }
                    }
                }
                ui.checkbox(
                    &mut self.operation.transform_strict,
                    "Fail if a selected encrypted commit lacks its password",
                );
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Write to");
                    ui.allocate_ui_with_layout(
                        ui.available_size(),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if icon_button(ui, &self.icons, UiIcon::Browse, "Browse...", true)
                                .clicked()
                                && let Some(path) = rfd::FileDialog::new().save_file()
                            {
                                self.operation.transform_output =
                                    path.to_string_lossy().into_owned();
                            }
                            ui.add(
                                egui::TextEdit::singleline(&mut self.operation.transform_output)
                                    .desired_width(ui.available_width())
                                    .hint_text("Optional; empty replaces the current archive"),
                            );
                        },
                    );
                });
            }
            OperationKind::JoinCommits => {
                Self::render_combo_row(ui, "From commit", |ui| {
                    render_commit_indicator(
                        ui,
                        &self.icons,
                        &self.operation.from_id,
                        &self.operation.encrypted_commit_ids,
                        &self.operation.archive_passwords,
                    );
                    egui::ComboBox::from_id_salt("join-from-commit")
                        .selected_text(&self.operation.from_id)
                        .show_ui(ui, |ui| {
                            for id in &self.operation.commit_ids {
                                let response = commit_selectable_option(
                                    ui,
                                    &self.icons,
                                    id,
                                    id,
                                    self.operation.from_id == *id,
                                    &self.operation.encrypted_commit_ids,
                                    &self.operation.archive_passwords,
                                );
                                if response.clicked() {
                                    self.operation.from_id.clone_from(id);
                                }
                            }
                        });
                });
                let min_to_position = self
                    .operation
                    .commit_ids
                    .iter()
                    .position(|id| id == &self.operation.from_id)
                    .unwrap_or(0);
                Self::render_combo_row(ui, "To commit", |ui| {
                    render_commit_indicator(
                        ui,
                        &self.icons,
                        &self.operation.to_id,
                        &self.operation.encrypted_commit_ids,
                        &self.operation.archive_passwords,
                    );
                    egui::ComboBox::from_id_salt("join-to-commit")
                        .selected_text(&self.operation.to_id)
                        .show_ui(ui, |ui| {
                            for id in self.operation.commit_ids.iter().skip(min_to_position) {
                                let response = commit_selectable_option(
                                    ui,
                                    &self.icons,
                                    id,
                                    id,
                                    self.operation.to_id == *id,
                                    &self.operation.encrypted_commit_ids,
                                    &self.operation.archive_passwords,
                                );
                                if response.clicked() {
                                    self.operation.to_id.clone_from(id);
                                }
                            }
                        });
                });
                if let (Some(from), Some(to)) = (
                    self.operation
                        .commit_ids
                        .iter()
                        .position(|id| id == &self.operation.from_id),
                    self.operation
                        .commit_ids
                        .iter()
                        .position(|id| id == &self.operation.to_id),
                ) && to < from
                {
                    self.operation.to_id = self.operation.from_id.clone();
                }
                Self::render_combo_row(ui, "Block size", |ui| {
                    egui::ComboBox::from_id_salt("join-block-size")
                        .selected_text(format!(
                            "{} MB",
                            self.operation.block_size.trim_end_matches(['M', 'm'])
                        ))
                        .show_ui(ui, |ui| {
                            for size in crate::settings::BLOCK_SIZE_OPTIONS {
                                ui.selectable_value(
                                    &mut self.operation.block_size,
                                    size.to_owned(),
                                    format!("{} MB", size.trim_end_matches('M')),
                                );
                            }
                        });
                });
                Self::render_combo_row(ui, "Compression level", |ui| {
                    egui::ComboBox::from_id_salt("join-compression-level")
                        .selected_text(&self.operation.compression_level)
                        .show_ui(ui, |ui| {
                            for level in -7..=22 {
                                let level = level.to_string();
                                ui.selectable_value(
                                    &mut self.operation.compression_level,
                                    level.clone(),
                                    level,
                                );
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Write to");
                    ui.allocate_ui_with_layout(
                        ui.available_size(),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if icon_button(ui, &self.icons, UiIcon::Browse, "Browse...", true)
                                .clicked()
                                && let Some(path) = rfd::FileDialog::new().save_file()
                            {
                                self.operation.join_output = path.to_string_lossy().into_owned();
                            }
                            ui.add(
                                egui::TextEdit::singleline(&mut self.operation.join_output)
                                    .desired_width(ui.available_width())
                                    .hint_text("Optional; empty replaces the current archive"),
                            );
                        },
                    );
                });
            }
            OperationKind::CustomMetadata => {
                ui.label("Existing entries (edit a value or press Remove):");
                if self.operation.meta_items.is_empty() {
                    ui.weak("No custom metadata.");
                }
                let total = ui.available_width();
                let gap = ui.spacing().item_spacing.x;
                let key_width = (total - gap) / 4.0;
                for (key, _, value, remove) in &mut self.operation.meta_items {
                    ui.horizontal(|ui| {
                        ui.add_sized([key_width, 20.0], egui::Label::new(key.as_str()).truncate());
                        let remove_width = 80.0;
                        let value_width = (total - key_width - remove_width - gap * 2.0).max(40.0);
                        ui.add_enabled(
                            !*remove,
                            egui::TextEdit::singleline(value).desired_width(value_width),
                        );
                        let label = if *remove { "Undo" } else { "Remove" };
                        if ui
                            .add_sized([remove_width, 20.0], egui::Button::new(label))
                            .clicked()
                        {
                            *remove = !*remove;
                        }
                    });
                }
                ui.separator();
                ui.label("New entries:");
                let total = ui.available_width();
                let gap = ui.spacing().item_spacing.x;
                let key_width = (total - gap) / 4.0;
                let remove_width = 80.0;
                let value_width = (total - key_width - remove_width - gap * 2.0).max(40.0);
                let mut delete_row = None;
                for (index, (key, value)) in self.operation.meta_new.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(key)
                                .hint_text("key")
                                .desired_width(key_width),
                        );
                        ui.add(
                            egui::TextEdit::singleline(value)
                                .hint_text("value")
                                .desired_width(value_width),
                        );
                        if ui
                            .add_sized([remove_width, 20.0], egui::Button::new("Remove"))
                            .clicked()
                        {
                            delete_row = Some(index);
                        }
                    });
                }
                if let Some(index) = delete_row {
                    self.operation.meta_new.remove(index);
                }
                if ui.button("Add another entry").clicked() {
                    self.operation.meta_new.push(Default::default());
                }
                ui.weak("Keys starting with \"zar.\" are reserved. Numbers are stored as numbers.");
            }
            OperationKind::TestArchive => {
                ui.checkbox(&mut self.operation.recover, "Recover past damaged framing");
                ui.checkbox(
                    &mut self.operation.password_is_set,
                    "Unlock encrypted archive",
                );
                if self.operation.password_is_set {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.operation.password)
                            .password(true)
                            .hint_text("Enter password"),
                    );
                }
            }
            OperationKind::ListArchive => {
                Self::render_combo_row(ui, "Last commit", |ui| {
                    render_commit_indicator(
                        ui,
                        &self.icons,
                        &self.operation.to,
                        &self.operation.encrypted_commit_ids,
                        &self.operation.archive_passwords,
                    );
                    egui::ComboBox::from_id_salt("list-last-commit")
                        .selected_text(selected_commit_label(
                            &self.operation.to,
                            &self.operation.commit_ids,
                        ))
                        .show_ui(ui, |ui| {
                            for (index, commit_id) in self.operation.commit_ids.iter().enumerate() {
                                let label = if index + 1 == self.operation.commit_ids.len() {
                                    format!("Last commit ({commit_id})")
                                } else {
                                    commit_id.clone()
                                };
                                let response = commit_selectable_option(
                                    ui,
                                    &self.icons,
                                    commit_id,
                                    &label,
                                    self.operation.to == *commit_id,
                                    &self.operation.encrypted_commit_ids,
                                    &self.operation.archive_passwords,
                                );
                                if response.clicked() {
                                    self.operation.to.clone_from(commit_id);
                                }
                            }
                        });
                });
                ui.checkbox(
                    &mut self.operation.password_is_set,
                    "Unlock encrypted headers",
                );
                if self.operation.password_is_set {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.operation.password)
                            .password(true)
                            .hint_text("Enter password"),
                    );
                }
            }
            OperationKind::ExtractArchive => {
                self.render_extract_tabs(ui);
                self.render_extract_tab_contents(ui);
            }
        }
    }

    fn render_extract_tabs(&mut self, ui: &mut egui::Ui) -> Vec<egui::Response> {
        ui.horizontal_wrapped(|ui| {
            ExtractTab::ALL
                .into_iter()
                .filter(|tab| {
                    *tab != ExtractTab::Encryption || self.operation.extract_has_encrypted_commits
                })
                .map(|tab| ui.selectable_value(&mut self.operation.extract_tab, tab, tab.label()))
                .collect()
        })
        .inner
    }

    fn render_extract_tab_contents(&mut self, ui: &mut egui::Ui) {
        match self.operation.extract_tab {
            ExtractTab::General => {
                ui.horizontal(|ui| {
                    ui.label("Destination:");
                    ui.allocate_ui_with_layout(
                        ui.available_size(),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if icon_button(ui, &self.icons, UiIcon::Browse, "Browse...", true)
                                .clicked()
                                && let Some(path) = rfd::FileDialog::new().pick_folder()
                            {
                                self.operation.destination = path.to_string_lossy().into_owned();
                            }
                            ui.add(
                                egui::TextEdit::singleline(&mut self.operation.destination)
                                    .desired_width(ui.available_width()),
                            );
                        },
                    );
                });
                Self::render_combo_row(ui, "Last commit", |ui| {
                    render_commit_indicator(
                        ui,
                        &self.icons,
                        &self.operation.to,
                        &self.operation.encrypted_commit_ids,
                        &self.operation.archive_passwords,
                    );
                    egui::ComboBox::from_id_salt("extract-last-commit")
                        .selected_text(selected_commit_label(
                            &self.operation.to,
                            &self.operation.commit_ids,
                        ))
                        .show_ui(ui, |ui| {
                            for (index, commit_id) in self.operation.commit_ids.iter().enumerate() {
                                let label = if index + 1 == self.operation.commit_ids.len() {
                                    format!("Last commit ({commit_id})")
                                } else {
                                    commit_id.clone()
                                };
                                let response = commit_selectable_option(
                                    ui,
                                    &self.icons,
                                    commit_id,
                                    &label,
                                    self.operation.to == *commit_id,
                                    &self.operation.encrypted_commit_ids,
                                    &self.operation.archive_passwords,
                                );
                                if response.clicked() {
                                    self.operation.to.clone_from(commit_id);
                                }
                            }
                        });
                });
                Self::render_combo_row(ui, "Case collision policy", |ui| {
                    egui::ComboBox::from_id_salt("extract-case-collision-policy")
                        .selected_text(
                            ["Error", "Latest", "First", "Rename", "Ask"][self.operation.collision],
                        )
                        .show_ui(ui, |ui| {
                            for (index, label) in ["Error", "Latest", "First", "Rename", "Ask"]
                                .into_iter()
                                .enumerate()
                            {
                                ui.selectable_value(&mut self.operation.collision, index, label);
                            }
                        });
                });
                Self::render_combo_row(ui, "Existing file policy", |ui| {
                    egui::ComboBox::from_id_salt("extract-existing-file-policy")
                        .selected_text(
                            ["Overwrite", "Skip", "Error", "Ask"][self.operation.existing],
                        )
                        .show_ui(ui, |ui| {
                            for (index, label) in ["Overwrite", "Skip", "Error", "Ask"]
                                .into_iter()
                                .enumerate()
                            {
                                ui.selectable_value(&mut self.operation.existing, index, label);
                            }
                        });
                });
            }
            ExtractTab::Advanced => {
                self.render_extract_advanced(ui);
            }
            ExtractTab::Encryption => {
                self.render_extract_encryption(ui);
            }
            ExtractTab::Filter => self.render_filter_fields_contents(ui),
        }
    }
    fn render_extract_advanced(&mut self, ui: &mut egui::Ui) -> [egui::Response; 5] {
        [
            ui.checkbox(&mut self.operation.dry_run, "Dry run"),
            ui.checkbox(&mut self.operation.metadata, "Restore metadata"),
            ui.checkbox(
                &mut self.operation.restore_symlinks,
                "Restore symbolic links",
            ),
            ui.checkbox(&mut self.operation.restore_devices, "Restore device nodes"),
            ui.checkbox(
                &mut self.operation.strip_top_level,
                "Skip top-level folder (extract its contents directly)",
            ),
        ]
    }

    fn render_extract_encryption(
        &mut self,
        ui: &mut egui::Ui,
    ) -> (egui::Response, Option<egui::Response>) {
        let unlock = ui.checkbox(
            &mut self.operation.password_is_set,
            "Unlock encrypted archive",
        );
        let password = self.operation.password_is_set.then(|| {
            ui.add(
                egui::TextEdit::singleline(&mut self.operation.password)
                    .password(true)
                    .hint_text("Enter password"),
            )
        });
        (unlock, password)
    }

    pub(super) fn render_operation_header(&mut self, ui: &mut egui::Ui, kind: OperationKind) {
        if kind.needs_archive()
            && !matches!(
                kind,
                OperationKind::AddToArchive | OperationKind::ExtractArchive
            )
            && !matches!(
                kind,
                OperationKind::CustomMetadata
                    | OperationKind::TestArchive
                    | OperationKind::JoinCommits
                    | OperationKind::RemoveCommits
                    | OperationKind::UndoCommits
                    | OperationKind::ChangeCommits
            )
        {
            ui.horizontal(|ui| {
                ui.label("Archive:");
                if matches!(
                    kind,
                    OperationKind::NewArchive
                        | OperationKind::AddToArchive
                        | OperationKind::TestArchive
                        | OperationKind::ExtractArchive
                        | OperationKind::JoinCommits
                        | OperationKind::UndoCommits
                        | OperationKind::ChangeCommits
                ) {
                    ui.allocate_ui_with_layout(
                        ui.available_size(),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if icon_button(ui, &self.icons, UiIcon::Browse, "Browse...", true)
                                .clicked()
                            {
                                self.choose_archive(kind == OperationKind::NewArchive);
                            }
                            ui.add(
                                egui::TextEdit::singleline(&mut self.operation.archive)
                                    .desired_width(ui.available_width()),
                            );
                        },
                    );
                } else {
                    ui.text_edit_singleline(&mut self.operation.archive);
                    if icon_button(ui, &self.icons, UiIcon::Browse, "Browse...", true).clicked() {
                        self.choose_archive(kind == OperationKind::NewArchive);
                    }
                }
            });
        }
    }

    fn render_add_archive_file_buttons(&mut self, ui: &mut egui::Ui) -> [egui::Response; 3] {
        ui.horizontal_wrapped(|ui| {
            let add = icon_button(
                ui,
                &self.icons,
                UiIcon::AddFolder,
                "Add folders/files",
                true,
            );
            if add.clicked() {
                self.choose_input_paths();
            }
            let remove = icon_button(
                ui,
                &self.icons,
                UiIcon::Remove,
                "Remove items",
                !self.operation.selected_input_paths.is_empty(),
            );
            if remove.clicked() {
                self.operation
                    .input_paths
                    .retain(|path| !self.operation.selected_input_paths.contains(path));
                self.operation.selected_input_paths.clear();
                self.operation.input_selection_anchor = None;
            }
            let clear = icon_button(
                ui,
                &self.icons,
                UiIcon::ClearInputs,
                "Clear list",
                !self.operation.input_paths.is_empty(),
            );
            if clear.clicked() {
                self.operation.input_paths.clear();
                self.operation.selected_input_paths.clear();
                self.operation.input_selection_anchor = None;
            }
            [add, remove, clear]
        })
        .inner
    }

    fn render_add_archive_files(&mut self, ui: &mut egui::Ui) -> (egui::Rect, [egui::Response; 3]) {
        let buttons = egui::Panel::bottom("add-archive-file-buttons")
            .frame(egui::Frame::new())
            .show_separator_line(false)
            .show(ui, |ui| self.render_add_archive_file_buttons(ui))
            .inner;
        let list = egui::CentralPanel::default()
            .frame(egui::Frame::new())
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("add-archive-input-paths")
                    .auto_shrink([false, false])
                    .scroll_bar_rect(ui.max_rect())
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        self.render_operation_input_items(ui);
                    })
                    .inner_rect
            })
            .inner;
        (list, buttons)
    }
    fn render_operation_input_items(&mut self, ui: &mut egui::Ui) {
        for path in &self.operation.input_paths {
            let path_text = path.display().to_string();
            let icon_kind = if path.is_dir() {
                UiIcon::Folder
            } else {
                file_icon(&path_text)
            };
            let icon = icon_texture(ui.ctx(), &self.icons, icon_kind);
            ui.horizontal(|ui| {
                let response =
                    ui.add(egui::Image::new(&icon).fit_to_exact_size(egui::vec2(16.0, 16.0)));
                let selected = self.operation.selected_input_paths.contains(path);
                let response = response.union(ui.selectable_label(selected, path_text));
                if response.clicked() {
                    let modifiers = ui.input(|input| input.modifiers);
                    let selection = extended_selection(
                        &self.operation.input_paths,
                        &self.operation.selected_input_paths,
                        &self.operation.input_selection_anchor,
                        path,
                        modifiers,
                    );
                    self.operation.selected_input_paths = selection.selected;
                    self.operation.input_selection_anchor = selection.anchor;
                }
            });
        }
    }
    fn render_new_archive_tabs(&mut self, ui: &mut egui::Ui) -> [egui::Response; 4] {
        ui.horizontal_wrapped(|ui| {
            NewArchiveTab::ALL.map(|tab| {
                ui.selectable_value(&mut self.operation.new_archive_tab, tab, tab.label())
            })
        })
        .inner
    }

    fn render_new_archive_scroll_area(
        &mut self,
        ui: &mut egui::Ui,
    ) -> egui::scroll_area::ScrollAreaOutput<()> {
        egui::ScrollArea::vertical()
            .id_salt(("new-archive-form", self.operation.new_archive_tab.label()))
            .auto_shrink([false, true])
            .scroll_bar_rect(ui.max_rect())
            .show(ui, |ui| self.render_new_archive_tab_contents(ui))
    }

    fn render_new_archive_tab_contents(&mut self, ui: &mut egui::Ui) {
        match self.operation.new_archive_tab {
            NewArchiveTab::General => {
                if !self.operation.input_paths.is_empty() {
                    self.render_operation_input_items(ui);
                }
                self.render_compression_options(ui);
            }
            NewArchiveTab::Advanced => {
                self.render_archive_checkboxes(ui, OperationKind::NewArchive);
            }
            NewArchiveTab::Encryption => {
                self.render_encryption_options(ui);
            }
            NewArchiveTab::Filters => self.render_filter_fields(ui),
        }
    }

    fn render_add_archive_tabs(&mut self, ui: &mut egui::Ui) -> Vec<egui::Response> {
        ui.horizontal_wrapped(|ui| {
            AddArchiveTab::ALL
                .into_iter()
                .filter(|tab| {
                    *tab != AddArchiveTab::Unlock || !self.operation.commit_unlocks.is_empty()
                })
                .map(|tab| {
                    ui.selectable_value(&mut self.operation.add_archive_tab, tab, tab.label())
                })
                .collect()
        })
        .inner
    }

    fn render_add_archive_unlock(
        &mut self,
        ui: &mut egui::Ui,
    ) -> Vec<(egui::Response, egui::Response, egui::Response)> {
        self.operation
            .commit_unlocks
            .iter_mut()
            .map(|commit| {
                ui.horizontal(|ui| {
                    let checkbox = ui.checkbox(&mut commit.selected, "Unlock");
                    let id = egui::Frame::group(ui.style())
                        .show(ui, |ui| {
                            ui.label(format!("{:08x}", u32::from_le_bytes(commit.id)))
                        })
                        .inner;
                    let password = ui.add_enabled(
                        commit.selected,
                        egui::TextEdit::singleline(&mut commit.password)
                            .password(!self.operation.show_password)
                            .hint_text("Password"),
                    );
                    (checkbox, id, password)
                })
                .inner
            })
            .collect()
    }

    fn render_add_archive_tab_contents(&mut self, ui: &mut egui::Ui) {
        match self.operation.add_archive_tab {
            AddArchiveTab::Files => {
                self.render_add_archive_files(ui);
            }
            AddArchiveTab::Compression => {
                self.render_compression_options(ui);
            }
            AddArchiveTab::Advanced => {
                self.render_archive_checkboxes(ui, OperationKind::AddToArchive);
            }
            AddArchiveTab::Unlock => {
                self.render_add_archive_unlock(ui);
            }
            AddArchiveTab::Encryption => {
                self.render_encryption_options(ui);
            }
            AddArchiveTab::Filters => {
                self.render_filter_fields_contents(ui);
            }
        }
    }

    pub(super) fn render_combo_row(
        ui: &mut egui::Ui,
        label: &str,
        add_combo: impl FnOnce(&mut egui::Ui),
    ) {
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                ui.available_size(),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(label);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add_combo);
                },
            );
        });
    }

    fn render_compression_options(&mut self, ui: &mut egui::Ui) {
        Self::render_combo_row(
            ui,
            "Compression level -7 (fastest) to 22 (smallest)",
            |ui| {
                egui::ComboBox::from_id_salt("compression-level")
                    .selected_text(&self.operation.compression_level)
                    .show_ui(ui, |ui| {
                        for level in -7..=22 {
                            let level = level.to_string();
                            ui.selectable_value(
                                &mut self.operation.compression_level,
                                level.clone(),
                                level,
                            );
                        }
                    });
            },
        );
        Self::render_combo_row(ui, "Block size", |ui| {
            egui::ComboBox::from_id_salt("block-size")
                .selected_text(format!(
                    "{} MB",
                    self.operation.block_size.trim_end_matches(['M', 'm'])
                ))
                .show_ui(ui, |ui| {
                    for size in crate::settings::BLOCK_SIZE_OPTIONS {
                        ui.selectable_value(
                            &mut self.operation.block_size,
                            size.to_owned(),
                            format!("{} MB", size.trim_end_matches('M')),
                        );
                    }
                });
        });
    }

    fn render_archive_checkboxes(&mut self, ui: &mut egui::Ui, kind: OperationKind) {
        ui.checkbox(
            &mut self.operation.metadata,
            "Preserve platform-specific metadata",
        );
        ui.checkbox(&mut self.operation.dates, "Save modification times");
        ui.checkbox(&mut self.operation.creation_time, "Save creation times");
        ui.checkbox(&mut self.operation.access_time, "Save access times");
        ui.checkbox(
            &mut self.operation.special_files,
            "Include named pipes and device nodes",
        );
        ui.checkbox(&mut self.operation.strict, "Strict input validation");
        ui.checkbox(&mut self.operation.verbose, "Show symbolic-link warnings");
        if kind == OperationKind::NewArchive {
            ui.checkbox(&mut self.operation.save_user, "Save current OS username");
        }
    }

    fn render_encryption_options(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(
            &mut self.operation.password_is_set,
            "Encrypt this archive / new commit",
        );
        let show_options = self.operation.password_is_set
            || self.operation.kind == Some(OperationKind::NewArchive);
        if show_options {
            let enabled = self.operation.password_is_set;
            ui.add_enabled_ui(enabled, |ui| {
                Self::render_combo_row(ui, "Encryption algorithm", |ui| {
                    egui::ComboBox::from_id_salt("encryption-algorithm")
                        .selected_text(
                            ["AES-256-GCM", "ChaCha20-Poly1305"][self.operation.aead.min(1)],
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.operation.aead, 0, "AES-256-GCM");
                            ui.selectable_value(&mut self.operation.aead, 1, "ChaCha20-Poly1305");
                        });
                });
                Self::render_combo_row(ui, "Encryption scope", |ui| {
                    egui::ComboBox::from_id_salt("encryption-scope")
                        .selected_text(
                            ["Header and data", "Data only", "Header only"]
                                [self.operation.encryption_scope.min(2)],
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.operation.encryption_scope,
                                0,
                                "Header and data",
                            );
                            ui.selectable_value(
                                &mut self.operation.encryption_scope,
                                1,
                                "Data only",
                            );
                            ui.selectable_value(
                                &mut self.operation.encryption_scope,
                                2,
                                "Header only",
                            );
                        });
                });
                ui.add(
                    egui::TextEdit::singleline(&mut self.operation.password)
                        .password(!self.operation.show_password)
                        .hint_text("Enter password"),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.operation.password_confirm)
                        .password(!self.operation.show_password)
                        .hint_text("Re-enter password"),
                );
                ui.checkbox(&mut self.operation.show_password, "Show password");
                if enabled {
                    let missing = password_requirements(&self.operation.password);
                    if missing.is_empty() {
                        ui.colored_label(
                            egui::Color32::GREEN,
                            "Password meets the recommended strength.",
                        );
                    } else {
                        ui.label("For stronger security, consider a password with:");
                        for requirement in missing {
                            ui.colored_label(egui::Color32::YELLOW, format!("• {requirement}"));
                        }
                    }
                    if !self.operation.password_confirm.is_empty()
                        && self.operation.password != self.operation.password_confirm
                    {
                        ui.colored_label(egui::Color32::RED, "Passwords do not match.");
                    }
                }
            });
        }
    }
    pub(in crate::ui) fn render_operation_actions(
        &mut self,
        ui: &mut egui::Ui,
        kind: OperationKind,
    ) -> Option<egui::Response> {
        ui.separator();
        if kind == OperationKind::NewArchive
            && self.operation.new_archive_tab != NewArchiveTab::Encryption
        {
            if self.operation.password != self.operation.password_confirm {
                ui.colored_label(
                    egui::Color32::RED,
                    "Passwords do not match (Encryption tab).",
                );
            } else if self.operation.password_is_set && self.operation.password.is_empty() {
                ui.colored_label(
                    egui::Color32::RED,
                    "Enter a password on the Encryption tab.",
                );
            }
        }
        if kind == OperationKind::AddToArchive {
            if self.operation.add_archive_tab != AddArchiveTab::Encryption
                && self.operation.password != self.operation.password_confirm
            {
                ui.colored_label(
                    egui::Color32::RED,
                    "Passwords do not match (Encryption tab).",
                );
            } else if self.operation.add_archive_tab != AddArchiveTab::Encryption
                && self.operation.password_is_set
                && self.operation.password.is_empty()
            {
                ui.colored_label(
                    egui::Color32::RED,
                    "Enter a password on the Encryption tab.",
                );
            }
            if self.operation.add_archive_tab != AddArchiveTab::Unlock
                && self
                    .operation
                    .commit_unlocks
                    .iter()
                    .any(|commit| commit.selected && commit.password.is_empty())
            {
                ui.colored_label(
                    egui::Color32::RED,
                    "Enter a password for every selected commit on the Unlock tab.",
                );
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (icon, label) = match kind {
                OperationKind::NewArchive => (UiIcon::NewArchive, "New"),
                OperationKind::AddToArchive => (UiIcon::Add, "Add"),
                OperationKind::RemoveEntries => return None,
                OperationKind::JoinCommits => (UiIcon::Join, "Join"),
                OperationKind::RemoveCommits => (UiIcon::Remove, "Remove"),
                OperationKind::UndoCommits => (UiIcon::Undo, "Undo"),
                OperationKind::ChangeCommits => (UiIcon::TestArchive, "Apply"),
                OperationKind::CustomMetadata => (UiIcon::ConfirmApply, "Apply"),
                OperationKind::TestArchive => (UiIcon::TestArchive, "Test"),
                OperationKind::ListArchive => (UiIcon::ListContents, "List"),
                OperationKind::ExtractArchive => (UiIcon::Extract, "Extract"),
            };
            let passwords_match = self.operation.password == self.operation.password_confirm;
            let encrypted_password_is_valid =
                !self.operation.password_is_set || !self.operation.password.is_empty();
            let can_start = match kind {
                OperationKind::NewArchive => passwords_match && encrypted_password_is_valid,
                OperationKind::AddToArchive => {
                    !self.operation.input_paths.is_empty()
                        && passwords_match
                        && encrypted_password_is_valid
                        && self
                            .operation
                            .commit_unlocks
                            .iter()
                            .all(|commit| !commit.selected || !commit.password.is_empty())
                }
                OperationKind::ExtractArchive => {
                    PathBuf::from(self.operation.archive.trim()).is_file()
                        && valid_destination_path(&self.operation.destination)
                }
                OperationKind::TestArchive => {
                    PathBuf::from(self.operation.archive.trim()).is_file()
                }
                OperationKind::CustomMetadata => {
                    PathBuf::from(self.operation.archive.trim()).is_file()
                        && self.operation.has_metadata_changes()
                }
                OperationKind::UndoCommits => {
                    self.has_current_archive()
                        && self.has_unsaved_changes
                        && self.operation.undo_commits > 0
                        && self.operation.undo_commits <= self.maximum_undo_steps()
                }
                OperationKind::RemoveCommits => removal_range(
                    &self.operation.commit_ids,
                    &self.operation.from_id,
                    &self.operation.to_id,
                )
                .is_some(),
                OperationKind::ChangeCommits => {
                    PathBuf::from(self.operation.archive.trim()).is_file()
                        && commit_range_is_valid(
                            &self.operation.commit_ids,
                            &self.operation.from_id,
                            &self.operation.to_id,
                        )
                        && (self.operation.transform_action == 0
                            || (self.operation.transform_action == 1
                                && !self.operation.transform_strict)
                            || !self.operation.existing_password.trim().is_empty())
                        && (self.operation.transform_action == 1
                            || (!self.operation.password.is_empty()
                                && self.operation.password == self.operation.password_confirm))
                }
                OperationKind::JoinCommits => join_range_is_valid(
                    &self.operation.commit_ids,
                    &self.operation.from_id,
                    &self.operation.to_id,
                ),
                _ => true,
            };
            if icon_button(ui, &self.icons, UiIcon::Cancel, "Cancel", true).clicked() {
                self.close_operation();
            }
            let response = icon_button(ui, &self.icons, icon, label, can_start);
            if response.clicked() {
                let form = std::mem::take(&mut self.operation);
                self.start_operation(form);
            }
            Some(response)
        })
        .inner
    }
    pub(super) fn render_filter_fields(&mut self, ui: &mut egui::Ui) {
        if self.operation.kind == Some(OperationKind::NewArchive) {
            self.render_filter_fields_contents(ui);
        } else {
            ui.collapsing("Include / exclude filters", |ui| {
                self.render_filter_fields_contents(ui);
            });
        }
    }

    fn render_filter_fields_contents(&mut self, ui: &mut egui::Ui) {
        render_filter_patterns(
            ui,
            &mut self.operation.include_files,
            &mut self.operation.exclude_files,
            &mut self.operation.include_folders,
            &mut self.operation.exclude_folders,
        );
        if self.operation.kind != Some(OperationKind::NewArchive) {
            ui.horizontal(|ui| {
                if !matches!(
                    self.operation.kind,
                    Some(OperationKind::AddToArchive | OperationKind::ExtractArchive)
                ) && icon_button(
                    ui,
                    &self.icons,
                    UiIcon::AddFilterList,
                    "Add filter list...",
                    true,
                )
                .clicked()
                    && let Some(paths) = rfd::FileDialog::new()
                        .add_filter("Text file", &["txt", "list"])
                        .pick_files()
                {
                    self.operation.filelists.extend(paths);
                }
                for path in &self.operation.filelists {
                    ui.label(path.file_name().unwrap_or_default().to_string_lossy());
                }
            });
        }
    }
}
