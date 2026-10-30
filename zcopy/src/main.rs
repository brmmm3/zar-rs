//! zcopy: fast compressed (optionally encrypted) file-tree copy over the network.

mod client;
mod config;
mod net;
mod scan;
mod server;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use config::Settings;

#[derive(Parser)]
#[command(name = "zcopy", version, author, about)]
struct Args {
    /// Run as server and store received files in this folder (created if missing)
    #[arg(
        short = 's',
        long = "server",
        value_name = "FOLDER",
        conflicts_with = "client"
    )]
    server: Option<PathBuf>,

    /// Run as client and push to this server (HOST or HOST:PORT)
    #[arg(short = 'c', long = "client", value_name = "HOST")]
    client: Option<String>,

    /// Folder to send (client only)
    #[arg(value_name = "SOURCE", default_value = ".")]
    source: PathBuf,

    /// Password for authentication and encryption (or ZCOPY_PASSWORD)
    #[arg(short = 'p', long)]
    password: Option<String>,

    /// Content encryption: aes-256-gcm or chacha20-poly1305
    #[arg(long)]
    aead: Option<String>,

    /// zstd compression level
    #[arg(short = 'l', long)]
    level: Option<i32>,

    /// Comma-separated machine names or IPs allowed to connect (server)
    #[arg(long, value_delimiter = ',')]
    allow: Option<Vec<String>>,

    /// TCP port
    #[arg(long)]
    port: Option<u16>,

    /// Address to listen on (server, default 0.0.0.0)
    #[arg(long)]
    bind: Option<String>,

    /// Approximate batch size in MiB (client, default 512)
    #[arg(long)]
    batch_mib: Option<u64>,

    /// Also transfer permissions and ownership (the server must enable it too)
    #[arg(long)]
    metadata: bool,

    /// Continue without asking when other clients are connected (client)
    #[arg(short = 'y', long)]
    yes: bool,

    /// Do not show a progress bar (client)
    #[arg(long)]
    no_progress: bool,

    /// Settings file, loaded if present (default zcopy.json)
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Directory for temporary archives (client)
    #[arg(long, value_name = "DIR")]
    temp: Option<PathBuf>,
}

fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();
    let file = Settings::load(
        args.config.as_deref().unwrap_or("zcopy.json".as_ref()),
        args.config.is_some(),
    )?;
    let settings = Settings {
        password: args
            .password
            .or_else(|| std::env::var("ZCOPY_PASSWORD").ok()),
        aead: args.aead,
        level: args.level,
        allow: args.allow,
        port: args.port,
        bind: args.bind,
        batch_mib: args.batch_mib,
        metadata: args.metadata.then_some(true),
    }
    .or(file);

    match (args.server, args.client) {
        (Some(folder), None) => {
            if settings.password.is_none() && settings.allow.is_none() {
                eprintln!("zcopy: warning: no password and no allow list; anyone can write here");
            }
            let server = server::Server::bind(&folder, settings)?;
            println!(
                "zcopy: serving {} ({} entries cached) on {}",
                folder.display(),
                server.cached_entries(),
                server.local_addr()
            );
            server.serve()
        }
        (None, Some(host)) => {
            let started = std::time::Instant::now();
            let ui = client::Ui {
                progress: !args.no_progress,
                confirm: &|others| client::ask_continue(others, args.yes),
            };
            let stats = client::run(&host, &args.source, &settings, args.temp.as_deref(), &ui)?;
            println!(
                "zcopy: sent {} file(s), {} byte(s) -> {} byte(s) on the wire, {} up to date, {:.1}s",
                stats.files,
                stats.source_bytes,
                stats.sent_bytes,
                stats.skipped,
                started.elapsed().as_secs_f64()
            );
            Ok(())
        }
        _ => Err("specify either -s <FOLDER> or -c <HOST>".into()),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("zcopy: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use crate::{client, config::Settings, net::*, scan, server::Server};

    fn start(root: &Path, password: Option<&str>, allow: Option<Vec<String>>) -> String {
        let server = Server::bind(
            root,
            Settings {
                password: password.map(str::to_owned),
                allow,
                port: Some(0),
                bind: Some("127.0.0.1".into()),
                ..Settings::default()
            },
        )
        .unwrap();
        let address = server.local_addr().to_string();
        std::thread::spawn(move || {
            let _ = server.serve();
        });
        address
    }

    fn run_client(
        address: &str,
        src: &Path,
        settings: &Settings,
        confirm: &dyn Fn(u32) -> bool,
    ) -> Result<client::Stats, Box<dyn std::error::Error + Send + Sync>> {
        let ui = client::Ui {
            progress: false,
            confirm,
        };
        client::run(address, src, settings, None, &ui)
    }

    fn client_settings(password: Option<&str>) -> Settings {
        Settings {
            password: password.map(str::to_owned),
            ..Settings::default()
        }
    }

    // One sequential test because the client changes the process directory.
    #[test]
    fn syncs_missing_and_newer_files_with_and_without_password() {
        let base = std::env::temp_dir().join(format!("zcopy-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (src, dst) = (base.join("src"), base.join("dst"));
        fs::create_dir_all(src.join("a/b")).unwrap();
        fs::create_dir_all(src.join("empty")).unwrap();
        fs::write(src.join("one.txt"), "one").unwrap();
        fs::write(src.join("a/two.txt"), "two".repeat(1000)).unwrap();
        fs::write(src.join("a/b/three.txt"), "three").unwrap();
        let old = SystemTime::now() - Duration::from_secs(1000);
        for name in ["one.txt", "a/two.txt", "a/b/three.txt"] {
            fs::File::options()
                .write(true)
                .open(src.join(name))
                .unwrap()
                .set_modified(old)
                .unwrap();
        }
        let cwd = std::env::current_dir().unwrap();

        let address = start(&dst, None, None);
        let stats = run_client(&address, &src, &client_settings(None), &|_| true).unwrap();
        assert_eq!(stats.files, 4); // 3 files + 1 empty dir
        assert_eq!(
            fs::read_to_string(dst.join("a/b/three.txt")).unwrap(),
            "three"
        );
        assert!(dst.join("empty").is_dir());
        let mtime = |p: &Path| scan::mtime_secs(fs::metadata(p).unwrap().modified().unwrap());
        assert_eq!(mtime(&dst.join("one.txt")), mtime(&src.join("one.txt")));
        assert_eq!(std::env::current_dir().unwrap(), cwd);

        let stats = run_client(&address, &src, &client_settings(None), &|_| true).unwrap();
        assert_eq!((stats.files, stats.skipped), (0, 3));

        fs::write(src.join("one.txt"), "ONE!").unwrap();
        let stats = run_client(&address, &src, &client_settings(None), &|_| true).unwrap();
        assert_eq!((stats.files, stats.skipped), (1, 2));
        assert_eq!(fs::read_to_string(dst.join("one.txt")).unwrap(), "ONE!");

        // A restarted server rescans and reaches the same conclusion.
        let address = start(&dst, None, None);
        let stats = run_client(&address, &src, &client_settings(None), &|_| true).unwrap();
        assert_eq!(stats.files, 0);

        let (secure_dst, secret) = (base.join("secure"), Some("pw"));
        let address = start(&secure_dst, secret, None);
        let stats = run_client(&address, &src, &client_settings(secret), &|_| true).unwrap();
        assert_eq!(stats.files, 4);
        assert_eq!(
            fs::read_to_string(secure_dst.join("a/two.txt"))
                .unwrap()
                .len(),
            3000
        );
        assert!(run_client(&address, &src, &client_settings(Some("bad")), &|_| true).is_err());
        assert!(run_client(&address, &src, &client_settings(None), &|_| true).is_err());

        let address = start(&base.join("closed"), None, Some(vec!["192.0.2.1".into()]));
        assert!(run_client(&address, &src, &client_settings(None), &|_| true).is_err());
        let address = start(&base.join("open"), None, Some(vec!["127.0.0.1".into()]));
        assert!(run_client(&address, &src, &client_settings(None), &|_| true).is_ok());

        // A second client while another one is connected is warned and may decline.
        let address = start(&base.join("busy"), None, None);
        let mut holder = std::net::TcpStream::connect(&address).unwrap();
        let (_, _) = crate::server::read_hello(&mut holder).unwrap();
        let mut hold_writer =
            FrameWriter::new(holder.try_clone().unwrap(), None, DIR_CLIENT_TO_SERVER);
        let mut hold_reader = FrameReader::new(holder, None, DIR_SERVER_TO_CLIENT);
        hold_writer.send_msg(&Msg::Auth).unwrap();
        assert!(matches!(
            hold_reader.recv_msg().unwrap(),
            Msg::Manifest {
                active_clients: 0,
                ..
            }
        ));
        let seen = std::sync::atomic::AtomicU32::new(0);
        let declined = run_client(&address, &src, &client_settings(None), &|others| {
            seen.store(others, std::sync::atomic::Ordering::SeqCst);
            false
        });
        assert!(declined.is_err());
        assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(!base.join("busy/one.txt").exists());
        let stats = run_client(&address, &src, &client_settings(None), &|_| true).unwrap();
        assert_eq!(stats.files, 4);
        drop(hold_writer);
        drop(hold_reader);

        // Metadata is applied only when both sides enable it.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(src.join("one.txt"), fs::Permissions::from_mode(0o640)).unwrap();
            let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
            let with = Settings {
                metadata: Some(true),
                ..client_settings(None)
            };
            let server_settings = Settings {
                port: Some(0),
                bind: Some("127.0.0.1".into()),
                metadata: Some(true),
                ..Settings::default()
            };
            let server = Server::bind(&base.join("meta"), server_settings).unwrap();
            let address = server.local_addr().to_string();
            std::thread::spawn(move || {
                let _ = server.serve();
            });
            run_client(&address, &src, &with, &|_| true).unwrap();
            assert_eq!(mode(&base.join("meta/one.txt")), 0o640);
            let address = start(&base.join("nometa"), None, None);
            run_client(&address, &src, &with, &|_| true).unwrap();
            assert_ne!(mode(&base.join("nometa/one.txt")), 0o640);
        }

        assert_eq!(std::env::current_dir().unwrap(), cwd);
        fs::remove_dir_all(&base).unwrap();
    }
}
