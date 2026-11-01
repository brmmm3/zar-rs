//! Receiving side: caches the destination tree and extracts incoming batches.

use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};

use zarlib::operations::{
    CollisionPolicy, ExistingPolicy, ExtractionOptions, FilterSet, PasswordSet,
    extract_archive_with_passwords_progress_and_filters,
};

use crate::config::{Settings, parse_aead};
use crate::net::*;
use crate::scan::{self, Meta, Tree};

pub const TEMP_DIR: &str = ".zcopy-tmp";
static EXTRACT_LOCK: Mutex<()> = Mutex::new(());
static CONNECTIONS: AtomicU64 = AtomicU64::new(0);

type Item = (String, i64, bool);

enum Job {
    Batch {
        archive: PathBuf,
        items: Vec<Item>,
        metadata: bool,
    },
    Done,
}

pub struct Server {
    root: PathBuf,
    settings: Settings,
    cache: Arc<Mutex<Tree>>,
    listener: TcpListener,
    active: AtomicUsize,
}

/// Counts an authenticated connection for as long as it lives.
struct ActiveClient<'a>(&'a AtomicUsize);

impl Drop for ActiveClient<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Server {
    /// Creates the folder if needed, scans it, and starts listening.
    pub fn bind(root: &Path, settings: Settings) -> Res<Self> {
        fs::create_dir_all(root)?;
        let root = root.canonicalize()?;
        let temp = root.join(TEMP_DIR);
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp)?;
        parse_aead(settings.aead.as_deref().unwrap_or("aes-256-gcm"))?;
        let tree = scan::scan_tree(&root, &[TEMP_DIR])?;
        let address = (
            settings.bind.as_deref().unwrap_or("0.0.0.0"),
            settings.port(),
        );
        let listener = TcpListener::bind(address)
            .map_err(|error| format!("cannot listen on {}:{}: {error}", address.0, address.1))?;
        Ok(Self {
            root,
            settings,
            cache: Arc::new(Mutex::new(tree)),
            listener,
            active: AtomicUsize::new(0),
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.listener.local_addr().expect("bound listener")
    }

    pub fn cached_entries(&self) -> usize {
        self.cache.lock().unwrap().len()
    }

    /// Accepts connections forever, one thread each.
    pub fn serve(self) -> Res<()> {
        let shared = Arc::new(self);
        for stream in shared.listener.incoming() {
            let stream = match stream {
                Ok(stream) => stream,
                Err(error) => {
                    eprintln!("zcopy: accept failed: {error}");
                    continue;
                }
            };
            let server = Arc::clone(&shared);
            std::thread::spawn(move || {
                let peer = stream
                    .peer_addr()
                    .map(|a| a.to_string())
                    .unwrap_or_default();
                if let Err(error) = server.handle(stream) {
                    eprintln!("zcopy: {peer}: {error}");
                }
            });
        }
        Ok(())
    }

    fn allowed(&self, peer: IpAddr) -> bool {
        let Some(allow) = &self.settings.allow else {
            return true;
        };
        let peer = match peer {
            IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(peer),
            other => other,
        };
        allow.iter().any(|entry| {
            let entry = entry.trim();
            match entry.parse::<IpAddr>() {
                Ok(ip) => ip == peer,
                Err(_) => (entry, 0)
                    .to_socket_addrs()
                    .map(|mut found| found.any(|addr| addr.ip() == peer))
                    .unwrap_or(false),
            }
        })
    }

    fn handle(&self, mut stream: TcpStream) -> Res<()> {
        let peer = stream.peer_addr()?;
        if !self.allowed(peer.ip()) {
            return Err("rejected: address not in allow list".into());
        }
        let password = self.settings.password.as_deref().map(str::as_bytes);
        let mut salt = [0; 16];
        getrandom::fill(&mut salt).map_err(|error| error.to_string())?;
        let mut hello = Vec::from(MAGIC);
        hello.push(password.is_some() as u8);
        hello.extend_from_slice(&salt);
        stream.write_all(&hello)?;
        let key = password.map(|pw| derive_key(pw, &salt)).transpose()?;

        let mut reader = FrameReader::new(stream.try_clone()?, key, DIR_CLIENT_TO_SERVER);
        let mut writer = FrameWriter::new(stream.try_clone()?, key, DIR_SERVER_TO_CLIENT);
        if !matches!(reader.recv_msg(), Ok(Msg::Auth)) {
            return Err("authentication failed".into());
        }
        let others = self.active.fetch_add(1, Ordering::SeqCst);
        let _active = ActiveClient(&self.active);
        let level = self.settings.level.unwrap_or(3);
        let entries = scan::to_entries(&self.cache.lock().unwrap());
        writer.send_msg(&Msg::Manifest {
            entries: entries.len() as u64,
            aead: self
                .settings
                .aead
                .clone()
                .unwrap_or_else(|| "aes-256-gcm".into()),
            level,
            active_clients: others as u32,
            metadata: self.settings.metadata.unwrap_or(false),
        })?;
        writer.send_compressed(&entries, level)?;
        drop(entries);
        let source = match reader.recv_msg()? {
            Msg::Proceed { go: true, source } => source,
            _ => {
                println!("zcopy: {peer} declined to continue");
                return Ok(());
            }
        };
        println!(
            "zcopy: {peer} connected, receiving folder '{source}' into '{}'",
            self.root.display()
        );

        let started = std::time::Instant::now();
        let id = CONNECTIONS.fetch_add(1, Ordering::Relaxed);
        let (jobs, queue) = sync_channel::<Job>(2);
        let passwords = password.map(|pw| pw.to_vec());
        let root = self.root.clone();
        let cache = Arc::clone(&self.cache);
        let label = (peer.to_string(), source);
        let worker =
            std::thread::spawn(move || extractor(&root, &cache, passwords, queue, writer, &label));

        let result = self.receive(&mut reader, &jobs, id);
        let _ = jobs.send(Job::Done);
        drop(jobs);
        let (files, bytes) = worker.join().unwrap_or((0, 0));
        let _ = stream.shutdown(Shutdown::Both);
        match &result {
            Ok(()) => {
                let seconds = started.elapsed().as_secs_f64().max(0.001);
                println!(
                    "zcopy: {peer} disconnected: {files} file(s), {} received in {seconds:.1}s ({}/s)",
                    human(bytes),
                    human((bytes as f64 / seconds) as u64)
                );
            }
            Err(error) => println!("zcopy: {peer} disconnected with error: {error}"),
        }
        result
    }

    fn receive(
        &self,
        reader: &mut FrameReader<TcpStream>,
        jobs: &std::sync::mpsc::SyncSender<Job>,
        id: u64,
    ) -> Res<()> {
        let temp = self.root.join(TEMP_DIR);
        for number in 0u64.. {
            match reader.recv_msg()? {
                Msg::Done => return Ok(()),
                Msg::Batch {
                    bytes, metadata, ..
                } => {
                    let metadata = metadata && self.settings.metadata.unwrap_or(false);
                    let items: Vec<Item> = reader.recv_compressed()?;
                    let archive = temp.join(format!("{id}-{number}.zar"));
                    let mut file = BufWriter::new(File::create(&archive)?);
                    let received = reader
                        .recv_raw(&mut file, bytes)
                        .and_then(|()| Ok(file.flush()?));
                    if let Err(error) = received {
                        let _ = fs::remove_file(&archive);
                        return Err(error);
                    }
                    jobs.send(Job::Batch {
                        archive,
                        items,
                        metadata,
                    })
                    .map_err(|_| "extractor stopped")?;
                }
                _ => return Err("unexpected message".into()),
            }
        }
        Ok(())
    }
}

fn extractor(
    root: &Path,
    cache: &Mutex<Tree>,
    password: Option<Vec<u8>>,
    queue: std::sync::mpsc::Receiver<Job>,
    mut writer: FrameWriter<TcpStream>,
    (peer, source): &(String, String),
) -> (u64, u64) {
    let (mut total_files, mut total_bytes) = (0u64, 0u64);
    for job in queue {
        match job {
            Job::Done => {
                let _ = writer.send_msg(&Msg::Done);
                return (total_files, total_bytes);
            }
            Job::Batch {
                archive,
                items,
                metadata,
            } => {
                let wire_bytes = fs::metadata(&archive).map(|m| m.len()).unwrap_or(0);
                let outcome = extract_batch(root, cache, &password, &archive, &items, metadata);
                let _ = fs::remove_file(&archive);
                match &outcome {
                    Ok(files) => {
                        total_files += files;
                        total_bytes += wire_bytes;
                        println!(
                            "zcopy: {peer} copied {files} file(s) ({}) from '{source}' into '{}': {}",
                            human(wire_bytes),
                            root.display(),
                            top_level(&items)
                        );
                    }
                    Err(message) => {
                        println!("zcopy: {peer} copy from '{source}' failed: {message}")
                    }
                }
                let ack = match outcome {
                    Ok(files) => Msg::Ack {
                        ok: true,
                        message: String::new(),
                        files,
                    },
                    Err(message) => Msg::Ack {
                        ok: false,
                        message,
                        files: 0,
                    },
                };
                if writer.send_msg(&ack).is_err() {
                    return (total_files, total_bytes);
                }
            }
        }
    }
    (total_files, total_bytes)
}

/// Names of the first-level entries a batch touches, abbreviated.
fn top_level(items: &[Item]) -> String {
    let names: std::collections::BTreeSet<&str> = items
        .iter()
        .filter_map(|(path, _, _)| path.split('/').next())
        .collect();
    let shown: Vec<&str> = names.iter().copied().take(5).collect();
    let more = names.len().saturating_sub(shown.len());
    if more > 0 {
        format!("{} and {more} more", shown.join(", "))
    } else {
        shown.join(", ")
    }
}

fn human(bytes: u64) -> String {
    indicatif::BinaryBytes(bytes).to_string()
}

fn extract_batch(
    root: &Path,
    cache: &Mutex<Tree>,
    password: &Option<Vec<u8>>,
    archive: &Path,
    items: &[Item],
    metadata: bool,
) -> Result<u64, String> {
    if let Some((bad, _, _)) = items
        .iter()
        .find(|(path, _, _)| !scan::is_safe_relative(path))
    {
        return Err(format!("unsafe path from client: {bad}"));
    }
    let _guard = EXTRACT_LOCK.lock().unwrap();
    let passwords = PasswordSet::new(password.clone());
    extract_archive_with_passwords_progress_and_filters(
        &archive.to_string_lossy(),
        root,
        ExtractionOptions {
            dry_run: false,
            through: None,
            collision_policy: CollisionPolicy::Error,
            existing_policy: ExistingPolicy::Overwrite,
            restore_metadata: metadata,
            restore_symlinks: false,
            restore_devices: false,
            strip_top_level: false,
        },
        &passwords,
        None,
        &FilterSet::default(),
    )
    .map_err(|error| error.to_string())?;

    let mut cache = cache.lock().unwrap();
    for (path, mtime, is_dir) in items {
        let target = resolve_inside(root, path)?;
        if !is_dir {
            let applied = File::options()
                .write(true)
                .open(&target)
                .and_then(|file| file.set_modified(scan::system_time(*mtime)));
            if let Err(error) = applied {
                return Err(format!("cannot set mtime of {path}: {error}"));
            }
        }
        let size = if *is_dir {
            0
        } else {
            fs::symlink_metadata(&target)
                .map_err(|e| format!("{path}: {e}"))?
                .len()
        };
        let mut ancestor = path.as_str();
        while let Some((parent, _)) = ancestor.rsplit_once('/') {
            cache.entry(parent.to_owned()).or_insert(Meta {
                size: 0,
                mtime: 0,
                dir: true,
            });
            ancestor = parent;
        }
        cache.insert(
            path.clone(),
            Meta {
                size,
                mtime: *mtime,
                dir: *is_dir,
            },
        );
    }
    Ok(items.len() as u64)
}

/// Joins a client-supplied relative path onto `root`, refusing unsafe paths and
/// any existing symlink component so nothing outside `root` can be touched.
fn resolve_inside(root: &Path, path: &str) -> Result<PathBuf, String> {
    if !scan::is_safe_relative(path) {
        return Err(format!("unsafe path from client: {path}"));
    }
    let mut target = root.to_path_buf();
    for part in path.split('/') {
        target.push(part);
        match fs::symlink_metadata(&target) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!("refusing symlink in path: {path}"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("{path}: {error}")),
        }
    }
    Ok(target)
}

/// Reads and checks the 21-byte server greeting.
pub fn read_hello(stream: &mut impl Read) -> Res<(bool, [u8; 16])> {
    let mut hello = [0; 21];
    stream.read_exact(&mut hello)?;
    if hello[..4] != MAGIC {
        return Err("not a zcopy server".into());
    }
    Ok((hello[4] != 0, hello[5..].try_into().unwrap()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_inside_rejects_escapes_and_symlinks() {
        let base = std::env::temp_dir().join(format!("zcopy-resolve-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (root, outside) = (base.join("root"), base.join("outside"));
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        assert!(resolve_inside(&root, "a/b.txt").is_ok());
        for bad in ["../outside/x", "/etc/passwd", "a/../../x", "a\\b"] {
            assert!(resolve_inside(&root, bad).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
            assert!(resolve_inside(&root, "link/x").is_err());
            assert!(resolve_inside(&root, "link").is_err());
        }
        fs::remove_dir_all(&base).unwrap();
    }
}
