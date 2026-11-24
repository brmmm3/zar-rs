use std::collections::BTreeSet;
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;
use zarlib::ArchivePath;
use zarlib::operations::{FilterSet, GrepOptions, PasswordSet, PreparedGrep, Progress};

enum Command<'a> {
    Find(&'a str),
    Grep(&'a str, &'a str),
    Before(usize),
    After(usize),
    Chain(bool),
    Help,
    Quit,
}

fn parse_command(line: &str) -> Result<Option<Command<'_>>, &'static str> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(None);
    }
    let (command, args) = line
        .split_once(char::is_whitespace)
        .map_or((line, ""), |(command, args)| (command, args.trim_start()));
    match command {
        "/help" if args.is_empty() => Ok(Some(Command::Help)),
        "/quit" if args.is_empty() => Ok(Some(Command::Quit)),
        "/before" | "/after" => {
            let count = args
                .parse::<usize>()
                .map_err(|_| "expected a nonnegative context line count")?;
            Ok(Some(if command == "/before" {
                Command::Before(count)
            } else {
                Command::After(count)
            }))
        }
        "/chain" => match args {
            "0" => Ok(Some(Command::Chain(false))),
            "1" => Ok(Some(Command::Chain(true))),
            _ => Err("usage: /chain <0|1>"),
        },
        "/find" | "/grep" => {
            let (filter, rest) =
                if let Some(quote) = args.chars().next().filter(|&ch| ch == '\'' || ch == '"') {
                    let remainder = &args[quote.len_utf8()..];
                    let end = remainder.find(quote).ok_or("unterminated quoted fnmatch")?;
                    let rest = &remainder[end + quote.len_utf8()..];
                    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
                        return Err("expected whitespace after quoted fnmatch");
                    }
                    (&remainder[..end], rest.trim_start())
                } else {
                    args.split_once(char::is_whitespace)
                        .map_or((args, ""), |(filter, rest)| (filter, rest.trim_start()))
                };
            if filter.is_empty() {
                return Err("expected a fnmatch pattern");
            }
            match command {
                "/find" if rest.is_empty() => Ok(Some(Command::Find(filter))),
                "/grep" if !rest.is_empty() => Ok(Some(Command::Grep(filter, rest))),
                "/find" => Err("usage: /find <fnmatch>"),
                _ => Err("usage: /grep <fnmatch> <regex>"),
            }
        }
        "/help" => Err("usage: /help"),
        "/quit" => Err("usage: /quit"),
        _ => Err("unknown command; use /help for available commands"),
    }
}

struct SearchCancellation(AtomicBool);

impl Progress for SearchCancellation {
    fn message(&self, _: &str) {}
    fn advance(&self, _: u64) {}
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

struct RawMode;

impl RawMode {
    fn enter() -> io::Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        Ok(Self)
    }

    fn finish(self) -> io::Result<()> {
        let result = crossterm::terminal::disable_raw_mode();
        if result.is_ok() {
            std::mem::forget(self);
        }
        result
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        if let Err(error) = crossterm::terminal::disable_raw_mode() {
            eprintln!("zarc grep: cannot restore terminal: {error}");
        }
    }
}

fn search(
    prepared: &PreparedGrep,
    filter: &str,
    pattern: &str,
    before: usize,
    after: usize,
    previous: Option<&BTreeSet<ArchivePath>>,
) -> Result<Option<BTreeSet<ArchivePath>>, Box<dyn std::error::Error>> {
    let mut filters =
        FilterSet::from_patterns(&[filter.to_owned()], &[], &[], &[], &[] as &[&Path])?;
    if let Some(previous) = previous {
        filters.restrict_to_paths(previous.iter().cloned());
    }
    let cancellation = SearchCancellation(AtomicBool::new(false));
    let matches = Mutex::new(BTreeSet::new());
    let terminal = io::stdin().is_terminal();
    let terminal_raw_output = terminal && io::stdout().is_terminal();
    let run = || {
        prepared
            .search_with_filters(
                &filters,
                pattern,
                before,
                after,
                Some(&cancellation),
                &|path, line| {
                    if line.is_match {
                        matches
                            .lock()
                            .map_err(|_| io::Error::other("search matches lock poisoned"))?
                            .insert(path.clone());
                    }
                    super::write_grep_line(
                        &mut io::stdout().lock(),
                        path,
                        &line,
                        terminal_raw_output,
                    )
                },
            )
            .map_err(|error| error.to_string())
    };
    let result = if terminal {
        let raw = RawMode::enter()?;
        let result = std::thread::scope(|scope| {
            let worker = scope.spawn(run);
            let input_result = loop {
                if worker.is_finished() {
                    break Ok(());
                }
                match event::poll(Duration::from_millis(25)) {
                    Ok(false) => {}
                    Ok(true) => match event::read() {
                        Ok(Event::Key(key))
                            if key.code == KeyCode::Esc && key.kind != KeyEventKind::Release =>
                        {
                            cancellation.0.store(true, Ordering::Relaxed);
                            break Ok(());
                        }
                        Ok(_) => {}
                        Err(error) => break Err(error),
                    },
                    Err(error) => break Err(error),
                }
            };
            if input_result.is_err() {
                cancellation.0.store(true, Ordering::Relaxed);
            }
            let result = worker
                .join()
                .map_err(|_| io::Error::other("search worker panicked"))?;
            input_result?;
            Ok::<_, io::Error>(result)
        });
        raw.finish()?;
        result?
    } else {
        run()
    };
    match result {
        Ok(()) => Ok(Some(
            matches
                .into_inner()
                .map_err(|_| "search matches lock poisoned")?,
        )),
        Err(error) if cancellation.is_cancelled() && error.contains("operation cancelled") => {
            Ok(None)
        }
        Err(error) => Err(error.into()),
    }
}

pub(super) fn run(
    archive: &str,
    passwords: &PasswordSet,
    options: &GrepOptions<'_>,
) -> Result<u8, Box<dyn std::error::Error>> {
    let prepared = PreparedGrep::new(archive, options.to, passwords, None)?;
    let mut editor = DefaultEditor::new()?;
    let mut before = options.before;
    let mut after = options.after;
    let mut chain = false;
    let mut previous = None;
    loop {
        let line = match editor.readline("zarc grep> ") {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(error) => return Err(error.into()),
        };
        if !line.trim().is_empty() {
            editor.add_history_entry(line.as_str())?;
        }
        let command = match parse_command(&line) {
            Ok(Some(command)) => command,
            Ok(None) => continue,
            Err(error) => {
                eprintln!("zarc grep: {error}");
                continue;
            }
        };
        match command {
            Command::Help => {
                let stdout = io::stdout();
                let mut output = stdout.lock();
                writeln!(
                    output,
                    "/find <fnmatch>         List matching archive paths"
                )?;
                writeln!(output, "/grep <fnmatch> <regex> Search matching files")?;
                writeln!(output, "/before <cnt>           Set before-context lines")?;
                writeln!(output, "/after <cnt>            Set after-context lines")?;
                writeln!(
                    output,
                    "/chain <0|1>            Disable or start a new grep chain"
                )?;
                writeln!(output, "/help                   Show this help")?;
                writeln!(output, "/quit                   Exit the shell")?;
                writeln!(output, "Press Esc to cancel a running grep.")?;
                output.flush()?;
            }
            Command::Quit => break,
            Command::Before(count) => before = count,
            Command::After(count) => after = count,
            Command::Chain(enabled) => {
                chain = enabled;
                previous = None;
            }
            Command::Find(filter) => {
                let result = (|| {
                    let filters = FilterSet::from_patterns(
                        &[filter.to_owned()],
                        &[],
                        &[],
                        &[],
                        &[] as &[&Path],
                    )?;
                    prepared.check_unchanged()?;
                    Ok::<_, Box<dyn std::error::Error>>(filters)
                })();
                match result {
                    Ok(filters) => {
                        let stdout = io::stdout();
                        let mut output = stdout.lock();
                        for (path, _) in prepared.toc().iter() {
                            if filters.includes(path.as_str(), false) {
                                writeln!(output, "{path}")?;
                            }
                        }
                        output.flush()?;
                    }
                    Err(error) => eprintln!("zarc grep: {error}"),
                }
            }
            Command::Grep(filter, pattern) => {
                match search(&prepared, filter, pattern, before, after, previous.as_ref()) {
                    Ok(Some(matches)) if chain => previous = Some(matches),
                    Ok(Some(_)) => {}
                    Ok(None) => eprintln!("zarc grep: search cancelled"),
                    Err(error) => eprintln!("zarc grep: {error}"),
                }
            }
        }
    }
    Ok(0)
}
