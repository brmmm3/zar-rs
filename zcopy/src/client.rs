//! Sending side: compares trees and pushes missing or newer files.

use std::fs::{self, File};
use std::io::{BufRead, BufReader, IsTerminal, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::sync_channel;
use std::time::{SystemTime, UNIX_EPOCH};

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use zarlib::EncryptionScope;
use zarlib::operations::{
    CreateFilterOptions, CreateInputOptions, EncodingOptions, FilterSet,
    create_archive_with_password_progress_and_filters_and_encryption,
};

use crate::config::{Settings, parse_aead};
use crate::net::*;
use crate::scan::{self, Entry, Tree};
use crate::server::{TEMP_DIR, read_hello};

/// Archive paths are relative to the process directory, so runs are serialized.
static CWD_LOCK: Mutex<()> = Mutex::new(());
const MAX_BATCH_FILES: usize = 100_000;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub files: u64,
    pub source_bytes: u64,
    pub sent_bytes: u64,
    pub skipped: u64,
}

/// Front-end hooks for the client.
pub struct Ui<'a> {
    pub progress: bool,
    /// Called with the number of other connected clients; false cancels.
    pub confirm: &'a dyn Fn(u32) -> bool,
}

/// Warns about other connected clients and asks whether to continue.
pub fn ask_continue(others: u32, assume_yes: bool) -> bool {
    eprintln!("zcopy: warning: {others} other client(s) are already connected to this server");
    if assume_yes {
        return true;
    }
    if !std::io::stdin().is_terminal() {
        eprintln!("zcopy: not a terminal; use --yes to continue anyway");
        return false;
    }
    eprint!("Continue anyway? [y/N] ");
    let _ = std::io::stderr().flush();
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer).is_ok()
        && matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

fn make_bar(total: u64, enabled: bool) -> ProgressBar {
    let target = if enabled {
        ProgressDrawTarget::stderr()
    } else {
        ProgressDrawTarget::hidden()
    };
    let bar = ProgressBar::with_draw_target(Some(total), target);
    bar.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] [{bar:40.cyan}] {binary_bytes}/{binary_total_bytes} ({eta}) {msg}",
        )
        .expect("static progress template is valid")
        .progress_chars("##>-"),
    );
    bar
}

struct Batch {
    archive: PathBuf,
    source_bytes: u64,
    items: Vec<(String, i64, bool)>,
    bytes: u64,
}

pub fn run(
    host: &str,
    source: &Path,
    settings: &Settings,
    temp: Option<&Path>,
    ui: &Ui,
) -> Res<Stats> {
    let _cwd = CWD_LOCK.lock().unwrap();
    let origin = std::env::current_dir()?;
    let source = source
        .canonicalize()
        .map_err(|error| format!("cannot open source {}: {error}", source.display()))?;
    // A named folder is sent as that folder (`foo/...`); a filesystem root has
    // no name, so its contents go directly into the server folder.
    let (base, prefix) = match (source.parent(), source.file_name()) {
        (Some(parent), Some(name)) => (parent.to_path_buf(), name.to_string_lossy().into_owned()),
        _ => (source.clone(), String::new()),
    };
    std::env::set_current_dir(&base)
        .map_err(|error| format!("cannot open source {}: {error}", source.display()))?;
    let work = match temp {
        Some(dir) => dir.to_path_buf(),
        None => std::env::temp_dir().join(format!(
            "zcopy-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        )),
    };
    let result = fs::create_dir_all(&work)
        .map_err(Into::into)
        .and_then(|()| transfer(host, &work, settings, ui, &source, &prefix));
    let _ = fs::remove_dir_all(&work);
    let _ = std::env::set_current_dir(origin);
    result
}

fn transfer(
    host: &str,
    work: &Path,
    settings: &Settings,
    ui: &Ui,
    source: &Path,
    prefix: &str,
) -> Res<Stats> {
    let has_port = if host.starts_with('[') {
        host.contains("]:")
    } else {
        host.matches(':').count() == 1
    };
    let address = match (
        has_port,
        host.matches(':').count() > 1 && !host.starts_with('['),
    ) {
        (true, _) => host.to_owned(),
        (false, true) => format!("[{host}]:{}", settings.port()),
        (false, false) => format!("{host}:{}", settings.port()),
    };
    let mut stream = TcpStream::connect(&address)
        .map_err(|error| format!("cannot connect to {address}: {error}"))?;
    stream.set_nodelay(true)?;

    let password = settings.password.as_deref().map(str::as_bytes);
    let (server_wants_password, salt) = read_hello(&mut stream)?;
    if server_wants_password != password.is_some() {
        return Err(if server_wants_password {
            "server requires a password"
        } else {
            "server does not use a password but one was given"
        }
        .into());
    }
    let key = password.map(|pw| derive_key(pw, &salt)).transpose()?;
    let mut writer = FrameWriter::new(stream.try_clone()?, key, DIR_CLIENT_TO_SERVER);
    let mut reader = FrameReader::new(stream.try_clone()?, key, DIR_SERVER_TO_CLIENT);
    writer.send_msg(&Msg::Auth)?;
    let Msg::Manifest {
        aead: server_aead,
        level: server_level,
        active_clients,
        metadata: server_metadata,
        ..
    } = reader.recv_msg()?
    else {
        return Err("unexpected reply from server".into());
    };
    let remote: Vec<Entry> = reader.recv_compressed()?;
    let remote: Tree = remote
        .into_iter()
        .map(|(path, size, mtime, dir)| (path, scan::Meta { size, mtime, dir }))
        .collect();

    if active_clients > 0 && !(ui.confirm)(active_clients) {
        let _ = writer.send_msg(&Msg::Proceed {
            go: false,
            source: String::new(),
        });
        return Err("cancelled: server has other clients connected".into());
    }
    let source_dir = source.display().to_string();
    writer.send_msg(&Msg::Proceed {
        go: true,
        source: source_dir,
    })?;
    let wants_metadata = settings.metadata.unwrap_or(false);
    if wants_metadata && !server_metadata {
        eprintln!("zcopy: warning: server does not accept metadata; sending files only");
    }
    let metadata = wants_metadata && server_metadata;

    let aead = parse_aead(settings.aead.as_deref().unwrap_or(&server_aead))?;
    let level = settings.level.unwrap_or(server_level);
    let local = scan::prefixed(scan::scan_tree(source, &[TEMP_DIR])?, prefix);
    let plan = scan::plan_transfer(&local, &remote);
    let mut stats = Stats {
        skipped: plan.skipped,
        ..Stats::default()
    };

    let batch_limit = settings.batch_mib.unwrap_or(512).max(1) << 20;
    let mut batches: Vec<Vec<String>> = Vec::new();
    let mut batch_sizes: Vec<u64> = Vec::new();
    let (mut current, mut current_bytes) = (Vec::new(), 0u64);
    for path in plan.files.into_iter().chain(plan.dirs) {
        let size = local[&path].size;
        stats.source_bytes += size;
        current_bytes += size;
        current.push(path);
        if current_bytes >= batch_limit || current.len() >= MAX_BATCH_FILES {
            batches.push(std::mem::take(&mut current));
            batch_sizes.push(current_bytes);
            current_bytes = 0;
        }
    }
    if !current.is_empty() {
        batches.push(current);
        batch_sizes.push(current_bytes);
    }

    let bar = make_bar(stats.source_bytes, ui.progress);
    let password_owned = password.map(<[u8]>::to_vec);
    let work = work.to_path_buf();
    let items_of = |paths: &[String]| -> Vec<(String, i64, bool)> {
        paths
            .iter()
            .map(|p| (p.clone(), local[p].mtime, local[p].dir))
            .collect()
    };
    let prepared: Vec<Vec<(String, i64, bool)>> = batches.iter().map(|b| items_of(b)).collect();

    let (queue, ready) = sync_channel::<Result<Batch, String>>(1);
    let producer = std::thread::spawn(move || {
        for (number, ((paths, items), source_bytes)) in batches
            .into_iter()
            .zip(prepared)
            .zip(batch_sizes)
            .enumerate()
        {
            let archive = work.join(format!("batch-{number}.zar"));
            let inputs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
            let created = create_archive_with_password_progress_and_filters_and_encryption(
                &archive.to_string_lossy(),
                &inputs,
                false,
                EncodingOptions {
                    compression_level: level,
                    ..EncodingOptions::default()
                },
                password_owned
                    .as_deref()
                    .map(|pw| (pw, aead, EncryptionScope::HeaderAndData)),
                None,
                CreateFilterOptions {
                    input: CreateInputOptions {
                        save_metadata: metadata,
                        save_dates: false,
                        include_special_files: false,
                        overwrite_existing: true,
                        verbose: false,
                    },
                    filters: &FilterSet::default(),
                },
            );
            let message = match created {
                Ok(_) => fs::metadata(&archive)
                    .map(|meta| Batch {
                        archive,
                        source_bytes,
                        items,
                        bytes: meta.len(),
                    })
                    .map_err(|error| error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let failed = message.is_err();
            if queue.send(message).is_err() || failed {
                return;
            }
        }
    });

    let ack_reader = std::thread::spawn(move || -> Result<u64, String> {
        let (mut files, mut first_error) = (0, None);
        loop {
            match reader.recv_msg().map_err(|e| e.to_string())? {
                Msg::Ack {
                    ok: true, files: n, ..
                } => files += n,
                Msg::Ack { message, .. } => {
                    first_error.get_or_insert(message);
                }
                Msg::Done => break,
                _ => return Err("unexpected message from server".into()),
            }
        }
        match first_error {
            Some(message) => Err(message),
            None => Ok(files),
        }
    });

    let mut failure: Option<String> = None;
    let mut wire_sent = 0u64;
    let started = std::time::Instant::now();
    for batch in ready {
        let batch = match batch {
            Ok(batch) => batch,
            Err(message) => {
                failure = Some(message);
                break;
            }
        };
        let sent = (|| -> Res<()> {
            writer.send_msg(&Msg::Batch {
                bytes: batch.bytes,
                files: batch.items.len() as u64,
                metadata,
            })?;
            writer.send_compressed(&batch.items, 3)?;
            let mut sent_in_batch = 0u64;
            let mut credited = 0u64;
            writer.send_raw(
                &mut BufReader::new(File::open(&batch.archive)?),
                batch.bytes,
                &mut |chunk| {
                    sent_in_batch += chunk;
                    wire_sent += chunk;
                    // Progress is shown in source bytes, scaled by the share of
                    // this batch already on the wire.
                    let target = (batch.source_bytes as u128 * sent_in_batch as u128
                        / batch.bytes.max(1) as u128) as u64;
                    bar.inc(target - credited);
                    credited = target;
                    let seconds = started.elapsed().as_secs_f64().max(0.001);
                    bar.set_message(format!(
                        "sent {} at {}/s",
                        indicatif::BinaryBytes(wire_sent),
                        indicatif::BinaryBytes((wire_sent as f64 / seconds) as u64)
                    ));
                },
            )
        })();
        let _ = fs::remove_file(&batch.archive);
        if let Err(error) = sent {
            failure = Some(error.to_string());
            break;
        }
        stats.sent_bytes += batch.bytes;
    }
    let _ = producer.join();
    if failure.is_some() {
        bar.abandon();
    } else {
        bar.finish();
    }
    let finished = writer.send_msg(&Msg::Done);
    let acked = ack_reader.join().map_err(|_| "ack thread panicked")?;
    if let Some(message) = failure {
        return Err(message.into());
    }
    finished?;
    let files = acked?;
    stats.files = files;
    Ok(stats)
}
