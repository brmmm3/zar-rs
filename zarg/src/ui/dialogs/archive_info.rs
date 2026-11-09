use super::super::*;

impl ZargApp {
    pub(in crate::ui) fn show_archive_info(&mut self) {
        if self.imported_archive_dir.is_some() {
            return;
        }
        let archive = self.archive.clone();
        let working_archive = self.read_archive.clone();
        let (sender, receiver) = mpsc::channel();
        self.info_loading = Some(receiver);
        self.info_view = None;
        self.status = "Reading archive information...".to_owned();
        std::thread::spawn(move || {
            let result = (|| {
                let commits = operations::info_archive_with_blocks(&working_archive, true)
                    .map_err(|error| error.to_string())?;
                let metadata = operations::read_custom_metadata(&working_archive)
                    .map(|items| {
                        items
                            .iter()
                            .map(|(key, value)| {
                                (key.clone(), operations::format_metadata_value(value))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(ArchiveInfoView {
                    archive,
                    commits,
                    metadata,
                })
            })();
            let _ = sender.send(result);
        });
    }
    pub(in crate::ui) fn poll_archive_info(&mut self, context: &egui::Context) {
        let Some(receiver) = &self.info_loading else {
            return;
        };
        match receiver.try_recv() {
            Ok(result) => {
                self.info_loading = None;
                match result {
                    Ok(view) => {
                        self.status = format!("{} commit(s)", view.commits.len());
                        self.info_view = Some(view);
                    }
                    Err(error) => {
                        self.status = error.clone();
                        self.report = Some(error);
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => {
                context.request_repaint_after(std::time::Duration::from_millis(100));
            }
            Err(mpsc::TryRecvError::Disconnected) => self.info_loading = None,
        }
    }
    pub(in crate::ui) fn render_archive_info(&mut self, context: &egui::Context) {
        let Some(view) = &self.info_view else {
            return;
        };
        let size_key = "archive-information";
        let size = self.dialog_size(size_key, [600.0, 520.0]);
        let mut actual_size = None;
        let viewport_id = egui::ViewportId::from_hash_of("archive-information-dialog");
        let mut open = true;
        context.show_viewport_immediate(
            viewport_id,
            Self::centered_dialog_viewport_builder(context, "Archive information", size, true),
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
                egui::Panel::bottom("archive-information-actions")
                    .frame(egui::Frame::new().inner_margin(3))
                    .show_separator_line(false)
                    .show(ui, |ui| {
                        ui.separator();
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_button(ui, &self.icons, UiIcon::Cancel, "Close", true).clicked()
                            {
                                open = false;
                            }
                        });
                    });
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().inner_margin(3))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| render_info_contents(ui, view));
                    });
                if !open {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            },
        );
        self.record_dialog_size(size_key, actual_size);
        if !open {
            self.info_view = None;
        }
    }
}
pub(super) fn info_ratio(compressed: u64, uncompressed: u64) -> Option<f64> {
    (uncompressed > 0).then(|| compressed as f64 / uncompressed as f64)
}
pub(super) fn info_row(ui: &mut egui::Ui, label: &str, value: impl Into<egui::WidgetText>) {
    ui.label(egui::RichText::new(label).weak());
    ui.label(value);
    ui.end_row();
}
pub(super) fn info_ratio_bar(ui: &mut egui::Ui, ratio: Option<f64>) {
    match ratio {
        Some(ratio) => {
            ui.add(
                egui::ProgressBar::new(ratio.clamp(0.0, 1.0) as f32)
                    .desired_width(110.0)
                    .text(format!("{:.1}%", ratio * 100.0)),
            );
        }
        None => {
            ui.label("n/a");
        }
    }
}
pub(super) fn render_info_contents(ui: &mut egui::Ui, view: &ArchiveInfoView) {
    let stored: u64 = view.commits.iter().map(|commit| commit.commit.size).sum();
    let uncompressed: Option<u64> = view
        .commits
        .iter()
        .map(|commit| commit.commit.uncompressed_size)
        .sum();
    ui.heading("Summary");
    egui::Grid::new("info_summary")
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            info_row(ui, "Archive", view.archive.as_str());
            info_row(ui, "Commits", view.commits.len().to_string());
            info_row(ui, "Stored size", display_size(stored));
            if let Some(total) = uncompressed {
                info_row(ui, "Uncompressed size", display_size(total));
                ui.label(egui::RichText::new("Ratio").weak());
                info_ratio_bar(ui, info_ratio(stored, total));
                ui.end_row();
            }
        });
    ui.add_space(6.0);
    ui.heading("Custom metadata");
    if view.metadata.is_empty() {
        ui.weak("None");
    } else {
        egui::Grid::new("info_metadata")
            .num_columns(2)
            .spacing([16.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                for (key, value) in &view.metadata {
                    ui.label(egui::RichText::new(key).strong());
                    ui.label(value);
                    ui.end_row();
                }
            });
    }
    ui.add_space(6.0);
    ui.heading("Commits");
    let last = view.commits.len().saturating_sub(1);
    for (index, commit) in view.commits.iter().enumerate() {
        let id = format!("{:08x}", u32::from_le_bytes(commit.commit.id));
        egui::CollapsingHeader::new(format!(
            "Commit {id} - {}",
            display_size(commit.commit.size)
        ))
        .id_salt(("info_commit", index))
        .default_open(index == last)
        .show(ui, |ui| render_commit_info(ui, index, commit));
    }
}
pub(super) fn render_commit_info(ui: &mut egui::Ui, index: usize, info: &CommitInfoWithBlocks) {
    let commit = &info.commit;
    let scope = match (commit.header_encrypted, commit.data_encrypted) {
        (false, false) => "none",
        (false, true) => "data",
        (true, false) => "header",
        (true, true) => "header and data",
    };
    let algorithm = commit
        .encryption_algorithm
        .map(|algorithm| match algorithm {
            ContentAead::Aes256Gcm => "AES-256-GCM",
            ContentAead::ChaCha20Poly1305 => "ChaCha20-Poly1305",
        });
    egui::Grid::new(("info_commit_grid", index))
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            info_row(ui, "Stored size", display_size(commit.size));
            if let Some(created) = commit.created_unix_millis {
                info_row(ui, "Created", operations::format_unix_millis(created));
            }
            if let Some(user) = &commit.user {
                info_row(ui, "User", user);
            }
            info_row(ui, "Header", display_size(u64::from(commit.header_size)));
            info_row(ui, "Data", display_size(commit.data_size));
            info_row(
                ui,
                "Uncompressed",
                commit
                    .uncompressed_size
                    .map(display_size)
                    .unwrap_or_else(|| "unavailable (encrypted header)".to_owned()),
            );
            ui.label(egui::RichText::new("Ratio").weak());
            info_ratio_bar(ui, commit.ratio_percent.map(|percent| percent / 100.0));
            ui.end_row();
            if let Some(header) = &commit.header_compression {
                info_row(
                    ui,
                    "Header compression",
                    format!(
                        "{} -> {}{}",
                        header
                            .uncompressed_size
                            .map(display_size)
                            .unwrap_or_else(|| "?".to_owned()),
                        display_size(header.compressed_size),
                        header
                            .ratio_percent
                            .map(|ratio| format!(" ({ratio:.1}%)"))
                            .unwrap_or_default()
                    ),
                );
            }
            info_row(
                ui,
                "Encryption",
                match algorithm {
                    Some(algorithm) => format!("{scope} ({algorithm})"),
                    None => scope.to_owned(),
                },
            );
        });
    let Some(file_stats) = &commit.details else {
        if commit.header_encrypted {
            ui.weak("File statistics unavailable (encrypted header)");
        }
        render_block_info(ui, index, &info.blocks);
        return;
    };
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Header content changes").strong());
    egui::Grid::new(("info_header_changes", index))
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            let changes = &file_stats.header_changes;
            let rows = [
                ("Files added or updated", changes.files_added_or_updated),
                (
                    "Directories added or updated",
                    changes.directories_added_or_updated,
                ),
                (
                    "Symlinks added or updated",
                    changes.symlinks_added_or_updated,
                ),
                (
                    "Named pipes added or updated",
                    changes.named_pipes_added_or_updated,
                ),
                ("Devices added or updated", changes.devices_added_or_updated),
                ("File entries removed", changes.files_removed),
                ("Directories removed", changes.directories_removed),
            ];
            let mut has_changes = false;
            for (label, count) in rows {
                if count == 0 {
                    continue;
                }
                has_changes = true;
                info_row(ui, label, count.to_string());
            }
            if !has_changes {
                info_row(ui, "Content changes", "none");
            }
        });
    ui.add_space(4.0);
    ui.label(egui::RichText::new("File groups").strong());
    egui::Grid::new(("info_groups", index))
        .num_columns(5)
        .spacing([16.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            for header in ["Group", "Files", "Stored", "Uncompressed", "Ratio"] {
                ui.label(egui::RichText::new(header).weak());
            }
            ui.end_row();
            for name in [
                "empty files",
                "tiny files",
                "small files",
                "big files",
                "files (block size unknown)",
            ] {
                let Some(group) = file_stats.groups.get(name) else {
                    continue;
                };
                ui.label(name);
                ui.label(group.files.to_string());
                ui.label(display_size(group.compressed_size));
                ui.label(display_size(group.uncompressed_size));
                info_ratio_bar(
                    ui,
                    info_ratio(group.compressed_size, group.uncompressed_size),
                );
                ui.end_row();
            }
        });
    ui.add_space(4.0);
    egui::Grid::new(("info_extra", index))
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            if file_stats.deduplicated_files > 0 {
                info_row(
                    ui,
                    "Deduplicated files",
                    format!(
                        "{} (saved {})",
                        file_stats.deduplicated_files,
                        display_size(file_stats.deduplicated_uncompressed_size)
                    ),
                );
            }
            if file_stats.symlinks > 0 || file_stats.named_pipes > 0 || file_stats.devices > 0 {
                info_row(
                    ui,
                    "Special entries",
                    format!(
                        "{} symlinks, {} pipes, {} devices",
                        file_stats.symlinks, file_stats.named_pipes, file_stats.devices
                    ),
                );
            }
            let metadata = &file_stats.entry_metadata;
            let saved = [
                ("owner", metadata.owner),
                ("group", metadata.group),
                ("permissions", metadata.permissions),
                ("date", metadata.dates),
            ]
            .iter()
            .filter(|(_, count)| *count > 0)
            .map(|(name, count)| format!("{name} ({count})"))
            .collect::<Vec<_>>();
            info_row(
                ui,
                "Saved metadata",
                if saved.is_empty() {
                    "none".to_owned()
                } else {
                    saved.join(", ")
                },
            );
        });
    ui.add_space(4.0);
    render_block_info(ui, index, &info.blocks);
}

fn render_block_info(ui: &mut egui::Ui, index: usize, blocks: &[BlockInfo]) {
    egui::CollapsingHeader::new(format!("Blocks ({})", blocks.len()))
        .id_salt(("info_blocks", index))
        .show(ui, |ui| {
            if blocks.is_empty() {
                ui.weak("No data blocks.");
                return;
            }
            egui::Grid::new(("info_blocks_grid", index))
                .num_columns(5)
                .spacing([16.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    for header in [
                        "Block",
                        "Files",
                        "Compressed size",
                        "Compression ratio",
                        "File group",
                    ] {
                        ui.label(egui::RichText::new(header).weak());
                    }
                    ui.end_row();
                    for block in blocks {
                        ui.label(format!("{:08x}", block.index));
                        ui.label(block.file_count.to_string());
                        ui.label(display_size(block.compressed_size));
                        info_ratio_bar(ui, block.ratio_percent.map(|percent| percent / 100.0));
                        ui.label(if block.file_groups.is_empty() {
                            "unavailable".to_owned()
                        } else {
                            block.file_groups.join(", ")
                        });
                        ui.end_row();
                    }
                });
        });
}
