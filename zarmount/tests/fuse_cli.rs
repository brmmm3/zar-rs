#![cfg(target_os = "linux")]

use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use zarlib::operations::{EncodingOptions, create_archive_with_password};

struct MountGuard {
    child: Child,
    mountpoint: PathBuf,
}

impl Drop for MountGuard {
    fn drop(&mut self) {
        let _ = Command::new("fusermount3")
            .arg("-u")
            .arg(&self.mountpoint)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

fn wait_for_mount(guard: &mut MountGuard, path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if path.exists() {
            return;
        }
        if let Some(status) = guard.child.try_wait().unwrap() {
            panic!("zarmount exited before becoming ready: {status}");
        }
        assert!(Instant::now() < deadline, "zarmount did not become ready");
        thread::sleep(Duration::from_millis(25));
    }
}

#[test]
#[ignore = "requires a usable /dev/fuse and fusermount3"]
fn cli_mounts_encrypted_archive_and_rejects_writes() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    let mountpoint = dir.path().join("mount");
    fs::create_dir(&input).unwrap();
    fs::create_dir(&mountpoint).unwrap();
    for i in 0..300 {
        fs::write(input.join(format!("file-{i:03}.txt")), b"").unwrap();
    }
    let content: Vec<u8> = (0..150_000).map(|i| (i % 251) as u8).collect();
    fs::write(input.join("data.bin"), &content).unwrap();
    let archive = dir.path().join("archive.zar");
    create_archive_with_password(
        archive.to_str().unwrap(),
        &[input],
        false,
        EncodingOptions {
            block_size: 65536,
            compression_level: 3,
        },
        Some(b"secret"),
    )
    .unwrap();
    let password = dir.path().join("password");
    fs::write(&password, b"secret\n").unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_zarmount"))
        .args(["--password-file"])
        .arg(password)
        .arg(&archive)
        .arg(&mountpoint)
        .spawn()
        .unwrap();
    let mut guard = MountGuard {
        child,
        mountpoint: mountpoint.clone(),
    };
    let mounted_input = mountpoint.join("input");
    wait_for_mount(&mut guard, &mounted_input);
    assert_eq!(fs::read_dir(&mounted_input).unwrap().count(), 301);
    let mut file = fs::File::open(mounted_input.join("data.bin")).unwrap();
    file.seek(SeekFrom::Start(65530)).unwrap();
    let mut bytes = [0; 30];
    file.read_exact(&mut bytes).unwrap();
    assert_eq!(bytes.as_slice(), &content[65530..65560]);
    assert!(fs::write(mounted_input.join("file-000.txt"), b"x").is_err());
    assert!(fs::write(mounted_input.join("new.txt"), b"x").is_err());
    drop(file);
    assert!(
        Command::new("fusermount3")
            .arg("-u")
            .arg(&mountpoint)
            .status()
            .unwrap()
            .success()
    );
    assert!(guard.child.wait().unwrap().success());
}

#[test]
#[ignore = "requires a usable /dev/fuse and fusermount3"]
fn cli_unmounts_on_termination_signals() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    fs::create_dir(&input).unwrap();
    fs::write(input.join("file"), b"content").unwrap();
    let archive = dir.path().join("archive.zar");
    create_archive_with_password(
        archive.to_str().unwrap(),
        &[input],
        false,
        EncodingOptions {
            block_size: 65536,
            compression_level: 3,
        },
        None,
    )
    .unwrap();

    for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
        let mountpoint = dir.path().join(format!("mount-{signal}"));
        fs::create_dir(&mountpoint).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_zarmount"))
            .arg(&archive)
            .arg(&mountpoint)
            .spawn()
            .unwrap();
        let mut guard = MountGuard {
            child,
            mountpoint: mountpoint.clone(),
        };
        wait_for_mount(&mut guard, &mountpoint.join("input/file"));
        let open_file = fs::File::open(mountpoint.join("input/file")).unwrap();
        assert_eq!(
            unsafe { libc::kill(guard.child.id() as libc::pid_t, signal) },
            0
        );

        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(status) = guard.child.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "zarmount did not exit on signal {signal}"
            );
            thread::sleep(Duration::from_millis(25));
        };
        assert_eq!(status.signal(), None);
        assert_eq!(status.code(), Some(128 + signal));
        fs::write(mountpoint.join("after-unmount"), b"accessible").unwrap();
        drop(open_file);
    }
}
