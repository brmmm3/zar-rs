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
            let stats = client::run(&host, &args.source, &settings, args.temp.as_deref())?;
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

    use crate::{client, config::Settings, scan, server::Server};

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
        let stats = client::run(&address, &src, &client_settings(None), None).unwrap();
        assert_eq!(stats.files, 4); // 3 files + 1 empty dir
        assert_eq!(
            fs::read_to_string(dst.join("a/b/three.txt")).unwrap(),
            "three"
        );
        assert!(dst.join("empty").is_dir());
        let mtime = |p: &Path| scan::mtime_secs(fs::metadata(p).unwrap().modified().unwrap());
        assert_eq!(mtime(&dst.join("one.txt")), mtime(&src.join("one.txt")));
        assert_eq!(std::env::current_dir().unwrap(), cwd);

        let stats = client::run(&address, &src, &client_settings(None), None).unwrap();
        assert_eq!((stats.files, stats.skipped), (0, 3));

        fs::write(src.join("one.txt"), "ONE!").unwrap();
        let stats = client::run(&address, &src, &client_settings(None), None).unwrap();
        assert_eq!((stats.files, stats.skipped), (1, 2));
        assert_eq!(fs::read_to_string(dst.join("one.txt")).unwrap(), "ONE!");

        // A restarted server rescans and reaches the same conclusion.
        let address = start(&dst, None, None);
        let stats = client::run(&address, &src, &client_settings(None), None).unwrap();
        assert_eq!(stats.files, 0);

        let (secure_dst, secret) = (base.join("secure"), Some("pw"));
        let address = start(&secure_dst, secret, None);
        let stats = client::run(&address, &src, &client_settings(secret), None).unwrap();
        assert_eq!(stats.files, 4);
        assert_eq!(
            fs::read_to_string(secure_dst.join("a/two.txt"))
                .unwrap()
                .len(),
            3000
        );
        assert!(client::run(&address, &src, &client_settings(Some("bad")), None).is_err());
        assert!(client::run(&address, &src, &client_settings(None), None).is_err());

        let address = start(&base.join("closed"), None, Some(vec!["192.0.2.1".into()]));
        assert!(client::run(&address, &src, &client_settings(None), None).is_err());
        let address = start(&base.join("open"), None, Some(vec!["127.0.0.1".into()]));
        assert!(client::run(&address, &src, &client_settings(None), None).is_ok());

        assert_eq!(std::env::current_dir().unwrap(), cwd);
        fs::remove_dir_all(&base).unwrap();
    }
}
