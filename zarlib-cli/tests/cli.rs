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
