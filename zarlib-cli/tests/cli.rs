use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
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
    assert_eq!(password_error.status.code(), Some(2));
    assert!(
        String::from_utf8(password_error.stderr)
            .unwrap()
            .contains("password entry requires an interactive terminal")
    );
}
