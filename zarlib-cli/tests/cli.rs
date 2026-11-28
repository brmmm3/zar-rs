use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("zarc-cli-{}-{id}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn zarc<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(env!("CARGO_BIN_EXE_zarc"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn diff_reports_commit_changes_and_reverse_sizes() {
    let directory = TestDirectory::new();
    let file = directory.path().join("input.txt");
    let archive = directory.path().join("archive.zar");
    std::fs::write(&file, b"old").unwrap();
    let name = archive.to_str().unwrap();
    zarlib::operations::create_archive(name, std::slice::from_ref(&file)).unwrap();
    std::fs::write(&file, b"newer").unwrap();
    zarlib::operations::add_archive(name, &[file]).unwrap();
    for (start, end, size) in [
        ("00000001", "00000002", "+2 B"),
        ("00000002", "00000001", "-2 B"),
    ] {
        let output = zarc(["--no-config", "diff", name, start, end]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains(&format!("* {size} input.txt\n")));
        assert!(stdout.contains(&format!("size change: {size}")));
    }
    let output = zarc(["--no-config", "diff", name, "00000001", "ffffffff"]);
    assert!(!output.status.success());
    assert!(!output.stderr.is_empty());
}

#[cfg(unix)]
fn zarc_with_terminal(args: &[&OsStr], prompts: &[(&str, &str)]) -> Output {
    use std::io::Read;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::time::{Duration, Instant};

    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            if matches!(self.0.try_wait(), Ok(None)) {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }

    let mut master = -1;
    let mut slave = -1;
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
            )
        },
        0
    );
    let mut terminal = unsafe { std::fs::File::from_raw_fd(master) };
    let input = unsafe { std::fs::File::from_raw_fd(slave) };
    let mut child = ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_zarc"))
            .arg("--no-config")
            .args(args)
            .stdin(Stdio::from(input))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut stderr = child.0.stderr.take().unwrap();
    let mut errors = Vec::new();
    for (prompt, password) in prompts {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !errors.ends_with(prompt.as_bytes()) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "timed out waiting for {prompt:?}");
            let mut fd = libc::pollfd {
                fd: stderr.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let ready = unsafe { libc::poll(&mut fd, 1, remaining.as_millis() as i32) };
            assert!(ready > 0, "could not read prompt {prompt:?}: {ready}");
            let mut byte = [0];
            assert_eq!(
                stderr.read(&mut byte).unwrap(),
                1,
                "process exited before prompt {prompt:?}: {}",
                String::from_utf8_lossy(&errors)
            );
            errors.push(byte[0]);
        }
        let mut attributes = std::mem::MaybeUninit::<libc::termios>::uninit();
        assert_eq!(
            unsafe { libc::tcgetattr(terminal.as_raw_fd(), attributes.as_mut_ptr()) },
            0
        );
        assert_eq!(unsafe { attributes.assume_init() }.c_lflag & libc::ECHO, 0);
        writeln!(terminal, "{password}").unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "zarc did not exit");
        std::thread::sleep(Duration::from_millis(10));
    };
    let mut attributes = std::mem::MaybeUninit::<libc::termios>::uninit();
    assert_eq!(
        unsafe { libc::tcgetattr(terminal.as_raw_fd(), attributes.as_mut_ptr()) },
        0
    );
    assert_ne!(unsafe { attributes.assume_init() }.c_lflag & libc::ECHO, 0);
    stderr.read_to_end(&mut errors).unwrap();
    let mut stdout = Vec::new();
    child
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut stdout)
        .unwrap();
    Output {
        status,
        stdout,
        stderr: errors,
    }
}

#[cfg(unix)]
#[test]
fn cli_interactively_encrypts_and_decrypts_a_commit() {
    let temp = TestDirectory::new();
    let source = temp.path().join("input.txt");
    let archive = temp.path().join("plain.zar");
    let encrypted = temp.path().join("encrypted.zar");
    let decrypted = temp.path().join("decrypted.zar");
    let destination = temp.path().join("output");
    std::fs::write(&source, b"interactive encryption").unwrap();
    let created = zarc([
        OsStr::new("--no-config"),
        OsStr::new("create"),
        OsStr::new("--no-progress"),
        archive.as_os_str(),
        source.as_os_str(),
    ]);
    assert!(created.status.success(), "{created:?}");
    let json = zarc([
        OsStr::new("--no-config"),
        OsStr::new("info"),
        OsStr::new("--json"),
        archive.as_os_str(),
    ]);
    assert!(json.status.success(), "{json:?}");
    let info: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    let id = OsStr::new(info["commits"][0]["id"].as_str().unwrap());

    let result = zarc_with_terminal(
        &[
            OsStr::new("encrypt"),
            archive.as_os_str(),
            OsStr::new("--from"),
            id,
            OsStr::new("--to"),
            id,
            OsStr::new("--output"),
            encrypted.as_os_str(),
            OsStr::new("--aead"),
            OsStr::new("chacha20-poly1305"),
            OsStr::new("--encryption-scope"),
            OsStr::new("header-and-data"),
            OsStr::new("--strict"),
        ],
        &[("New encryption password: ", "test password")],
    );
    assert!(result.status.success(), "{result:?}");
    assert!(String::from_utf8_lossy(&result.stdout).contains("transformation completed"));
    let commit_prompt = format!("Password for commit {}: ", id.to_string_lossy());
    let duplicate = zarc_with_terminal(
        &[
            OsStr::new("decrypt"),
            encrypted.as_os_str(),
            OsStr::new("--from"),
            id,
            OsStr::new("--to"),
            id,
            OsStr::new("--password-for"),
            id,
            OsStr::new("--password-for"),
            id,
        ],
        &[(&commit_prompt, "test password")],
    );
    assert!(!duplicate.status.success(), "{duplicate:?}");
    assert!(String::from_utf8_lossy(&duplicate.stderr).contains("specified more than once"));
    let result = zarc_with_terminal(
        &[
            OsStr::new("decrypt"),
            encrypted.as_os_str(),
            OsStr::new("--from"),
            id,
            OsStr::new("--to"),
            id,
            OsStr::new("--output"),
            decrypted.as_os_str(),
            OsStr::new("--password-for"),
            id,
            OsStr::new("--strict"),
        ],
        &[(&commit_prompt, "test password")],
    );
    assert!(result.status.success(), "{result:?}");
    assert!(String::from_utf8_lossy(&result.stdout).contains("transformation completed"));
    assert!(!String::from_utf8_lossy(&result.stderr).contains("test password"));
    let extracted = zarc([
        OsStr::new("--no-config"),
        OsStr::new("extract"),
        OsStr::new("--no-progress"),
        decrypted.as_os_str(),
        OsStr::new("--output"),
        destination.as_os_str(),
    ]);
    assert!(extracted.status.success(), "{extracted:?}");
    assert_eq!(
        std::fs::read(destination.join("input.txt")).unwrap(),
        b"interactive encryption"
    );
}

#[cfg(unix)]
#[test]
fn cli_interactively_creates_and_adds_with_commit_passwords() {
    let temp = TestDirectory::new();
    let first = temp.path().join("first.txt");
    let second = temp.path().join("second.txt");
    let archive = temp.path().join("archive.zar");
    std::fs::write(&first, b"first secret").unwrap();
    std::fs::write(&second, b"second secret").unwrap();
    let created = zarc_with_terminal(
        &[
            OsStr::new("create"),
            OsStr::new("--no-progress"),
            OsStr::new("--password"),
            OsStr::new("--encryption-scope"),
            OsStr::new("data"),
            archive.as_os_str(),
            first.as_os_str(),
        ],
        &[("Archive password: ", "first password")],
    );
    assert!(created.status.success(), "{created:?}");
    let info = zarc([
        OsStr::new("--no-config"),
        OsStr::new("info"),
        OsStr::new("--json"),
        archive.as_os_str(),
    ]);
    assert!(info.status.success(), "{info:?}");
    let info: serde_json::Value = serde_json::from_slice(&info.stdout).unwrap();
    let id = OsStr::new(info["commits"][0]["id"].as_str().unwrap());
    let commit_prompt = format!("Password for commit {}: ", id.to_string_lossy());
    let added = zarc_with_terminal(
        &[
            OsStr::new("add"),
            OsStr::new("--no-progress"),
            OsStr::new("--password"),
            OsStr::new("--password-for"),
            id,
            OsStr::new("--encryption-scope"),
            OsStr::new("header"),
            OsStr::new("--aead"),
            OsStr::new("chacha20-poly1305"),
            archive.as_os_str(),
            second.as_os_str(),
        ],
        &[
            ("New commit password: ", "second password"),
            (&commit_prompt, "first password"),
        ],
    );
    assert!(added.status.success(), "{added:?}");
    let info = zarc([
        OsStr::new("--no-config"),
        OsStr::new("info"),
        OsStr::new("--json"),
        archive.as_os_str(),
    ]);
    assert!(info.status.success(), "{info:?}");
    let info: serde_json::Value = serde_json::from_slice(&info.stdout).unwrap();
    assert_eq!(info["commit_count"], 2);
    assert_eq!(info["commits"][0]["header_encrypted"], false);
    assert_eq!(info["commits"][0]["data_encrypted"], true);
    assert_eq!(info["commits"][1]["header_encrypted"], true);
    assert_eq!(info["commits"][1]["data_encrypted"], false);
    assert_eq!(
        info["commits"][1]["encryption_algorithm"],
        "chacha20-poly1305"
    );
}

#[test]
fn cli_grep_returns_context_and_preserves_bytes() {
    let temp = TestDirectory::new();
    let source = temp.path().join("input.txt");
    let other = temp.path().join("other.bin");
    let archive = temp.path().join("archive.zar");
    std::fs::write(&source, b"before\n\xffmatch\nafter\n").unwrap();
    std::fs::write(&other, b"match\n").unwrap();
    let output = zarc([
        OsStr::new("--no-config"),
        OsStr::new("create"),
        archive.as_os_str(),
        source.as_os_str(),
        other.as_os_str(),
    ]);
    assert!(output.status.success(), "{output:?}");
    let output = zarc([
        OsStr::new("--no-config"),
        OsStr::new("grep"),
        archive.as_os_str(),
        OsStr::new("match"),
        OsStr::new("--file-filter"),
        OsStr::new("*.txt"),
        OsStr::new("-B"),
        OsStr::new("1"),
        OsStr::new("-A"),
        OsStr::new("1"),
    ]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"input.txt-1-before\ninput.txt:2:\xffmatch\ninput.txt-3-after\n"
    );
    let fancy = zarc([
        OsStr::new("--no-config"),
        OsStr::new("grep"),
        archive.as_os_str(),
        OsStr::new("(?<=mat)ch"),
        OsStr::new("--file-filter"),
        OsStr::new("*.txt"),
    ]);
    assert!(fancy.status.success(), "{fancy:?}");
    assert_eq!(fancy.stdout, b"input.txt:2:\xffmatch\n");
    let absent = zarc([
        OsStr::new("--no-config"),
        OsStr::new("grep"),
        archive.as_os_str(),
        OsStr::new("absent"),
    ]);
    assert_eq!(absent.status.code(), Some(1));
    assert!(absent.stdout.is_empty());
    let invalid = zarc([
        OsStr::new("--no-config"),
        OsStr::new("grep"),
        archive.as_os_str(),
        OsStr::new("("),
    ]);
    assert!(!invalid.status.success());
    assert!(!invalid.stderr.is_empty());
}

#[test]
fn cli_grep_keeps_streamed_output_when_a_later_match_errors() {
    let temp = TestDirectory::new();
    let source = temp.path().join("input.txt");
    let archive = temp.path().join("archive.zar");
    std::fs::write(
        &source,
        b"match\nabababababababababababababababababababababababababababab\n",
    )
    .unwrap();
    let created = zarc([
        OsStr::new("--no-config"),
        OsStr::new("create"),
        archive.as_os_str(),
        source.as_os_str(),
    ]);
    assert!(created.status.success(), "{created:?}");
    let output = zarc([
        OsStr::new("--no-config"),
        OsStr::new("grep"),
        archive.as_os_str(),
        OsStr::new(r"match|(?i)(a|b|ab)*(?>c)"),
    ]);
    assert!(!output.status.success());
    assert_eq!(output.stdout, b"input.txt:1:match\n");
    assert!(String::from_utf8_lossy(&output.stderr).contains("backtrack"));
}

#[test]
fn cli_grep_shell_searches_and_recovers_from_command_errors() {
    let temp = TestDirectory::new();
    let source = temp.path().join("my file.txt");
    let other = temp.path().join("other.bin");
    let archive = temp.path().join("archive.zar");
    std::fs::write(&source, b"before\n\xffhello world\nafter\n").unwrap();
    std::fs::write(&other, b"hello world\n").unwrap();
    let created = zarc([
        OsStr::new("--no-config"),
        OsStr::new("create"),
        archive.as_os_str(),
        source.as_os_str(),
        other.as_os_str(),
    ]);
    assert!(created.status.success(), "{created:?}");

    let mut child = Command::new(env!("CARGO_BIN_EXE_zarc"))
        .args(["--no-config", "grep", "--shell", "-B", "1", "-A", "1"])
        .arg(&archive)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(
        b"/help\n/help extra\n/unknown\n/find\n/grep *.txt (\n/find 'my file.txt'\n/grep '*.txt' hello world\n/quit\n/find *\n",
    ).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        output.stdout,
        b"/find <fnmatch>         List matching archive paths\n\
/grep <fnmatch> <pattern> Search matching files\n\
/before <cnt>           Set before-context lines\n\
/after <cnt>            Set after-context lines\n\
/chain <0|1>            Disable or start a new grep chain\n\
/regex <0|1>            Use literal text or regex\n\
/case <0|1>             Disable or enable case sensitivity\n\
/word <0|1>             Disable or enable word boundaries\n\
/help                   Show this help\n\
/quit                   Exit the shell\n\
Press Esc to cancel a running grep.\n\
my file.txt\nmy file.txt-1-before\nmy file.txt:2:\xffhello world\nmy file.txt-3-after\n"
    );
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(errors.contains("unknown command"), "{errors}");
    assert!(errors.contains("usage: /help"), "{errors}");
    assert!(errors.contains("expected a fnmatch pattern"), "{errors}");
    assert!(errors.contains("zarc grep:"), "{errors}");
}

#[test]
fn cli_grep_shell_changes_pattern_case_and_word_modes() {
    let temp = TestDirectory::new();
    let archive = temp.path().join("archive.zar");
    let source = temp.path().join("input.txt");
    std::fs::write(
        &source,
        "cat\nCAT\nscatter\nc.t\ncot\nélan\nélanx\ndog\ndogs\n[cat]\n",
    )
    .unwrap();
    let created = zarc([
        OsStr::new("--no-config"),
        OsStr::new("create"),
        archive.as_os_str(),
        source.as_os_str(),
    ]);
    assert!(created.status.success(), "{created:?}");
    let mut child = Command::new(env!("CARGO_BIN_EXE_zarc"))
        .args(["--no-config", "grep", "--shell"])
        .arg(&archive)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            "/grep *.txt c.t\n/regex 0\n/grep *.txt c.t\n\
         /case 0\n/grep *.txt CAT\n/word 1\n/grep *.txt CAT\n\
         /grep *.txt élan\n/regex 1\n/grep *.txt cat|dog\n\
         /case 1\n/grep *.txt cat|dog\n/regex 2\n/case -1\n/word foo\n\
         /grep *.txt c.t|dog\n/word 0\n/grep *.txt cat\n\
         /regex 0\n/grep *.txt [\n/quit\n"
                .as_bytes(),
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "input.txt:1:cat\ninput.txt:3:scatter\ninput.txt:4:c.t\ninput.txt:5:cot\ninput.txt:10:[cat]\n\
         input.txt:4:c.t\n\
         input.txt:1:cat\ninput.txt:2:CAT\ninput.txt:3:scatter\ninput.txt:10:[cat]\n\
         input.txt:1:cat\ninput.txt:2:CAT\ninput.txt:10:[cat]\n\
         input.txt:6:élan\n\
         input.txt:1:cat\ninput.txt:2:CAT\ninput.txt:8:dog\ninput.txt:10:[cat]\n\
         input.txt:1:cat\ninput.txt:8:dog\ninput.txt:10:[cat]\n\
         input.txt:1:cat\ninput.txt:4:c.t\ninput.txt:5:cot\ninput.txt:8:dog\ninput.txt:10:[cat]\n\
         input.txt:1:cat\ninput.txt:3:scatter\ninput.txt:10:[cat]\n\
         input.txt:10:[cat]\n"
    );
    let errors = String::from_utf8(output.stderr).unwrap();
    assert!(errors.contains("usage: /regex <0|1>"), "{errors}");
    assert!(errors.contains("usage: /case <0|1>"), "{errors}");
    assert!(errors.contains("usage: /word <0|1>"), "{errors}");
}

#[test]
fn cli_grep_shell_chains_filters_and_changes_context() {
    let temp = TestDirectory::new();
    let archive = temp.path().join("archive.zar");
    let a = temp.path().join("a.txt");
    let b = temp.path().join("b.txt");
    let c = temp.path().join("c.txt");
    std::fs::write(&a, b"before\nalpha\nbeta\nafter\n").unwrap();
    std::fs::write(&b, b"before\nalpha\nother\n").unwrap();
    std::fs::write(&c, b"beta\n").unwrap();
    let created = zarc([
        OsStr::new("--no-config"),
        OsStr::new("create"),
        archive.as_os_str(),
        a.as_os_str(),
        b.as_os_str(),
        c.as_os_str(),
    ]);
    assert!(created.status.success(), "{created:?}");
    let mut child = Command::new(env!("CARGO_BIN_EXE_zarc"))
        .args(["--no-config", "grep", "--shell"])
        .arg(&archive)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"/before 1\n/after 1\n/chain 1\n/grep *.txt alpha\n/before 0\n/after 0\n\
          /regex 0\n/case 0\n/word 1\n/grep a*.txt BETA\n\
          /regex 1\n/case 1\n/word 0\n/grep *.txt (\n/grep *.txt alpha\n/grep *.txt absent\n\
          /grep *.txt beta\n/chain 1\n/grep c*.txt beta\n/chain 0\n/grep *.txt beta\n\
          /chain 2\n/before -1\n/after foo\n/quit\n",
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.matches("a.txt:2:alpha\n").count(), 2, "{stdout}");
    assert_eq!(stdout.matches("b.txt:2:alpha\n").count(), 1, "{stdout}");
    assert_eq!(stdout.matches("a.txt:3:beta\n").count(), 2, "{stdout}");
    assert_eq!(stdout.matches("c.txt:1:beta\n").count(), 2, "{stdout}");
    assert_eq!(stdout.matches("a.txt-1-before\n").count(), 1, "{stdout}");
    assert_eq!(stdout.matches("a.txt-3-beta\n").count(), 1, "{stdout}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("usage: /chain <0|1>"), "{stderr}");
    assert_eq!(
        stderr
            .matches("expected a nonnegative context line count")
            .count(),
        2,
        "{stderr}"
    );
}

#[cfg(unix)]
#[test]
fn cli_grep_shell_escape_cancels_and_restores_terminal() {
    use std::io::Read;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::process::CommandExt;
    use std::time::{Duration, Instant};

    let temp = TestDirectory::new();
    let source = temp.path().join("long.txt");
    let other = temp.path().join("other.txt");
    let archive = temp.path().join("archive.zar");
    let mut contents = b"needle\n".to_vec();
    contents.extend_from_slice(&b"padding\n".repeat(2_000_000));
    std::fs::write(&source, contents).unwrap();
    std::fs::write(&other, b"needle\n").unwrap();
    let created = zarc([
        OsStr::new("--no-config"),
        OsStr::new("create"),
        archive.as_os_str(),
        source.as_os_str(),
        other.as_os_str(),
    ]);
    assert!(created.status.success(), "{created:?}");

    let mut master_fd = -1;
    let mut slave_fd = -1;
    assert_eq!(
        unsafe {
            libc::openpty(
                &mut master_fd,
                &mut slave_fd,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
            )
        },
        0
    );
    let mut master = unsafe { std::fs::File::from_raw_fd(master_fd) };
    let slave = unsafe { std::fs::File::from_raw_fd(slave_fd) };
    let mut initial = std::mem::MaybeUninit::<libc::termios>::uninit();
    assert_eq!(
        unsafe { libc::tcgetattr(slave_fd, initial.as_mut_ptr()) },
        0
    );
    let initial = unsafe { initial.assume_init() };
    let mut command = Command::new(env!("CARGO_BIN_EXE_zarc"));
    command
        .args(["--no-config", "grep", "--shell"])
        .arg(&archive)
        .stdin(Stdio::from(slave.try_clone().unwrap()))
        .stdout(Stdio::from(slave.try_clone().unwrap()))
        .stderr(Stdio::from(slave.try_clone().unwrap()));
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 || libc::ioctl(0, libc::TIOCSCTTY, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }
    let mut child = ChildGuard(command.spawn().unwrap());
    let mut output = Vec::new();
    let mut read_until = |needle: &[u8], master: &mut std::fs::File| {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !output.windows(needle.len()).any(|window| window == needle) {
            assert!(
                Instant::now() < deadline,
                "terminal output: {}",
                String::from_utf8_lossy(&output)
            );
            let mut fd = libc::pollfd {
                fd: master.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let ready = unsafe { libc::poll(&mut fd, 1, 100) };
            assert!(ready >= 0);
            if ready > 0 {
                let mut bytes = [0; 4096];
                let read = master.read(&mut bytes).unwrap();
                assert!(read > 0, "terminal closed");
                output.extend_from_slice(&bytes[..read]);
            }
        }
        let end = output
            .windows(needle.len())
            .position(|window| window == needle)
            .unwrap()
            + needle.len();
        output.drain(..end).collect::<Vec<_>>()
    };
    read_until(b"zarc grep> ", &mut master);
    master.write_all(b"/chain 1\n").unwrap();
    read_until(b"zarc grep> ", &mut master);
    master.write_all(b"/grep long.txt needle\n").unwrap();
    read_until(b"long.txt:1:needle", &mut master);
    read_until(b"zarc grep> ", &mut master);
    master.write_all(b"/grep * needle\n").unwrap();
    read_until(b"long.txt:1:needle", &mut master);
    master.write_all(b"\x1b").unwrap();
    read_until(b"search cancelled", &mut master);
    read_until(b"zarc grep> ", &mut master);
    master.write_all(b"/grep other.txt needle\n").unwrap();
    let restricted = read_until(b"zarc grep> ", &mut master);
    assert!(
        !restricted
            .windows(b"other.txt:1:needle".len())
            .any(|bytes| bytes == b"other.txt:1:needle")
    );
    master.write_all(b"/chain 0\n").unwrap();
    read_until(b"zarc grep> ", &mut master);
    master.write_all(b"/grep other.txt needle\n").unwrap();
    read_until(b"other.txt:1:needle", &mut master);
    read_until(b"zarc grep> ", &mut master);
    master.write_all(b"/quit\n").unwrap();
    assert!(child.0.wait().unwrap().success());
    let mut restored = std::mem::MaybeUninit::<libc::termios>::uninit();
    assert_eq!(
        unsafe { libc::tcgetattr(slave_fd, restored.as_mut_ptr()) },
        0
    );
    let restored = unsafe { restored.assume_init() };
    assert_eq!(
        restored.c_lflag & (libc::ICANON | libc::ECHO),
        initial.c_lflag & (libc::ICANON | libc::ECHO)
    );
    assert_eq!(
        restored.c_oflag & libc::OPOST,
        initial.c_oflag & libc::OPOST
    );
}

#[test]
fn cli_grep_shell_requires_archive_and_rejects_regular_search_arguments() {
    for args in [
        vec!["grep", "--shell"],
        vec!["grep", "archive.zar"],
        vec!["grep", "archive.zar", "pattern", "--shell"],
        vec!["grep", "archive.zar", "--shell", "--file-filter", "*.txt"],
    ] {
        let output = zarc(std::iter::once("--no-config").chain(args));
        assert!(!output.status.success(), "{output:?}");
    }
    let output = zarc(["--no-config", "grep", "missing.zar", "--shell"]);
    assert!(!output.status.success());
    assert!(!output.stderr.is_empty());
}

#[test]
fn cli_encrypted_round_trip_with_password_stdin() {
    let temp = TestDirectory::new();
    let source = temp.path().join("input.txt");
    let archive = temp.path().join("archive.zar");
    let destination = temp.path().join("output");
    std::fs::write(&source, b"encrypted contents").unwrap();
    for args in [
        vec![
            OsStr::new("create"),
            OsStr::new("--password-stdin"),
            archive.as_os_str(),
            source.as_os_str(),
        ],
        vec![
            OsStr::new("extract"),
            OsStr::new("--password-stdin"),
            archive.as_os_str(),
            OsStr::new("--output"),
            destination.as_os_str(),
        ],
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_zarc"))
            .arg("--no-config")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"test password\n")
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(!String::from_utf8_lossy(&output.stderr).contains("test password"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("test password"));
    }
    assert_eq!(
        std::fs::read(destination.join("input.txt")).unwrap(),
        b"encrypted contents"
    );
    for password in ["wrong password\n", "\n"] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_zarc"))
            .args(["--no-config", "extract", "--password-stdin"])
            .arg(&archive)
            .arg("--output")
            .arg(temp.path().join("rejected"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(password.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert!(!String::from_utf8_lossy(&output.stderr).contains("wrong password"));
    }
}

#[test]
fn cli_reports_encrypted_archives_without_disclosing_contents() {
    let temp = TestDirectory::new();
    let source = temp.path().join("secret.txt");
    std::fs::write(&source, b"secret content").unwrap();
    for algorithm in ["aes-256-gcm", "chacha20-poly1305"] {
        let archive = temp.path().join(format!("{algorithm}.zar"));
        let mut child = Command::new(env!("CARGO_BIN_EXE_zarc"))
            .args([
                "--no-config",
                "create",
                "--no-progress",
                "--password-stdin",
                "--aead",
                algorithm,
            ])
            .arg(&archive)
            .arg(&source)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"password\n")
            .unwrap();
        let created = child.wait_with_output().unwrap();
        assert!(created.status.success(), "{created:?}");

        let json = zarc([
            OsStr::new("--no-config"),
            OsStr::new("info"),
            OsStr::new("--json"),
            archive.as_os_str(),
        ]);
        assert!(json.status.success(), "{json:?}");
        let info: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
        assert_eq!(info["commits"][0]["encryption_algorithm"], algorithm);
        assert_eq!(info["commits"][0]["header_encrypted"], true);
        assert_eq!(info["commits"][0]["data_encrypted"], true);

        let info = zarc([
            OsStr::new("--no-config"),
            OsStr::new("info"),
            OsStr::new("--verbose"),
            archive.as_os_str(),
        ]);
        assert!(info.status.success(), "{info:?}");
        let info = String::from_utf8(info.stdout).unwrap();
        assert!(
            info.contains("detailed file statistics unavailable"),
            "{info}"
        );
        assert!(info.contains("unavailable (encrypted header)"), "{info}");

        let check = zarc([
            OsStr::new("--no-config"),
            OsStr::new("check"),
            OsStr::new("--no-progress"),
            archive.as_os_str(),
        ]);
        assert!(check.status.success(), "{check:?}");
        let check = String::from_utf8(check.stdout).unwrap();
        assert!(check.contains("content not checked:"), "{check}");
        assert!(
            check.contains("logical TOC consistency not fully checked"),
            "{check}"
        );

        let list = zarc([
            OsStr::new("--no-config"),
            OsStr::new("list"),
            OsStr::new("--verbose"),
            archive.as_os_str(),
        ]);
        assert!(list.status.success(), "{list:?}");
        let list = String::from_utf8(list.stdout).unwrap();
        assert!(!list.contains("secret.txt"), "{list}");
        assert!(
            list.contains("Ignored commits (password not provided)"),
            "{list}"
        );
    }
}

#[test]
fn cli_extracts_with_each_checksum_recheck_mode() {
    let temp = TestDirectory::new();
    let source = temp.path().join("input.txt");
    let archive = temp.path().join("archive.zar");
    std::fs::write(&source, b"checksum recheck contents").unwrap();
    let output = zarc([
        OsStr::new("--no-config"),
        OsStr::new("create"),
        archive.as_os_str(),
        source.as_os_str(),
    ]);
    assert!(output.status.success(), "{output:?}");
    for mode in ["auto", "always", "never"] {
        let destination = temp.path().join(mode);
        let output = zarc([
            OsStr::new("--no-config"),
            OsStr::new("extract"),
            archive.as_os_str(),
            OsStr::new("--output"),
            destination.as_os_str(),
            OsStr::new("--recheck-block-checksums"),
            OsStr::new(mode),
        ]);
        assert!(output.status.success(), "{mode}: {output:?}");
        assert_eq!(
            std::fs::read(destination.join("input.txt")).unwrap(),
            b"checksum recheck contents"
        );
    }
}

#[test]
fn cli_configuration_controls_defaults_and_can_be_overridden_or_disabled() {
    let temp = TestDirectory::new();
    std::fs::write(temp.path().join("keep.txt"), b"text").unwrap();
    std::fs::write(temp.path().join("keep.bin"), b"binary").unwrap();
    let config = temp.path().join("zarc.json");
    std::fs::write(
        &config,
        r#"{"create":{"include-file":["*.txt"],"no-progress":true}}"#,
    )
    .unwrap();
    for (archive, extra, expected, excluded) in [
        ("configured.zar", vec![], "keep.txt", Some("keep.bin")),
        (
            "overridden.zar",
            vec!["--include-file", "*.bin"],
            "keep.bin",
            Some("keep.txt"),
        ),
        ("disabled.zar", vec!["--no-config"], "keep.bin", None),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_zarc"))
            .current_dir(temp.path())
            .args(["create", archive, "keep.txt", "keep.bin"])
            .args(extra)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let output = zarc([
            OsStr::new("--no-config"),
            OsStr::new("list"),
            temp.path().join(archive).as_os_str(),
        ]);
        assert!(output.status.success(), "{output:?}");
        let listing = String::from_utf8(output.stdout).unwrap();
        assert!(listing.contains(expected), "{listing}");
        if let Some(excluded) = excluded {
            assert!(!listing.contains(excluded), "{listing}");
        } else {
            assert!(listing.contains("keep.txt"), "{listing}");
        }
    }
    std::fs::write(&config, "invalid JSON").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_zarc"))
        .current_dir(temp.path())
        .args(["list", "configured.zar"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("zarc.json"));
    let output = Command::new(env!("CARGO_BIN_EXE_zarc"))
        .current_dir(temp.path())
        .args(["list", "configured.zar", "--no-config"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    #[cfg(target_os = "linux")]
    {
        std::fs::remove_file(&config).unwrap();
        let user_config = temp.path().join("user-config");
        std::fs::create_dir(&user_config).unwrap();
        std::fs::write(user_config.join("zarc.json"), r#"{"info":{"json":true}}"#).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_zarc"))
            .current_dir(temp.path())
            .env("XDG_CONFIG_HOME", &user_config)
            .args(["info", "configured.zar"])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let info: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(info["commit_count"], 1);
    }
}

#[test]
fn cli_process_creates_lists_and_extracts_with_filters() {
    let temp = TestDirectory::new();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("keep.txt"), b"keep this").unwrap();
    std::fs::write(source.join("skip.bin"), b"skip this").unwrap();
    let archive = temp.path().join("archive.zar");
    let output = zarc([
        OsStr::new("create"),
        OsStr::new("--include-file"),
        OsStr::new("*.txt"),
        archive.as_os_str(),
        source.as_os_str(),
    ]);
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("Archive size:"));

    let output = zarc([OsStr::new("list"), archive.as_os_str()]);
    assert!(output.status.success(), "{output:?}");
    let listing = String::from_utf8(output.stdout).unwrap();
    assert!(listing.contains("source/keep.txt"));
    assert!(!listing.contains("skip.bin"));

    let output = zarc([
        OsStr::new("info"),
        OsStr::new("--json"),
        archive.as_os_str(),
    ]);
    assert!(output.status.success(), "{output:?}");
    let json = String::from_utf8(output.stdout).unwrap();
    assert!(json.contains("\"commit_count\":1"));
    assert!(json.contains("\"files_added_or_updated\":1"));

    let destination = temp.path().join("output");
    let output = zarc([
        OsStr::new("extract"),
        archive.as_os_str(),
        OsStr::new("--output"),
        destination.as_os_str(),
    ]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        std::fs::read(destination.join("source/keep.txt")).unwrap(),
        b"keep this"
    );
    assert!(!destination.join("source/skip.bin").exists());
}

#[test]
fn cli_reports_cross_commit_references_and_blocks_source_removal() {
    let temp = TestDirectory::new();
    let first = temp.path().join("first.txt");
    let duplicate = temp.path().join("duplicate.txt");
    let archive = temp.path().join("archive.zar");
    std::fs::write(&first, b"identical content").unwrap();
    std::fs::write(&duplicate, b"identical content").unwrap();
    let created = zarc([OsStr::new("create"), archive.as_os_str(), first.as_os_str()]);
    assert!(created.status.success(), "{created:?}");
    let added = zarc([
        OsStr::new("add"),
        archive.as_os_str(),
        duplicate.as_os_str(),
    ]);
    assert!(added.status.success(), "{added:?}");
    let references = zarc([OsStr::new("references"), archive.as_os_str()]);
    assert!(references.status.success(), "{references:?}");
    let references = String::from_utf8(references.stdout).unwrap();
    assert!(references.contains("00000002\t00000001\tduplicate.txt"));
    let info = zarc([
        OsStr::new("info"),
        OsStr::new("--json"),
        archive.as_os_str(),
    ]);
    assert!(info.status.success(), "{info:?}");
    assert!(
        String::from_utf8(info.stdout)
            .unwrap()
            .contains("\"cross_commit_reference_count\":1")
    );
    let before = std::fs::read(&archive).unwrap();
    let removed = zarc([
        OsStr::new("remove-commits"),
        archive.as_os_str(),
        OsStr::new("00000001"),
    ]);
    assert!(!removed.status.success());
    let error = String::from_utf8(removed.stderr).unwrap();
    assert!(
        error.contains("00000001") && error.contains("00000002") && error.contains("duplicate.txt")
    );
    assert_eq!(std::fs::read(archive).unwrap(), before);
}

#[test]
fn cli_rename_and_move_preserve_file_contents() {
    let temp = TestDirectory::new();
    let first = temp.path().join("first.txt");
    let archive = temp.path().join("archive.zar");
    std::fs::write(&first, b"rename contents").unwrap();
    let created = zarc([OsStr::new("create"), archive.as_os_str(), first.as_os_str()]);
    assert!(created.status.success(), "{created:?}");
    let renamed = zarc([
        OsStr::new("rename"),
        archive.as_os_str(),
        OsStr::new("first.txt"),
        OsStr::new("renamed.txt"),
    ]);
    assert!(renamed.status.success(), "{renamed:?}");
    let moved = zarc([
        OsStr::new("move"),
        archive.as_os_str(),
        OsStr::new("renamed.txt"),
        OsStr::new("folder/moved.txt"),
    ]);
    assert!(moved.status.success(), "{moved:?}");
    let output = temp.path().join("output");
    let extracted = zarc([
        OsStr::new("extract"),
        archive.as_os_str(),
        OsStr::new("--output"),
        output.as_os_str(),
    ]);
    assert!(extracted.status.success(), "{extracted:?}");
    assert_eq!(
        std::fs::read(output.join("folder/moved.txt")).unwrap(),
        b"rename contents"
    );
}

#[test]
fn cli_process_reports_help_and_error_exit_codes() {
    let help = zarc(["--help"]);
    assert!(help.status.success());
    assert!(String::from_utf8(help.stdout).unwrap().contains("Usage:"));

    let invalid = zarc(["not-a-command"]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8(invalid.stderr).unwrap().contains("error"));

    let temp = TestDirectory::new();
    let missing = temp.path().join("missing.zar");
    let io_error = zarc([OsStr::new("check"), missing.as_os_str()]);
    assert_eq!(io_error.status.code(), Some(3));
    assert!(
        String::from_utf8(io_error.stderr)
            .unwrap()
            .contains("zarc:")
    );

    let invalid_archive = temp.path().join("invalid.zar");
    std::fs::write(&invalid_archive, b"not an archive").unwrap();
    let format_error = zarc([OsStr::new("check"), invalid_archive.as_os_str()]);
    assert_eq!(format_error.status.code(), Some(4));
    assert!(
        String::from_utf8(format_error.stderr)
            .unwrap()
            .contains("zarc:")
    );

    let password_error = zarc([
        OsStr::new("check"),
        OsStr::new("--password"),
        missing.as_os_str(),
    ]);
    #[cfg(unix)]
    let (expected_code, expected_message) = (2, "password entry requires an interactive terminal");
    #[cfg(not(unix))]
    let (expected_code, expected_message) = (
        4,
        "password entry without terminal echo is not supported on this platform",
    );
    assert_eq!(password_error.status.code(), Some(expected_code));
    assert!(
        String::from_utf8(password_error.stderr)
            .unwrap()
            .contains(expected_message)
    );
}
