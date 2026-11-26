use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use clap::Parser;
use ksni::blocking::TrayMethods;
use serde::{Deserialize, Serialize};
use zarlib::operations::PasswordSet;
use zeroize::Zeroizing;

use crate::{linux_dialog, mount};

#[derive(Parser)]
#[command(
    version,
    about = "Mount a ZAR archive as a read-only Linux FUSE filesystem"
)]
struct Cli {
    /// Archive to mount.
    archive: PathBuf,
    /// Mount directory; defaults to a temporary directory.
    mountpoint: Option<PathBuf>,
    /// Choose the mount directory in a dialog.
    #[arg(long, conflicts_with = "foreground")]
    ask: bool,
    /// Run in the foreground without a tray; requires a mount directory.
    #[arg(long, requires = "mountpoint")]
    foreground: bool,
    /// Select the archive state through this eight-digit commit ID.
    #[arg(long)]
    to: Option<String>,
    /// File containing the default password.
    #[arg(long)]
    password_file: Option<PathBuf>,
    /// Commit-specific password file, as COMMIT_ID=PATH. May be repeated.
    #[arg(long, value_name = "COMMIT_ID=PATH")]
    password_for: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct Request {
    archive: PathBuf,
    mountpoint: Option<PathBuf>,
    cwd: PathBuf,
    ask: bool,
    to: Option<String>,
    password_file: Option<PathBuf>,
    password_for: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct Response {
    error: Option<String>,
    message: String,
}

struct Mount {
    id: u64,
    archive: PathBuf,
    path: PathBuf,
    temporary: bool,
    session: mount::MountedSession,
}

enum Action {
    Open(u64),
    Unmount(u64),
    Settings,
    About,
    Exit,
}

struct Tray {
    mounts: Vec<(u64, String)>,
    actions: Sender<Action>,
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "zarmount".into()
    }

    fn title(&self) -> String {
        "ZAR archive mounts".into()
    }

    fn icon_name(&self) -> String {
        "application-x-zar".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let icon = image::load_from_memory(include_bytes!(
            "../../zarlib-gui/assets/application-x-zar.png"
        ))
        .expect("bundled icon must be valid")
        .into_rgba8();
        let (width, height) = icon.dimensions();
        let pixels = icon
            .pixels()
            .flat_map(|pixel| [pixel[3], pixel[0], pixel[1], pixel[2]])
            .collect();
        vec![ksni::Icon {
            width: width as i32,
            height: height as i32,
            data: pixels,
        }]
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{StandardItem, SubMenu};
        let mut items: Vec<_> = self
            .mounts
            .iter()
            .map(|(id, label)| {
                let id = *id;
                SubMenu {
                    label: label.clone(),
                    submenu: vec![
                        StandardItem {
                            label: "Open".into(),
                            activate: Box::new(move |tray: &mut Self| {
                                if let Err(error) = tray.actions.send(Action::Open(id)) {
                                    eprintln!("zarmount: tray action failed: {error}");
                                }
                            }),
                            ..Default::default()
                        }
                        .into(),
                        StandardItem {
                            label: "Unmount".into(),
                            activate: Box::new(move |tray: &mut Self| {
                                if let Err(error) = tray.actions.send(Action::Unmount(id)) {
                                    eprintln!("zarmount: tray action failed: {error}");
                                }
                            }),
                            ..Default::default()
                        }
                        .into(),
                    ],
                    ..Default::default()
                }
                .into()
            })
            .collect();
        if !items.is_empty() {
            items.push(ksni::MenuItem::Separator);
        }
        for (label, action) in [
            ("Settings", Action::Settings),
            ("About", Action::About),
            ("Exit", Action::Exit),
        ] {
            items.push(
                StandardItem {
                    label: label.into(),
                    activate: Box::new(move |tray: &mut Self| {
                        let action = match action {
                            Action::Settings => Action::Settings,
                            Action::About => Action::About,
                            Action::Exit => Action::Exit,
                            _ => unreachable!(),
                        };
                        if let Err(error) = tray.actions.send(action) {
                            eprintln!("zarmount: tray action failed: {error}");
                        }
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }
        items
    }
}

fn runtime_dir() -> Result<PathBuf, String> {
    let root = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or("XDG_RUNTIME_DIR is not set; use --foreground without a desktop session")?;
    let root_meta = fs::symlink_metadata(&root).map_err(|error| error.to_string())?;
    if !root_meta.is_dir()
        || root_meta.uid() != unsafe { libc::geteuid() }
        || root_meta.mode() & 0o077 != 0
    {
        return Err("XDG_RUNTIME_DIR must be a private directory owned by the current user".into());
    }
    let path = root.join("zarmount");
    match fs::create_dir(&path) {
        Ok(()) => fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.to_string()),
    }
    let meta = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
    if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
        return Err(
            "zarmount runtime directory must be private and owned by the current user".into(),
        );
    }
    Ok(path)
}

fn acquire_lock(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path.join("daemon.lock"))?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(file)
}

fn serve(runtime: &Path) -> Result<(), String> {
    let _lock =
        acquire_lock(runtime).map_err(|error| format!("cannot acquire daemon lock: {error}"))?;
    let socket = runtime.join("daemon.sock");
    match fs::symlink_metadata(&socket) {
        Ok(meta) if meta.file_type().is_socket() && meta.uid() == unsafe { libc::geteuid() } => {
            fs::remove_file(&socket).map_err(|error| error.to_string())?;
        }
        Ok(_) => return Err("daemon socket path is not a user-owned socket".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    let listener = UnixListener::bind(&socket).map_err(|error| error.to_string())?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let (action_tx, action_rx) = mpsc::channel();
    let tray = Tray {
        mounts: Vec::new(),
        actions: action_tx,
    };
    let handle = tray
        .spawn()
        .map_err(|error| format!("system tray unavailable: {error}"))?;
    let mut signals = signal_hook::iterator::Signals::new([
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
    ])
    .map_err(|error| error.to_string())?;
    let mut mounts: Vec<Mount> = Vec::new();
    let mut next_id = 1u64;
    println!("READY");
    io::stdout().flush().map_err(|error| error.to_string())?;
    loop {
        let mut changed = false;
        if signals.pending().next().is_some() {
            break;
        }
        while let Ok(action) = action_rx.try_recv() {
            match action {
                Action::Open(id) => {
                    if let Some(mount) = mounts.iter().find(|mount| mount.id == id)
                        && let Err(error) = open(&mount.path)
                    {
                        report(&error);
                    }
                }
                Action::Unmount(id) => {
                    if let Some(index) = mounts.iter().position(|mount| mount.id == id) {
                        match unmount(&mut mounts, index) {
                            Ok(()) => changed = true,
                            Err(error) => {
                                report(&error);
                                changed = true;
                            }
                        }
                    }
                }
                Action::Settings => {
                    if let Err(error) = linux_dialog::show("settings") {
                        eprintln!("zarmount: {error}");
                    }
                }
                Action::About => {
                    if let Err(error) = linux_dialog::show("about") {
                        eprintln!("zarmount: {error}");
                    }
                }
                Action::Exit => {
                    for index in (0..mounts.len()).rev() {
                        if let Err(error) = unmount(&mut mounts, index) {
                            report(&error);
                        }
                    }
                    if mounts.is_empty() {
                        fs::remove_file(&socket).map_err(|error| error.to_string())?;
                        return Ok(());
                    }
                    changed = true;
                }
            }
        }
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .map_err(|error| error.to_string())?;
                let result = read_request(&stream)
                    .and_then(|request| create_mount(request, &mut mounts, &mut next_id, runtime));
                changed |= result.is_ok();
                let response = match result {
                    Ok(message) => Response {
                        error: None,
                        message,
                    },
                    Err(error) => Response {
                        error: Some(error),
                        message: String::new(),
                    },
                };
                if let Err(error) = serde_json::to_writer(&mut stream, &response)
                    .and_then(|()| stream.write_all(b"\n").map_err(serde_json::Error::io))
                {
                    eprintln!("zarmount: cannot send daemon response: {error}");
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(format!("daemon socket failed: {error}")),
        }
        for index in (0..mounts.len()).rev() {
            if mounts[index].session.finished() {
                if let Err(error) = unmount(&mut mounts, index) {
                    eprintln!("zarmount: cleanup failed: {error}");
                } else {
                    changed = true;
                }
            }
        }
        if changed {
            let entries = mounts
                .iter()
                .map(|mount| {
                    (
                        mount.id,
                        format!("{} ({})", mount.archive.display(), mount.path.display()),
                    )
                })
                .collect();
            handle.update(|tray| tray.mounts = entries);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    for index in (0..mounts.len()).rev() {
        if let Err(error) = unmount(&mut mounts, index) {
            eprintln!("zarmount: {error}");
        }
    }
    fs::remove_file(socket).map_err(|error| error.to_string())?;
    if mounts.is_empty() {
        Ok(())
    } else {
        Err("one or more mounts could not be unmounted".into())
    }
}

fn read_request(stream: &UnixStream) -> Result<Request, String> {
    let mut line = String::new();
    let mut reader = BufReader::new(stream.take(1024 * 1024));
    reader
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    if !line.ends_with('\n') {
        return Err("invalid or oversized daemon request".into());
    }
    serde_json::from_str(&line).map_err(|error| format!("invalid daemon request: {error}"))
}

fn absolute(path: PathBuf, cwd: &Path) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}

fn open(path: &Path) -> Result<(), String> {
    let mut child = Command::new("xdg-open")
        .arg(path)
        .stdin(Stdio::null())
        .spawn()
        .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    let path = path.to_owned();
    std::thread::spawn(move || match child.wait() {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!("zarmount: opening {} failed: {status}", path.display()),
        Err(error) => eprintln!("zarmount: cannot wait for xdg-open: {error}"),
    });
    Ok(())
}

fn create_mount(
    request: Request,
    mounts: &mut Vec<Mount>,
    next_id: &mut u64,
    runtime: &Path,
) -> Result<String, String> {
    let archive = fs::canonicalize(absolute(request.archive, &request.cwd))
        .map_err(|error| format!("cannot open archive: {error}"))?;
    let mut temporary = None;
    let suggested = if let Some(path) = request.mountpoint {
        absolute(path, &request.cwd)
    } else {
        let dir = tempfile::Builder::new()
            .prefix("mount-")
            .tempdir_in(runtime)
            .map_err(|error| error.to_string())?
            .keep();
        temporary = Some(dir.clone());
        dir
    };
    let result = (|| {
        let path = if request.ask {
            let Some(answer) = linux_dialog::ask("ask", &suggested)? else {
                return Err("mount cancelled".into());
            };
            let selected: PathBuf =
                serde_json::from_slice(&answer).map_err(|error| error.to_string())?;
            absolute(selected, &request.cwd)
        } else {
            suggested.clone()
        };
        fs::create_dir_all(&path)
            .map_err(|error| format!("cannot create mount directory: {error}"))?;
        let path = fs::canonicalize(path).map_err(|error| error.to_string())?;
        if fs::read_dir(&path)
            .map_err(|error| format!("cannot read mount directory: {error}"))?
            .next()
            .is_some()
        {
            return Err("mountpoint must be empty".into());
        }
        let password_file = request
            .password_file
            .map(|path| absolute(path, &request.cwd));
        let password_for = request
            .password_for
            .into_iter()
            .map(|spec| {
                let (id, path) = spec
                    .split_once('=')
                    .ok_or("--password-for must be COMMIT_ID=PATH")?;
                if path.is_empty() {
                    return Err("--password-for requires a password file path".into());
                }
                let path = absolute(PathBuf::from(path), &request.cwd);
                let path = path.to_str().ok_or("password file path must be UTF-8")?;
                Ok(format!("{id}={path}"))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let options = mount::Options {
            archive: archive.clone(),
            mountpoint: path.clone(),
            to: request.to,
            password_file,
            password_for,
        };
        let passwords = if options.password_file.is_none() && options.password_for.is_empty() {
            let name = archive.to_str().ok_or("archive path must be UTF-8")?;
            let commits =
                zarlib::operations::info_archive(name, false).map_err(|error| error.to_string())?;
            if commits
                .iter()
                .any(|commit| commit.header_encrypted || commit.data_encrypted)
            {
                let bytes =
                    linux_dialog::ask("password", &archive)?.ok_or("password entry cancelled")?;
                let mut rows: Vec<linux_dialog::CommitPassword> =
                    serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
                let mut set = PasswordSet::new(None);
                for row in &mut rows {
                    if !row.password.is_empty() {
                        set.insert_for_commit(
                            row.id,
                            Zeroizing::new(std::mem::take(&mut row.password))
                                .as_bytes()
                                .to_vec(),
                        );
                    }
                }
                Some(set)
            } else {
                None
            }
        } else {
            None
        };
        let next = next_id.checked_add(1).ok_or("too many mounts")?;
        let session =
            mount::start(&options, passwords.as_ref()).map_err(|error| error.to_string())?;
        let is_temporary = temporary.as_ref().is_some_and(|temp| temp == &path);
        let id = *next_id;
        *next_id = next;
        mounts.push(Mount {
            id,
            archive,
            path: path.clone(),
            temporary: is_temporary,
            session,
        });
        if is_temporary {
            temporary = None;
        }
        let mut message = format!("mounted at {}", path.display());
        if let Err(error) = open(&path) {
            message.push_str(&format!("; warning: {error}"));
        }
        Ok(message)
    })();
    if let Some(path) = temporary
        && let Err(error) = fs::remove_dir(&path)
    {
        eprintln!(
            "zarmount: cannot remove temporary mount directory {}: {error}",
            path.display()
        );
    }
    result
}

fn unmount(mounts: &mut Vec<Mount>, index: usize) -> Result<(), String> {
    let mount = &mut mounts[index];
    mount
        .session
        .unmount()
        .map_err(|error| format!("cannot unmount {}: {error}", mount.path.display()))?;
    let mount = mounts.remove(index);
    if mount.temporary {
        fs::remove_dir(&mount.path)
            .map_err(|error| format!("cannot remove {}: {error}", mount.path.display()))?;
    }
    Ok(())
}

fn report(error: &str) {
    eprintln!("zarmount: {error}");
    if let Err(dialog_error) = linux_dialog::show_error(error) {
        eprintln!("zarmount: cannot display error: {dialog_error}");
    }
}

fn send(mut stream: UnixStream, request: &Request) -> Result<Response, String> {
    serde_json::to_writer(&mut stream, request).map_err(|error| error.to_string())?;
    stream.write_all(b"\n").map_err(|error| error.to_string())?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&line).map_err(|error| format!("invalid daemon response: {error}"))
}

fn client(cli: Cli) -> Result<String, String> {
    let runtime = runtime_dir()?;
    let socket = runtime.join("daemon.sock");
    let request = Request {
        archive: cli.archive,
        mountpoint: cli.mountpoint,
        cwd: std::env::current_dir().map_err(|error| error.to_string())?,
        ask: cli.ask,
        to: cli.to,
        password_file: cli.password_file,
        password_for: cli.password_for,
    };
    let stream = match UnixStream::connect(&socket) {
        Ok(stream) => stream,
        Err(_) => {
            let log = OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(runtime.join("daemon.log"))
                .map_err(|error| format!("cannot open daemon log: {error}"))?;
            let mut child = Command::new(crate::SELF_EXECUTABLE)
                .arg("--linux-daemon")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::from(log))
                .process_group(0)
                .spawn()
                .map_err(|error| format!("cannot start daemon: {error}"))?;
            let mut line = String::new();
            BufReader::new(
                child
                    .stdout
                    .take()
                    .ok_or("daemon startup pipe unavailable")?,
            )
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
            if line.trim() != "READY" && !line.starts_with("BUSY") {
                return Err(format!("daemon failed to start: {}", line.trim()));
            }
            let mut connection = None;
            for _ in 0..100 {
                if let Ok(value) = UnixStream::connect(&socket) {
                    connection = Some(value);
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            connection.ok_or("daemon is unavailable")?
        }
    };
    let response = send(stream, &request)?;
    match response.error {
        Some(error) => Err(error),
        None => Ok(response.message),
    }
}

pub(crate) fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1).peekable();
    if args.next_if_eq("--linux-daemon").is_some() {
        return match runtime_dir().and_then(|runtime| serve(&runtime)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                if error.contains("cannot acquire daemon lock") {
                    let _ = writeln!(io::stdout(), "BUSY");
                } else {
                    let _ = writeln!(io::stdout(), "ERROR {error}");
                }
                eprintln!("zarmount: {error}");
                ExitCode::FAILURE
            }
        };
    }
    if args.next_if_eq("--linux-dialog").is_some() {
        let Some(kind) = args.next().and_then(|value| value.into_string().ok()) else {
            eprintln!("zarmount: missing or invalid dialog kind");
            return ExitCode::FAILURE;
        };
        let argument = match args.next() {
            Some(value) => match value.into_string() {
                Ok(value) => Some(value),
                Err(_) => {
                    eprintln!("zarmount: dialog argument must be UTF-8");
                    return ExitCode::FAILURE;
                }
            },
            None => None,
        };
        return linux_dialog::run(&kind, argument.as_deref());
    }
    let cli = Cli::parse();
    if cli.foreground {
        let options = mount::Options {
            archive: cli.archive,
            mountpoint: cli.mountpoint.expect("clap requires the mountpoint"),
            to: cli.to,
            password_file: cli.password_file,
            password_for: cli.password_for,
        };
        return match mount::run(options) {
            Ok(Some(signal)) => ExitCode::from(128 + signal as u8),
            Ok(None) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("zarmount: {error}");
                ExitCode::FAILURE
            }
        };
    }
    match client(cli) {
        Ok(message) => {
            if !message.is_empty() {
                println!("zarmount: {message}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("zarmount: {error}");
            ExitCode::FAILURE
        }
    }
}
