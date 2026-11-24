use super::super::*;
use fancy_regex::{BytesMode, Regex, RegexBuilder};
use std::ops::Range;

const MAX_LINE_CHARS: usize = 250;

type FindResult = Result<FindResults, String>;

#[derive(Debug, Default)]
struct FindResults {
    files: Vec<String>,
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

impl FindResults {
    fn new(
        files: Vec<operations::GrepFile>,
        pattern: &str,
        include_context: bool,
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
    before: usize,
    after: usize,
    receiver: Option<Receiver<FindResult>>,
    result: Option<FindResult>,
    selected_row: Option<usize>,
}

impl Default for FindState {
    fn default() -> Self {
        Self {
            open: false,
            file_filter: "*".to_owned(),
            pattern: String::new(),
            before: 0,
            after: 0,
            receiver: None,
            result: None,
            selected_row: None,
        }
    }
}

impl FindState {
    pub(in crate::ui) fn clear_results(&mut self) {
        self.result = None;
        self.selected_row = None;
    }
}

impl ZargApp {
    fn start_find(&mut self) {
        if !self.can_find() || self.operation.kind.is_some() {
            return;
        }
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
        let pattern = self.find.pattern.clone();
        let before = self.find.before;
        let after = self.find.after;
        let (sender, receiver) = mpsc::channel();
        let (events_tx, events_rx) = mpsc::channel();
        let (answers, _answer_rx) = mpsc::channel();
        self.find.receiver = Some(receiver);
        self.find.result = None;
        self.find.selected_row = None;
        self.begin_worker(events_rx, answers, "Searching archive");
        self.status = "Searching archive...".to_owned();
        std::thread::spawn(move || {
            let result = (|| -> Result<_, Box<dyn std::error::Error>> {
                let view = if let Some(overlay) = overlay {
                    let directory = tempfile::Builder::new()
                        .prefix("zarg-find-view-")
                        .tempdir()?;
                    operations::combine_archives(
                        Path::new(&archive),
                        &overlay,
                        &directory.path().join("view.zar"),
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
                let files = operations::grep_archive(
                    source,
                    &file_filter,
                    &pattern,
                    operations::GrepOptions {
                        before,
                        after,
                        to: commit.as_deref(),
                        passwords: Some(&passwords),
                    },
                )?;
                FindResults::new(files, &pattern, before > 0 || after > 0).map_err(Into::into)
            })()
            .map_err(|error| error.to_string());
            let status = if result.is_ok() {
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
        match receiver.try_recv() {
            Ok(result) => {
                self.find.receiver = None;
                self.find.result = Some(result);
            }
            Err(mpsc::TryRecvError::Empty) => {
                context.request_repaint_after(Duration::from_millis(100));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.find.receiver = None;
                self.find.result = Some(Err("Search worker stopped unexpectedly.".to_owned()));
            }
        }
    }

    pub(in crate::ui) fn render_find(&mut self, context: &egui::Context) {
        if !self.find.open {
            return;
        }
        let running = self.find.receiver.is_some();
        let can_search = self.can_find() && self.operation.kind.is_none();
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
                            if close_or_cancel(icon_button(ui, &self.icons, UiIcon::Close, "Close", !running), true) {
                                open = false;
                            }
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                let status = if running {
                                    "Searching...".to_owned()
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
                                    ui.add_sized([ui.available_width(), ui.spacing().interact_size.y], egui::TextEdit::singleline(&mut state.file_filter));
                                });
                            });
                            ui.horizontal(|ui| {
                                ui.label("Regex");
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
                        if running {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Searching archive...");
                            });
                        } else if let Some(result) = &state.result {
                            match result {
                                Ok(results) if results.rows.is_empty() => {
                                    ui.label("No matches found.");
                                }
                                Ok(results) => {
                                    ui.label(format!("{} matching file(s).", results.files.len()));
                                    if results.invalid_utf8 {
                                        ui.weak("Invalid UTF-8 bytes are shown as replacement characters.");
                                    }
                                    open_path = render_find_results(ui, results, &mut state.selected_row, can_search);
                                }
                                Err(error) => {
                                    ui.colored_label(ui.visuals().error_fg_color, error);
                                }
                            }
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
        let grouped = FindResults::new(files.clone(), "match", true).unwrap();
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

        let ungrouped = FindResults::new(files, "match", false).unwrap();
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
        app.find.result = Some(Ok(FindResults::new(Vec::new(), "match", true).unwrap()));
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
        app.find.result = Some(Ok(FindResults::new(files, "match", true).unwrap()));
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
        let (_sender, receiver) = mpsc::channel();
        app.find.receiver = Some(receiver);
        assert!(
            frame(&context, &mut app)
                .iter()
                .any(|(text, _)| text == "Searching...")
        );
    }

    #[test]
    fn result_list_virtualizes_and_opens_double_clicked_row() {
        let results = FindResults {
            files: vec!["first.txt".into(), "second.txt".into()],
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
        wait(&mut app);
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

        for (before, after) in [(1, 0), (0, 1)] {
            app.find.before = before;
            app.find.after = after;
            app.start_find();
            wait(&mut app);
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

        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("archive.zar");
        let file = directory.path().join("file.txt");
        std::fs::write(&file, b"matching text\n").unwrap();
        operations::create_archive(archive.to_str().unwrap(), &[file]).unwrap();
        let mut app = ZargApp::default();
        app.open_archive(archive, None);
        wait(&mut app);
        app.find.open = true;
        app.find.pattern = "matching".into();
        let context = egui::Context::default();
        frame(&context, &mut app, Vec::new());

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

        app.operation.kind = Some(OperationKind::NewArchive);
        frame(&context, &mut app, key(egui::Key::F, egui::Modifiers::ALT));
        assert!(app.find.receiver.is_none());
        app.operation.kind = None;
        frame(&context, &mut app, key(egui::Key::F, egui::Modifiers::ALT));
        assert!(app.find.receiver.is_some());
        wait(&mut app);

        let (sender, receiver) = mpsc::channel();
        app.find.receiver = Some(receiver);
        frame(&context, &mut app, key(egui::Key::F, egui::Modifiers::ALT));
        frame(&context, &mut app, key(egui::Key::C, egui::Modifiers::ALT));
        assert!(app.find.open);
        sender.send(Ok(FindResults::default())).unwrap();
        app.poll_find(&context);
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
