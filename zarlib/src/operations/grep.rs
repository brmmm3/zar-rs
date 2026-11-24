use super::{FilterSet, PasswordSet, PreparedGrep, Progress, ProgressReporter};
use crate::ArchivePath;
use fancy_regex::{BytesMode, Regex, RegexBuilder};
#[cfg(not(target_os = "wasi"))]
use rayon::prelude::*;
use std::collections::{BTreeMap, VecDeque};
use std::io::{self, Write};
use std::path::Path;
use std::sync::Mutex;

/// Search options. By default, the latest archive state is searched without
/// passwords or surrounding context.
#[derive(Default)]
pub struct GrepOptions<'a> {
    pub before: usize,
    pub after: usize,
    pub to: Option<&'a str>,
    pub passwords: Option<&'a PasswordSet>,
}

/// A matching or context line, without its line terminator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepLine {
    /// One-based line number.
    pub line_number: u64,
    pub bytes: Vec<u8>,
    pub is_match: bool,
}

/// Search results for one regular file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepFile {
    pub path: ArchivePath,
    pub lines: Vec<GrepLine>,
}

/// Searches regular files selected by an archive-relative glob for a line regex.
///
/// Results are ordered by path and then line number. Overlapping context is
/// returned once. Lines are byte-preserving, except that LF and CRLF line
/// terminators are removed. A trailing newline does not create an extra line.
/// The byte regex uses `fancy_regex` syntax, including lookaround and
/// backreferences. Matching is Unicode-aware even on non-UTF-8 input; invalid
/// bytes cannot be matched individually. Inline Unicode mode changes such as
/// `(?-u:...)` are not supported. Matching can fail if the regex engine's
/// backtracking limit is exceeded.
/// Selected files are fully read and checked, including files with no matches.
/// File contents are searched in memory as archive blocks are decoded; no
/// temporary files are created. A line spanning blocks is buffered until its
/// terminator so that the regex sees the complete line.
/// The archive must not change during the search. Native targets search files
/// in parallel; WASI searches sequentially.
pub fn grep_archive(
    archive: &str,
    file_filter: &str,
    pattern: &str,
    options: GrepOptions<'_>,
) -> Result<Vec<GrepFile>, Box<dyn std::error::Error>> {
    grep_archive_with_progress(archive, file_filter, pattern, options, None)
}

/// Searches an archive like [`grep_archive`], checking for cancellation during
/// archive preparation and while scanning file contents.
pub fn grep_archive_with_progress(
    archive: &str,
    file_filter: &str,
    pattern: &str,
    options: GrepOptions<'_>,
    progress: Option<&dyn Progress>,
) -> Result<Vec<GrepFile>, Box<dyn std::error::Error>> {
    let progress = ProgressReporter::new(progress);
    progress.check_cancelled()?;
    RegexBuilder::new(pattern)
        .bytes_mode(BytesMode::UnicodeBytes)
        .build()?;
    let default_passwords = PasswordSet::default();
    let prepared = PreparedGrep::new(
        archive,
        options.to,
        options.passwords.unwrap_or(&default_passwords),
        progress.0,
    )?;
    let results = Mutex::new(BTreeMap::<ArchivePath, Vec<GrepLine>>::new());
    prepared.search(
        file_filter,
        pattern,
        options.before,
        options.after,
        progress.0,
        &|path, line| {
            results
                .lock()
                .map_err(|_| io::Error::other("grep results lock poisoned"))?
                .entry(path.clone())
                .or_default()
                .push(line);
            Ok(())
        },
    )?;
    let results = results
        .into_inner()
        .map_err(|_| "grep results lock poisoned")?;
    Ok(results
        .into_iter()
        .map(|(path, lines)| GrepFile { path, lines })
        .collect())
}

impl PreparedGrep {
    /// Searches matching files in parallel and delivers each line immediately.
    ///
    /// Calls can occur concurrently, and lines from different files may be
    /// interleaved. Lines within each file are delivered in order. A later
    /// cancellation, output error, or integrity failure may follow partial output.
    pub fn search(
        &self,
        file_filter: &str,
        pattern: &str,
        before: usize,
        after: usize,
        progress: Option<&dyn Progress>,
        on_line: &(impl Fn(&ArchivePath, GrepLine) -> io::Result<()> + Sync),
    ) -> Result<(), Box<dyn std::error::Error>> {
        let filters =
            FilterSet::from_patterns(&[file_filter.to_owned()], &[], &[], &[], &[] as &[&Path])?;
        self.search_with_filters(&filters, pattern, before, after, progress, on_line)
    }

    /// Searches using a filter set, including any exact-path restrictions.
    /// Delivery, ordering, and partial-output behavior are the same as [`Self::search`].
    pub fn search_with_filters(
        &self,
        filters: &FilterSet,
        pattern: &str,
        before: usize,
        after: usize,
        progress: Option<&dyn Progress>,
        on_line: &(impl Fn(&ArchivePath, GrepLine) -> io::Result<()> + Sync),
    ) -> Result<(), Box<dyn std::error::Error>> {
        let progress = ProgressReporter::new(progress);
        progress.check_cancelled()?;
        let regex = RegexBuilder::new(pattern)
            .bytes_mode(BytesMode::UnicodeBytes)
            .build()?;
        let sources = self.selected_sources(filters, &progress)?;
        let paths: Vec<_> = sources.iter().collect();
        let search = |reader: &mut Result<_, String>,
                      (path, source): &(&ArchivePath, &super::ExtractedSource)|
         -> Result<(), String> {
            progress
                .check_cancelled()
                .map_err(|error| error.to_string())?;
            let reader = reader.as_mut().map_err(|error| error.clone())?;
            let mut writer = SearchWriter::new(&regex, before, after, &progress, path, on_line);
            self.write_source(reader, path, source, &mut writer)
                .map_err(|error| format!("cannot search {path}: {error}"))?;
            writer
                .finish()
                .map_err(|error| format!("cannot search {path}: {error}"))
        };
        #[cfg(not(target_os = "wasi"))]
        if !paths.is_empty() {
            let workers =
                super::compression_worker_count(super::CREATE_BLOCK_SIZE).min(paths.len());
            let pool = super::worker_priority::worker_pool(
                workers,
                "zarc-grep",
                super::worker_priority::worker_priority(),
            )?;
            pool.install(|| {
                paths
                    .par_iter()
                    .map_init(
                        || self.open_reader().map_err(|error| error.to_string()),
                        search,
                    )
                    .collect::<Result<Vec<_>, _>>()
            })?;
        }
        #[cfg(target_os = "wasi")]
        {
            let mut reader = self.open_reader().map_err(|error| error.to_string());
            paths
                .iter()
                .try_for_each(|item| search(&mut reader, item))?;
        }
        progress.check_cancelled()?;
        self.check_unchanged()?;
        Ok(())
    }
}

struct SearchWriter<'a, F> {
    regex: &'a Regex,
    progress: &'a ProgressReporter<'a>,
    path: &'a ArchivePath,
    on_line: &'a F,
    before: usize,
    after: usize,
    remaining_after: usize,
    line_number: u64,
    pending: Vec<u8>,
    previous: VecDeque<GrepLine>,
}

impl<'a, F: Fn(&ArchivePath, GrepLine) -> io::Result<()>> SearchWriter<'a, F> {
    fn new(
        regex: &'a Regex,
        before: usize,
        after: usize,
        progress: &'a ProgressReporter<'a>,
        path: &'a ArchivePath,
        on_line: &'a F,
    ) -> Self {
        Self {
            regex,
            progress,
            path,
            on_line,
            before,
            after,
            remaining_after: 0,
            line_number: 0,
            pending: Vec::new(),
            previous: VecDeque::new(),
        }
    }

    fn line(&mut self, crlf: bool) -> io::Result<()> {
        self.check_cancelled()?;
        if crlf && self.pending.last() == Some(&b'\r') {
            self.pending.pop();
        }
        self.line_number = self
            .line_number
            .checked_add(1)
            .ok_or_else(|| io::Error::other("line number overflow"))?;
        let bytes = std::mem::take(&mut self.pending);
        let is_match = self.regex.is_match(bytes.as_slice()).map_err(|error| {
            io::Error::other(format!(
                "regex match failed on line {}: {error}",
                self.line_number
            ))
        })?;
        let line = GrepLine {
            line_number: self.line_number,
            bytes,
            is_match,
        };
        if is_match {
            for previous in self.previous.drain(..) {
                (self.on_line)(self.path, previous)?;
            }
            (self.on_line)(self.path, line)?;
            self.remaining_after = self.after;
        } else if self.remaining_after > 0 {
            (self.on_line)(self.path, line)?;
            self.remaining_after -= 1;
        } else if self.before > 0 {
            if self.previous.len() == self.before {
                self.previous.pop_front();
            }
            self.previous.push_back(line);
        }
        Ok(())
    }

    fn finish(&mut self) -> io::Result<()> {
        if !self.pending.is_empty() {
            self.line(false)?;
        }
        Ok(())
    }

    fn check_cancelled(&self) -> io::Result<()> {
        if self.progress.is_cancelled() {
            // Write::write_all retries Interrupted errors.
            Err(io::Error::other("operation cancelled"))
        } else {
            Ok(())
        }
    }
}

impl<F: Fn(&ArchivePath, GrepLine) -> io::Result<()>> Write for SearchWriter<'_, F> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.check_cancelled()?;
        for part in buf.split_inclusive(|byte| *byte == b'\n') {
            if let Some(line) = part.strip_suffix(b"\n") {
                self.pending.extend_from_slice(line);
                self.line(true)?;
            } else {
                self.pending.extend_from_slice(part);
            }
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.check_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CancelDuringLines(AtomicUsize);

    impl Progress for CancelDuringLines {
        fn message(&self, _: &str) {}
        fn advance(&self, _: u64) {}
        fn is_cancelled(&self) -> bool {
            self.0.fetch_add(1, Ordering::Relaxed) >= 2
        }
    }

    #[test]
    fn cancellation_interrupts_write_all_while_scanning_lines() {
        let progress = CancelDuringLines(AtomicUsize::new(0));
        let reporter = ProgressReporter::new(Some(&progress));
        let regex = Regex::new("match").unwrap();
        let path = ArchivePath::new("file").unwrap();
        let lines = Mutex::new(Vec::new());
        let on_line = |_: &ArchivePath, line| {
            lines.lock().unwrap().push(line);
            Ok(())
        };
        let mut writer = SearchWriter::new(&regex, 0, 0, &reporter, &path, &on_line);
        let error = writer.write_all(b"match\nmatch\nmatch\n").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(error.to_string().contains("operation cancelled"));
        assert_eq!(lines.lock().unwrap().len(), 1);
    }
}
