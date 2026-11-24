use super::super::*;
use fancy_regex::{BytesMode, Regex, RegexBuilder};
use std::ops::Range;

const MAX_LINE_CHARS: usize = 250;

type FindResult = Result<FindResults, String>;

#[derive(Debug, Default)]
struct FindResults {
    files: Vec<String>,
    file_indices: std::collections::HashMap<String, usize>,
    rows: Vec<FindRow>,
    result_count: usize,
    invalid_utf8: bool,
}

#[derive(Debug)]
struct FindRow {
    file: usize,
    group_start: usize,
    line_number: u64,
    text: String,
    highlights: Vec<Range<usize>>,
    is_match: bool,
}

fn search_pattern(text: &str, regex: bool, case_sensitive: bool, word: bool) -> String {
    let pattern = if regex {
        text.to_owned()
    } else {
        let literal = regex::escape(text);
        if word {
            format!(r"(?<!\w){literal}(?!\w)")
        } else {
            literal
        }
    };
    if case_sensitive {
        pattern
    } else {
        format!("(?i){pattern}")
    }
}

impl FindResults {
    fn finish(&mut self, include_context: bool) {
        self.rows.sort_by(|left, right| {
            self.files[left.file]
                .cmp(&self.files[right.file])
                .then(left.line_number.cmp(&right.line_number))
        });
        self.result_count = 0;
        let mut previous: Option<(usize, u64, usize)> = None;
        for (index, row) in self.rows.iter_mut().enumerate() {
            row.group_start = previous
                .filter(|(file, line, _)| {
                    include_context
                        && *file == row.file
                        && line.checked_add(1) == Some(row.line_number)
                })
                .map_or(index, |(_, _, group)| group);
            if row.group_start == index {
                self.result_count += 1;
            }
            previous = Some((row.file, row.line_number, row.group_start));
        }
    }

    fn push_line(
        &mut self,
        path: &str,
        line: operations::GrepLine,
        regex: &Regex,
        include_context: bool,
    ) -> Result<(), String> {
        let file_index = match self.file_indices.get(path) {
            Some(&index) => index,
            None => {
                self.files.push(path.to_owned());
                let index = self.files.len() - 1;
                self.file_indices.insert(path.to_owned(), index);
                index
            }
        };
        self.invalid_utf8 |= std::str::from_utf8(&line.bytes).is_err();
        let (text, highlights) =
            format_result_line(&line.bytes, line.is_match, regex).map_err(|error| {
                format!("cannot highlight {path} line {}: {error}", line.line_number)
            })?;
        let group_start = self
            .rows
            .last()
            .filter(|previous| {
                include_context
                    && previous.file == file_index
                    && previous.line_number.checked_add(1) == Some(line.line_number)
            })
            .map_or(self.rows.len(), |previous| previous.group_start);
        if group_start == self.rows.len() {
            self.result_count += 1;
        }
        self.rows.push(FindRow {
            file: file_index,
            group_start,
            line_number: line.line_number,
            text,
            highlights,
            is_match: line.is_match,
        });
        Ok(())
    }

    #[cfg(test)]
    fn new(
        files: Vec<operations::GrepFile>,
        pattern: &str,
        include_context: bool,
        progress: Option<&dyn Progress>,
    ) -> Result<Self, String> {
        let regex = RegexBuilder::new(pattern)
            .bytes_mode(BytesMode::UnicodeBytes)
            .build()
            .map_err(|error| error.to_string())?;
        let mut results = Self::default();
        for file in files {
            let file_index = results.files.len();
            let path = file.path.to_string();
            for line in file.lines {
                if progress.is_some_and(Progress::is_cancelled) {
                    return Err("operation cancelled".to_owned());
                }
                results.invalid_utf8 |= std::str::from_utf8(&line.bytes).is_err();
                let (text, highlights) = format_result_line(&line.bytes, line.is_match, &regex)
                    .map_err(|error| {
                        format!("cannot highlight {path} line {}: {error}", line.line_number)
                    })?;
                let group_start = results
                    .rows
                    .last()
                    .filter(|previous| {
                        include_context
                            && previous.file == file_index
                            && previous.line_number.checked_add(1) == Some(line.line_number)
                    })
                    .map_or(results.rows.len(), |previous| previous.group_start);
                if group_start == results.rows.len() {
                    results.result_count += 1;
                }
                results.rows.push(FindRow {
                    file: file_index,
                    group_start,
                    line_number: line.line_number,
                    text,
                    highlights,
                    is_match: line.is_match,
                });
            }
            results.files.push(path);
        }
        Ok(results)
    }
}

fn format_result_line(
    bytes: &[u8],
    is_match: bool,
    regex: &Regex,
) -> Result<(String, Vec<Range<usize>>), fancy_regex::Error> {
    let prefix = &bytes[..bytes.len().min((MAX_LINE_CHARS + 1) * 4)];
    let display = String::from_utf8_lossy(prefix);
    let truncated = display.chars().count() > MAX_LINE_CHARS;
    let visible_chars = if truncated {
        MAX_LINE_CHARS - 1
    } else {
        MAX_LINE_CHARS
    };
    let mut text: String = display.chars().take(visible_chars).collect();
    if truncated {
        text.push('…');
    }
    let mut highlights = Vec::new();
    if is_match {
        for matched in regex.find_iter(bytes) {
            let matched = matched?;
            if matched.start() >= prefix.len() {
                break;
            }
            let start = String::from_utf8_lossy(&bytes[..matched.start()])
                .chars()
                .count();
            if start >= visible_chars {
                break;
            }
            let end = String::from_utf8_lossy(&bytes[..matched.end().min(prefix.len())])
                .chars()
                .count()
                .min(visible_chars);
            if start < end {
                highlights.push(start..end);
            }
        }
    }
    Ok((text, highlights))
}

fn render_find_results(
    ui: &mut egui::Ui,
    results: &FindResults,
    selected: &mut Option<usize>,
    can_open: bool,
) -> Option<String> {
    let mut open_path = None;
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let row_height = ui.spacing().interact_size.y.max(24.0);
    let line_width = 72.0;
    egui::ScrollArea::both()
        .id_salt("find-results")
        .auto_shrink([false, false])
        .show_rows(ui, row_height, results.rows.len(), |ui, row_range| {
            for index in row_range {
                let row = &results.rows[index];
                let path = &results.files[row.file];
                let color = if row.is_match {
                    ui.visuals().text_color()
                } else {
                    ui.visuals().weak_text_color()
                };
                let galley = ui
                    .painter()
                    .layout_no_wrap(row.text.clone(), font.clone(), color);
                let path_galley = ui.painter().layout_no_wrap(
                    path.clone(),
                    font.clone(),
                    ui.visuals().text_color(),
                );
                let path_width = path_galley.size().x + 12.0;
                let width = ui
                    .available_width()
                    .max(path_width + line_width + galley.size().x + 16.0);
                ui.set_min_width(width);
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(width, row_height), egui::Sense::click());
                let response = response.on_hover_text(path);
                if *selected == Some(row.group_start) {
                    ui.painter().rect_filled(
                        rect.expand2(egui::vec2(0.0, ui.spacing().item_spacing.y / 2.0)),
                        0.0,
                        ui.visuals().selection.bg_fill,
                    );
                } else if response.hovered() {
                    ui.painter()
                        .rect_filled(rect, 0.0, ui.visuals().widgets.hovered.bg_fill);
                }
                if response.clicked() {
                    *selected = Some(row.group_start);
                }
                if can_open && response.double_clicked() {
                    open_path = Some(path.clone());
                }
                let painter = ui.painter().with_clip_rect(rect);
                if row.group_start == index {
                    if index > 0 {
                        let y = rect.top() - ui.spacing().item_spacing.y / 2.0;
                        ui.painter().line_segment(
                            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                            ui.visuals().widgets.noninteractive.bg_stroke,
                        );
                    }
                    painter.galley(
                        egui::pos2(
                            rect.left() + 4.0,
                            rect.center().y - path_galley.size().y / 2.0,
                        ),
                        path_galley,
                        ui.visuals().text_color(),
                    );
                }
                painter.text(
                    egui::pos2(rect.left() + path_width + 4.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    row.line_number.to_string(),
                    font.clone(),
                    ui.visuals().text_color(),
                );
                let position = egui::pos2(
                    rect.left() + path_width + line_width,
                    rect.center().y - galley.size().y / 2.0,
                );
                painter.galley(position, galley.clone(), color);
                // Egui's strong text changes color, not weight; overdraw only match spans.
                let bold_offset = 1.0 / ui.ctx().pixels_per_point();
                for highlight in &row.highlights {
                    let start = galley.pos_from_cursor(egui::text::CCursor::new(highlight.start));
                    let end = galley.pos_from_cursor(egui::text::CCursor::new(highlight.end));
                    let clip = egui::Rect::from_min_max(
                        egui::pos2(position.x + start.min.x.min(end.min.x), rect.top()),
                        egui::pos2(
                            position.x + start.min.x.max(end.min.x) + bold_offset,
                            rect.bottom(),
                        ),
                    );
                    painter.with_clip_rect(clip).galley(
                        position + egui::vec2(bold_offset, 0.0),
                        galley.clone(),
                        color,
                    );
                }
            }
        });
    open_path
}

pub(in crate::ui) struct FindState {
    pub(in crate::ui) open: bool,
    file_filter: String,
    pattern: String,
    case_sensitive: bool,
    word: bool,
    regex: bool,
    chain: bool,
    chain_paths: Option<BTreeSet<ArchivePath>>,
    before: usize,
    after: usize,
    receiver: Option<Receiver<FindResult>>,
    lines: Option<Receiver<(String, operations::GrepLine)>>,
    headers: Option<Receiver<Arc<FindCache>>>,
    cache: Option<Arc<FindCache>>,
    result_regex: Option<Regex>,
    cancellation: Option<Arc<AtomicBool>>,
    pub(in crate::ui) cancelled: bool,
    result: Option<FindResult>,
    selected_row: Option<usize>,
}

impl Default for FindState {
    fn default() -> Self {
        Self {
            open: false,
            file_filter: "*".to_owned(),
            pattern: String::new(),
            case_sensitive: true,
            word: false,
            regex: true,
            chain: false,
            chain_paths: None,
            before: 0,
            after: 0,
            receiver: None,
            lines: None,
            headers: None,
            cache: None,
            result_regex: None,
            cancellation: None,
            cancelled: false,
            result: None,
            selected_row: None,
        }
    }
}

impl FindState {
    pub(in crate::ui) fn clear_results(&mut self) {
        self.cache = None;
        self.chain_paths = None;
        self.result = None;
        self.selected_row = None;
        self.cancelled = false;
    }
}

#[derive(PartialEq, Eq)]
struct FindStamp {
    size: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

impl FindStamp {
    fn read(path: &Path) -> std::io::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        #[cfg(unix)]
        let identity = {
            use std::os::unix::fs::MetadataExt;
            (
                metadata.dev(),
                metadata.ino(),
                metadata.ctime(),
                metadata.ctime_nsec(),
            )
        };
        Ok(Self {
            size: metadata.len(),
            modified: metadata.modified()?,
            #[cfg(unix)]
            identity,
        })
    }
}

struct FindCache {
    archive: String,
    archive_stamp: FindStamp,
    overlay: Option<(PathBuf, FindStamp)>,
    commit: Option<String>,
    passwords: operations::PasswordSet,
    prepared: operations::PreparedGrep,
    _view: Option<tempfile::TempDir>,
}

impl ZargApp {
    fn start_find(&mut self) {
        if !self.can_find() || self.operation.kind.is_some() || self.find.pattern.is_empty() {
            return;
        }
        self.find.cancelled = false;
        let overlay = self
            .overlay_cache
            .as_ref()
            .filter(|_| self.has_unsaved_changes);
        if let Some(cache) = overlay
            && let Err(error) = cache.verify_base(Path::new(&self.read_archive))
        {
            self.find.result = Some(Err(error.to_string()));
            return;
        }
        let overlay = overlay.map(OverlayCache::path);
        let archive = self.read_archive.clone();
        let commit = (!self.selected_commit.is_empty()).then(|| self.selected_commit.clone());
        let passwords = password_set(&self.archive_passwords, self.archive_password.as_deref());
        let file_filter = self.find.file_filter.clone();
        let pattern = search_pattern(
            &self.find.pattern,
            self.find.regex,
            self.find.case_sensitive,
            self.find.word && !self.find.regex,
        );
        let before = self.find.before;
        let after = self.find.after;
        let regex = match RegexBuilder::new(&pattern)
            .bytes_mode(BytesMode::UnicodeBytes)
            .build()
        {
            Ok(regex) => regex,
            Err(error) => {
                self.find.result = Some(Err(error.to_string()));
                return;
            }
        };
        let archive_stamp = match FindStamp::read(Path::new(&archive)) {
            Ok(stamp) => stamp,
            Err(error) => {
                self.find.result = Some(Err(error.to_string()));
                return;
            }
        };
        let overlay_stamp = match overlay
            .as_ref()
            .map(|path| FindStamp::read(path).map(|stamp| (path.clone(), stamp)))
            .transpose()
        {
            Ok(stamp) => stamp,
            Err(error) => {
                self.find.result = Some(Err(error.to_string()));
                return;
            }
        };
        let cached = self
            .find
            .cache
            .as_ref()
            .filter(|cache| {
                cache.archive == archive
                    && cache.archive_stamp == archive_stamp
                    && cache.overlay == overlay_stamp
                    && cache.commit == commit
                    && cache.passwords == passwords
                    && cache.prepared.check_unchanged().is_ok()
            })
            .cloned();
        if cached.is_none() {
            self.find.chain_paths = None;
        }
        let chain_paths = self
            .find
            .chain
            .then(|| self.find.chain_paths.clone())
            .flatten();
        let (sender, receiver) = mpsc::channel();
        let (line_tx, line_rx) = mpsc::sync_channel(2048);
        let (header_tx, header_rx) = mpsc::channel();
        let (events_tx, events_rx) = mpsc::channel();
        let (answers, answer_rx) = mpsc::channel();
        let cancellation = Arc::new(AtomicBool::new(false));
        let progress = UiProgress {
            events: events_tx.clone(),
            answers: Arc::new(Mutex::new(answer_rx)),
            cancellation: Some(Arc::clone(&cancellation)),
            limit_status_updates: false,
            last_status_update: Mutex::new(None),
        };
        self.find.receiver = Some(receiver);
        self.find.lines = Some(line_rx);
        self.find.headers = Some(header_rx);
        self.find.result_regex = Some(regex);
        self.find.cancellation = Some(Arc::clone(&cancellation));
        self.find.cancelled = false;
        self.find.result = None;
        self.find.selected_row = None;
        self.begin_worker_with_cancellation(
            events_rx,
            answers,
            "Searching archive",
            Some(cancellation),
        );
        self.status = "Searching archive...".to_owned();
        std::thread::spawn(move || {
            let result = (|| -> Result<_, Box<dyn std::error::Error>> {
                if progress.is_cancelled() {
                    return Err("operation cancelled".into());
                }
                let cache = if let Some(cache) = cached {
                    cache
                } else {
                    let view = if let Some(overlay) = &overlay {
                        let directory = tempfile::Builder::new()
                            .prefix("zarg-find-view-")
                            .tempdir()?;
                        operations::combine_archives_with_progress(
                            Path::new(&archive),
                            overlay,
                            &directory.path().join("view.zar"),
                            Some(&progress),
                        )?;
                        Some(directory)
                    } else {
                        None
                    };
                    let view_path = view
                        .as_ref()
                        .map(|directory| directory.path().join("view.zar"));
                    let source = match &view_path {
                        Some(path) => path
                            .to_str()
                            .ok_or("Temporary archive path is not valid UTF-8")?,
                        None => &archive,
                    };
                    let prepared = operations::PreparedGrep::new(
                        source,
                        commit.as_deref(),
                        &passwords,
                        Some(&progress),
                    )?;
                    if FindStamp::read(Path::new(&archive))? != archive_stamp
                        || overlay_stamp.as_ref().is_some_and(|(path, stamp)| {
                            FindStamp::read(path).ok().as_ref() != Some(stamp)
                        })
                    {
                        return Err("archive changed during search preparation".into());
                    }
                    Arc::new(FindCache {
                        archive: archive.clone(),
                        archive_stamp,
                        overlay: overlay_stamp,
                        commit: commit.clone(),
                        passwords,
                        prepared,
                        _view: view,
                    })
                };
                header_tx
                    .send(Arc::clone(&cache))
                    .map_err(|_| "search result receiver closed")?;
                let mut filters =
                    FilterSet::from_patterns(&[file_filter], &[], &[], &[], &[] as &[&Path])?;
                if let Some(paths) = chain_paths {
                    filters.restrict_to_paths(paths);
                }
                cache.prepared.search_with_filters(
                    &filters,
                    &pattern,
                    before,
                    after,
                    Some(&progress),
                    &|path, line| {
                        line_tx
                            .send((path.to_string(), line))
                            .map_err(|_| std::io::Error::other("search result receiver closed"))
                    },
                )?;
                if FindStamp::read(Path::new(&cache.archive))? != cache.archive_stamp
                    || cache.overlay.as_ref().is_some_and(|(path, stamp)| {
                        FindStamp::read(path).ok().as_ref() != Some(stamp)
                    })
                {
                    return Err("archive changed during search".into());
                }
                Ok(FindResults::default())
            })()
            .map_err(|error| error.to_string());
            let status = if find_cancelled(&result, progress.is_cancelled()) {
                "Search cancelled."
            } else if result.is_ok() {
                "Search complete."
            } else {
                "Search failed."
            };
            let _ = sender.send(result);
            finish_with_events(events_tx, Ok(status.to_owned()));
        });
    }

    pub(in crate::ui) fn poll_find(&mut self, context: &egui::Context) {
        let Some(receiver) = &self.find.receiver else {
            return;
        };
        let completion = receiver.try_recv();
        if let Some(headers) = &self.find.headers
            && let Ok(cache) = headers.try_recv()
        {
            self.find.cache = Some(cache);
            self.find.headers = None;
        }
        if let (Some(lines), Some(regex)) = (&self.find.lines, &self.find.result_regex) {
            let mut processed = 0;
            for (path, line) in lines.try_iter().take(2048) {
                processed += 1;
                let results = self
                    .find
                    .result
                    .get_or_insert_with(|| Ok(FindResults::default()));
                if let Ok(results) = results
                    && let Err(error) = results.push_line(
                        &path,
                        line,
                        regex,
                        self.find.before > 0 || self.find.after > 0,
                    )
                {
                    self.find.result = Some(Err(error));
                    if let Some(token) = &self.find.cancellation {
                        token.store(true, Ordering::Relaxed);
                    }
                    break;
                }
            }
            if processed == 2048 {
                context.request_repaint();
            }
        }
        match completion {
            Ok(result) => {
                self.find.receiver = None;
                self.find.lines = None;
                self.find.headers = None;
                self.find.result_regex = None;
                let cancelled = self
                    .find
                    .cancellation
                    .take()
                    .is_some_and(|token| token.load(Ordering::Relaxed));
                let highlight_failed = self.find.result.as_ref().is_some_and(Result::is_err);
                self.find.cancelled = !highlight_failed && find_cancelled(&result, cancelled);
                if self.find.cancelled {
                    self.find.result = None;
                    self.status = "Search cancelled.".to_owned();
                } else {
                    if !highlight_failed && (result.is_err() || self.find.result.is_none()) {
                        self.find.result = Some(result);
                    }
                    if let Some(Ok(results)) = &mut self.find.result {
                        results.finish(self.find.before > 0 || self.find.after > 0);
                        self.find.selected_row = None;
                    }
                    if self.find.chain
                        && !highlight_failed
                        && let Some(Ok(results)) = &self.find.result
                    {
                        match results.files.iter().map(ArchivePath::new).collect() {
                            Ok(paths) => self.find.chain_paths = Some(paths),
                            Err(error) => self.find.result = Some(Err(error.to_string())),
                        }
                    }
                }
            }
            Err(mpsc::TryRecvError::Empty) => {
                context.request_repaint_after(Duration::from_millis(100));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.find.receiver = None;
                self.find.lines = None;
                self.find.headers = None;
                self.find.result_regex = None;
                self.find.cancellation = None;
                self.find.result = Some(Err("Search worker stopped unexpectedly.".to_owned()));
            }
        }
    }

    pub(in crate::ui) fn render_find(&mut self, context: &egui::Context) {
        if !self.find.open {
            return;
        }
        let running = self.find.receiver.is_some();
        let cancelling = self
            .find
            .cancellation
            .as_ref()
            .is_some_and(|token| token.load(Ordering::Relaxed));
        let can_open = self.can_find() && self.operation.kind.is_none();
        let can_search = can_open && !self.find.pattern.is_empty();
        let mut open = true;
        let mut start = false;
        let mut open_path = None;
        let size_key = "find";
        let size = self.dialog_size(size_key, [720.0, 520.0]);
        let mut actual_size = None;
        let viewport_id = egui::ViewportId::from_hash_of("find-dialog");
        let state = &mut self.find;
        context.show_viewport_immediate(
            viewport_id,
            Self::centered_dialog_viewport_builder(context, viewport_id, "Find", size, true),
            |ui, _class| {
                ui.painter().rect_filled(ui.max_rect(), 0.0, ui.visuals().window_fill().to_opaque());
                actual_size = Self::viewport_inner_size(ui.ctx());
                if ui.ctx().input(|input| input.viewport().close_requested()) {
                    if running {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    } else {
                        open = false;
                    }
                }
                if consume_alt_key(ui.ctx(), egui::Key::F) && can_search && !running {
                    start = true;
                }
                egui::Panel::bottom("find-actions")
                    .frame(egui::Frame::new().inner_margin(3))
                    .show_separator_line(false)
                    .show(ui, |ui| {
                        ui.separator();
                        if !running && self.operation.kind.is_some() {
                            ui.weak("Close the operation dialog before searching.");
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if running {
                                if close_or_cancel(icon_button(ui, &self.icons, UiIcon::Cancel, "Cancel", !cancelling && state.cancellation.is_some()), true)
                                    && let Some(token) = &state.cancellation
                                {
                                    token.store(true, Ordering::Relaxed);
                                }
                            } else if close_or_cancel(icon_button(ui, &self.icons, UiIcon::Close, "Close", true), true) {
                                open = false;
                            }
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                let status = if cancelling {
                                    "Cancelling...".to_owned()
                                } else if running {
                                    "Searching...".to_owned()
                                } else if state.cancelled {
                                    "Search cancelled.".to_owned()
                                } else {
                                    match &state.result {
                                        Some(Ok(results)) => format!("Results: {}", results.result_count),
                                        Some(Err(_)) => "Search failed.".to_owned(),
                                        None => "Results: 0".to_owned(),
                                    }
                                };
                                ui.label(status);
                            });
                        });
                    });
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().inner_margin(6))
                    .show(ui, |ui| {
                        ui.add_enabled_ui(!running, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("File name glob");
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if icon_button(ui, &self.icons, UiIcon::Find, "Find", can_search)
                                        .on_hover_text("Alt+F")
                                        .clicked()
                                    {
                                        start = true;
                                    }
                                    if ui.selectable_label(state.chain, "Chain")
                                        .on_hover_text("Search only files matched by the previous search")
                                        .clicked()
                                    {
                                        state.chain = !state.chain;
                                        state.chain_paths = None;
                                    }
                                    if ui.selectable_label(state.regex, "Regex")
                                        .on_hover_text("Use a regular expression")
                                        .clicked()
                                    {
                                        state.regex = !state.regex;
                                    }
                                    let word = ui.add_enabled_ui(!state.regex, |ui| {
                                        ui.selectable_label(state.word, "Word")
                                    }).inner.on_hover_text("Match whole words (literal searches only)");
                                    if word.clicked() {
                                        state.word = !state.word;
                                    }
                                    if ui.selectable_label(state.case_sensitive, "Aa")
                                        .on_hover_text("Match upper and lower case")
                                        .clicked()
                                    {
                                        state.case_sensitive = !state.case_sensitive;
                                    }
                                    let glob = ui.add_sized(
                                        [ui.available_width(), ui.spacing().interact_size.y],
                                        egui::TextEdit::singleline(&mut state.file_filter)
                                            .id(egui::Id::unique("find-file-filter")),
                                    );
                                    if can_search && glob.lost_focus() && ui.input(|input| {
                                        input.events.iter().any(|event| matches!(
                                            event,
                                            egui::Event::Key {
                                                key: egui::Key::Enter,
                                                pressed: true,
                                                modifiers: egui::Modifiers::NONE,
                                                ..
                                            }
                                        ))
                                    }) {
                                        start = true;
                                    }
                                });
                            });
                            ui.horizontal(|ui| {
                                ui.label(if state.regex { "Regex" } else { "Text" });
                                let width = (ui.available_width() - 295.0).max(40.0);
                                let regex = ui.add_sized(
                                    [width, ui.spacing().interact_size.y],
                                    egui::TextEdit::singleline(&mut state.pattern)
                                        .id(egui::Id::unique("find-regex")),
                                );
                                if can_search && regex.lost_focus() && ui.input(|input| {
                                    input.events.iter().any(|event| matches!(
                                        event,
                                        egui::Event::Key {
                                            key: egui::Key::Enter,
                                            pressed: true,
                                            modifiers: egui::Modifiers::NONE,
                                            ..
                                        }
                                    ))
                                }) {
                                    start = true;
                                }
                                ui.label("Lines before");
                                ui.add_sized([55.0, ui.spacing().interact_size.y], egui::DragValue::new(&mut state.before));
                                ui.label("Lines after");
                                ui.add_sized([55.0, ui.spacing().interact_size.y], egui::DragValue::new(&mut state.after));
                            });
                        });
                        ui.separator();
                        if cancelling {
                            ui.label("Cancelling search...");
                        } else if let Some(result) = &state.result {
                            match result {
                                Ok(results) if results.rows.is_empty() && !running => {
                                    ui.label("No matches found.");
                                }
                                Ok(results) => {
                                    ui.label(format!("{} matching file(s).", results.files.len()));
                                    if results.invalid_utf8 {
                                        ui.weak("Invalid UTF-8 bytes are shown as replacement characters.");
                                    }
                                    open_path = render_find_results(ui, results, &mut state.selected_row, can_open);
                                }
                                Err(error) => {
                                    ui.colored_label(ui.visuals().error_fg_color, error);
                                }
                            }
                        } else if running {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Searching archive...");
                            });
                        }
                    });
                if !open {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            },
        );
        self.record_dialog_size(size_key, actual_size);
        if !open {
            self.find.open = false;
        }
        if start && open {
            self.start_find();
        }
        if let Some(path) = open_path {
            self.open_entry_externally(path);
        }
    }
}

fn find_cancelled(result: &FindResult, requested: bool) -> bool {
    requested
        && (result.is_ok()
            || result.as_ref().is_err_and(|error| {
                error == "operation cancelled" || error.ends_with(": operation cancelled")
            }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regex(pattern: &str) -> Regex {
        RegexBuilder::new(pattern)
            .bytes_mode(BytesMode::UnicodeBytes)
            .build()
            .unwrap()
    }

    #[test]
    fn result_lines_truncate_and_highlight_unicode_and_invalid_bytes() {
        let (text, ranges) =
            format_result_line(b"\xffcaf\xc3\xa9 word word", true, &regex(r"(?<=é )word")).unwrap();
        assert_eq!(text, "�café word word");
        assert_eq!(ranges, vec![6..10]);

        let bytes = format!("{}abc {}", "é".repeat(248), "abc");
        let (text, ranges) = format_result_line(bytes.as_bytes(), true, &regex("abc")).unwrap();
        assert_eq!(text.chars().count(), 250);
        assert!(text.ends_with("a…"));
        assert_eq!(ranges, vec![248..249]);

        let exact = "é".repeat(250);
        assert_eq!(
            format_result_line(exact.as_bytes(), false, &regex(".")).unwrap(),
            (exact, Vec::new())
        );
        assert!(
            format_result_line(b"abc", true, &regex(r"(?=b)"))
                .unwrap()
                .1
                .is_empty()
        );
        assert_eq!(
            format_result_line(b"aba", true, &regex("a")).unwrap().1,
            vec![0..1, 2..3]
        );
    }

    #[test]
    fn result_highlighting_reports_regex_runtime_errors() {
        let regex = RegexBuilder::new(r"(x+x+)+(?>y)")
            .bytes_mode(BytesMode::UnicodeBytes)
            .backtrack_limit(1)
            .build()
            .unwrap();
        assert!(format_result_line(b"xxxxxxxxxxy", true, &regex).is_err());
    }

    #[test]
    fn context_results_group_consecutive_lines_per_file() {
        fn file(path: &str, lines: &[(u64, bool)]) -> operations::GrepFile {
            operations::GrepFile {
                path: ArchivePath::new(path).unwrap(),
                lines: lines
                    .iter()
                    .map(|&(line_number, is_match)| operations::GrepLine {
                        line_number,
                        bytes: if is_match { b"match" } else { b"other" }.to_vec(),
                        is_match,
                    })
                    .collect(),
            }
        }
        let files = vec![
            file(
                "a.txt",
                &[(1, false), (2, true), (3, true), (4, false), (8, true)],
            ),
            file("b.txt", &[(9, false), (10, true)]),
        ];
        let grouped = FindResults::new(files.clone(), "match", true, None).unwrap();
        assert_eq!(grouped.result_count, 3);
        assert_eq!(
            grouped
                .rows
                .iter()
                .map(|row| row.group_start)
                .collect::<Vec<_>>(),
            vec![0, 0, 0, 0, 4, 5, 5]
        );
        assert_eq!(grouped.rows[1].highlights, vec![0..5]);
        assert_eq!(grouped.rows[2].highlights, vec![0..5]);
        assert!(grouped.rows[0].highlights.is_empty());

        let ungrouped = FindResults::new(files, "match", false, None).unwrap();
        assert_eq!(ungrouped.result_count, 7);
        assert_eq!(
            ungrouped
                .rows
                .iter()
                .map(|row| row.group_start)
                .collect::<Vec<_>>(),
            (0..7).collect::<Vec<_>>()
        );
    }

    #[test]
    fn find_status_bar_displays_result_count_and_search_state() {
        fn texts(shape: &egui::Shape, output: &mut Vec<(String, egui::Pos2)>) {
            match shape {
                egui::Shape::Text(text) => output.push((text.galley.job.text.clone(), text.pos)),
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        texts(shape, output);
                    }
                }
                _ => {}
            }
        }
        fn frame(context: &egui::Context, app: &mut ZargApp) -> Vec<(String, egui::Pos2)> {
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ui| app.render_find(ui.ctx()),
            );
            let mut labels = Vec::new();
            for shape in &output.shapes {
                texts(&shape.shape, &mut labels);
            }
            output.drop_without_applying_deltas();
            labels
        }

        let context = egui::Context::default();
        let mut app = ZargApp::default();
        app.find.open = true;
        frame(&context, &mut app);
        assert!(
            frame(&context, &mut app)
                .iter()
                .any(|(text, _)| text == "Results: 0")
        );
        app.find.result = Some(Ok(
            FindResults::new(Vec::new(), "match", true, None).unwrap()
        ));
        assert!(
            frame(&context, &mut app)
                .iter()
                .any(|(text, _)| text == "Results: 0")
        );
        let files = vec![operations::GrepFile {
            path: ArchivePath::new("file.txt").unwrap(),
            lines: [(1, false), (2, true), (3, false), (7, true)]
                .into_iter()
                .map(|(line_number, is_match)| operations::GrepLine {
                    line_number,
                    bytes: if is_match { b"match" } else { b"other" }.to_vec(),
                    is_match,
                })
                .collect(),
        }];
        app.find.result = Some(Ok(FindResults::new(files, "match", true, None).unwrap()));
        let labels = frame(&context, &mut app);
        let status = labels
            .iter()
            .find(|(text, _)| text == "Results: 2")
            .unwrap()
            .1;
        let close = labels.iter().find(|(text, _)| text == "Close").unwrap().1;
        assert!(status.x < close.x);
        assert!((status.y - close.y).abs() < 10.0);

        app.find.result = Some(Err("invalid regex".into()));
        assert!(
            frame(&context, &mut app)
                .iter()
                .any(|(text, _)| text == "Search failed.")
        );
        app.find.clear_results();
        assert!(
            frame(&context, &mut app)
                .iter()
                .any(|(text, _)| text == "Results: 0")
        );
        let (sender, receiver) = mpsc::channel();
        app.find.receiver = Some(receiver);
        let cancellation = Arc::new(AtomicBool::new(false));
        app.find.cancellation = Some(Arc::clone(&cancellation));
        let labels = frame(&context, &mut app);
        assert!(labels.iter().any(|(text, _)| text == "Searching..."));
        assert!(labels.iter().any(|(text, _)| text == "Cancel"));
        assert!(!labels.iter().any(|(text, _)| text == "Close"));
        cancellation.store(true, Ordering::Relaxed);
        assert!(
            frame(&context, &mut app)
                .iter()
                .any(|(text, _)| text == "Cancelling...")
        );
        sender.send(Ok(FindResults::default())).unwrap();
        app.poll_find(&context);
        let labels = frame(&context, &mut app);
        assert!(labels.iter().any(|(text, _)| text == "Search cancelled."));
        assert!(labels.iter().any(|(text, _)| text == "Close"));
    }

    #[test]
    fn result_list_virtualizes_and_opens_double_clicked_row() {
        let results = FindResults {
            files: vec!["first.txt".into(), "second.txt".into()],
            file_indices: Default::default(),
            rows: (0_usize..10_000)
                .map(|index| FindRow {
                    file: usize::from(index >= 2),
                    group_start: if index == 1 { 0 } else { index },
                    line_number: u64::try_from(index).unwrap() + 1,
                    text: "match".into(),
                    highlights: std::iter::once(0..5).collect(),
                    is_match: true,
                })
                .collect(),
            result_count: 9_999,
            invalid_utf8: false,
        };
        let context = egui::Context::default();
        let mut selected = None;
        let position = egui::pos2(20.0, 38.0);
        let mut opened = None;
        let mut frame = |events: Vec<egui::Event>, time: f64, can_open: bool| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(700.0, 400.0),
                )),
                events,
                time: Some(time),
                ..Default::default()
            };
            let output = context.run_ui(input, |ui| {
                opened = render_find_results(ui, &results, &mut selected, can_open);
            });
            let shapes = output.shapes.len();
            output.drop_without_applying_deltas();
            shapes
        };
        assert!(frame(Vec::new(), 0.0, true) < 200);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            vec![egui::Event::PointerMoved(position), button(position, true)],
            0.1,
            true,
        );
        frame(vec![button(position, false)], 0.11, true);
        frame(vec![button(position, true)], 0.2, true);
        frame(vec![button(position, false)], 0.21, true);
        assert_eq!(selected, Some(0));
        assert_eq!(opened.as_deref(), Some("first.txt"));
    }

    fn wait(app: &mut ZargApp) {
        let context = egui::Context::default();
        for _ in 0..12_000 {
            app.poll_worker(&context);
            app.poll_find(&context);
            if !app.busy && app.find.receiver.is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("worker did not finish");
    }

    #[test]
    fn find_reuses_headers_until_archive_changes() {
        use std::io::{Read, Seek, Write};
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("archive.zar");
        let input = directory.path().join("file.txt");
        std::fs::write(&input, b"match\n").unwrap();
        operations::create_archive(archive.to_str().unwrap(), &[input]).unwrap();
        let mut app = ZargApp::default();
        app.open_archive(archive.clone(), None);
        wait(&mut app);
        app.find.pattern = "match".into();
        app.start_find();
        wait(&mut app);
        let first = Arc::clone(app.find.cache.as_ref().unwrap());
        app.start_find();
        wait(&mut app);
        assert!(Arc::ptr_eq(&first, app.find.cache.as_ref().unwrap()));

        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&archive)
            .unwrap();
        let mut byte = [0];
        file.read_exact(&mut byte).unwrap();
        file.rewind().unwrap();
        file.write_all(&byte).unwrap();
        app.start_find();
        wait(&mut app);
        assert!(!Arc::ptr_eq(&first, app.find.cache.as_ref().unwrap()));
    }

    #[test]
    fn find_shows_lines_while_search_is_running() {
        let mut app = ZargApp::default();
        let (_done_tx, done_rx) = mpsc::channel();
        let (line_tx, line_rx) = mpsc::sync_channel(2);
        app.find.receiver = Some(done_rx);
        app.find.lines = Some(line_rx);
        app.find.result_regex = Some(Regex::new("match").unwrap());
        line_tx
            .send((
                "file.txt".into(),
                operations::GrepLine {
                    line_number: 1,
                    bytes: b"match".to_vec(),
                    is_match: true,
                },
            ))
            .unwrap();
        app.poll_find(&egui::Context::default());
        assert!(app.find.receiver.is_some());
        assert_eq!(
            app.find
                .result
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .rows
                .len(),
            1
        );
    }

    #[test]
    fn find_supports_literal_case_and_word_modes() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("archive.zar");
        let file = directory.path().join("file.txt");
        std::fs::write(
            &file,
            "cat catalog Cat _cat caté cat-cat\nliteral a.b and A.B and axb\n",
        )
        .unwrap();
        operations::create_archive(archive.to_str().unwrap(), &[file]).unwrap();
        let mut app = ZargApp::default();
        app.open_archive(archive, None);
        wait(&mut app);

        app.find.regex = false;
        app.find.case_sensitive = false;
        app.find.word = true;
        app.find.pattern = "cat".into();
        app.start_find();
        wait(&mut app);
        let results = app.find.result.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(results.rows.len(), 1);
        assert_eq!(results.rows[0].highlights.len(), 4);

        app.find.word = false;
        app.find.pattern = "a.b".into();
        app.start_find();
        wait(&mut app);
        let results = app.find.result.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(results.rows[0].highlights.len(), 2);

        app.find.case_sensitive = true;
        app.start_find();
        wait(&mut app);
        let results = app.find.result.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(results.rows[0].highlights.len(), 1);

        app.find.regex = true;
        app.find.word = true;
        app.start_find();
        wait(&mut app);
        let results = app.find.result.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(results.rows[0].highlights.len(), 2);

        app.find.case_sensitive = false;
        app.start_find();
        wait(&mut app);
        let results = app.find.result.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(results.rows[0].highlights.len(), 3);
    }

    #[test]
    fn find_toggles_are_ordered_and_word_requires_literal_mode() {
        fn texts(shape: &egui::Shape, output: &mut HashMap<String, egui::Pos2>) {
            match shape {
                egui::Shape::Text(text) if text.pos.y > 40.0 => {
                    output
                        .entry(text.galley.job.text.clone())
                        .or_insert(text.pos);
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        texts(shape, output);
                    }
                }
                _ => {}
            }
        }
        fn frame(
            context: &egui::Context,
            app: &mut ZargApp,
            events: Vec<egui::Event>,
        ) -> HashMap<String, egui::Pos2> {
            let output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| app.render_find(ui.ctx()),
            );
            let mut labels = HashMap::new();
            for shape in &output.shapes {
                texts(&shape.shape, &mut labels);
            }
            output.drop_without_applying_deltas();
            labels
        }
        fn click(context: &egui::Context, app: &mut ZargApp, label: &str) {
            let labels = frame(context, app, Vec::new());
            let position = labels[label] + egui::vec2(3.0, 3.0);
            for pressed in [true, false] {
                frame(
                    context,
                    app,
                    vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                );
            }
        }

        let context = egui::Context::default();
        let mut app = ZargApp::default();
        app.find.open = true;
        frame(&context, &mut app, Vec::new());
        let labels = frame(&context, &mut app, Vec::new());
        for pair in ["Aa", "Word", "Regex", "Chain", "Find"].windows(2) {
            assert!(labels[pair[0]].x < labels[pair[1]].x, "{labels:?}");
        }
        click(&context, &mut app, "Word");
        assert!(!app.find.word);
        click(&context, &mut app, "Regex");
        assert!(!app.find.regex);
        click(&context, &mut app, "Word");
        assert!(app.find.word);
        click(&context, &mut app, "Aa");
        assert!(!app.find.case_sensitive);
        click(&context, &mut app, "Chain");
        assert!(app.find.chain);
        app.find.chain_paths = Some([ArchivePath::new("file.txt").unwrap()].into());
        click(&context, &mut app, "Chain");
        assert!(!app.find.chain);
        assert!(app.find.chain_paths.is_none());
    }

    #[test]
    fn find_chains_successful_searches_and_preserves_chain_on_failure() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("archive.zar");
        let a = directory.path().join("a.txt");
        let b = directory.path().join("b.txt");
        let c = directory.path().join("c.txt");
        std::fs::write(&a, b"alpha\nbeta\n").unwrap();
        std::fs::write(&b, b"alpha\n").unwrap();
        std::fs::write(&c, b"beta\n").unwrap();
        operations::create_archive(archive.to_str().unwrap(), &[a, b, c]).unwrap();
        let mut app = ZargApp::default();
        app.open_archive(archive, None);
        wait(&mut app);
        app.find.chain = true;
        app.find.pattern = "alpha".into();
        app.start_find();
        wait(&mut app);
        assert_eq!(app.find.chain_paths.as_ref().unwrap().len(), 2);

        app.find.file_filter = "a*".into();
        app.find.pattern = "beta".into();
        app.start_find();
        wait(&mut app);
        assert_eq!(
            app.find.result.as_ref().unwrap().as_ref().unwrap().files,
            ["a.txt"]
        );
        let previous = app.find.chain_paths.clone();

        app.find.pattern = "(".into();
        app.start_find();
        assert!(app.find.result.as_ref().unwrap().is_err());
        assert_eq!(app.find.chain_paths, previous);

        app.find.file_filter = "*".into();
        app.find.pattern = "alpha".into();
        app.start_find();
        app.find
            .cancellation
            .as_ref()
            .unwrap()
            .store(true, Ordering::Relaxed);
        wait(&mut app);
        assert!(app.find.cancelled);
        assert_eq!(app.find.chain_paths, previous);

        app.find.pattern = "absent".into();
        app.start_find();
        wait(&mut app);
        assert!(app.find.chain_paths.as_ref().unwrap().is_empty());
        app.find.pattern = "beta".into();
        app.start_find();
        wait(&mut app);
        assert!(
            app.find
                .result
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .files
                .is_empty()
        );

        app.find.clear_results();
        app.start_find();
        wait(&mut app);
        let mut files = app
            .find
            .result
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .files
            .clone();
        files.sort();
        assert_eq!(files, ["a.txt", "c.txt"]);
    }

    #[test]
    fn find_searches_staged_changes_and_selected_history() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("archive.zar");
        let base = directory.path().join("base.txt");
        let added = directory.path().join("added.txt");
        std::fs::write(&base, b"before\nmatch\nafter\n").unwrap();
        std::fs::write(&added, b"match\n").unwrap();
        operations::create_archive(archive.to_str().unwrap(), &[base]).unwrap();
        let original = std::fs::read(&archive).unwrap();
        let mut app = ZargApp::default();
        app.open_archive(archive.clone(), None);
        wait(&mut app);
        let base_commit = app.selected_commit.clone();
        app.show_operation(OperationKind::AddToArchive);
        app.operation.input_paths.push(added);
        let form = std::mem::take(&mut app.operation);
        app.start_cached_add(form);
        wait(&mut app);
        assert!(app.report.is_none(), "{:?}", app.report);

        app.find.pattern = "(?<=mat)ch".into();
        app.find.before = 1;
        app.find.after = 1;
        app.start_find();
        app.find
            .cancellation
            .as_ref()
            .unwrap()
            .store(true, Ordering::Relaxed);
        wait(&mut app);
        assert!(app.find.cancelled);
        assert!(app.find.result.is_none());
        assert!(!app.busy);

        app.start_find();
        wait(&mut app);
        assert!(!app.find.cancelled);
        let results = app.find.result.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(results.files.len(), 2);
        assert_eq!(
            results
                .rows
                .iter()
                .filter(|row| results.files[row.file] == "base.txt")
                .map(|line| (line.line_number, line.is_match))
                .collect::<Vec<_>>(),
            vec![(1, false), (2, true), (3, false)]
        );
        assert_eq!(
            results
                .rows
                .iter()
                .find(|row| row.is_match && results.files[row.file] == "base.txt")
                .unwrap()
                .highlights,
            vec![3..5]
        );
        let base_rows = results
            .rows
            .iter()
            .filter(|row| results.files[row.file] == "base.txt")
            .collect::<Vec<_>>();
        assert!(
            base_rows
                .iter()
                .all(|row| row.group_start == base_rows[0].group_start)
        );
        assert_eq!(std::fs::read(&archive).unwrap(), original);

        let cached = Arc::clone(app.find.cache.as_ref().unwrap());
        for (before, after) in [(1, 0), (0, 1)] {
            app.find.before = before;
            app.find.after = after;
            app.start_find();
            wait(&mut app);
            assert!(Arc::ptr_eq(&cached, app.find.cache.as_ref().unwrap()));
            let results = app.find.result.as_ref().unwrap().as_ref().unwrap();
            let base_rows = results
                .rows
                .iter()
                .filter(|row| results.files[row.file] == "base.txt")
                .collect::<Vec<_>>();
            assert_eq!(base_rows.len(), 2);
            assert_eq!(base_rows[0].group_start, base_rows[1].group_start);
        }

        app.find.file_filter = "added*".into();
        app.start_find();
        wait(&mut app);
        let results = app.find.result.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(results.files, ["added.txt"]);

        app.select_commit(base_commit);
        wait(&mut app);
        assert!(app.find.result.is_none());
        app.find.file_filter = "added*".into();
        app.start_find();
        wait(&mut app);
        assert!(
            app.find
                .result
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .rows
                .is_empty()
        );
    }

    #[test]
    fn find_reports_errors_and_uses_commit_passwords() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("encrypted.zar");
        let input = directory.path().join("input.txt");
        std::fs::write(&input, b"secret\n").unwrap();
        operations::create_archive_with_password_progress_and_filters_and_encryption(
            archive.to_str().unwrap(),
            &[input],
            false,
            EncodingOptions::default(),
            Some((
                b"password",
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
        let id = operations::info_archive(archive.to_str().unwrap(), false).unwrap()[0].id;
        let mut app = ZargApp {
            read_archive: archive.to_string_lossy().into_owned(),
            entries: vec![("input.txt".into(), EntryData::File(7))],
            ..Default::default()
        };
        app.find.pattern = "(".into();
        app.start_find();
        wait(&mut app);
        assert!(app.find.result.as_ref().unwrap().is_err());
        app.find.pattern = "secret".into();
        app.find.file_filter = "[".into();
        app.start_find();
        wait(&mut app);
        assert!(app.find.result.as_ref().unwrap().is_err());
        app.find.file_filter = "*".into();
        app.start_find();
        wait(&mut app);
        assert!(app.find.result.as_ref().unwrap().is_err());
        app.archive_passwords.insert(id, "password".into());
        app.start_find();
        wait(&mut app);
        let result = app.find.result.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.rows[0].text, "secret");
    }

    #[test]
    fn find_reports_worker_disconnection() {
        let mut app = ZargApp::default();
        let (sender, receiver) = mpsc::channel();
        app.find.receiver = Some(receiver);
        drop(sender);
        app.poll_find(&egui::Context::default());
        assert!(
            app.find
                .result
                .unwrap()
                .unwrap_err()
                .contains("stopped unexpectedly")
        );
    }

    #[test]
    fn cancellation_does_not_hide_unrelated_search_errors() {
        assert!(!find_cancelled(&Err("archive is corrupt".into()), true));
        assert!(find_cancelled(&Err("operation cancelled".into()), true));
        assert!(!find_cancelled(&Ok(FindResults::default()), false));
    }

    #[test]
    fn find_requires_a_loaded_nonempty_archive_not_just_visible_entries() {
        let mut app = ZargApp::default();
        assert!(!app.can_find());
        app.read_archive = "archive.zar".into();
        assert!(!app.can_find());
        app.start_find();
        assert!(app.find.receiver.is_none());
        app.entries.push(("hidden.txt".into(), EntryData::File(1)));
        assert!(app.visible_entry_indices.is_empty());
        assert!(app.can_find());
        app.busy = true;
        assert!(!app.can_find());
        app.busy = false;
        app.read_archive.clear();
        assert!(!app.can_find());
    }

    #[test]
    fn find_keyboard_shortcuts_search_and_respect_disabled_states() {
        fn frame(context: &egui::Context, app: &mut ZargApp, events: Vec<egui::Event>) {
            context
                .run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000.0, 800.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| app.render_find(ui.ctx()),
                )
                .drop_without_applying_deltas();
        }
        fn key(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
            [true, false]
                .into_iter()
                .map(|pressed| egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers,
                })
                .collect()
        }
        fn find_position(shape: &egui::Shape) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(text) if text.galley.job.text == "Find" && text.pos.y > 40.0 => {
                    Some(text.pos + egui::vec2(3.0, 3.0))
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(find_position),
                _ => None,
            }
        }

        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("archive.zar");
        let file = directory.path().join("file.txt");
        std::fs::write(&file, b"matching text\n").unwrap();
        operations::create_archive(archive.to_str().unwrap(), &[file]).unwrap();
        let mut app = ZargApp::default();
        app.open_archive(archive, None);
        wait(&mut app);
        app.find.open = true;
        let context = egui::Context::default();
        frame(&context, &mut app, Vec::new());
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 800.0),
                )),
                ..Default::default()
            },
            |ui| app.render_find(ui.ctx()),
        );
        let position = output
            .shapes
            .iter()
            .find_map(|shape| find_position(&shape.shape))
            .unwrap();
        output.drop_without_applying_deltas();
        for pressed in [true, false] {
            frame(
                &context,
                &mut app,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(app.find.receiver.is_none());
        for id in ["find-regex", "find-file-filter"] {
            context.memory_mut(|memory| memory.request_focus(egui::Id::unique(id)));
            frame(
                &context,
                &mut app,
                key(egui::Key::Enter, egui::Modifiers::NONE),
            );
            assert!(app.find.receiver.is_none());
        }
        app.find.regex = false;
        frame(&context, &mut app, key(egui::Key::F, egui::Modifiers::ALT));
        assert!(app.find.receiver.is_none());
        app.start_find();
        assert!(app.find.receiver.is_none());
        app.find.pattern = " ".into();
        frame(&context, &mut app, key(egui::Key::F, egui::Modifiers::ALT));
        assert!(app.find.receiver.is_some());
        wait(&mut app);
        assert_eq!(
            app.find.result.as_ref().unwrap().as_ref().unwrap().files,
            ["file.txt"]
        );
        app.find.regex = true;
        app.find.pattern = "matching".into();
        context.memory_mut(|memory| {
            memory.surrender_focus(egui::Id::unique("find-regex"));
            memory.surrender_focus(egui::Id::unique("find-file-filter"));
        });

        frame(
            &context,
            &mut app,
            key(egui::Key::Enter, egui::Modifiers::NONE),
        );
        assert!(app.find.receiver.is_none());
        frame(
            &context,
            &mut app,
            key(egui::Key::F, egui::Modifiers::ALT | egui::Modifiers::CTRL),
        );
        assert!(app.find.receiver.is_none());

        context.memory_mut(|memory| memory.request_focus(egui::Id::unique("find-regex")));
        frame(
            &context,
            &mut app,
            key(egui::Key::Enter, egui::Modifiers::NONE),
        );
        assert!(app.find.receiver.is_some());
        wait(&mut app);
        assert_eq!(
            app.find.result.as_ref().unwrap().as_ref().unwrap().files,
            ["file.txt"]
        );

        context.memory_mut(|memory| memory.request_focus(egui::Id::unique("find-file-filter")));
        frame(
            &context,
            &mut app,
            key(egui::Key::Enter, egui::Modifiers::NONE),
        );
        assert!(app.find.receiver.is_some());
        wait(&mut app);

        app.operation.kind = Some(OperationKind::NewArchive);
        context.memory_mut(|memory| memory.request_focus(egui::Id::unique("find-file-filter")));
        frame(
            &context,
            &mut app,
            key(egui::Key::Enter, egui::Modifiers::NONE),
        );
        assert!(app.find.receiver.is_none());
        frame(&context, &mut app, key(egui::Key::F, egui::Modifiers::ALT));
        assert!(app.find.receiver.is_none());
        app.operation.kind = None;
        frame(&context, &mut app, key(egui::Key::F, egui::Modifiers::ALT));
        assert!(app.find.receiver.is_some());
        wait(&mut app);

        let (sender, receiver) = mpsc::channel();
        app.find.receiver = Some(receiver);
        let cancellation = Arc::new(AtomicBool::new(false));
        app.find.cancellation = Some(Arc::clone(&cancellation));
        frame(&context, &mut app, key(egui::Key::F, egui::Modifiers::ALT));
        frame(&context, &mut app, key(egui::Key::C, egui::Modifiers::ALT));
        assert!(app.find.open);
        assert!(cancellation.load(Ordering::Relaxed));
        sender.send(Ok(FindResults::default())).unwrap();
        app.poll_find(&context);
        assert!(app.find.cancelled);
        frame(&context, &mut app, key(egui::Key::C, egui::Modifiers::ALT));
        assert!(!app.find.open);
    }

    #[test]
    fn find_dialog_stays_open_during_search_and_closes_afterwards() {
        let context = egui::Context::default();
        let mut app = ZargApp::default();
        app.find.open = true;
        let (sender, receiver) = mpsc::channel();
        app.find.receiver = Some(receiver);
        let input = |close: bool| {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 800.0),
                )),
                ..Default::default()
            };
            if close {
                input
                    .viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .events
                    .push(egui::ViewportEvent::Close);
            }
            input
        };
        context
            .run_ui(input(true), |ui| app.render_find(ui.ctx()))
            .drop_without_applying_deltas();
        assert!(app.find.open);
        sender.send(Ok(FindResults::default())).unwrap();
        app.poll_find(&context);
        context
            .run_ui(input(false), |ui| app.render_find(ui.ctx()))
            .drop_without_applying_deltas();
        context
            .run_ui(input(true), |ui| app.render_find(ui.ctx()))
            .drop_without_applying_deltas();
        assert!(!app.find.open);
    }
}
