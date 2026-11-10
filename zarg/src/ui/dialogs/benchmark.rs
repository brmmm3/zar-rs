use super::archive_info::{info_ratio, info_ratio_bar, info_row};

use super::super::*;

pub(in crate::ui) struct BenchmarkSettings {
    block_size: String,
    compression_level: i32,
    owner_permissions: bool,
    dates: bool,
    special_files: bool,
}
impl Default for BenchmarkSettings {
    fn default() -> Self {
        Self {
            block_size: "8M".to_owned(),
            compression_level: 3,
            owner_permissions: false,
            dates: false,
            special_files: true,
        }
    }
}
#[derive(Clone)]
pub(in crate::ui) struct BenchmarkResult {
    pub(in crate::ui) files: u64,
    pub(in crate::ui) uncompressed: u64,
    pub(in crate::ui) archive_size: u64,
    pub(in crate::ui) create_seconds: f64,
    pub(in crate::ui) extract_seconds: f64,
}

#[derive(Default)]
pub(in crate::ui) struct BenchmarkState {
    pub(in crate::ui) open: bool,
    settings: BenchmarkSettings,
    receiver: Option<Receiver<Result<BenchmarkResult, String>>>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    result: Option<Result<BenchmarkResult, String>>,
}
/// Removes the benchmark's scratch folder when dropped.
struct ScratchDir(PathBuf);

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
pub(super) fn directory_stats(path: &std::path::Path) -> (u64, u64) {
    let mut files = 0;
    let mut bytes = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                files += 1;
                bytes += meta.len();
            }
        }
    }
    (files, bytes)
}
pub(in crate::ui) fn run_benchmark(
    archive: &str,
    password: Option<Vec<u8>>,
    settings: &BenchmarkSettings,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<BenchmarkResult, String> {
    let check_cancel = || {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            Err("Benchmark cancelled.".to_owned())
        } else {
            Ok(())
        }
    };
    let error = |error: Box<dyn std::error::Error>| error.to_string();
    let encoding = EncodingOptions {
        block_size: parse_block_size(&settings.block_size).map_err(error)?,
        compression_level: settings.compression_level,
    };
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_nanos());
    let scratch =
        ScratchDir(std::env::temp_dir().join(format!("zarg-bench-{}-{nanos}", std::process::id())));
    let source = scratch.0.join("source");
    let restored = scratch.0.join("restored");
    std::fs::create_dir_all(&source).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&restored).map_err(|e| e.to_string())?;
    let extraction = |restore_metadata| ExtractionOptions {
        dry_run: false,
        to: None,
        collision_policy: CollisionPolicy::Rename,
        existing_policy: ExistingPolicy::Overwrite,
        restore_metadata,
        restore_symlinks: settings.special_files,
        restore_devices: false,
        strip_top_level: false,
    };
    operations::extract_archive_with_progress(
        archive,
        &source,
        extraction(false),
        password.as_deref(),
        None,
    )
    .map_err(error)?;
    check_cancel()?;
    let (files, uncompressed) = directory_stats(&source);
    let inputs: Vec<PathBuf> = std::fs::read_dir(&source)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|entry| entry.path())
        .collect();
    if inputs.is_empty() {
        return Err("The archive has no entries to benchmark.".to_owned());
    }
    let benchmark_archive = scratch.0.join("bench.zar");
    let benchmark_archive = benchmark_archive.to_string_lossy().into_owned();

    check_cancel()?;
    let started = std::time::Instant::now();
    operations::create_archive_with_password_progress_and_filters(
        &benchmark_archive,
        &inputs,
        false,
        encoding,
        None,
        None,
        CreateFilterOptions {
            input: CreateInputOptions {
                save_metadata: settings.owner_permissions,
                save_dates: settings.dates,
                save_user: false,
                include_special_files: settings.special_files,
                overwrite_existing: false,
                verbose: false,
            },
            filters: &FilterSet::default(),
        },
    )
    .map_err(error)?;
    let create_seconds = started.elapsed().as_secs_f64();
    let archive_size = std::fs::metadata(&benchmark_archive)
        .map_err(|e| e.to_string())?
        .len();

    check_cancel()?;
    let started = std::time::Instant::now();
    operations::extract_archive_with_progress(
        &benchmark_archive,
        &restored,
        extraction(settings.owner_permissions || settings.dates),
        None,
        None,
    )
    .map_err(error)?;
    let extract_seconds = started.elapsed().as_secs_f64();
    check_cancel()?;
    Ok(BenchmarkResult {
        files,
        uncompressed,
        archive_size,
        create_seconds,
        extract_seconds,
    })
}
pub(super) fn throughput(bytes: u64, seconds: f64) -> String {
    if seconds <= 0.0 {
        return "n/a".to_owned();
    }
    format!("{}/s", display_size((bytes as f64 / seconds) as u64))
}
impl ZargApp {
    pub(in crate::ui) fn poll_benchmark(&mut self, context: &egui::Context) {
        let Some(receiver) = &self.benchmark.receiver else {
            return;
        };
        match receiver.try_recv() {
            Ok(result) => {
                self.benchmark.receiver = None;
                self.benchmark.result = Some(result);
            }
            Err(mpsc::TryRecvError::Empty) => {
                context.request_repaint_after(std::time::Duration::from_millis(100));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.benchmark.receiver = None;
                self.benchmark.result = Some(Err("Benchmark worker stopped unexpectedly.".into()));
            }
        }
    }
    pub(in crate::ui) fn render_benchmark(&mut self, context: &egui::Context) {
        if !self.benchmark.open {
            return;
        }
        let running = self.benchmark.receiver.is_some();
        let cancelling = running
            && self
                .benchmark
                .cancel
                .load(std::sync::atomic::Ordering::Relaxed);
        let mut open = true;
        let mut start = false;
        let mut cancel = false;
        let size_key = "benchmark";
        let size = self.dialog_size(size_key, [560.0, 240.0]);
        let mut actual_size = None;
        let state = &mut self.benchmark;
        let viewport_id = egui::ViewportId::from_hash_of("benchmark-dialog");
        context.show_viewport_immediate(
            viewport_id,
            Self::centered_dialog_viewport_builder(context, viewport_id, "Benchmark", size, true),
            |ui, _class| {
                ui.painter().rect_filled(
                    ui.max_rect(),
                    0.0,
                    ui.visuals().window_fill().to_opaque(),
                );
                actual_size = Self::viewport_inner_size(ui.ctx());
                if ui.ctx().input(|input| input.viewport().close_requested()) {
                    if running {
                        ui.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    } else {
                        open = false;
                    }
                }
                egui::Panel::bottom("benchmark-actions")
                    .frame(egui::Frame::new().inner_margin(3))
                    .show_separator_line(false)
                    .show(ui, |ui| {
                        ui.separator();
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_button(
                                ui,
                                &self.icons,
                                UiIcon::Cancel,
                                "Close",
                                !running,
                            )
                            .clicked()
                            {
                                open = false;
                            }
                            if icon_button(
                                ui,
                                &self.icons,
                                UiIcon::Cancel,
                                "Cancel",
                                running && !cancelling,
                            ).clicked() {
                                cancel = true;
                            }
                            if icon_button(ui, &self.icons, UiIcon::Confirm, "Start", !running)
                                .clicked()
                            {
                                start = true;
                            }
                        });
                    });
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().inner_margin(3))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.weak(
                                    "Unpacks the archive to a temporary folder, then times creating and \
                                     extracting a new archive with these settings. Nothing else is written.",
                                );
                                ui.add_enabled_ui(!running, |ui| {
                                    Self::render_combo_row(ui, "Block size", |ui| {
                                        egui::ComboBox::from_id_salt("bench_block")
                                            .selected_text(&state.settings.block_size)
                                            .show_ui(ui, |ui| {
                                                for size in [
                                                    "64K", "256K", "1M", "2M", "4M", "8M", "16M",
                                                    "32M",
                                                ] {
                                                    ui.selectable_value(
                                                        &mut state.settings.block_size,
                                                        size.to_owned(),
                                                        size,
                                                    );
                                                }
                                            });
                                    });
                                    Self::render_combo_row(ui, "Compression level", |ui| {
                                        ui.add(egui::Slider::new(
                                            &mut state.settings.compression_level,
                                            -7..=22,
                                        ));
                                    });
                                    ui.checkbox(
                                        &mut state.settings.owner_permissions,
                                        "Save owner and permissions",
                                    );
                                    ui.checkbox(&mut state.settings.dates, "Save modification dates");
                                    ui.checkbox(
                                        &mut state.settings.special_files,
                                        "Include symbolic links and special files",
                                    );
                                });
                                ui.separator();
                                match &state.result {
                                    _ if running => {
                                        ui.horizontal(|ui| {
                                            ui.spinner();
                                            ui.label(if cancelling {
                                                "Cancelling after the current step..."
                                            } else {
                                                "Running benchmark..."
                                            });
                                        });
                                    }
                                    Some(Ok(result)) => {
                                        egui::Grid::new("benchmark_result")
                                            .num_columns(2)
                                            .spacing([16.0, 4.0])
                                            .show(ui, |ui| {
                                                info_row(ui, "Files", result.files.to_string());
                                                info_row(
                                                    ui,
                                                    "Original size",
                                                    display_size(result.uncompressed),
                                                );
                                                info_row(
                                                    ui,
                                                    "Archive size",
                                                    display_size(result.archive_size),
                                                );
                                                ui.label(egui::RichText::new("Ratio").weak());
                                                info_ratio_bar(
                                                    ui,
                                                    info_ratio(
                                                        result.archive_size,
                                                        result.uncompressed,
                                                    ),
                                                );
                                                ui.end_row();
                                                info_row(
                                                    ui,
                                                    "Create",
                                                    format!(
                                                        "{:.2} s, {}",
                                                        result.create_seconds,
                                                        throughput(
                                                            result.uncompressed,
                                                            result.create_seconds
                                                        )
                                                    ),
                                                );
                                                info_row(
                                                    ui,
                                                    "Extract",
                                                    format!(
                                                        "{:.2} s, {}",
                                                        result.extract_seconds,
                                                        throughput(
                                                            result.uncompressed,
                                                            result.extract_seconds
                                                        )
                                                    ),
                                                );
                                            });
                                    }
                                    Some(Err(error)) => {
                                        ui.colored_label(ui.visuals().error_fg_color, error);
                                    }
                                    None => {}
                                }
                            });
                });
                if !open {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            },
        );
        self.record_dialog_size(size_key, actual_size);
        if !open && !running {
            self.benchmark.open = false;
        }
        if cancel {
            self.benchmark
                .cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if start {
            let settings = BenchmarkSettings {
                block_size: self.benchmark.settings.block_size.clone(),
                ..self.benchmark.settings
            };
            let archive = self.read_archive.clone();
            let password = self
                .archive_password
                .as_ref()
                .map(|p| p.as_bytes().to_vec());
            let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
            self.benchmark.cancel = cancel_flag.clone();
            let (sender, receiver) = mpsc::channel();
            self.benchmark.receiver = Some(receiver);
            self.benchmark.result = None;
            std::thread::spawn(move || {
                let _ = sender.send(run_benchmark(&archive, password, &settings, &cancel_flag));
            });
        }
    }
}
