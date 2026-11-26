#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zarlib::operations::{add_archive, create_archive};

struct Watcher(Arc<Mutex<Vec<String>>>);

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    fn register_status_notifier_item(&self, name: &str) {
        self.0.lock().unwrap().push(name.to_owned());
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }
}

struct DaemonGuard {
    pid: Option<i32>,
    mounts: Vec<std::path::PathBuf>,
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        if let Some(pid) = self.pid {
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }
        }
        for path in &self.mounts {
            let deadline = Instant::now() + Duration::from_secs(5);
            while path.join("input.txt").exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(25));
            }
            if path.join("input.txt").exists() {
                let _ = Command::new("fusermount3").arg("-u").arg(path).status();
            }
        }
    }
}

fn layout(service: &str) -> String {
    let output = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            service,
            "--object-path",
            "/MenuBar",
            "--method",
            "com.canonical.dbusmenu.GetLayout",
            "--",
            "0",
            "-1",
            "[]",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn menu_id(layout: &str, label: &str) -> i32 {
    let position = layout.find(&format!("'label': <'{label}'>")).unwrap();
    let prefix = &layout[..position];
    let start = prefix.rfind('(').unwrap();
    prefix[start + 1..]
        .split(',')
        .next()
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

fn click(service: &str, id: i32) {
    let output = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            service,
            "--object-path",
            "/MenuBar",
            "--method",
            "com.canonical.dbusmenu.Event",
            &id.to_string(),
            "clicked",
            "<int32 0>",
            "0",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for daemon");
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
#[ignore = "requires dbus-run-session, gdbus, /dev/fuse, and fusermount3"]
fn tray_daemon_mounts_and_unmounts_multiple_archives() {
    if std::env::var_os("ZARMOUNT_ISOLATED_TEST").is_none() {
        let output = Command::new("dbus-run-session")
            .arg("--")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "tray_daemon_mounts_and_unmounts_multiple_archives",
                "--ignored",
                "--nocapture",
            ])
            .env("ZARMOUNT_ISOLATED_TEST", "1")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        return;
    }

    let registered = Arc::new(Mutex::new(Vec::new()));
    let _watcher = zbus::blocking::connection::Builder::session()
        .unwrap()
        .name("org.kde.StatusNotifierWatcher")
        .unwrap()
        .serve_at("/StatusNotifierWatcher", Watcher(registered.clone()))
        .unwrap()
        .build()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let runtime = temp.path().join("runtime");
    let bin = temp.path().join("bin");
    fs::create_dir(&runtime).unwrap();
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700)).unwrap();
    fs::create_dir(&bin).unwrap();
    let executable = bin.join("zarmount");
    fs::copy(env!("CARGO_BIN_EXE_zarmount"), &executable).unwrap();
    let opener = bin.join("xdg-open");
    fs::write(&opener, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&opener, fs::Permissions::from_mode(0o700)).unwrap();
    let helper = bin.join("test-fusermount");
    let fail_unmount = temp.path().join("fail-unmount");
    fs::write(
        &helper,
        format!(
            "#!/bin/sh\nif [ \"$1\" = -u ] && [ -f '{}' ]; then exit 1; fi\nexec fusermount3 \"$@\"\n",
            fail_unmount.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let source = temp.path().join("input.txt");
    let archive = temp.path().join("archive with spaces.zar");
    fs::write(&source, b"old").unwrap();
    create_archive(archive.to_str().unwrap(), std::slice::from_ref(&source)).unwrap();
    fs::write(&source, b"new").unwrap();
    add_archive(archive.to_str().unwrap(), std::slice::from_ref(&source)).unwrap();

    let run = |args: &[&Path], revision: bool| {
        let mut command = Command::new(&executable);
        command
            .env("XDG_RUNTIME_DIR", &runtime)
            .env("PATH", &path)
            .env("FUSERMOUNT_PATH", &helper)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY");
        if revision {
            command.args(["--to", "00000001"]);
        }
        let output = command.args(args).output().unwrap();
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap()
    };
    let first_output = run(&[&archive], true);
    let first = first_output
        .trim()
        .strip_prefix("zarmount: mounted at ")
        .unwrap();
    let first = std::path::PathBuf::from(first);
    let service = registered.lock().unwrap()[0].clone();
    let pid: i32 = service
        .strip_prefix("org.kde.StatusNotifierItem-")
        .unwrap()
        .split('-')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let second = temp.path().join("explicit mount");
    let mut guard = DaemonGuard {
        pid: Some(pid),
        mounts: vec![first.clone(), second.clone()],
    };
    fs::remove_file(&executable).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_zarmount"), &executable).unwrap();
    let output = Command::new(&executable)
        .env("XDG_RUNTIME_DIR", &runtime)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .args(["--ask"])
        .arg(&archive)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("ask dialog failed: zarmount: dialog failed:"),
        "{output:?}"
    );
    click(&service, menu_id(&layout(&service), "About"));
    let daemon_log = runtime.join("zarmount/daemon.log");
    wait_until(|| {
        fs::read_to_string(&daemon_log)
            .unwrap()
            .contains("about dialog failed:")
    });
    assert_eq!(fs::read(first.join("input.txt")).unwrap(), b"old");
    run(&[&archive, &second], false);
    assert_eq!(fs::read(second.join("input.txt")).unwrap(), b"new");
    assert_eq!(registered.lock().unwrap().len(), 1);
    let third_output = run(&[&archive], false);
    let third = std::path::PathBuf::from(
        third_output
            .trim()
            .strip_prefix("zarmount: mounted at ")
            .unwrap(),
    );
    guard.mounts.push(third.clone());
    assert!(
        Command::new("fusermount3")
            .arg("-u")
            .arg(&third)
            .status()
            .unwrap()
            .success()
    );
    wait_until(|| !third.exists());
    let shown = layout(&service);
    assert!(shown.contains("archive with spaces.zar"), "{shown}");
    let unmount = menu_id(&shown, "Unmount");
    click(&service, unmount);
    wait_until(|| !first.exists());
    assert_eq!(fs::read(second.join("input.txt")).unwrap(), b"new");
    let shown = layout(&service);
    let exit = menu_id(&shown, "Exit");
    click(&service, exit);
    wait_until(|| !second.join("input.txt").exists());
    wait_until(|| !runtime.join("zarmount/daemon.sock").exists());
    guard.pid = None;
    guard.mounts.clear();

    let fourth_output = run(&[&archive], false);
    let fourth = std::path::PathBuf::from(
        fourth_output
            .trim()
            .strip_prefix("zarmount: mounted at ")
            .unwrap(),
    );
    wait_until(|| registered.lock().unwrap().len() == 2);
    let service = registered.lock().unwrap()[1].clone();
    guard.pid = Some(
        service
            .strip_prefix("org.kde.StatusNotifierItem-")
            .unwrap()
            .split('-')
            .next()
            .unwrap()
            .parse()
            .unwrap(),
    );
    guard.mounts.push(fourth.clone());
    wait_until(|| layout(&service).contains("Unmount"));
    fs::write(&fail_unmount, b"fail").unwrap();
    let id = menu_id(&layout(&service), "Unmount");
    let log = runtime.join("zarmount/daemon.log");
    for count in 1..=2 {
        click(&service, id);
        wait_until(|| {
            fs::read_to_string(&log)
                .unwrap()
                .matches("cannot unmount ")
                .count()
                >= count
        });
        assert_eq!(fs::read(fourth.join("input.txt")).unwrap(), b"new");
        assert!(layout(&service).contains("Unmount"));
    }
    assert!(
        Command::new("fusermount3")
            .arg("-u")
            .arg(&fourth)
            .status()
            .unwrap()
            .success()
    );
    unsafe {
        assert_eq!(libc::kill(guard.pid.unwrap(), libc::SIGTERM), 0);
    }
    wait_until(|| !runtime.join("zarmount/daemon.sock").exists());
    guard.pid = None;
    guard.mounts.clear();
}
