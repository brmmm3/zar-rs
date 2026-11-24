use super::{FilterSet, PasswordSet, PreparedExtraction};
use crate::{ArchivePath, TocEntry};
use fancy_regex::{BytesMode, Regex, RegexBuilder};
#[cfg(not(target_os = "wasi"))]
use rayon::prelude::*;
use std::collections::VecDeque;
use std::io::{self, Write};
use std::path::Path;

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
    let regex = RegexBuilder::new(pattern)
        .bytes_mode(BytesMode::UnicodeBytes)
        .build()?;
    let filters =
        FilterSet::from_patterns(&[file_filter.to_owned()], &[], &[], &[], &[] as &[&Path])?;
    let default_passwords = PasswordSet::default();
    let prepared = PreparedExtraction::new_complete_filtered(
        archive,
        options.to,
        options.passwords.unwrap_or(&default_passwords),
        &filters,
    )?;
    let paths: Vec<_> = prepared
        .toc()
        .iter()
        .filter_map(|(path, entry)| {
            (matches!(entry, TocEntry::File(_)) && filters.includes(path.as_str(), false))
                .then_some(path)
        })
        .collect();
    let search = |path: &&ArchivePath| -> Result<Option<GrepFile>, String> {
        let mut writer = SearchWriter::new(&regex, options.before, options.after);
        prepared
            .write_file_to(path, &mut writer)
            .map_err(|error| format!("cannot search {path}: {error}"))?;
        writer
            .finish()
            .map_err(|error| format!("cannot search {path}: {error}"))?;
        Ok((!writer.lines.is_empty()).then(|| GrepFile {
            path: (*path).clone(),
            lines: writer.lines,
        }))
    };
    #[cfg(not(target_os = "wasi"))]
    let results: Vec<_> = paths.par_iter().map(search).collect();
    #[cfg(target_os = "wasi")]
    let results: Vec<_> = paths.iter().map(search).collect();
    results
        .into_iter()
        .filter_map(|result| result.transpose())
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

struct SearchWriter<'a> {
    regex: &'a Regex,
    before: usize,
    after: usize,
    remaining_after: usize,
    line_number: u64,
    pending: Vec<u8>,
    previous: VecDeque<GrepLine>,
    lines: Vec<GrepLine>,
}

impl<'a> SearchWriter<'a> {
    fn new(regex: &'a Regex, before: usize, after: usize) -> Self {
        Self {
            regex,
            before,
            after,
            remaining_after: 0,
            line_number: 0,
            pending: Vec::new(),
            previous: VecDeque::new(),
            lines: Vec::new(),
        }
    }

    fn line(&mut self, crlf: bool) -> io::Result<()> {
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
            self.lines.extend(self.previous.drain(..));
            self.lines.push(line);
            self.remaining_after = self.after;
        } else if self.remaining_after > 0 {
            self.lines.push(line);
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
}

impl Write for SearchWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
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
        Ok(())
    }
}
