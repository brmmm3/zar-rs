use super::*;
use crate::{ArchivePath, TocEntry};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

struct Fixture {
    dir: tempfile::TempDir,
    archive: PathBuf,
    input: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input");
        std::fs::create_dir_all(input.join("nested")).unwrap();
        std::fs::write(input.join("a.txt"), b"alpha content").unwrap();
        std::fs::write(input.join("nested/b.txt"), b"beta content").unwrap();
        let archive = dir.path().join("archive.zar");
        create_archive_with_password_progress_and_special_files(
            archive.to_str().unwrap(),
            std::slice::from_ref(&input),
            false,
            EncodingOptions::default(),
            None,
            None,
            CreateInputOptions {
                save_metadata: true,
                save_dates: true,
                ..Default::default()
            },
        )
        .unwrap();
        Self {
            dir,
            archive,
            input,
        }
    }

    fn archive(&self) -> &str {
        self.archive.to_str().unwrap()
    }
}

#[derive(Default)]
struct TestProgress {
    messages: Mutex<Vec<String>>,
    advances: Mutex<Vec<u64>>,
    total: Mutex<u64>,
    decision: Mutex<Option<OverwriteDecision>>,
    confirm: bool,
}

impl Progress for TestProgress {
    fn message(&self, message: &str) {
        self.messages.lock().unwrap().push(message.to_owned());
    }

    fn advance(&self, amount: u64) {
        self.advances.lock().unwrap().push(amount);
    }

    fn set_total(&self, total: u64) {
        *self.total.lock().unwrap() = total;
    }

    fn is_interactive(&self) -> bool {
        true
    }

    fn confirm(&self, _: &str) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(self.confirm)
    }

    fn confirm_overwrite(&self, _: &str) -> Result<OverwriteDecision, Box<dyn std::error::Error>> {
        Ok(self
            .decision
            .lock()
            .unwrap()
            .unwrap_or(OverwriteDecision::No))
    }
}

#[derive(Default)]
struct CancelAfterAddProgress(AtomicBool);

impl Progress for CancelAfterAddProgress {
    fn message(&self, message: &str) {
        if message.starts_with("Adding ") {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    fn advance(&self, _: u64) {}

    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

struct CancelOnMessageProgress {
    prefix: &'static str,
    cancelled: AtomicBool,
}

impl CancelOnMessageProgress {
    fn new(prefix: &'static str) -> Self {
        Self {
            prefix,
            cancelled: AtomicBool::new(false),
        }
    }
}

impl Progress for CancelOnMessageProgress {
    fn message(&self, message: &str) {
        if message.starts_with(self.prefix) {
            self.cancelled.store(true, Ordering::Relaxed);
        }
    }

    fn advance(&self, _: u64) {}

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

fn options(
    collision_policy: CollisionPolicy,
    existing_policy: ExistingPolicy,
) -> ExtractionOptions<'static> {
    ExtractionOptions {
        dry_run: false,
        to: None,
        collision_policy,
        existing_policy,
        restore_metadata: true,
        restore_symlinks: false,
        restore_devices: false,
        strip_top_level: false,
    }
}

#[test]
fn cancelling_add_during_file_processing_preserves_archive() {
    let fixture = Fixture::new();
    let input = fixture.dir.path().join("cancelled.txt");
    std::fs::write(&input, vec![b'x'; 1024 * 1024]).unwrap();
    let original = std::fs::read(fixture.archive()).unwrap();
    let progress = CancelAfterAddProgress::default();

    let result = add_archive_with_encoding_progress(
        fixture.archive(),
        &[input],
        EncodingOptions::default(),
        Some(&progress),
        false,
    );

    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("operation cancelled")
    );
    assert_eq!(std::fs::read(fixture.archive()).unwrap(), original);
}

#[test]
fn cancelling_create_does_not_publish_archive() {
    let fixture = Fixture::new();
    let archive = fixture.dir.path().join("cancelled.zar");
    let progress = CancelOnMessageProgress::new("Creating ");

    let result = create_archive_with_password_progress(
        archive.to_str().unwrap(),
        std::slice::from_ref(&fixture.input),
        false,
        EncodingOptions::default(),
        None,
        Some(&progress),
        false,
    );

    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("operation cancelled")
    );
    assert!(!archive.exists());
}

#[test]
fn cancelling_extract_before_writing_leaves_destination_untouched() {
    let fixture = Fixture::new();
    let destination = fixture.dir.path().join("cancelled-extract");
    let progress = CancelOnMessageProgress::new("Extracting files");

    let result = extract_archive_with_progress(
        fixture.archive(),
        &destination,
        options(CollisionPolicy::Rename, ExistingPolicy::Error),
        None,
        Some(&progress),
    );

    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("operation cancelled")
    );
    assert!(!destination.exists());
}

#[test]
fn extract_single_file_through_reads_the_selected_revision() {
    let fixture = Fixture::new();
    std::fs::write(fixture.input.join("a.txt"), b"new content").unwrap();
    add_archive(fixture.archive(), std::slice::from_ref(&fixture.input)).unwrap();
    let output = fixture.dir.path().join("extracted.txt");
    let passwords = PasswordSet::default();
    extract_single_file_through(
        fixture.archive(),
        "input/a.txt",
        &output,
        &passwords,
        Some("00000001"),
    )
    .unwrap();
    assert_eq!(std::fs::read(&output).unwrap(), b"alpha content");
    extract_single_file(fixture.archive(), "input/a.txt", &output, &passwords).unwrap();
    assert_eq!(std::fs::read(&output).unwrap(), b"new content");
}

#[test]
fn list_details_tracks_removals_and_selected_commits() {
    let fixture = Fixture::new();
    let archive = fixture.archive();
    let (toc, details) = list_archive_with_details(archive, None, None).unwrap();
    let path = ArchivePath::new("input/a.txt").unwrap();
    assert!(matches!(toc.get(&path), Some(TocEntry::File(_))));
    assert!(details[&path].modified_unix_millis.is_some());
    assert!(
        load_toc(&mut std::fs::File::open(archive).unwrap())
            .unwrap()
            .get(&path)
            .is_some()
    );

    remove_archive_entries_with_progress(archive, std::slice::from_ref(&path), None).unwrap();
    let (latest, latest_details) = list_archive_with_details(archive, None, None).unwrap();
    assert!(latest.get(&path).is_none());
    assert!(!latest_details.contains_key(&path));
    let (first, first_details) =
        list_archive_with_details(archive, Some("00000001"), None).unwrap();
    assert!(first.get(&path).is_some());
    assert!(first_details.contains_key(&path));
    assert!(list_archive_with_details(archive, Some("ffffffff"), None).is_err());
    assert!(list_archive_with_details(archive, Some("invalid"), None).is_err());
    assert!(
        load_toc_through(archive, Some("00000001"), None)
            .unwrap()
            .get(&path)
            .is_some()
    );
    assert!(load_toc_through(archive, Some("ffffffff"), None).is_err());
}

#[test]
fn list_details_estimates_file_sizes_from_block_ratios() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("archive.zar");
    let first = dir.path().join("a.txt");
    let second = dir.path().join("b.txt");
    let large = dir.path().join("large.txt");
    let empty = dir.path().join("empty.txt");
    let tiny = dir.path().join("tiny.txt");
    std::fs::write(&first, vec![b'a'; 128]).unwrap();
    std::fs::write(&second, vec![b'b'; 256]).unwrap();
    let mut large_bytes = vec![b'c'; 8192];
    large_bytes.extend((0..137).map(|i| (i * 73) as u8));
    std::fs::write(&large, &large_bytes).unwrap();
    std::fs::write(&empty, []).unwrap();
    std::fs::write(&tiny, b"hey").unwrap();
    create_archive_with_encoding(
        archive.to_str().unwrap(),
        &[first, second, large, empty, tiny],
        false,
        EncodingOptions {
            block_size: 4096,
            compression_level: 3,
        },
    )
    .unwrap();
    let (toc, details) = list_archive_with_details(archive.to_str().unwrap(), None, None).unwrap();
    let info = info_archive_with_blocks(archive.to_str().unwrap(), true).unwrap();
    for (name, expected_size) in [
        ("a.txt", 128),
        ("b.txt", 256),
        ("tiny.txt", 3),
        ("large.txt", 8329),
    ] {
        let path = ArchivePath::new(name).unwrap();
        let Some(TocEntry::File(file)) = toc.get(&path) else {
            panic!("missing file {name}");
        };
        assert_eq!(file.size, expected_size);
        if name == "a.txt" || name == "b.txt" {
            assert!(file.offset_in_block.is_some());
        } else if name == "large.txt" {
            assert!(file.block_indices.len() > 1);
        }
        let ratios: Vec<f64> = file
            .block_indices
            .iter()
            .map(|index| {
                info[0]
                    .blocks
                    .iter()
                    .find(|block| block.index == *index)
                    .unwrap()
                    .ratio_percent
                    .unwrap()
                    / 100.0
            })
            .collect();
        let expected_ratio = ratios.iter().sum::<f64>() / ratios.len() as f64;
        if name == "large.txt" {
            assert!(ratios.iter().any(|ratio| (*ratio - ratios[0]).abs() > 0.1));
            let weighted = info[0]
                .blocks
                .iter()
                .filter(|block| file.block_indices.contains(&block.index))
                .map(|block| block.compressed_size)
                .sum::<u64>() as f64
                / file.size as f64;
            assert!((expected_ratio - weighted).abs() > 0.01);
        }
        assert!((details[&path].compression_ratio.unwrap() - expected_ratio).abs() < 1e-12);
        assert_eq!(
            details[&path].compressed_size,
            Some((expected_ratio * file.size as f64).round() as u64)
        );
    }
    let path = ArchivePath::new("empty.txt").unwrap();
    assert_eq!(details[&path].compressed_size, Some(0));
    assert_eq!(details[&path].compression_ratio, None);
}

#[test]
fn remove_directory_and_files_preserves_archive_on_errors() {
    let fixture = Fixture::new();
    let archive = fixture.archive();
    let progress = TestProgress::default();
    let missing = ArchivePath::new("missing").unwrap();
    let original = std::fs::read(&fixture.archive).unwrap();
    assert!(remove_archive_entries_with_progress(archive, &[missing], Some(&progress)).is_err());
    assert_eq!(std::fs::read(&fixture.archive).unwrap(), original);
    let file = ArchivePath::new("input/a.txt").unwrap();
    assert!(remove_archive_entries_with_progress(archive, &[file.clone(), file], None).is_err());
    assert_eq!(std::fs::read(&fixture.archive).unwrap(), original);
    remove_archive_entries_with_progress(
        archive,
        &[ArchivePath::new("input/nested").unwrap()],
        Some(&progress),
    )
    .unwrap();
    let (toc, details) = list_archive_with_details(archive, None, None).unwrap();
    assert!(
        !toc.iter()
            .any(|(path, _)| path.as_str().starts_with("input/nested"))
    );
    assert!(
        !details
            .keys()
            .any(|path| path.as_str().starts_with("input/nested"))
    );
    assert_eq!(*progress.total.lock().unwrap(), 1);
    assert!(!progress.messages.lock().unwrap().is_empty());
    remove_archive(archive, std::slice::from_ref(&fixture.input)).unwrap();
    assert!(
        list_archive(archive, None, None)
            .unwrap()
            .get(&ArchivePath::new("input/a.txt").unwrap())
            .is_none()
    );
}

#[test]
fn check_valid_and_corrupted_archives_and_progress() {
    let fixture = Fixture::new();
    let progress = TestProgress::default();
    let report =
        check_archive_with_recovery_progress(fixture.archive(), false, None, Some(&progress))
            .unwrap();
    assert_eq!(report.commits, 1);
    assert!(report.checked_blocks > 0);
    assert!(report.logical_state_readable);
    assert!(*progress.total.lock().unwrap() > 0);
    assert!(!progress.advances.lock().unwrap().is_empty());

    let mut bytes = std::fs::read(&fixture.archive).unwrap();
    let offset = u64::from_le_bytes(bytes[4..12].try_into().unwrap()) as usize;
    bytes[offset..offset + 4].copy_from_slice(b"BAD!");
    let damaged = fixture.dir.path().join("damaged.zar");
    std::fs::write(&damaged, bytes).unwrap();
    assert!(check_archive_with_recovery(damaged.to_str().unwrap(), false, None).is_err());
    assert!(check_archive_with_recovery(damaged.to_str().unwrap(), true, None).is_err());
}

#[test]
fn check_recovers_next_valid_commit_after_damaged_header() {
    let fixture = Fixture::new();
    add_archive(fixture.archive(), std::slice::from_ref(&fixture.input)).unwrap();
    let mut bytes = std::fs::read(&fixture.archive).unwrap();
    let offset = u64::from_le_bytes(bytes[4..12].try_into().unwrap()) as usize;
    bytes[offset..offset + 4].copy_from_slice(b"BAD!");
    let damaged = fixture.dir.path().join("recover.zar");
    std::fs::write(&damaged, bytes).unwrap();
    assert!(check_archive_with_recovery(damaged.to_str().unwrap(), false, None).is_err());
    let report = check_archive_with_recovery(damaged.to_str().unwrap(), true, None).unwrap();
    assert_eq!(report.recoveries, 1);
    assert_eq!(report.commits, 1);
    assert_eq!(report.recovery_messages.len(), 1);
}

#[test]
fn check_encrypted_archive_with_and_without_password() {
    let fixture = Fixture::new();
    let encrypted = fixture.dir.path().join("encrypted.zar");
    create_archive_with_password_progress_and_filters_and_encryption(
        encrypted.to_str().unwrap(),
        std::slice::from_ref(&fixture.input),
        false,
        EncodingOptions::default(),
        Some((
            b"correct password",
            crate::ContentAead::Aes256Gcm,
            crate::EncryptionScope::HeaderAndData,
        )),
        None,
        CreateFilterOptions {
            input: CreateInputOptions::default(),
            filters: &FilterSet::default(),
        },
    )
    .unwrap();
    let archive = encrypted.to_str().unwrap();
    let locked = check_archive_with_recovery(archive, false, None).unwrap();
    assert!(!locked.logical_state_readable);
    assert!(locked.encrypted_blocks > 0);
    assert!(check_archive_with_recovery(archive, false, Some(b"wrong")).is_err());
    let unlocked = check_archive_with_recovery(archive, false, Some(b"correct password")).unwrap();
    assert!(unlocked.logical_state_readable);
    assert!(unlocked.checked_blocks > 0);
    assert!(list_archive_with_details(archive, None, None).is_err());
    assert!(list_archive_with_details(archive, None, Some(b"correct password")).is_ok());
    let (_, details) = list_archive_with_details(archive, None, Some(b"correct password")).unwrap();
    let path = ArchivePath::new("input/a.txt").unwrap();
    assert!(details[&path].compression_ratio.is_some());
    assert!(details[&path].compressed_size.is_some());
    let data_only = fixture.dir.path().join("data-only.zar");
    create_archive_with_password_progress_and_filters_and_encryption(
        data_only.to_str().unwrap(),
        std::slice::from_ref(&fixture.input),
        false,
        EncodingOptions::default(),
        Some((
            b"correct password",
            crate::ContentAead::Aes256Gcm,
            crate::EncryptionScope::Data,
        )),
        None,
        CreateFilterOptions {
            input: CreateInputOptions::default(),
            filters: &FilterSet::default(),
        },
    )
    .unwrap();
    let (_, locked) = list_archive_with_details(data_only.to_str().unwrap(), None, None).unwrap();
    assert_eq!(locked[&path].compression_ratio, None);
    let (_, unlocked) =
        list_archive_with_details(data_only.to_str().unwrap(), None, Some(b"correct password"))
            .unwrap();
    assert!(unlocked[&path].compression_ratio.is_some());
    let contents = extract_archive_to_memory_with_passwords(
        archive,
        None,
        &PasswordSet::new(Some(b"correct password".to_vec())),
        None,
        0,
    )
    .unwrap();
    assert_eq!(contents["input/a.txt"], b"alpha content");
}

#[cfg(unix)]
#[test]
fn extraction_restores_safe_symlinks_and_pipes_and_rejects_escaping_links() {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{FileTypeExt, symlink};
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    std::fs::create_dir(&input).unwrap();
    std::fs::write(input.join("file"), b"content").unwrap();
    symlink("file", input.join("link")).unwrap();
    let pipe = std::ffi::CString::new(input.join("pipe").as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(pipe.as_ptr(), 0o600) }, 0);
    let archive = dir.path().join("special.zar");
    create_archive_with_password_progress_and_special_files(
        archive.to_str().unwrap(),
        std::slice::from_ref(&input),
        false,
        EncodingOptions::default(),
        None,
        None,
        CreateInputOptions {
            include_special_files: true,
            save_metadata: true,
            ..Default::default()
        },
    )
    .unwrap();
    let mut opts = options(CollisionPolicy::Error, ExistingPolicy::Error);
    opts.restore_symlinks = true;
    let out = dir.path().join("out");
    extract_archive_with_progress(archive.to_str().unwrap(), &out, opts, None, None).unwrap();
    assert_eq!(
        std::fs::read_link(out.join("input/link")).unwrap(),
        Path::new("file")
    );
    assert!(
        std::fs::symlink_metadata(out.join("input/pipe"))
            .unwrap()
            .file_type()
            .is_fifo()
    );
    opts.existing_policy = ExistingPolicy::Skip;
    extract_archive_with_progress(archive.to_str().unwrap(), &out, opts, None, None).unwrap();
    opts.existing_policy = ExistingPolicy::Overwrite;
    extract_archive_with_progress(archive.to_str().unwrap(), &out, opts, None, None).unwrap();
    assert!(
        std::fs::symlink_metadata(out.join("input/pipe"))
            .unwrap()
            .file_type()
            .is_fifo()
    );
    symlink("../../escape", input.join("unsafe")).unwrap();
    let unsafe_archive = dir.path().join("unsafe.zar");
    create_archive(unsafe_archive.to_str().unwrap(), &[input]).unwrap();
    assert!(
        extract_archive_with_progress(
            unsafe_archive.to_str().unwrap(),
            &dir.path().join("unsafe-out"),
            opts,
            None,
            None,
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn extraction_refuses_destination_symlink_components() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let dest = fixture.dir.path().join("dest");
    let outside = fixture.dir.path().join("outside");
    std::fs::create_dir(&dest).unwrap();
    std::fs::create_dir(&outside).unwrap();
    symlink(&outside, dest.join("input")).unwrap();
    assert!(
        extract_archive_with_progress(
            fixture.archive(),
            &dest,
            options(CollisionPolicy::Error, ExistingPolicy::Overwrite),
            None,
            None,
        )
        .is_err()
    );
    assert!(!outside.join("a.txt").exists());
}

#[test]
fn compression_helpers_validate_boundaries_and_budget() {
    assert_eq!(parse_block_size("auto").unwrap(), 0);
    assert_eq!(parse_block_size("64k").unwrap(), 65536);
    assert_eq!(parse_block_size("32M").unwrap(), 32 * 1024 * 1024);
    for input in ["", "foo", "1K", "33M", "9999999999999999999999999M"] {
        assert!(parse_block_size(input).is_err(), "{input}");
    }
    for input in ["-7", "0", "22"] {
        assert!(parse_compression_level(input).is_ok());
    }
    for input in ["-8", "23", "bad"] {
        assert!(parse_compression_level(input).is_err());
    }
    assert_eq!(select_compression_workers(1024, 0, 0), 1);
    let enough = select_automatic_encoding(1024 * 1024 * 1024, 4);
    assert_eq!(enough.workers, 4);
    assert!(!enough.warning);
    let low = select_automatic_encoding(0, 4);
    assert_eq!(low.workers, 1);
    assert!(low.warning);
    let mut opts = EncodingOptions {
        block_size: 0,
        ..Default::default()
    };
    resolve_encoding_options(&mut opts);
    assert!(opts.block_size >= AUTO_MIN_BLOCK_SIZE);
    assert!(should_skip_compression(Path::new("photo.JPG")));
    assert!(!should_skip_compression(Path::new("text.txt")));
    assert_eq!(format_unix_millis(0), "1970-01-01 00:00 UTC");
    assert_eq!(format_unix_millis(-1), "1969-12-31 23:59 UTC");
    assert_eq!(parse_commit_id("000000ff").unwrap(), 255u32.to_le_bytes());
    assert!(parse_commit_id("xyz").is_err());
    assert_eq!(encode_hex(&[0, 15, 255]), "000fff");
}

#[test]
fn input_summary_and_metadata_preserve_dates_only_when_requested() {
    let fixture = Fixture::new();
    let summary =
        scan_input_summary(std::slice::from_ref(&fixture.input), fixture.dir.path()).unwrap();
    assert_eq!(summary.files, 2);
    assert_eq!(summary.directories, 2);
    assert_eq!(summary.total_bytes, 25);
    let path = ArchivePath::new("input/a.txt").unwrap();
    let metadata = std::fs::metadata(fixture.input.join("a.txt")).unwrap();
    let captured = capture_entry_metadata(&path, &metadata, true).unwrap();
    assert!(captured.modified_unix_millis.is_some());
    assert_eq!(
        finalize_entry_metadata(vec![entry_date_metadata(&path, Some(1))], false).len(),
        0
    );
    assert_eq!(finalize_entry_metadata(vec![captured], true).len(), 1);
}

#[test]
fn extraction_handles_existing_policies_and_dry_run() {
    let fixture = Fixture::new();
    let dest = fixture.dir.path().join("out");
    let mut opts = options(CollisionPolicy::Error, ExistingPolicy::Error);
    opts.dry_run = true;
    let plan = extract_archive_with_progress(fixture.archive(), &dest, opts, None, None).unwrap();
    assert_eq!(plan.files.len(), 2);
    assert!(!dest.exists());
    opts.dry_run = false;
    extract_archive_with_progress(fixture.archive(), &dest, opts, None, None).unwrap();
    let target = dest.join("input/a.txt");
    assert_eq!(std::fs::read(&target).unwrap(), b"alpha content");
    std::fs::write(&target, b"existing").unwrap();
    assert!(extract_archive_with_progress(fixture.archive(), &dest, opts, None, None).is_err());
    opts.existing_policy = ExistingPolicy::Skip;
    extract_archive_with_progress(fixture.archive(), &dest, opts, None, None).unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"existing");
    opts.existing_policy = ExistingPolicy::Overwrite;
    extract_archive_with_progress(fixture.archive(), &dest, opts, None, None).unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"alpha content");
    opts.existing_policy = ExistingPolicy::Ask;
    assert!(extract_archive_with_progress(fixture.archive(), &dest, opts, None, None).is_err());
    let progress = TestProgress::default();
    *progress.decision.lock().unwrap() = Some(OverwriteDecision::Never);
    std::fs::write(&target, b"keep").unwrap();
    extract_archive_with_progress(fixture.archive(), &dest, opts, None, Some(&progress)).unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"keep");
    *progress.decision.lock().unwrap() = Some(OverwriteDecision::Always);
    extract_archive_with_progress(fixture.archive(), &dest, opts, None, Some(&progress)).unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"alpha content");
}

#[test]
fn extraction_collision_policies_and_strip_top_level() {
    let fixture = Fixture::new();
    std::fs::write(fixture.input.join("A.txt"), b"upper").unwrap();
    add_archive(fixture.archive(), std::slice::from_ref(&fixture.input)).unwrap();
    let mut opts = options(CollisionPolicy::Error, ExistingPolicy::Error);
    let out = fixture.dir.path().join("collision");
    assert!(extract_archive_with_progress(fixture.archive(), &out, opts, None, None).is_err());
    for policy in [
        CollisionPolicy::First,
        CollisionPolicy::Latest,
        CollisionPolicy::Rename,
    ] {
        opts.collision_policy = policy;
        opts.dry_run = true;
        let plan =
            extract_archive_with_progress(fixture.archive(), &out, opts, None, None).unwrap();
        assert_eq!(
            plan.files.len(),
            if policy == CollisionPolicy::Rename {
                3
            } else {
                2
            }
        );
    }
    opts.collision_policy = CollisionPolicy::Ask;
    let progress = TestProgress {
        confirm: true,
        ..Default::default()
    };
    let plan = extract_archive_with_progress(fixture.archive(), &out, opts, None, Some(&progress))
        .unwrap();
    assert_eq!(plan.files.len(), 2);

    let fresh = Fixture::new();
    let mut stripped = options(CollisionPolicy::Error, ExistingPolicy::Error);
    stripped.strip_top_level = true;
    let out = fresh.dir.path().join("stripped");
    extract_archive_with_progress(fresh.archive(), &out, stripped, None, None).unwrap();
    assert_eq!(std::fs::read(out.join("a.txt")).unwrap(), b"alpha content");
    assert_eq!(
        std::fs::read(out.join("nested/b.txt")).unwrap(),
        b"beta content"
    );
}
