use super::*;
use crate::{ArchivePath, TocEntry};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Fixture {
    dir: tempfile::TempDir,
    archive: PathBuf,
    input: PathBuf,
}

#[test]
fn streaming_readers_roundtrip_with_existing_operations() {
    use std::io::{Cursor, Read};

    struct ShortReads<R>(R);
    impl<R: Read> Read for ShortReads<R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let len = buf.len().min(7);
            self.0.read(&mut buf[..len])
        }
    }

    let data = (0..150_000).map(|i| (i % 251) as u8).collect::<Vec<_>>();
    for encryption in [
        None,
        Some(StreamEncryption {
            password: b"secret",
            algorithm: crate::ContentAead::ChaCha20Poly1305,
            scope: crate::EncryptionScope::HeaderAndData,
        }),
        Some(StreamEncryption {
            password: b"secret",
            algorithm: crate::ContentAead::Aes256Gcm,
            scope: crate::EncryptionScope::Data,
        }),
        Some(StreamEncryption {
            password: b"secret",
            algorithm: crate::ContentAead::Aes256Gcm,
            scope: crate::EncryptionScope::Header,
        }),
    ] {
        let mut archive = Cursor::new(Vec::new());
        let mut bytes = ShortReads(Cursor::new(&data));
        let mut empty = std::io::empty();
        let mut inputs = [
            StreamInput {
                path: "dir/data",
                reader: &mut bytes,
            },
            StreamInput {
                path: "empty",
                reader: &mut empty,
            },
        ];
        create_archive_from_readers(
            &mut archive,
            &mut inputs,
            EncodingOptions {
                block_size: 64 * 1024,
                compression_level: 3,
            },
            encryption,
        )
        .unwrap();
        let passwords = PasswordSet::new(encryption.map(|_| b"secret".to_vec()));
        let mut output = Vec::new();
        assert_eq!(
            extract_single_file_to_writer(&mut archive, "dir/data", &mut output, &passwords, None)
                .unwrap(),
            data.len() as u64
        );
        assert_eq!(output, data);
        output.clear();
        assert_eq!(
            extract_single_file_to_writer(&mut archive, "empty", &mut output, &passwords, None)
                .unwrap(),
            0
        );

        let directory = tempfile::tempdir().unwrap();
        let archive_path = directory.path().join("stream.zar");
        std::fs::write(&archive_path, archive.get_ref()).unwrap();
        let files = extract_archive_to_memory_with_passwords(
            archive_path.to_str().unwrap(),
            None,
            &passwords,
            None,
            0,
        )
        .unwrap();
        assert_eq!(files["dir/data"], data);
        assert!(files["empty"].is_empty());
        check_archive_with_recovery(
            archive_path.to_str().unwrap(),
            false,
            encryption.map(|_| b"secret".as_slice()),
        )
        .unwrap();
        if encryption.is_some() {
            let wrong = PasswordSet::new(Some(b"wrong".to_vec()));
            assert!(
                extract_single_file_to_writer(
                    &mut archive,
                    "dir/data",
                    &mut Vec::new(),
                    &wrong,
                    None
                )
                .is_err()
            );
            assert!(
                extract_single_file_to_writer(
                    &mut archive,
                    "dir/data",
                    &mut Vec::new(),
                    &PasswordSet::default(),
                    None
                )
                .is_err()
            );
        } else {
            let mut damaged = archive.get_ref().clone();
            damaged[40] ^= 1;
            assert!(
                extract_single_file_to_writer(
                    &mut Cursor::new(damaged),
                    "dir/data",
                    &mut Vec::new(),
                    &passwords,
                    None,
                )
                .is_err()
            );
        }
    }
}

#[test]
fn streaming_extraction_reads_existing_history_and_propagates_errors() {
    use std::io::{self, Cursor, Write};

    struct BrokenWriter;
    impl Write for BrokenWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("writer failed"))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let fixture = Fixture::new();
    let archive_path = fixture.archive();
    let original = std::fs::read(fixture.input.join("a.txt")).unwrap();
    std::fs::write(fixture.input.join("a.txt"), b"new contents").unwrap();
    add_archive(archive_path, std::slice::from_ref(&fixture.input)).unwrap();
    let mut archive = Cursor::new(std::fs::read(archive_path).unwrap());
    let mut output = Vec::new();
    extract_single_file_to_writer(
        &mut archive,
        "input/a.txt",
        &mut output,
        &PasswordSet::default(),
        Some("00000001"),
    )
    .unwrap();
    assert_eq!(output, original);
    output.clear();
    extract_single_file_to_writer(
        &mut archive,
        "input/a.txt",
        &mut output,
        &PasswordSet::default(),
        None,
    )
    .unwrap();
    assert_eq!(output, b"new contents");
    let error = extract_single_file_to_writer(
        &mut archive,
        "input/a.txt",
        &mut BrokenWriter,
        &PasswordSet::default(),
        None,
    )
    .unwrap_err();
    assert!(error.to_string().contains("writer failed"));
    assert!(
        extract_single_file_to_writer(
            &mut archive,
            "../invalid",
            &mut Vec::new(),
            &PasswordSet::default(),
            None,
        )
        .is_err()
    );
}

#[test]
fn streaming_creation_rejects_bad_inputs_and_reader_failures() {
    use std::io::{self, Cursor, Read};
    struct BrokenReader;
    impl Read for BrokenReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("reader failed"))
        }
    }
    let mut archive = Cursor::new(Vec::new());
    let mut first = io::empty();
    let mut second = io::empty();
    let mut inputs = [
        StreamInput {
            path: "same",
            reader: &mut first,
        },
        StreamInput {
            path: "same",
            reader: &mut second,
        },
    ];
    assert!(
        create_archive_from_readers(&mut archive, &mut inputs, EncodingOptions::default(), None)
            .is_err()
    );
    assert!(archive.get_ref().is_empty());

    let mut first = io::empty();
    let mut second = io::empty();
    let mut inputs = [
        StreamInput {
            path: "parent",
            reader: &mut first,
        },
        StreamInput {
            path: "parent/child",
            reader: &mut second,
        },
    ];
    assert!(
        create_archive_from_readers(&mut archive, &mut inputs, EncodingOptions::default(), None)
            .is_err()
    );
    assert!(archive.get_ref().is_empty());

    let mut reader = BrokenReader;
    let mut inputs = [StreamInput {
        path: "file",
        reader: &mut reader,
    }];
    let error =
        create_archive_from_readers(&mut archive, &mut inputs, EncodingOptions::default(), None)
            .unwrap_err();
    assert!(error.to_string().contains("reader failed"));
    assert!(!archive.get_ref().is_empty());
    assert!(
        create_archive_from_readers(&mut archive, &mut [], EncodingOptions::default(), None)
            .is_err()
    );

    let directory = tempfile::tempdir().unwrap();
    let mut append = std::fs::OpenOptions::new()
        .append(true)
        .create_new(true)
        .open(directory.path().join("append.zar"))
        .unwrap();
    let error = create_archive_from_readers(&mut append, &mut [], EncodingOptions::default(), None)
        .unwrap_err();
    assert!(error.to_string().contains("requested position"));
}

#[test]
fn interruptible_copy_propagates_disk_full_after_partial_write() {
    use std::io::{self, Write};

    struct FullWriter {
        written: Vec<u8>,
        error_code: i32,
    }

    impl Write for FullWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.written.len() == 4 {
                return Err(io::Error::from_raw_os_error(self.error_code));
            }
            let count = bytes.len().min(4 - self.written.len());
            self.written.extend_from_slice(&bytes[..count]);
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[cfg(windows)]
    let error_code = 112;
    #[cfg(unix)]
    let error_code = libc::ENOSPC;
    #[cfg(not(any(windows, unix)))]
    let error_code = 28;
    let mut writer = FullWriter {
        written: Vec::new(),
        error_code,
    };
    let error = copy_interruptibly(
        &mut b"more than four bytes".as_slice(),
        &mut writer,
        &ProgressReporter(None),
    )
    .unwrap_err();
    assert_eq!(error.raw_os_error(), Some(error_code));
    assert_eq!(writer.written, b"more");
}

#[cfg(target_os = "linux")]
#[test]
fn interruptible_copy_reports_real_disk_full_error() {
    let mut full = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .unwrap();
    let error = copy_interruptibly(&mut b"data".as_slice(), &mut full, &ProgressReporter(None))
        .unwrap_err();
    assert_eq!(error.raw_os_error(), Some(libc::ENOSPC));
}

#[cfg(target_os = "linux")]
#[test]
fn archive_pipeline_reports_real_disk_full_error() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.txt");
    std::fs::write(&input, b"archive input content").unwrap();
    let entries = scan_tree(&input, "").unwrap();
    let mut full = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .unwrap();
    let mut changes = Vec::new();
    let mut block_index = Vec::new();
    let mut next_block_index = 0;
    let mut block_sizes = std::collections::HashMap::new();
    let mut seen_paths = std::collections::BTreeSet::new();
    let mut entry_metadata = Vec::new();
    let mut policy = ScanPolicy {
        strict: false,
        include_special_files: false,
        verbose: false,
        warnings: false,
        warning_messages: Vec::new(),
    };
    let mut context = ScanContext {
        changes: &mut changes,
        data: &mut full,
        block_index: &mut block_index,
        next_block_index: &mut next_block_index,
        block_sizes: &mut block_sizes,
        seen_paths: &mut seen_paths,
        entry_metadata: &mut entry_metadata,
        policy: &mut policy,
        options: EncodingOptions::default(),
        compression_workers: 1,
        progress: ProgressReporter(None),
        save_metadata: false,
        action: "Adding",
        encryption: None,
        historical_duplicates: None,
    };
    let error = process_tree_entries(&entries, &mut context).unwrap_err();
    assert!(
        error
            .to_string()
            .contains(&std::io::Error::from_raw_os_error(libc::ENOSPC).to_string())
    );
    assert!(changes.is_empty());
    assert!(block_index.is_empty());
}

#[cfg(unix)]
#[test]
fn unreadable_input_is_skipped_or_rejected_according_to_strict_mode() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input");
    std::fs::create_dir(&input).unwrap();
    let secret = input.join("secret.txt");
    std::fs::write(&secret, b"private contents").unwrap();
    std::fs::write(input.join("public.txt"), b"public contents").unwrap();
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::File::open(&secret).is_ok() {
        return;
    }

    let archive = directory.path().join("non-strict.zar");
    let report = create_archive_with_options(
        archive.to_str().unwrap(),
        std::slice::from_ref(&input),
        false,
    )
    .unwrap();
    assert!(
        report.warnings.iter().any(
            |warning| warning.contains("W_PERMISSION_DENIED") && warning.contains("secret.txt")
        )
    );
    let files = extract_archive_to_memory(archive.to_str().unwrap(), None, None, None, 0).unwrap();
    assert_eq!(files["input/public.txt"], b"public contents");
    assert!(!files.contains_key("input/secret.txt"));

    let strict_archive = directory.path().join("strict.zar");
    let error =
        create_archive_with_options(strict_archive.to_str().unwrap(), &[input], true).unwrap_err();
    assert!(error.to_string().contains("cannot read input file"));
    assert!(error.to_string().contains("secret.txt"));
    assert!(!strict_archive.exists());
}

#[cfg(unix)]
#[test]
fn unwritable_extraction_target_reports_permission_denied_without_changing_it() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new();
    let destination = fixture.dir.path().join("output");
    std::fs::create_dir_all(destination.join("input")).unwrap();
    let target = destination.join("input/a.txt");
    std::fs::write(&target, b"protected contents").unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o444)).unwrap();
    if std::fs::OpenOptions::new()
        .write(true)
        .open(&target)
        .is_ok()
    {
        return;
    }
    let error = extract_archive_with_progress(
        fixture.archive(),
        &destination,
        options(CollisionPolicy::Error, ExistingPolicy::Overwrite),
        None,
        None,
    )
    .err()
    .unwrap();
    assert!(
        error
            .to_string()
            .contains(&std::io::Error::from_raw_os_error(libc::EACCES).to_string())
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"protected contents");
}

#[cfg(windows)]
#[test]
fn windows_locked_input_fails_without_creating_an_archive() {
    use std::os::windows::fs::OpenOptionsExt;

    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.txt");
    let archive = directory.path().join("archive.zar");
    std::fs::write(&input, b"input contents").unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&input)
        .unwrap();
    let error =
        create_archive(archive.to_str().unwrap(), std::slice::from_ref(&input)).unwrap_err();
    assert!(error.to_string().contains("cannot read input file"));
    assert!(error.to_string().contains("os error 32"));
    assert!(!archive.exists());
    drop(lock);
    create_archive(archive.to_str().unwrap(), &[input]).unwrap();
    assert!(archive.is_file());
}

#[cfg(windows)]
#[test]
fn windows_locked_extraction_target_is_not_overwritten() {
    use std::os::windows::fs::OpenOptionsExt;

    let fixture = Fixture::new();
    let destination = fixture.dir.path().join("output");
    std::fs::create_dir_all(destination.join("input")).unwrap();
    let target = destination.join("input/a.txt");
    std::fs::write(&target, b"original contents").unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&target)
        .unwrap();
    let error = extract_archive_with_progress(
        fixture.archive(),
        &destination,
        options(CollisionPolicy::Error, ExistingPolicy::Overwrite),
        None,
        None,
    )
    .err()
    .unwrap();
    assert!(error.to_string().contains("os error 32"));
    drop(lock);
    assert_eq!(std::fs::read(&target).unwrap(), b"original contents");
}

#[cfg(windows)]
#[test]
fn windows_locked_archive_replacement_preserves_original_and_cleans_up() {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ: u32 = 1;
    const FILE_SHARE_WRITE: u32 = 2;
    let fixture = Fixture::new();
    let original = std::fs::read(&fixture.archive).unwrap();
    let input = fixture.dir.path().join("additional.txt");
    std::fs::write(&input, b"additional contents").unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(&fixture.archive)
        .unwrap();
    let error = add_archive(fixture.archive(), &[input]).unwrap_err();
    assert!(error.to_string().contains("cannot replace archive"));
    drop(lock);
    assert_eq!(std::fs::read(&fixture.archive).unwrap(), original);
    assert!(std::fs::read_dir(fixture.dir.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
}

#[cfg(windows)]
#[test]
fn windows_readonly_extraction_target_is_not_overwritten() {
    let fixture = Fixture::new();
    let destination = fixture.dir.path().join("output");
    std::fs::create_dir_all(destination.join("input")).unwrap();
    let target = destination.join("input/a.txt");
    std::fs::write(&target, b"protected contents").unwrap();
    let original_permissions = std::fs::metadata(&target).unwrap().permissions();
    let mut readonly_permissions = original_permissions.clone();
    readonly_permissions.set_readonly(true);
    std::fs::set_permissions(&target, readonly_permissions).unwrap();
    let result = extract_archive_with_progress(
        fixture.archive(),
        &destination,
        options(CollisionPolicy::Error, ExistingPolicy::Overwrite),
        None,
        None,
    );
    std::fs::set_permissions(&target, original_permissions).unwrap();
    let error = result.err().unwrap();
    assert!(error.to_string().contains("os error 5"));
    assert_eq!(std::fs::read(&target).unwrap(), b"protected contents");
}

#[test]
fn extraction_state_and_dry_run_follow_renamed_history() {
    let fixture = Fixture::new();
    let original = ArchivePath::new("input/a.txt").unwrap();
    let renamed = ArchivePath::new("input/nested/renamed.txt").unwrap();
    rename_archive_entry(fixture.archive(), &original, &renamed).unwrap();

    let (latest, sources, metadata) = load_extraction_state(fixture.archive(), None, None).unwrap();
    assert!(latest.get(&original).is_none());
    assert!(latest.get(&renamed).is_some());
    assert!(!sources[&renamed].blocks.is_empty());
    assert_eq!(sources[&renamed].kind, ExtractedKind::File);
    assert!(metadata.contains_key(&renamed));

    let reporter = ProgressReporter(None);
    let (historical, historical_sources, _) = load_extraction_state_with_progress(
        fixture.archive(),
        Some("00000001"),
        None,
        Some(&reporter),
    )
    .unwrap();
    assert!(historical.get(&original).is_some());
    assert!(historical.get(&renamed).is_none());
    assert!(!historical_sources[&original].blocks.is_empty());
    assert!(load_extraction_state(fixture.archive(), Some("ffffffff"), None).is_err());

    let destination = fixture.dir.path().join("dry-run-output");
    let plan = extract_archive_with_options(
        fixture.archive(),
        &destination,
        true,
        None,
        None,
        CollisionPolicy::Error,
        ExistingPolicy::Error,
    )
    .unwrap();
    assert!(
        plan.files
            .iter()
            .any(|(path, _, _)| path.ends_with("renamed.txt"))
    );
    assert!(!destination.exists());
}

#[test]
fn removing_directory_clears_descendant_sources_and_metadata() {
    let fixture = Fixture::new();
    let nested = ArchivePath::new("input/nested").unwrap();
    let child = ArchivePath::new("input/nested/b.txt").unwrap();
    let sibling = ArchivePath::new("input/a.txt").unwrap();
    remove_archive(fixture.archive(), &[PathBuf::from(nested.as_str())]).unwrap();
    let (toc, sources, metadata) = load_extraction_state(fixture.archive(), None, None).unwrap();
    assert!(toc.get(&nested).is_none());
    assert!(toc.get(&child).is_none());
    assert!(!sources.contains_key(&child));
    assert!(!metadata.contains_key(&child));
    assert!(toc.get(&sibling).is_some());
    assert!(sources.contains_key(&sibling));
    let (original, original_sources, original_metadata) =
        load_extraction_state(fixture.archive(), Some("00000001"), None).unwrap();
    assert!(original.get(&child).is_some());
    assert!(original_sources.contains_key(&child));
    assert!(original_metadata.contains_key(&child));
    join_archive_with_encoding(
        fixture.archive(),
        "00000001",
        "00000002",
        EncodingOptions::default(),
    )
    .unwrap();
    let contents = extract_archive_to_memory(fixture.archive(), None, None, None, 0).unwrap();
    assert_eq!(contents["input/a.txt"], b"alpha content");
    assert!(!contents.contains_key("input/nested/b.txt"));
}

#[test]
fn joining_later_commits_preserves_removal_of_a_previous_directory() {
    let fixture = Fixture::new();
    remove_archive(fixture.archive(), &[PathBuf::from("input/nested")]).unwrap();
    let added = fixture.dir.path().join("later.txt");
    std::fs::write(&added, b"later").unwrap();
    add_archive(fixture.archive(), &[added]).unwrap();
    join_archive_with_encoding(
        fixture.archive(),
        "00000002",
        "00000003",
        EncodingOptions::default(),
    )
    .unwrap();
    let files = extract_archive_to_memory(fixture.archive(), None, None, None, 0).unwrap();
    assert_eq!(files["input/a.txt"], b"alpha content");
    assert_eq!(files["later.txt"], b"later");
    assert!(!files.contains_key("input/nested/b.txt"));
    let old =
        extract_archive_to_memory(fixture.archive(), Some("00000001"), None, None, 0).unwrap();
    assert_eq!(old["input/nested/b.txt"], b"beta content");
}

#[test]
fn prepared_extraction_rejects_changed_archive_and_mismatched_history() {
    let fixture = Fixture::new();
    let prepared =
        PreparedExtraction::new(fixture.archive(), Some("00000001"), &PasswordSet::default())
            .unwrap();
    let destination = fixture.dir.path().join("prepared-output");
    let mut settings = options(CollisionPolicy::Error, ExistingPolicy::Error);
    let error = prepared
        .extract(&destination, settings, None, &FilterSet::default())
        .err()
        .unwrap();
    assert!(error.to_string().contains("does not match"));
    settings.to = Some("00000001");
    prepared
        .extract(&destination, settings, None, &FilterSet::default())
        .unwrap();
    assert_eq!(
        std::fs::read(destination.join("input/a.txt")).unwrap(),
        b"alpha content"
    );
    let mut bytes = std::fs::read(&fixture.archive).unwrap();
    bytes.push(0);
    std::fs::write(&fixture.archive, bytes).unwrap();
    let error = prepared.read_file_range("input/a.txt", 0, 5).unwrap_err();
    assert!(error.to_string().contains("archive changed"));
}

#[test]
fn extraction_rejects_files_occupying_destination_directories() {
    let fixture = Fixture::new();
    let tar_path = fixture.dir.path().join("input.tar");
    let mut tar = tar::Builder::new(std::fs::File::create(&tar_path).unwrap());
    tar.append_dir_all("input", &fixture.input).unwrap();
    tar.finish().unwrap();
    let destination = fixture.dir.path().join("destination");
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("input"), b"existing file").unwrap();
    for archive in [&fixture.archive, &tar_path] {
        for dry_run in [false, true] {
            let mut settings = options(CollisionPolicy::Error, ExistingPolicy::Overwrite);
            settings.dry_run = dry_run;
            let error = extract_archive_with_progress(
                archive.to_str().unwrap(),
                &destination,
                settings,
                None,
                None,
            )
            .err()
            .unwrap();
            assert!(
                error
                    .to_string()
                    .contains("destination path is not a directory")
            );
            assert_eq!(
                std::fs::read(destination.join("input")).unwrap(),
                b"existing file"
            );
        }
    }
}

#[test]
fn cross_commit_add_reuses_original_bytes() {
    let fixture = Fixture::new();
    let copy = fixture.dir.path().join("duplicate.txt");
    std::fs::copy(fixture.input.join("a.txt"), &copy).unwrap();
    let before = std::fs::metadata(&fixture.archive).unwrap().len();
    add_archive(fixture.archive(), &[copy]).unwrap();
    let after = std::fs::metadata(&fixture.archive).unwrap().len();
    let toc = list_archive(fixture.archive(), None, None).unwrap();
    let entry = toc
        .get(&ArchivePath::new("duplicate.txt").unwrap())
        .unwrap();
    assert!(matches!(entry, TocEntry::File(file) if file.source_commit.is_some()));
    let matches = grep_archive(
        fixture.archive(),
        "duplicate.txt",
        "alpha",
        GrepOptions::default(),
    )
    .unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].lines[0].bytes, b"alpha content");
    let references = cross_commit_references(fixture.archive()).unwrap();
    assert_eq!(references.len(), 1);
    let info = info_archive(fixture.archive(), false).unwrap();
    assert_eq!(
        info.iter()
            .map(|commit| commit.cross_commit_references.as_ref().unwrap().len())
            .sum::<usize>(),
        1
    );
    assert_eq!(references[0].path.as_str(), "duplicate.txt");
    let source = format!("{:08x}", u32::from_le_bytes(references[0].source_commit));
    let referencing = format!(
        "{:08x}",
        u32::from_le_bytes(references[0].referencing_commit)
    );
    let before_removal = std::fs::read(&fixture.archive).unwrap();
    let error = remove_archive_commits(fixture.archive(), std::slice::from_ref(&source), None)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains(&source) && error.contains(&referencing) && error.contains("duplicate.txt")
    );
    assert_eq!(std::fs::read(&fixture.archive).unwrap(), before_removal);
    assert!(after - before < 4096);
    let output = fixture.dir.path().join("duplicate-output.txt");
    extract_single_file(
        fixture.archive(),
        "duplicate.txt",
        &output,
        &PasswordSet::default(),
    )
    .unwrap();
    assert_eq!(
        std::fs::read(output).unwrap(),
        std::fs::read(fixture.input.join("a.txt")).unwrap()
    );
}

#[test]
fn add_with_special_file_options_preserves_metadata_and_rejects_empty_input() {
    let fixture = Fixture::new();
    let original = std::fs::read(&fixture.archive).unwrap();
    let error = add_archive_with_encoding_progress_and_special_files(
        fixture.archive(),
        &[],
        EncodingOptions::default(),
        None,
        true,
        true,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("no supported files or directories")
    );
    assert_eq!(std::fs::read(&fixture.archive).unwrap(), original);
    let input = fixture.dir.path().join("added.txt");
    std::fs::write(&input, b"new content").unwrap();
    add_archive_with_encoding_progress_and_special_files(
        fixture.archive(),
        &[input],
        EncodingOptions::default(),
        None,
        true,
        true,
    )
    .unwrap();
    let (_, details) = list_archive_with_details(fixture.archive(), None, None).unwrap();
    assert_eq!(
        details[&ArchivePath::new("added.txt").unwrap()]
            .metadata_platform
            .as_deref(),
        Some(platform_identifier())
    );
    let contents = extract_archive_to_memory(fixture.archive(), None, None, None, 0).unwrap();
    assert_eq!(contents["added.txt"], b"new content");
}

#[test]
fn cross_commit_overlay_save_preserves_base_reference() {
    let fixture = Fixture::new();
    let overlay = fixture.dir.path().join("cache.zar");
    create_overlay_archive(&fixture.archive, &overlay).unwrap();
    let copy = fixture.dir.path().join("duplicate.txt");
    std::fs::copy(fixture.input.join("a.txt"), &copy).unwrap();
    add_overlay_with_encoding_progress_and_filters_and_encryption(
        fixture.archive(),
        overlay.to_str().unwrap(),
        &[copy],
        EncodingOptions::default(),
        None,
        AddInputOptions::default(),
        &FilterSet::default(),
        None,
        &PasswordSet::default(),
    )
    .unwrap();
    let original = std::fs::read(&fixture.archive).unwrap();
    let saved = fixture.dir.path().join("saved.zar");
    save_overlay_archive(
        &fixture.archive,
        &overlay,
        &saved,
        EncodingOptions::default(),
        &PasswordSet::default(),
        None,
    )
    .unwrap();
    assert_eq!(std::fs::read(&fixture.archive).unwrap(), original);
    assert!(std::fs::metadata(&saved).unwrap().len() - (original.len() as u64) < 4096);
    assert_eq!(
        cross_commit_references(saved.to_str().unwrap())
            .unwrap()
            .len(),
        1
    );
    check_archive_with_recovery(saved.to_str().unwrap(), false, None).unwrap();
    let output = fixture.dir.path().join("output.txt");
    extract_single_file(
        saved.to_str().unwrap(),
        "duplicate.txt",
        &output,
        &PasswordSet::default(),
    )
    .unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"alpha content");
}

#[test]
fn overlay_add_into_existing_folder_preserves_base_and_contents() {
    let fixture = Fixture::new();
    let overlay = fixture.dir.path().join("folder-overlay.zar");
    let input = fixture.dir.path().join("added.txt");
    std::fs::write(&input, b"added to folder").unwrap();
    create_overlay_archive(&fixture.archive, &overlay).unwrap();
    let before = std::fs::read(&fixture.archive).unwrap();
    add_overlay_in_folder_with_encoding_progress_and_filters_and_encryption(
        fixture.archive(),
        overlay.to_str().unwrap(),
        &[input],
        &ArchivePath::new("input/nested").unwrap(),
        EncodingOptions::default(),
        None,
        AddInputOptions::default(),
        &FilterSet::default(),
        None,
        &PasswordSet::default(),
    )
    .unwrap();
    let (toc, _) = list_archive_with_overlay_and_passwords(
        fixture.archive(),
        overlay.to_str().unwrap(),
        &PasswordSet::default(),
    )
    .unwrap();
    assert!(matches!(
        toc.get(&ArchivePath::new("input/nested/added.txt").unwrap()),
        Some(TocEntry::File(entry)) if entry.size == 15
    ));
    let saved = fixture.dir.path().join("saved-folder.zar");
    save_overlay_archive(
        &fixture.archive,
        &overlay,
        &saved,
        EncodingOptions::default(),
        &PasswordSet::default(),
        None,
    )
    .unwrap();
    assert_eq!(std::fs::read(&fixture.archive).unwrap(), before);
    let files = extract_archive_to_memory(saved.to_str().unwrap(), None, None, None, 0).unwrap();
    assert_eq!(files["input/nested/added.txt"], b"added to folder");
    assert_eq!(files["input/nested/b.txt"], b"beta content");
}

#[test]
fn encrypted_overlay_rename_survives_saving_without_changing_base() {
    let fixture = Fixture::new();
    let original = std::fs::read(&fixture.archive).unwrap();
    let overlay = fixture.dir.path().join("rename-overlay.zar");
    create_overlay_archive(&fixture.archive, &overlay).unwrap();
    let password = b"overlay password";
    let passwords = PasswordSet::new(Some(password.to_vec()));
    rename_overlay_entry_with_encryption(
        fixture.archive(),
        overlay.to_str().unwrap(),
        &ArchivePath::new("input/a.txt").unwrap(),
        &ArchivePath::new("input/renamed.txt").unwrap(),
        Some((
            password,
            crate::ContentAead::Aes256Gcm,
            crate::EncryptionScope::Header,
        )),
        &passwords,
    )
    .unwrap();
    let saved = fixture.dir.path().join("renamed.zar");
    let error = save_overlay_archive(
        &fixture.archive,
        &overlay,
        &saved,
        EncodingOptions::default(),
        &passwords,
        None,
    )
    .unwrap_err();
    assert!(error.to_string().contains("encrypted output"));
    assert!(!saved.exists());
    save_overlay_archive(
        &fixture.archive,
        &overlay,
        &saved,
        EncodingOptions::default(),
        &passwords,
        Some((
            password,
            crate::ContentAead::Aes256Gcm,
            crate::EncryptionScope::Header,
        )),
    )
    .unwrap();
    assert_eq!(std::fs::read(&fixture.archive).unwrap(), original);
    let output = fixture.dir.path().join("renamed-output");
    assert!(
        extract_single_file(
            saved.to_str().unwrap(),
            "input/renamed.txt",
            &output,
            &PasswordSet::default()
        )
        .is_err()
    );
    extract_single_file(
        saved.to_str().unwrap(),
        "input/renamed.txt",
        &output,
        &passwords,
    )
    .unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"alpha content");
    let toc = list_archive(saved.to_str().unwrap(), None, Some(password)).unwrap();
    assert!(toc.get(&ArchivePath::new("input/a.txt").unwrap()).is_none());
    assert!(
        toc.get(&ArchivePath::new("input/renamed.txt").unwrap())
            .is_some()
    );
}

#[test]
fn combining_archives_reports_progress_and_rejects_cycles() {
    use std::io::{Seek, SeekFrom, Write};

    let fixture = Fixture::new();
    let overlay = fixture.dir.path().join("overlay.zar");
    create_overlay_archive(&fixture.archive, &overlay).unwrap();
    let output = fixture.dir.path().join("combined.zar");
    let progress = TestProgress::default();
    combine_archives_with_progress(&fixture.archive, &overlay, &output, Some(&progress)).unwrap();
    assert_eq!(
        info_archive(output.to_str().unwrap(), false).unwrap().len(),
        2
    );
    let data = extract_archive_to_memory(output.to_str().unwrap(), None, None, None, 0).unwrap();
    assert_eq!(data["input/a.txt"], b"alpha content");

    let cancelled_output = fixture.dir.path().join("cancelled.zar");
    let cancelled = CancelAfterAddProgress(AtomicBool::new(true));
    assert!(
        combine_archives_with_progress(
            &fixture.archive,
            &overlay,
            &cancelled_output,
            Some(&cancelled)
        )
        .unwrap_err()
        .to_string()
        .contains("cancelled")
    );
    assert!(!cancelled_output.exists());

    let duplicate = fixture.dir.path().join("duplicate.zar");
    assert!(
        combine_archives(&fixture.archive, &fixture.archive, &duplicate)
            .unwrap_err()
            .to_string()
            .contains("IDs collide")
    );
    assert!(!duplicate.exists());

    let cycle = |path: &Path| {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        let offset = read_first_header_offset(&mut file).unwrap();
        let (_, data_start, _) =
            read_commit_at_offset(&mut file, offset, crate::CommitValidationLimits::default())
                .unwrap();
        file.seek(SeekFrom::Start(data_start - crate::HEADER_OFFSET_SIZE))
            .unwrap();
        file.write_all(&offset.to_le_bytes()).unwrap();
    };
    let bad_base = fixture.dir.path().join("bad-base.zar");
    std::fs::copy(&fixture.archive, &bad_base).unwrap();
    cycle(&bad_base);
    let error = combine_archives(&bad_base, &overlay, &duplicate).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("base archive commit chain contains a cycle")
    );
    let bad_overlay = fixture.dir.path().join("bad-overlay.zar");
    std::fs::copy(&overlay, &bad_overlay).unwrap();
    cycle(&bad_overlay);
    let error = combine_archives(&fixture.archive, &bad_overlay, &duplicate).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("overlay archive commit chain contains a cycle")
    );
    assert!(!duplicate.exists());
}

#[test]
fn cross_commit_overlay_compacts_cache_local_references() {
    let fixture = Fixture::new();
    let overlay = fixture.dir.path().join("cache.zar");
    create_overlay_archive(&fixture.archive, &overlay).unwrap();
    let first = fixture.dir.path().join("first.bin");
    let second = fixture.dir.path().join("second.bin");
    let contents: Vec<u8> = (0..160_000).map(|index| (index % 251) as u8).collect();
    std::fs::write(&first, &contents).unwrap();
    std::fs::write(&second, &contents).unwrap();
    let options = EncodingOptions {
        block_size: 64 * 1024,
        compression_level: 3,
    };
    for path in [first, second] {
        add_overlay_with_encoding_progress_and_filters_and_encryption(
            fixture.archive(),
            overlay.to_str().unwrap(),
            &[path],
            options,
            None,
            AddInputOptions::default(),
            &FilterSet::default(),
            None,
            &PasswordSet::default(),
        )
        .unwrap();
    }
    let saved = fixture.dir.path().join("saved.zar");
    save_overlay_archive(
        &fixture.archive,
        &overlay,
        &saved,
        options,
        &PasswordSet::default(),
        None,
    )
    .unwrap();
    let saved = saved.to_str().unwrap();
    let toc = list_archive(saved, None, None).unwrap();
    let first = toc.get(&ArchivePath::new("first.bin").unwrap()).unwrap();
    let second = toc.get(&ArchivePath::new("second.bin").unwrap()).unwrap();
    assert!(
        matches!((first, second), (TocEntry::File(first), TocEntry::File(second)) if first.source_commit.is_none() && first.block_indices == second.block_indices)
    );
    let output = fixture.dir.path().join("second-output.bin");
    extract_single_file(saved, "second.bin", &output, &PasswordSet::default()).unwrap();
    assert_eq!(std::fs::read(output).unwrap(), contents);
    check_archive_with_recovery(saved, false, None).unwrap();
}

#[test]
fn cross_commit_tiny_files_and_deleted_originals() {
    let fixture = Fixture::new();
    let tiny = fixture.dir.path().join("tiny.txt");
    std::fs::write(&tiny, b"small").unwrap();
    add_archive(fixture.archive(), std::slice::from_ref(&tiny)).unwrap();
    let duplicate = fixture.dir.path().join("tiny-copy.txt");
    let different = fixture.dir.path().join("different.txt");
    std::fs::write(&duplicate, b"small").unwrap();
    std::fs::write(&different, b"large").unwrap();
    add_archive(fixture.archive(), &[duplicate, different]).unwrap();
    let references = cross_commit_references(fixture.archive()).unwrap();
    assert_eq!(references.len(), 1);
    assert_eq!(references[0].path.as_str(), "tiny-copy.txt");
    let info = info_archive_with_blocks(fixture.archive(), true).unwrap();
    assert_eq!(
        info[2]
            .blocks
            .iter()
            .map(|block| block.file_count)
            .sum::<u64>(),
        1
    );
    remove_archive_entries_with_progress(
        fixture.archive(),
        &[ArchivePath::new("tiny.txt").unwrap()],
        None,
    )
    .unwrap();
    let output = fixture.dir.path().join("tiny-output.txt");
    extract_single_file(
        fixture.archive(),
        "tiny-copy.txt",
        &output,
        &PasswordSet::default(),
    )
    .unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"small");
    check_archive_with_recovery(fixture.archive(), false, None).unwrap();
}

#[test]
fn rename_archive_entry_preserves_data_and_history() {
    let fixture = Fixture::new();
    let before = std::fs::read(&fixture.archive).unwrap();
    let old = ArchivePath::new("input/a.txt").unwrap();
    let new = ArchivePath::new("input/renamed.txt").unwrap();
    let collision = ArchivePath::new("input/nested/b.txt").unwrap();
    let (_, before_details) = list_archive_with_details(fixture.archive(), None, None).unwrap();
    assert!(rename_archive_entry(fixture.archive(), &old, &collision).is_err());
    assert_eq!(std::fs::read(&fixture.archive).unwrap(), before);
    rename_archive_entry(fixture.archive(), &old, &new).unwrap();
    let toc =
        list_archive_with_passwords(fixture.archive(), None, &PasswordSet::default()).unwrap();
    assert!(toc.get(&old).is_none());
    assert!(toc.get(&new).is_some());
    let (_, after_details) = list_archive_with_details(fixture.archive(), None, None).unwrap();
    assert_eq!(
        before_details[&old].modified_unix_millis,
        after_details[&new].modified_unix_millis
    );
    let output = fixture.dir.path().join("renamed-output.txt");
    extract_single_file(
        fixture.archive(),
        new.as_str(),
        &output,
        &PasswordSet::default(),
    )
    .unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"alpha content");
    check_archive_with_recovery(fixture.archive(), false, None).unwrap();
    remove_commits(fixture.archive(), 1).unwrap();
    assert!(
        list_archive_with_passwords(fixture.archive(), None, &PasswordSet::default())
            .unwrap()
            .get(&old)
            .is_some()
    );
}

#[test]
fn overlay_move_multiple_files_is_one_undoable_commit() {
    let fixture = Fixture::new();
    let overlay = fixture.dir.path().join("move-cache.zar");
    create_overlay_archive(&fixture.archive, &overlay).unwrap();
    let changes = [
        (
            ArchivePath::new("input/a.txt").unwrap(),
            ArchivePath::new("input/nested/a.txt").unwrap(),
        ),
        (
            ArchivePath::new("input/nested/b.txt").unwrap(),
            ArchivePath::new("input/b-moved.txt").unwrap(),
        ),
    ];
    rename_overlay_entries_with_encryption(
        fixture.archive(),
        overlay.to_str().unwrap(),
        &changes,
        None,
        &PasswordSet::default(),
    )
    .unwrap();
    let saved = fixture.dir.path().join("moved.zar");
    save_overlay_archive(
        &fixture.archive,
        &overlay,
        &saved,
        EncodingOptions::default(),
        &PasswordSet::default(),
        None,
    )
    .unwrap();
    let saved = saved.to_str().unwrap();
    let output = fixture.dir.path().join("moved-output");
    extract_single_file(
        saved,
        "input/nested/a.txt",
        &output,
        &PasswordSet::default(),
    )
    .unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"alpha content");
    check_archive_with_recovery(saved, false, None).unwrap();
    remove_commits(saved, 1).unwrap();
    let toc = list_archive(saved, None, None).unwrap();
    assert!(toc.get(&changes[0].0).is_some());
    assert!(toc.get(&changes[1].0).is_some());
}

#[test]
fn rename_uses_the_live_files_actual_data_commit() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.txt");
    let second = directory.path().join("second.txt");
    let archive = directory.path().join("origin.zar");
    std::fs::write(&first, b"a").unwrap();
    std::fs::write(&second, b"b").unwrap();
    let archive = archive.to_str().unwrap();
    create_archive(archive, &[first]).unwrap();
    add_archive(archive, &[second]).unwrap();
    let from = ArchivePath::new("second.txt").unwrap();
    let to = ArchivePath::new("renamed.txt").unwrap();
    rename_archive_entry(archive, &from, &to).unwrap();
    let output = directory.path().join("output");
    extract_single_file(archive, to.as_str(), &output, &PasswordSet::default()).unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"b");
    check_archive_with_recovery(archive, false, None).unwrap();
}

#[test]
fn rename_implicit_directory_moves_all_children() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("file.txt");
    let archive = directory.path().join("implicit.zar");
    std::fs::write(&source, b"contents").unwrap();
    let archive = archive.to_str().unwrap();
    create_archive(archive, &[source]).unwrap();
    rename_archive_entry(
        archive,
        &ArchivePath::new("file.txt").unwrap(),
        &ArchivePath::new("old/file.txt").unwrap(),
    )
    .unwrap();
    rename_archive_entry(
        archive,
        &ArchivePath::new("old").unwrap(),
        &ArchivePath::new("new").unwrap(),
    )
    .unwrap();
    let output = directory.path().join("output");
    extract_single_file(archive, "new/file.txt", &output, &PasswordSet::default()).unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"contents");
    let toc = list_archive(archive, None, None).unwrap();
    assert!(
        toc.get(&ArchivePath::new("old/file.txt").unwrap())
            .is_none()
    );
}

#[test]
fn invalid_renames_leave_archive_unchanged() {
    let fixture = Fixture::new();
    let original = std::fs::read(&fixture.archive).unwrap();
    let path = |value: &str| ArchivePath::new(value).unwrap();
    let cases = [
        (vec![], "no archive entries"),
        (
            vec![(path("input/a.txt"), path("input/a.txt"))],
            "must be different",
        ),
        (
            vec![(path("input"), path("input/nested/new"))],
            "outside the source",
        ),
        (vec![(path("missing"), path("new"))], "path not found"),
        (
            vec![(path("input/a.txt"), path("input/nested/b.txt"))],
            "destination already exists",
        ),
        (
            vec![
                (path("input/a.txt"), path("new")),
                (path("input/nested/b.txt"), path("new")),
            ],
            "destination already exists",
        ),
        (
            vec![
                (path("input"), path("renamed")),
                (path("input/a.txt"), path("other")),
            ],
            "overlapping source paths",
        ),
    ];
    for (paths, expected) in cases {
        let error = rename_archive_entries_with_encryption(
            fixture.archive(),
            &paths,
            None,
            &PasswordSet::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
        assert_eq!(std::fs::read(&fixture.archive).unwrap(), original);
    }
}

#[test]
fn cross_commit_encrypted_references_require_passwords() {
    let fixture = Fixture::new();
    let archive = fixture.dir.path().join("encrypted.zar");
    let archive = archive.to_str().unwrap();
    let password = b"reference password";
    let algorithm = crate::ContentAead::Aes256Gcm;
    let scope = crate::EncryptionScope::HeaderAndData;
    create_archive_with_password_progress_and_filters_and_encryption(
        archive,
        &[fixture.input.join("a.txt")],
        false,
        EncodingOptions::default(),
        Some((password, algorithm, scope)),
        None,
        CreateFilterOptions {
            input: CreateInputOptions::default(),
            filters: &FilterSet::default(),
        },
    )
    .unwrap();
    let duplicate = fixture.dir.path().join("encrypted-copy.txt");
    std::fs::copy(fixture.input.join("a.txt"), &duplicate).unwrap();
    let passwords = PasswordSet::new(Some(password.to_vec()));
    add_archive_with_encoding_progress_and_filters_and_encryption(
        archive,
        &[duplicate],
        EncodingOptions::default(),
        None,
        AddInputOptions::default(),
        &FilterSet::default(),
        Some((password, algorithm, scope)),
        &passwords,
    )
    .unwrap();
    assert!(cross_commit_references(archive).is_err());
    let references = cross_commit_references_with_passwords(archive, &passwords).unwrap();
    assert_eq!(references.len(), 1);
    let mut reader = std::io::Cursor::new(std::fs::read(archive).unwrap());
    let mut streamed = Vec::new();
    extract_single_file_to_writer(
        &mut reader,
        "encrypted-copy.txt",
        &mut streamed,
        &passwords,
        None,
    )
    .unwrap();
    assert_eq!(
        streamed,
        std::fs::read(fixture.input.join("a.txt")).unwrap()
    );
    assert!(
        info_archive(archive, false).unwrap()[1]
            .cross_commit_references
            .is_none()
    );
    assert_eq!(
        info_archive_with_passwords(archive, false, &passwords).unwrap()[1]
            .cross_commit_references
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    let source = format!("{:08x}", u32::from_le_bytes(references[0].source_commit));
    assert!(
        remove_archive_commits(archive, std::slice::from_ref(&source), None)
            .unwrap_err()
            .to_string()
            .contains("password")
    );
    assert!(
        remove_archive_commits_with_passwords(
            archive,
            std::slice::from_ref(&source),
            None,
            &passwords
        )
        .unwrap_err()
        .to_string()
        .contains("encrypted-copy.txt")
    );
    let output = fixture.dir.path().join("encrypted-output.txt");
    extract_single_file(archive, "encrypted-copy.txt", &output, &passwords).unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"alpha content");
    check_archive_with_recovery_and_passwords(archive, false, &passwords).unwrap();

    let referencing = format!(
        "{:08x}",
        u32::from_le_bytes(references[0].referencing_commit)
    );
    let original = std::fs::read(archive).unwrap();
    let unencrypted = join_archive_with_encryption(
        archive,
        &referencing,
        &referencing,
        EncodingOptions::default(),
        None,
        &passwords,
        None,
    )
    .unwrap_err();
    assert!(
        unencrypted
            .to_string()
            .contains("different data encryption")
    );
    assert_eq!(std::fs::read(archive).unwrap(), original);
    let wrong_output_password = join_archive_with_encryption(
        archive,
        &referencing,
        &referencing,
        EncodingOptions::default(),
        None,
        &passwords,
        Some((b"different password", algorithm, scope)),
    )
    .unwrap_err();
    assert!(
        wrong_output_password
            .to_string()
            .contains("different password")
    );
    assert_eq!(std::fs::read(archive).unwrap(), original);
}

#[test]
fn rename_encrypted_file_requires_password_and_preserves_reference() {
    let fixture = Fixture::new();
    let archive = fixture.dir.path().join("encrypted-rename.zar");
    let archive = archive.to_str().unwrap();
    let password = b"rename password";
    let algorithm = crate::ContentAead::Aes256Gcm;
    let scope = crate::EncryptionScope::HeaderAndData;
    create_archive_with_password_progress_and_filters_and_encryption(
        archive,
        &[fixture.input.join("a.txt")],
        false,
        EncodingOptions::default(),
        Some((password, algorithm, scope)),
        None,
        CreateFilterOptions {
            input: CreateInputOptions::default(),
            filters: &FilterSet::default(),
        },
    )
    .unwrap();
    let from = ArchivePath::new("a.txt").unwrap();
    let to = ArchivePath::new("renamed.txt").unwrap();
    let original = std::fs::read(archive).unwrap();
    assert!(rename_archive_entry(archive, &from, &to).is_err());
    assert_eq!(std::fs::read(archive).unwrap(), original);
    let passwords = PasswordSet::new(Some(password.to_vec()));
    rename_archive_entry_with_encryption(
        archive,
        &from,
        &to,
        Some((password, algorithm, scope)),
        &passwords,
    )
    .unwrap();
    let references = cross_commit_references_with_passwords(archive, &passwords).unwrap();
    assert_eq!(references.len(), 1);
    let source = format!("{:08x}", u32::from_le_bytes(references[0].source_commit));
    assert!(remove_archive_commits_with_passwords(archive, &[source], None, &passwords).is_err());
    let output = fixture.dir.path().join("renamed-encrypted-output");
    extract_single_file(archive, to.as_str(), &output, &passwords).unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"alpha content");
    check_archive_with_recovery_and_passwords(archive, false, &passwords).unwrap();
}

#[test]
fn cross_commit_rewrites_protect_references() {
    let fixture = Fixture::new();
    let independent = fixture.dir.path().join("independent.txt");
    std::fs::write(&independent, b"independent").unwrap();
    add_archive(fixture.archive(), &[independent]).unwrap();
    let duplicate = fixture.dir.path().join("duplicate.txt");
    std::fs::copy(fixture.input.join("a.txt"), &duplicate).unwrap();
    add_archive(fixture.archive(), &[duplicate]).unwrap();
    let before = std::fs::read(&fixture.archive).unwrap();
    let join_error = join_archive_with_encoding(
        fixture.archive(),
        "00000001",
        "00000002",
        EncodingOptions::default(),
    )
    .unwrap_err()
    .to_string();
    assert!(
        join_error.contains("00000001")
            && join_error.contains("00000003")
            && join_error.contains("duplicate.txt")
    );
    let passwords = PasswordSet::default();
    for (from, to) in [("00000001", "00000002"), ("00000003", "00000003")] {
        let error = encrypt_archive_commits(
            fixture.archive(),
            CommitRangeOptions {
                from_id: from,
                to_id: to,
                output: None,
                passwords: &passwords,
                strict: true,
            },
            b"new password",
            crate::ContentAead::Aes256Gcm,
            crate::EncryptionScope::HeaderAndData,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("duplicate.txt"));
    }
    assert_eq!(std::fs::read(&fixture.archive).unwrap(), before);
    encrypt_archive_commits(
        fixture.archive(),
        CommitRangeOptions {
            from_id: "00000001",
            to_id: "00000003",
            output: None,
            passwords: &passwords,
            strict: true,
        },
        b"new password",
        crate::ContentAead::Aes256Gcm,
        crate::EncryptionScope::HeaderAndData,
    )
    .unwrap();
    let passwords = PasswordSet::new(Some(b"new password".to_vec()));
    let output = fixture.dir.path().join("transformed.txt");
    extract_single_file(fixture.archive(), "duplicate.txt", &output, &passwords).unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"alpha content");
    check_archive_with_recovery_and_passwords(fixture.archive(), false, &passwords).unwrap();
    remove_archive_commits_with_passwords(
        fixture.archive(),
        &["00000001".into(), "00000003".into()],
        None,
        &passwords,
    )
    .unwrap();
    assert_eq!(info_archive(fixture.archive(), false).unwrap().len(), 1);
}

#[test]
fn encryption_of_selected_commit_preserves_independent_history() {
    let fixture = Fixture::new();
    let input = fixture.dir.path().join("independent.txt");
    std::fs::write(&input, b"independent content").unwrap();
    add_archive(fixture.archive(), &[input]).unwrap();
    let output = fixture.dir.path().join("partial-encrypted.zar");
    let passwords = PasswordSet::default();
    encrypt_archive_commits(
        fixture.archive(),
        CommitRangeOptions {
            from_id: "00000002",
            to_id: "00000002",
            output: Some(output.to_str().unwrap()),
            passwords: &passwords,
            strict: true,
        },
        b"new password",
        crate::ContentAead::Aes256Gcm,
        crate::EncryptionScope::HeaderAndData,
    )
    .unwrap();
    let prepared =
        PreparedExtraction::new_complete(output.to_str().unwrap(), Some("00000001"), &passwords)
            .unwrap();
    assert_eq!(prepared.to(), Some("00000001"));
    assert_eq!(
        prepared.read_file_range("input/a.txt", 0, 5).unwrap(),
        b"alpha"
    );
    assert!(prepared.read_file_range("input", 0, 1).is_err());
    assert!(
        PreparedExtraction::new_complete(output.to_str().unwrap(), None, &passwords)
            .unwrap_err()
            .to_string()
            .contains("locked commit headers")
    );
    let passwords = PasswordSet::new(Some(b"new password".to_vec()));
    let files = extract_archive_to_memory_with_passwords(
        output.to_str().unwrap(),
        None,
        &passwords,
        None,
        0,
    )
    .unwrap();
    assert_eq!(files["input/a.txt"], b"alpha content");
    assert_eq!(files["independent.txt"], b"independent content");
}

#[test]
#[ignore = "requires ZAR_OVERLAY_BENCH_BASE and ZAR_OVERLAY_BENCH_INPUT"]
fn overlay_add_large_archive_measurement() {
    let base = PathBuf::from(std::env::var_os("ZAR_OVERLAY_BENCH_BASE").unwrap());
    let input = PathBuf::from(std::env::var_os("ZAR_OVERLAY_BENCH_INPUT").unwrap());
    let directory = tempfile::tempdir().unwrap();
    let overlay = directory.path().join("cache.zar");
    let copy = directory.path().join("overlay-benchmark-input.txt");
    std::fs::copy(input, &copy).unwrap();
    let before = std::fs::metadata(&base).unwrap();
    let started = std::time::Instant::now();
    create_overlay_archive(&base, &overlay).unwrap();
    add_overlay_with_encoding_progress_and_filters_and_encryption(
        base.to_str().unwrap(),
        overlay.to_str().unwrap(),
        &[copy],
        EncodingOptions::default(),
        None,
        AddInputOptions::default(),
        &FilterSet::default(),
        None,
        &PasswordSet::default(),
    )
    .unwrap();
    let after = std::fs::metadata(&base).unwrap();
    let cache_bytes = std::fs::metadata(&overlay).unwrap().len();
    println!(
        "base_bytes={} cache_bytes={} elapsed={:?}",
        before.len(),
        cache_bytes,
        started.elapsed()
    );
    assert_eq!(before.len(), after.len());
    assert_eq!(before.modified().unwrap(), after.modified().unwrap());
    assert!(cache_bytes < before.len());
}

#[test]
fn saving_overlay_compacts_changes_without_modifying_inputs_on_failure() {
    let fixture = Fixture::new();
    let overlay = fixture.dir.path().join("cache.zar");
    create_overlay_archive(&fixture.archive, &overlay).unwrap();
    let password = b"overlay password";
    let passwords = PasswordSet::new(Some(password.to_vec()));
    for name in ["first.txt", "second.txt"] {
        let path = fixture.dir.path().join(name);
        std::fs::write(&path, name.as_bytes()).unwrap();
        add_overlay_with_encoding_progress_and_filters_and_encryption(
            fixture.archive(),
            overlay.to_str().unwrap(),
            &[path],
            EncodingOptions::default(),
            None,
            AddInputOptions::default(),
            &FilterSet::default(),
            Some((
                password,
                crate::ContentAead::Aes256Gcm,
                crate::EncryptionScope::HeaderAndData,
            )),
            &passwords,
        )
        .unwrap();
    }
    let base_before = std::fs::read(&fixture.archive).unwrap();
    let cache_before = std::fs::read(&overlay).unwrap();
    assert!(
        save_overlay_archive(
            &fixture.archive,
            &overlay,
            &fixture.archive,
            EncodingOptions::default(),
            &passwords,
            None
        )
        .is_err()
    );
    assert!(
        save_overlay_archive(
            &fixture.archive,
            &overlay,
            &fixture.archive,
            EncodingOptions::default(),
            &passwords,
            Some((
                password,
                crate::ContentAead::Aes256Gcm,
                crate::EncryptionScope::Header
            )),
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&fixture.archive).unwrap(), base_before);
    assert_eq!(std::fs::read(&overlay).unwrap(), cache_before);
    let saved_id = save_overlay_archive(
        &fixture.archive,
        &overlay,
        &fixture.archive,
        EncodingOptions::default(),
        &passwords,
        Some((
            password,
            crate::ContentAead::Aes256Gcm,
            crate::EncryptionScope::HeaderAndData,
        )),
    )
    .unwrap();
    let commits = info_archive(fixture.archive(), false).unwrap();
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[1].id, saved_id);
    assert!(commits[1].header_encrypted && commits[1].data_encrypted);
    let output = fixture.dir.path().join("extracted.txt");
    extract_single_file_through(fixture.archive(), "second.txt", &output, &passwords, None)
        .unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"second.txt");
    assert_eq!(std::fs::read(&overlay).unwrap(), cache_before);
}

#[test]
fn joined_overlay_can_preserve_encryption() {
    for scope in [
        crate::EncryptionScope::Header,
        crate::EncryptionScope::Data,
        crate::EncryptionScope::HeaderAndData,
    ] {
        let fixture = Fixture::new();
        let overlay = fixture.dir.path().join("cache.zar");
        create_overlay_archive(&fixture.archive, &overlay).unwrap();
        let password = b"overlay password";
        let passwords = PasswordSet::new(Some(password.to_vec()));
        for name in ["first.txt", "second.txt"] {
            let path = fixture.dir.path().join(name);
            std::fs::write(&path, name.as_bytes()).unwrap();
            add_overlay_with_encoding_progress_and_filters_and_encryption(
                fixture.archive(),
                overlay.to_str().unwrap(),
                &[path],
                EncodingOptions::default(),
                None,
                AddInputOptions::default(),
                &FilterSet::default(),
                Some((password, crate::ContentAead::Aes256Gcm, scope)),
                &passwords,
            )
            .unwrap();
        }
        let staged = info_archive(overlay.to_str().unwrap(), false).unwrap();
        let first = format!("{:08x}", u32::from_le_bytes(staged[0].id));
        let last = format!("{:08x}", u32::from_le_bytes(staged[1].id));
        let combined = fixture.dir.path().join("combined.zar");
        combine_archives(&fixture.archive, &overlay, &combined).unwrap();
        join_archive_with_encryption(
            combined.to_str().unwrap(),
            &first,
            &last,
            EncodingOptions::default(),
            None,
            &passwords,
            Some((password, crate::ContentAead::Aes256Gcm, scope)),
        )
        .unwrap();
        let commits = info_archive(combined.to_str().unwrap(), false).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(
            commits[1].header_encrypted,
            matches!(
                scope,
                crate::EncryptionScope::Header | crate::EncryptionScope::HeaderAndData
            )
        );
        assert_eq!(
            commits[1].data_encrypted,
            matches!(
                scope,
                crate::EncryptionScope::Data | crate::EncryptionScope::HeaderAndData
            )
        );
        let output = fixture.dir.path().join("extracted.txt");
        extract_single_file_through(
            combined.to_str().unwrap(),
            "second.txt",
            &output,
            &passwords,
            None,
        )
        .unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), b"second.txt");
        let wrong = PasswordSet::new(Some(b"wrong password".to_vec()));
        assert!(
            extract_single_file_through(
                combined.to_str().unwrap(),
                "second.txt",
                &output,
                &wrong,
                None
            )
            .is_err()
        );
    }
}

#[test]
fn overlay_metadata_changes_compact_with_file_additions() {
    let fixture = Fixture::new();
    update_custom_metadata(
        fixture.archive(),
        &[("old".into(), crate::MetadataType::I64(1))],
        &[],
    )
    .unwrap();
    let overlay = fixture.dir.path().join("cache.zar");
    create_overlay_archive(&fixture.archive, &overlay).unwrap();
    let added = fixture.dir.path().join("added.txt");
    std::fs::write(&added, b"added").unwrap();
    add_overlay_with_encoding_progress_and_filters_and_encryption(
        fixture.archive(),
        overlay.to_str().unwrap(),
        &[added],
        EncodingOptions::default(),
        None,
        AddInputOptions::default(),
        &FilterSet::default(),
        None,
        &PasswordSet::default(),
    )
    .unwrap();
    update_overlay_custom_metadata(
        fixture.archive(),
        overlay.to_str().unwrap(),
        &[("new".into(), crate::MetadataType::I64(2))],
        &["old".into()],
    )
    .unwrap();
    let metadata =
        read_custom_metadata_with_overlay(fixture.archive(), overlay.to_str().unwrap()).unwrap();
    assert!(!metadata.contains_key("old"));
    assert_eq!(metadata.get("new"), Some(&crate::MetadataType::I64(2)));
    let base_count = info_archive(fixture.archive(), false).unwrap().len();
    save_overlay_archive(
        &fixture.archive,
        &overlay,
        &fixture.archive,
        EncodingOptions::default(),
        &PasswordSet::default(),
        None,
    )
    .unwrap();
    assert_eq!(
        info_archive(fixture.archive(), false).unwrap().len(),
        base_count + 1
    );
    assert_eq!(read_custom_metadata(fixture.archive()).unwrap(), metadata);
}

#[test]
fn overlay_add_rejects_conflicts_with_base_entries() {
    let fixture = Fixture::new();
    let overlay = fixture.dir.path().join("cache.zar");
    create_overlay_archive(&fixture.archive, &overlay).unwrap();
    let conflict_dir = fixture.dir.path().join("conflict");
    std::fs::create_dir(&conflict_dir).unwrap();
    let conflict = conflict_dir.join("input");
    std::fs::write(&conflict, b"not a directory").unwrap();
    let before = std::fs::read(&overlay).unwrap();
    let result = add_overlay_with_encoding_progress_and_filters_and_encryption(
        fixture.archive(),
        overlay.to_str().unwrap(),
        &[conflict],
        EncodingOptions::default(),
        None,
        AddInputOptions::default(),
        &FilterSet::default(),
        None,
        &PasswordSet::default(),
    );
    assert!(result.is_err());
    assert_eq!(std::fs::read(&overlay).unwrap(), before);
}

#[test]
fn overlay_add_requires_password_for_encrypted_base_header() {
    let fixture = Fixture::new();
    let encrypted = fixture.dir.path().join("encrypted.zar");
    let password = b"base password";
    create_archive_with_password_progress_and_filters_and_encryption(
        encrypted.to_str().unwrap(),
        std::slice::from_ref(&fixture.input),
        false,
        EncodingOptions::default(),
        Some((
            password,
            crate::ContentAead::Aes256Gcm,
            crate::EncryptionScope::Header,
        )),
        None,
        CreateFilterOptions {
            input: CreateInputOptions::default(),
            filters: &FilterSet::default(),
        },
    )
    .unwrap();
    let overlay = fixture.dir.path().join("cache.zar");
    create_overlay_archive(&encrypted, &overlay).unwrap();
    let before = std::fs::read(&overlay).unwrap();
    let input = fixture.dir.path().join("new.txt");
    std::fs::write(&input, b"new").unwrap();
    let result = add_overlay_with_encoding_progress_and_filters_and_encryption(
        encrypted.to_str().unwrap(),
        overlay.to_str().unwrap(),
        std::slice::from_ref(&input),
        EncodingOptions::default(),
        None,
        AddInputOptions::default(),
        &FilterSet::default(),
        None,
        &PasswordSet::default(),
    );
    assert!(result.unwrap_err().to_string().contains("password"));
    assert_eq!(std::fs::read(&overlay).unwrap(), before);
    add_overlay_with_encoding_progress_and_filters_and_encryption(
        encrypted.to_str().unwrap(),
        overlay.to_str().unwrap(),
        &[input],
        EncodingOptions::default(),
        None,
        AddInputOptions::default(),
        &FilterSet::default(),
        None,
        &PasswordSet::new(Some(password.to_vec())),
    )
    .unwrap();
}

#[test]
fn overlay_removal_survives_compaction_against_base() {
    let fixture = Fixture::new();
    let overlay = fixture.dir.path().join("cache.zar");
    create_overlay_archive(&fixture.archive, &overlay).unwrap();
    let removed = ArchivePath::new("input/a.txt".to_owned()).unwrap();
    remove_overlay_entries_with_progress_and_encryption(
        fixture.archive(),
        overlay.to_str().unwrap(),
        std::slice::from_ref(&removed),
        None,
        None,
        &PasswordSet::default(),
    )
    .unwrap();
    let (visible, _) = list_archive_with_overlay_and_passwords(
        fixture.archive(),
        overlay.to_str().unwrap(),
        &PasswordSet::default(),
    )
    .unwrap();
    assert!(visible.get(&removed).is_none());

    let commits = info_archive(overlay.to_str().unwrap(), false).unwrap();
    let first = format!("{:08x}", u32::from_le_bytes(commits.first().unwrap().id));
    let last = format!("{:08x}", u32::from_le_bytes(commits.last().unwrap().id));
    let combined = fixture.dir.path().join("combined.zar");
    combine_archives(&fixture.archive, &overlay, &combined).unwrap();
    join_archive_with_encoding(
        combined.to_str().unwrap(),
        &first,
        &last,
        EncodingOptions::default(),
    )
    .unwrap();
    let saved = list_archive(combined.to_str().unwrap(), None, None).unwrap();
    assert!(saved.get(&removed).is_none());
    assert_eq!(
        info_archive(combined.to_str().unwrap(), false)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn combining_and_joining_overlay_produces_one_additional_commit() {
    let fixture = Fixture::new();
    let overlay = fixture.dir.path().join("cache.zar");
    create_overlay_archive(&fixture.archive, &overlay).unwrap();
    for (name, contents) in [
        ("first.txt", b"first".as_slice()),
        ("second.txt", b"second"),
    ] {
        let path = fixture.dir.path().join(name);
        std::fs::write(&path, contents).unwrap();
        add_archive(overlay.to_str().unwrap(), &[path]).unwrap();
    }
    let staged = info_archive(overlay.to_str().unwrap(), false).unwrap();
    assert_eq!(staged.len(), 2);
    let first = format!("{:08x}", u32::from_le_bytes(staged[0].id));
    let last = format!("{:08x}", u32::from_le_bytes(staged[1].id));
    let combined = fixture.dir.path().join("combined.zar");
    combine_archives(&fixture.archive, &overlay, &combined).unwrap();
    join_archive_with_encoding(
        combined.to_str().unwrap(),
        &first,
        &last,
        EncodingOptions::default(),
    )
    .unwrap();
    assert_eq!(
        info_archive(combined.to_str().unwrap(), false)
            .unwrap()
            .len(),
        2
    );
    let entries = list_archive(combined.to_str().unwrap(), None, None).unwrap();
    for name in ["input/a.txt", "first.txt", "second.txt"] {
        assert!(entries.iter().any(|(path, _)| path.as_str() == name));
    }
}

#[test]
fn combining_archives_preserves_inputs_and_commit_order() {
    let fixture = Fixture::new();
    let added = fixture.dir.path().join("added.txt");
    std::fs::write(&added, b"added").unwrap();
    let overlay = fixture.dir.path().join("cache.zar");
    create_overlay_archive(&fixture.archive, &overlay).unwrap();
    add_archive(overlay.to_str().unwrap(), &[added]).unwrap();
    let base_before = std::fs::read(&fixture.archive).unwrap();
    let overlay_before = std::fs::read(&overlay).unwrap();
    let combined = fixture.dir.path().join("combined.zar");

    combine_archives(&fixture.archive, &overlay, &combined).unwrap();
    let commits = info_archive(combined.to_str().unwrap(), false).unwrap();
    assert_eq!(commits.len(), 2);
    let entries = list_archive(combined.to_str().unwrap(), None, None).unwrap();
    assert!(
        entries
            .iter()
            .any(|(path, _)| path.as_str() == "input/a.txt")
    );
    assert!(entries.iter().any(|(path, _)| path.as_str() == "added.txt"));
    assert_eq!(std::fs::read(&fixture.archive).unwrap(), base_before);
    assert_eq!(std::fs::read(&overlay).unwrap(), overlay_before);
    assert!(combine_archives(&fixture.archive, &overlay, &combined).is_err());
    let collision = fixture.dir.path().join("collision.zar");
    assert!(combine_archives(&fixture.archive, &fixture.archive, &collision).is_err());
    assert!(!collision.exists());
}

#[test]
fn overlay_listing_combines_base_and_newer_entries() {
    let fixture = Fixture::new();
    let overlay_dir = fixture.dir.path().join("overlay");
    let overlay_input = overlay_dir.join("input");
    std::fs::create_dir_all(&overlay_input).unwrap();
    std::fs::write(overlay_input.join("a.txt"), b"replacement").unwrap();
    std::fs::write(overlay_input.join("new.txt"), b"new").unwrap();
    let overlay = overlay_dir.join("cache.zar");
    create_archive(overlay.to_str().unwrap(), &[overlay_input]).unwrap();

    let (toc, details) = list_archive_with_overlay_and_passwords(
        fixture.archive(),
        overlay.to_str().unwrap(),
        &PasswordSet::default(),
    )
    .unwrap();
    let replacement = ArchivePath::new("input/a.txt".to_owned()).unwrap();
    let original = ArchivePath::new("input/nested/b.txt".to_owned()).unwrap();
    let added = ArchivePath::new("input/new.txt".to_owned()).unwrap();
    assert!(matches!(toc.get(&replacement), Some(TocEntry::File(entry)) if entry.size == 11));
    assert!(toc.get(&original).is_some());
    assert!(toc.get(&added).is_some());
    assert!(details.contains_key(&original));
    assert!(details.contains_key(&added));
}

#[test]
fn overlay_listing_can_select_base_and_overlay_commits() {
    let fixture = Fixture::new();
    let overlay = fixture.dir.path().join("cache.zar");
    create_overlay_archive(&fixture.archive, &overlay).unwrap();
    for name in ["first.txt", "second.txt"] {
        let input = fixture.dir.path().join(name);
        std::fs::write(&input, name).unwrap();
        add_overlay_with_encoding_progress_and_filters_and_encryption(
            fixture.archive(),
            overlay.to_str().unwrap(),
            &[input],
            EncodingOptions::default(),
            None,
            AddInputOptions::default(),
            &FilterSet::default(),
            None,
            &PasswordSet::default(),
        )
        .unwrap();
    }
    let base_id = info_archive(fixture.archive(), false).unwrap()[0].id;
    let overlay_commits = info_archive(overlay.to_str().unwrap(), false).unwrap();
    let first_id = overlay_commits[0].id;
    let list_through = |id| {
        list_archive_with_overlay_through_and_passwords(
            fixture.archive(),
            overlay.to_str().unwrap(),
            Some(&format!("{:08x}", u32::from_le_bytes(id))),
            &PasswordSet::default(),
        )
        .unwrap()
        .0
    };
    let base = list_through(base_id);
    let first = list_through(first_id);
    assert!(base.get(&ArchivePath::new("first.txt").unwrap()).is_none());
    assert!(first.get(&ArchivePath::new("first.txt").unwrap()).is_some());
    assert!(
        first
            .get(&ArchivePath::new("second.txt").unwrap())
            .is_none()
    );
}

#[test]
fn join_rejects_invalid_ranges_and_unreadable_encryption_without_changes() {
    let fixture = Fixture::new();
    let added = fixture.dir.path().join("secret.txt");
    std::fs::write(&added, b"secret content").unwrap();
    let password = b"join password";
    add_archive_with_encoding_progress_and_filters_and_encryption(
        fixture.archive(),
        &[added],
        EncodingOptions::default(),
        None,
        AddInputOptions::default(),
        &FilterSet::default(),
        Some((
            password,
            crate::ContentAead::Aes256Gcm,
            crate::EncryptionScope::HeaderAndData,
        )),
        &PasswordSet::default(),
    )
    .unwrap();
    let commits = info_archive(fixture.archive(), false).unwrap();
    let first = format!("{:08x}", u32::from_le_bytes(commits[0].id));
    let last = format!("{:08x}", u32::from_le_bytes(commits[1].id));
    let original = std::fs::read(&fixture.archive).unwrap();
    let options = EncodingOptions::default();

    for (from, to, expected) in [
        ("ffffffff", last.as_str(), "exactly one"),
        (last.as_str(), first.as_str(), "must not appear after"),
        (last.as_str(), last.as_str(), "no commits readable"),
    ] {
        let error = join_archive_with_encoding(fixture.archive(), from, to, options).unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
        assert_eq!(std::fs::read(&fixture.archive).unwrap(), original);
    }
    let error = join_archive_with_encryption(
        fixture.archive(),
        &first,
        &last,
        options,
        None,
        &PasswordSet::default(),
        Some((
            password,
            crate::ContentAead::Aes256Gcm,
            crate::EncryptionScope::HeaderAndData,
        )),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("all selected commits must be readable")
    );
    assert_eq!(std::fs::read(&fixture.archive).unwrap(), original);
    let passwords = PasswordSet::new(Some(password.to_vec()));
    let error = join_archive_with_encryption(
        fixture.archive(),
        &first,
        &last,
        options,
        None,
        &passwords,
        Some((
            b"",
            crate::ContentAead::Aes256Gcm,
            crate::EncryptionScope::HeaderAndData,
        )),
    )
    .unwrap_err();
    assert!(error.to_string().contains("password must not be empty"));
    assert_eq!(std::fs::read(&fixture.archive).unwrap(), original);

    let no_passwords = PasswordSet::default();
    let range = |from_id, to_id, output| CommitRangeOptions {
        from_id,
        to_id,
        output,
        passwords: &no_passwords,
        strict: true,
    };
    let error = encrypt_archive_commits(
        fixture.archive(),
        range(&first, &last, None),
        b"",
        crate::ContentAead::Aes256Gcm,
        crate::EncryptionScope::HeaderAndData,
    )
    .unwrap_err();
    assert!(error.to_string().contains("password must not be empty"));
    let error = change_encryption_for_commits(
        fixture.archive(),
        range(&first, &last, None),
        b"",
        crate::ContentAead::Aes256Gcm,
        crate::EncryptionScope::HeaderAndData,
    )
    .unwrap_err();
    assert!(error.to_string().contains("password must not be empty"));
    let error = decrypt_archive_commits(fixture.archive(), range(&last, &last, None)).unwrap_err();
    assert!(error.to_string().contains("without its password"));
    let error = encrypt_archive_commits(
        fixture.archive(),
        range(&first, &first, None),
        b"new password",
        crate::ContentAead::Aes256Gcm,
        crate::EncryptionScope::HeaderAndData,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("cannot check cross-commit references")
    );
    let missing_parent = fixture.dir.path().join("missing/output.zar");
    let error = decrypt_archive_commits(
        fixture.archive(),
        range(&first, &last, Some(missing_parent.to_str().unwrap())),
    )
    .unwrap_err();
    assert_eq!(
        error.downcast_ref::<std::io::Error>().unwrap().kind(),
        std::io::ErrorKind::NotFound
    );
    assert_eq!(std::fs::read(&fixture.archive).unwrap(), original);
}

#[test]
fn add_in_folder_keeps_same_named_root_entry() {
    let fixture = Fixture::new();
    let root = fixture.dir.path().join("duplicate.txt");
    std::fs::write(&root, b"root").unwrap();
    add_archive(fixture.archive(), std::slice::from_ref(&root)).unwrap();
    let input_dir = fixture.dir.path().join("other");
    std::fs::create_dir(&input_dir).unwrap();
    let nested = input_dir.join("duplicate.txt");
    std::fs::write(&nested, b"nested").unwrap();
    add_archive_in_folder_with_encoding_progress_and_filters_and_encryption(
        fixture.archive(),
        &[nested],
        &crate::ArchivePath::new("target/sub").unwrap(),
        EncodingOptions::default(),
        None,
        AddInputOptions::default(),
        &FilterSet::default(),
        None,
        &PasswordSet::default(),
    )
    .unwrap();
    let toc = list_archive(fixture.archive(), None, None).unwrap();
    assert!(toc.iter().any(|(path, _)| path.as_str() == "duplicate.txt"));
    assert!(
        toc.iter()
            .any(|(path, _)| path.as_str() == "target/sub/duplicate.txt")
    );
}

#[test]
fn joining_commits_preserves_entry_metadata() {
    let fixture = Fixture::new();
    let added = fixture.dir.path().join("added.txt");
    std::fs::write(&added, b"added").unwrap();
    add_archive_with_encoding_progress_and_filters_and_encryption(
        fixture.archive(),
        &[added],
        EncodingOptions::default(),
        None,
        AddInputOptions {
            save_metadata: true,
            save_modification_time: true,
            ..Default::default()
        },
        &FilterSet::default(),
        None,
        &PasswordSet::default(),
    )
    .unwrap();
    let before = list_archive_with_details(fixture.archive(), None, None)
        .unwrap()
        .1;
    let commits = info_archive(fixture.archive(), false).unwrap();
    let first = format!("{:08x}", u32::from_le_bytes(commits.first().unwrap().id));
    let last = format!("{:08x}", u32::from_le_bytes(commits.last().unwrap().id));
    join_archive_with_encoding(fixture.archive(), &first, &last, EncodingOptions::default())
        .unwrap();
    let after = list_archive_with_details(fixture.archive(), None, None)
        .unwrap()
        .1;
    for (path, details) in before {
        let joined = &after[&path];
        assert_eq!(joined.metadata_platform, details.metadata_platform);
        assert_eq!(joined.owner, details.owner);
        assert_eq!(joined.group, details.group);
        assert_eq!(joined.permissions, details.permissions);
        assert_eq!(joined.modified_unix_millis, details.modified_unix_millis);
    }
}

#[test]
fn transformations_reject_empty_and_cyclic_chains_without_writing_output() {
    let fixture = Fixture::new();
    let output = fixture.dir.path().join("transformed.zar");
    let passwords = PasswordSet::default();
    let range = || CommitRangeOptions {
        from_id: "00000001",
        to_id: "00000001",
        output: Some(output.to_str().unwrap()),
        passwords: &passwords,
        strict: true,
    };
    let empty = fixture.dir.path().join("empty.zar");
    crate::ContainerHeader::new(0)
        .write_to(&mut std::fs::File::create(&empty).unwrap())
        .unwrap();
    let error = decrypt_archive_commits(empty.to_str().unwrap(), range()).unwrap_err();
    assert!(error.to_string().contains("archive contains no commits"));
    assert!(!output.exists());
    let mut file = std::fs::File::open(&fixture.archive).unwrap();
    let first = crate::ContainerHeader::read_from(&mut file)
        .unwrap()
        .first_header_offset;
    let (_, data_start, _) =
        read_commit_at_offset(&mut file, first, crate::CommitValidationLimits::default()).unwrap();
    drop(file);
    let mut bytes = std::fs::read(&fixture.archive).unwrap();
    bytes[(data_start as usize - 8)..data_start as usize].copy_from_slice(&first.to_le_bytes());
    let cyclic = fixture.dir.path().join("cyclic.zar");
    std::fs::write(&cyclic, &bytes).unwrap();
    let error = decrypt_archive_commits(cyclic.to_str().unwrap(), range()).unwrap_err();
    assert!(error.to_string().contains("commit chain contains a cycle"));
    for recover in [false, true] {
        let error =
            check_archive_with_recovery(cyclic.to_str().unwrap(), recover, None).unwrap_err();
        assert!(error.to_string().contains("commit chain contains a cycle"));
    }
    let overlay_before = std::fs::read(&fixture.archive).unwrap();
    let error = update_overlay_custom_metadata(
        cyclic.to_str().unwrap(),
        fixture.archive(),
        &[("key".to_owned(), crate::MetadataType::I64(1))],
        &[],
    )
    .unwrap_err();
    assert!(error.to_string().contains("commit chain contains a cycle"));
    assert_eq!(std::fs::read(&cyclic).unwrap(), bytes);
    assert_eq!(std::fs::read(&fixture.archive).unwrap(), overlay_before);
    assert!(!output.exists());

    let encrypted = fixture.dir.path().join("encrypted.zar");
    encrypt_archive_commits(
        fixture.archive(),
        CommitRangeOptions {
            output: Some(encrypted.to_str().unwrap()),
            ..range()
        },
        b"cycle password",
        crate::ContentAead::Aes256Gcm,
        crate::EncryptionScope::Header,
    )
    .unwrap();
    let mut file = std::fs::File::open(&encrypted).unwrap();
    let first = crate::ContainerHeader::read_from(&mut file)
        .unwrap()
        .first_header_offset;
    let (_, data_start, _) =
        read_commit_at_offset(&mut file, first, crate::CommitValidationLimits::default()).unwrap();
    drop(file);
    let mut bytes = std::fs::read(&encrypted).unwrap();
    bytes[(data_start as usize - 8)..data_start as usize].copy_from_slice(&first.to_le_bytes());
    std::fs::write(&encrypted, bytes).unwrap();
    for password in [None, Some(&b"cycle password"[..])] {
        let error =
            check_archive_with_recovery(encrypted.to_str().unwrap(), false, password).unwrap_err();
        assert!(error.to_string().contains("commit chain contains a cycle"));
    }
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
                save_modification_time: true,
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
fn indexed_single_file_skips_unrelated_block_frames() {
    use std::io::{Seek, SeekFrom, Write};

    let directory = tempfile::tempdir().unwrap();
    let wanted = directory.path().join("wanted.bin");
    let other = directory.path().join("other.bin");
    let expected = vec![37_u8; 70_000];
    std::fs::write(&wanted, &expected).unwrap();
    std::fs::write(&other, vec![42_u8; 70_000]).unwrap();
    let archive_path = directory.path().join("archive.zar");
    let archive = archive_path.to_str().unwrap();
    create_archive_with_encoding(
        archive,
        &[wanted, other],
        false,
        EncodingOptions {
            block_size: 64 * 1024,
            compression_level: 1,
        },
    )
    .unwrap();
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&archive_path)
        .unwrap();
    let limits = crate::CommitValidationLimits::default();
    let header_offset = super::common::read_first_header_offset(&mut file).unwrap();
    let (_, data_start, _) =
        super::common::read_commit_at_offset(&mut file, header_offset, limits).unwrap();
    file.seek(SeekFrom::Start(header_offset)).unwrap();
    let (validated, _) =
        super::common::read_header_with_passwords(&mut file, limits, &PasswordSet::default())
            .unwrap();
    let header = validated.logical_header.unwrap();
    let wanted_blocks = header
        .changes
        .iter()
        .find_map(|change| match change {
            crate::TocChange::PutFile { path, entry } if path.as_str() == "wanted.bin" => {
                Some(&entry.block_indices)
            }
            _ => None,
        })
        .unwrap();
    let unrelated = header
        .block_index
        .as_ref()
        .unwrap()
        .iter()
        .find(|block| !wanted_blocks.contains(&block.index))
        .unwrap();
    file.seek(SeekFrom::Start(data_start + unrelated.offset + 6))
        .unwrap();
    file.write_all(&[1]).unwrap();
    drop(file);

    let output = directory.path().join("output.bin");
    extract_single_file(archive, "wanted.bin", &output, &PasswordSet::default()).unwrap();
    assert_eq!(std::fs::read(output).unwrap(), expected);
    assert!(check_archive_with_recovery(archive, false, None).is_err());
}

#[test]
fn single_file_extracts_legacy_unindexed_commit() {
    use std::io::Write;

    let directory = tempfile::tempdir().unwrap();
    let archive_path = directory.path().join("legacy.zar");
    let frame = crate::BlockFrame::from_uncompressed(0, b"legacy content", 1).unwrap();
    let frame_size = frame.encoded_size().unwrap() as u64;
    let header = crate::LogicalHeader::new(vec![crate::TocChange::PutFile {
        path: ArchivePath::new("legacy.txt").unwrap(),
        entry: crate::FileEntry::new(14, vec![0]).unwrap(),
    }]);
    let header_bytes = header.encode().unwrap();
    let provisional = crate::CommitHeader::new(0, [1, 0, 0, 0], frame_size, 0).unwrap();
    let probe = crate::HeaderFrame::from_uncompressed(
        provisional,
        &header_bytes,
        crate::UNENCRYPTED_DESCRIPTOR.to_vec(),
        1,
    )
    .unwrap();
    let commit = crate::CommitHeader::new(
        0,
        [1, 0, 0, 0],
        frame_size,
        probe.encoded_size().unwrap() as u32,
    )
    .unwrap();
    let header_frame = crate::HeaderFrame::from_uncompressed(
        commit,
        &header_bytes,
        crate::UNENCRYPTED_DESCRIPTOR.to_vec(),
        1,
    )
    .unwrap();
    let header_offset = crate::CONTAINER_HEADER_SIZE + crate::HEADER_OFFSET_SIZE + frame_size;
    let mut archive = std::fs::File::create(&archive_path).unwrap();
    crate::ContainerHeader::new(header_offset)
        .write_to(&mut archive)
        .unwrap();
    archive.write_all(&0_u64.to_le_bytes()).unwrap();
    frame.write_to(&mut archive).unwrap();
    header_frame.write_to(&mut archive).unwrap();
    archive.write_all(&header_offset.to_le_bytes()).unwrap();
    drop(archive);

    let info = info_archive(archive_path.to_str().unwrap(), true).unwrap();
    let with_blocks = info_archive_with_blocks(archive_path.to_str().unwrap(), true).unwrap();
    assert_eq!(info[0].details, with_blocks[0].commit.details);
    assert_eq!(with_blocks[0].blocks.len(), 1);
    assert_eq!(with_blocks[0].blocks[0].index, 0);
    assert_eq!(info[0].uncompressed_size, Some(14));

    let output = directory.path().join("output");
    extract_single_file(
        archive_path.to_str().unwrap(),
        "legacy.txt",
        &output,
        &PasswordSet::default(),
    )
    .unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"legacy content");

    let mut filters = FilterSet::default();
    filters.restrict_to_paths([ArchivePath::new("legacy.txt").unwrap()]);
    let selected_output = directory.path().join("selected");
    extract_archive_with_passwords_progress_and_filters_and_frame_validation(
        archive_path.to_str().unwrap(),
        &selected_output,
        options(CollisionPolicy::Error, ExistingPolicy::Error),
        &PasswordSet::default(),
        None,
        &filters,
        true,
    )
    .unwrap();
    assert_eq!(
        std::fs::read(selected_output.join("legacy.txt")).unwrap(),
        b"legacy content"
    );

    let duplicate = directory.path().join("duplicate.txt");
    let different = directory.path().join("different.txt");
    std::fs::write(&duplicate, b"legacy content").unwrap();
    std::fs::write(&different, b"legacy changed").unwrap();
    add_archive(archive_path.to_str().unwrap(), &[duplicate, different]).unwrap();
    let toc = list_archive(archive_path.to_str().unwrap(), None, None).unwrap();
    assert!(
        matches!(toc.get(&ArchivePath::new("duplicate.txt").unwrap()), Some(TocEntry::File(entry)) if entry.source_commit == Some([1, 0, 0, 0]))
    );
    assert!(
        matches!(toc.get(&ArchivePath::new("different.txt").unwrap()), Some(TocEntry::File(entry)) if entry.source_commit.is_none())
    );
    let contents =
        extract_archive_to_memory(archive_path.to_str().unwrap(), None, None, None, 0).unwrap();
    assert_eq!(contents["duplicate.txt"], b"legacy content");
    assert_eq!(contents["different.txt"], b"legacy changed");
}

#[test]
fn single_file_rejects_malformed_block_indexes() {
    use std::io::Write;

    let directory = tempfile::tempdir().unwrap();
    let archive_path = directory.path().join("indexed.zar");
    let output = directory.path().join("output");
    let block = crate::BlockFrame::from_stored(0, b"indexed content").unwrap();
    let block_size = block.encoded_size().unwrap() as u64;
    let good = crate::HeaderBlockIndexEntry {
        index: 0,
        offset: 0,
        encoded_size: block_size,
    };
    let write_archive = |index: Vec<crate::HeaderBlockIndexEntry>, checksum, size| {
        let mut entry = crate::FileEntry::new(size, vec![0]).unwrap();
        entry.checksum = checksum;
        let logical = crate::LogicalHeader::new(vec![crate::TocChange::PutFile {
            path: ArchivePath::new("file").unwrap(),
            entry,
        }])
        .with_block_index(index);
        let bytes = logical.encode().unwrap();
        let provisional = crate::CommitHeader::new(0, [1, 0, 0, 0], block_size, 0).unwrap();
        let probe = crate::HeaderFrame::from_uncompressed(
            provisional,
            &bytes,
            crate::UNENCRYPTED_DESCRIPTOR.to_vec(),
            1,
        )
        .unwrap();
        let header = crate::CommitHeader::new(
            0,
            [1, 0, 0, 0],
            block_size,
            probe.encoded_size().unwrap() as u32,
        )
        .unwrap();
        let frame = crate::HeaderFrame::from_uncompressed(
            header,
            &bytes,
            crate::UNENCRYPTED_DESCRIPTOR.to_vec(),
            1,
        )
        .unwrap();
        let offset = crate::CONTAINER_HEADER_SIZE + crate::HEADER_OFFSET_SIZE + block_size;
        let mut archive = std::fs::File::create(&archive_path).unwrap();
        crate::ContainerHeader::new(offset)
            .write_to(&mut archive)
            .unwrap();
        archive.write_all(&0_u64.to_le_bytes()).unwrap();
        block.write_to(&mut archive).unwrap();
        frame.write_to(&mut archive).unwrap();
        archive.write_all(&offset.to_le_bytes()).unwrap();
    };
    let extract = || {
        extract_single_file(
            archive_path.to_str().unwrap(),
            "file",
            &output,
            &PasswordSet::default(),
        )
    };
    write_archive(vec![good], None, 15);
    extract().unwrap();
    assert_eq!(std::fs::read(&output).unwrap(), b"indexed content");

    for bad in [
        vec![crate::HeaderBlockIndexEntry { offset: 1, ..good }],
        vec![crate::HeaderBlockIndexEntry {
            encoded_size: 19,
            ..good
        }],
        vec![crate::HeaderBlockIndexEntry {
            encoded_size: block_size - 1,
            ..good
        }],
        vec![crate::HeaderBlockIndexEntry {
            encoded_size: block_size + 1,
            ..good
        }],
        vec![crate::HeaderBlockIndexEntry { index: 1, ..good }],
        vec![
            good,
            crate::HeaderBlockIndexEntry {
                offset: block_size,
                ..good
            },
        ],
        Vec::new(),
    ] {
        write_archive(bad, None, 15);
        assert!(extract().is_err());
    }
    write_archive(
        vec![crate::HeaderBlockIndexEntry {
            encoded_size: block_size + 1,
            ..good
        }],
        None,
        15,
    );
    let duplicate = directory.path().join("duplicate");
    std::fs::write(&duplicate, b"indexed content").unwrap();
    let before = std::fs::read(&archive_path).unwrap();
    let error = add_archive(archive_path.to_str().unwrap(), &[duplicate]).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("source block index exceeds commit data")
    );
    assert_eq!(std::fs::read(&archive_path).unwrap(), before);
    write_archive(vec![good], Some(0), 15);
    assert!(
        extract()
            .unwrap_err()
            .to_string()
            .contains("checksum mismatch")
    );
    let error =
        extract_archive_to_memory(archive_path.to_str().unwrap(), None, None, None, 0).unwrap_err();
    assert!(error.to_string().contains("checksum mismatch"));
    let error =
        check_archive_with_recovery(archive_path.to_str().unwrap(), false, None).unwrap_err();
    assert!(error.to_string().contains("checksum mismatch"));
    let error = grep_archive(
        archive_path.to_str().unwrap(),
        "*",
        "no match",
        GrepOptions::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("checksum mismatch"));
    let partial =
        extract_archive_to_memory(archive_path.to_str().unwrap(), None, None, Some(5), 1).unwrap();
    assert_eq!(partial["file"], b"ndexe");
    write_archive(vec![good], None, 16);
    let error =
        check_archive_with_recovery(archive_path.to_str().unwrap(), false, None).unwrap_err();
    assert!(error.to_string().contains("decoded size mismatch"));
    assert!(
        extract()
            .unwrap_err()
            .to_string()
            .contains("decoded size mismatch")
    );
    let error =
        extract_archive_to_memory(archive_path.to_str().unwrap(), None, None, None, 0).unwrap_err();
    assert!(error.to_string().contains("decoded size mismatch"));
}

#[test]
fn listing_accepts_compressed_blocks_without_declared_content_size() {
    use std::io::Write;

    let directory = tempfile::tempdir().unwrap();
    let archive_path = directory.path().join("streamed.zar");
    let content = vec![b'x'; 4096];
    let mut encoder = zstd::stream::Encoder::new(Vec::new(), 3).unwrap();
    encoder.include_contentsize(false).unwrap();
    encoder.write_all(&content).unwrap();
    let compressed = encoder.finish().unwrap();
    assert_eq!(
        zstd::zstd_safe::get_frame_content_size(&compressed).unwrap(),
        None
    );
    let mut block = crate::BlockFrame::from_stored(0, &compressed).unwrap();
    block.flags = 0;
    let block_size = block.encoded_size().unwrap() as u64;
    let logical = crate::LogicalHeader::new(vec![crate::TocChange::PutFile {
        path: ArchivePath::new("streamed").unwrap(),
        entry: crate::FileEntry::new(content.len() as u64, vec![0]).unwrap(),
    }]);
    let bytes = logical.encode().unwrap();
    let provisional = crate::CommitHeader::new(0, [1, 0, 0, 0], block_size, 0).unwrap();
    let probe = crate::HeaderFrame::from_uncompressed(
        provisional,
        &bytes,
        crate::UNENCRYPTED_DESCRIPTOR.to_vec(),
        1,
    )
    .unwrap();
    let header = crate::CommitHeader::new(
        0,
        [1, 0, 0, 0],
        block_size,
        probe.encoded_size().unwrap() as u32,
    )
    .unwrap();
    let frame = crate::HeaderFrame::from_uncompressed(
        header,
        &bytes,
        crate::UNENCRYPTED_DESCRIPTOR.to_vec(),
        1,
    )
    .unwrap();
    let offset = crate::CONTAINER_HEADER_SIZE + crate::HEADER_OFFSET_SIZE + block_size;
    let mut archive = std::fs::File::create(&archive_path).unwrap();
    crate::ContainerHeader::new(offset)
        .write_to(&mut archive)
        .unwrap();
    archive.write_all(&0_u64.to_le_bytes()).unwrap();
    block.write_to(&mut archive).unwrap();
    frame.write_to(&mut archive).unwrap();
    archive.write_all(&offset.to_le_bytes()).unwrap();
    drop(archive);
    let (_, details) =
        list_archive_with_details(archive_path.to_str().unwrap(), None, None).unwrap();
    assert_eq!(
        details[&ArchivePath::new("streamed").unwrap()].compressed_size,
        Some(compressed.len() as u64)
    );
    let extracted =
        extract_archive_to_memory(archive_path.to_str().unwrap(), None, None, None, 0).unwrap();
    assert_eq!(extracted["streamed"], content);
}

#[test]
fn staged_blocks_encrypt_and_round_trip_for_each_content_cipher() {
    let header = crate::CommitHeader::new(0, [1, 0, 0, 0], 0, 64).unwrap();
    let key = [7; crate::DEK_SIZE];
    for contents in [b"small".to_vec(), vec![b'x'; 4096]] {
        let plain = crate::BlockFrame::from_uncompressed(7, &contents, 3).unwrap();
        for algorithm in [
            crate::ContentAead::Aes256Gcm,
            crate::ContentAead::ChaCha20Poly1305,
        ] {
            let encrypted =
                crate::BlockFrame::from_staged_encrypted(&plain, &header, &key, algorithm).unwrap();
            assert_ne!(encrypted.checksum(), plain.checksum());
            assert_eq!(
                encrypted
                    .uncompressed_encrypted(&header, &key, algorithm, contents.len())
                    .unwrap(),
                contents
            );
            assert!(
                crate::BlockFrame::from_staged_encrypted(&encrypted, &header, &key, algorithm)
                    .is_err()
            );
            assert!(
                encrypted
                    .uncompressed_encrypted(
                        &header,
                        &[8; crate::DEK_SIZE],
                        algorithm,
                        contents.len()
                    )
                    .is_err()
            );
            let mut encoded = Vec::new();
            encrypted.write_to(&mut encoded).unwrap();
            let decoded = crate::BlockFrame::read_from(&mut &encoded[..], encoded.len()).unwrap();
            assert_eq!(
                decoded
                    .uncompressed_encrypted(&header, &key, algorithm, contents.len())
                    .unwrap(),
                contents
            );
        }
    }
}

#[test]
fn frame_decoding_rejects_bad_descriptors_and_small_output_limits() {
    let header = crate::CommitHeader::new(0, [1, 0, 0, 0], 0, 64).unwrap();
    let key = [7; crate::DEK_SIZE];
    let content = b"stored contents";
    let invalid_descriptor = vec![255];
    assert_eq!(
        crate::HeaderFrame::from_uncompressed(
            header.clone(),
            content,
            invalid_descriptor.clone(),
            1
        ),
        Err(crate::FramingError::UnsupportedEncryption)
    );
    assert_eq!(
        crate::HeaderFrame::from_uncompressed_encrypted(
            header.clone(),
            content,
            invalid_descriptor.clone(),
            1,
            &key
        ),
        Err(crate::FramingError::UnsupportedEncryption)
    );
    let descriptor = crate::EncryptionDescriptor::Symmetric {
        scope: crate::EncryptionScope::Data,
        algorithm: crate::ContentAead::Aes256Gcm,
        nonce: [9; 12],
    }
    .encode()
    .unwrap();
    assert_eq!(
        crate::HeaderFrame::from_uncompressed_encrypted(
            header.clone(),
            content,
            descriptor.clone(),
            1,
            &key
        ),
        Err(crate::FramingError::InvalidDescriptor)
    );
    let mut frame =
        crate::HeaderFrame::from_uncompressed(header.clone(), content, descriptor, 1).unwrap();
    assert_eq!(frame.uncompressed_size(), Some(content.len() as u64));
    assert_eq!(frame.checksum(), xxhash_rust::xxh3::xxh3_64(&frame.payload));
    assert_eq!(
        frame.uncompressed_encrypted(&key, 1024),
        Err(crate::FramingError::InvalidDescriptor)
    );
    frame.encryption_descriptor = invalid_descriptor;
    assert_eq!(
        frame.uncompressed_encrypted(&key, 1024),
        Err(crate::FramingError::UnsupportedEncryption)
    );
    assert_eq!(
        frame.uncompressed(1024),
        Err(crate::FramingError::UnsupportedEncryption)
    );
    let stored = crate::BlockFrame::from_stored(1, content).unwrap();
    assert_eq!(
        stored.uncompressed_at_most(content.len() - 1),
        Err(crate::FramingError::InvalidSize)
    );
    assert_eq!(
        stored.uncompressed_encrypted(&header, &key, crate::ContentAead::Aes256Gcm, 1024),
        Err(crate::FramingError::InvalidDescriptor)
    );
    let encrypted = crate::BlockFrame::from_staged_encrypted(
        &stored,
        &header,
        &key,
        crate::ContentAead::Aes256Gcm,
    )
    .unwrap();
    assert_eq!(
        encrypted.uncompressed_encrypted(
            &header,
            &key,
            crate::ContentAead::Aes256Gcm,
            content.len() - 1
        ),
        Err(crate::FramingError::InvalidSize)
    );
}

#[test]
fn logical_header_enforces_metadata_limits_on_read_and_write() {
    let path = ArchivePath::new("entry").unwrap();
    let entry = crate::EntryMetadata {
        path: path.clone(),
        platform: "test".to_owned(),
        owner: None,
        group: None,
        permissions: None,
        modified_unix_millis: None,
        created_unix_millis: None,
        accessed_unix_millis: None,
    };
    let header = crate::LogicalHeader::new(vec![crate::TocChange::PutDirectory { path }])
        .with_metadata(std::collections::HashMap::from([(
            "key".to_owned(),
            crate::MetadataType::String("value".to_owned()),
        )]))
        .with_entry_metadata(vec![entry]);
    let encoded = header.encode().unwrap();
    for (limits, encode_error) in [
        (
            crate::LogicalHeaderLimits {
                max_encoded_size: encoded.len() - 1,
                ..Default::default()
            },
            crate::LogicalHeaderError::LimitExceeded,
        ),
        (
            crate::LogicalHeaderLimits {
                max_metadata_entries: 0,
                ..Default::default()
            },
            crate::LogicalHeaderError::LimitExceeded,
        ),
        (
            crate::LogicalHeaderLimits {
                max_metadata_key_size: 2,
                ..Default::default()
            },
            crate::LogicalHeaderError::InvalidMetadata,
        ),
        (
            crate::LogicalHeaderLimits {
                max_metadata_string_size: 4,
                ..Default::default()
            },
            crate::LogicalHeaderError::LimitExceeded,
        ),
        (
            crate::LogicalHeaderLimits {
                max_entry_metadata: 0,
                ..Default::default()
            },
            crate::LogicalHeaderError::LimitExceeded,
        ),
        (
            crate::LogicalHeaderLimits {
                max_path_size: 4,
                ..Default::default()
            },
            crate::LogicalHeaderError::LimitExceeded,
        ),
    ] {
        assert_eq!(header.encode_with_limits(limits), Err(encode_error));
        assert_eq!(
            crate::LogicalHeader::decode_with_limits(&encoded, limits),
            Err(crate::LogicalHeaderError::LimitExceeded)
        );
    }
    let mut invalid = encoded.clone();
    invalid[0] = b'X';
    let error = crate::LogicalHeader::decode(&invalid).unwrap_err();
    assert_eq!(error, crate::LogicalHeaderError::InvalidMagic);
    assert!(error.to_string().contains("magic"));
}

#[test]
fn logical_header_rejects_malformed_metadata_and_reference_flags() {
    let header =
        crate::LogicalHeader::new(Vec::new()).with_metadata(std::collections::HashMap::from([(
            "key".to_owned(),
            crate::MetadataType::String("value".to_owned()),
        )]));
    let encoded = header.encode().unwrap();
    let key = encoded
        .windows(3)
        .position(|bytes| bytes == b"key")
        .unwrap();
    let value = encoded
        .windows(5)
        .position(|bytes| bytes == b"value")
        .unwrap();
    for offset in [key, value] {
        let mut invalid = encoded.clone();
        invalid[offset] = 0xff;
        assert_eq!(
            crate::LogicalHeader::decode(&invalid),
            Err(crate::LogicalHeaderError::InvalidUtf8Metadata)
        );
    }
    let mut invalid = encoded;
    invalid[key + 3] = 0xff;
    assert_eq!(
        crate::LogicalHeader::decode(&invalid),
        Err(crate::LogicalHeaderError::InvalidMetadata)
    );

    let path = ArchivePath::new("file").unwrap();
    let mut entry = crate::FileEntry::new(3, vec![0]).unwrap();
    entry.source_commit = Some(*b"base");
    let header =
        crate::LogicalHeader::with_block_size(vec![crate::TocChange::PutFile { path, entry }], 8);
    let encoded = header.encode().unwrap();
    assert_eq!(crate::LogicalHeader::decode(&encoded).unwrap(), header);
    for offset in [
        encoded.len() - 8,
        encoded.len() - 7,
        encoded.len() - 6,
        encoded.len() - 1,
    ] {
        let mut invalid = encoded.clone();
        invalid[offset] = 2;
        assert_eq!(
            crate::LogicalHeader::decode(&invalid),
            Err(crate::LogicalHeaderError::InvalidFileEntry)
        );
    }

    let device = crate::LogicalHeader::new(vec![crate::TocChange::PutDevice {
        path: ArchivePath::new("device").unwrap(),
        kind: crate::DeviceKind::Block,
        major: 1,
        minor: 2,
    }]);
    let mut encoded = device.encode().unwrap();
    assert_eq!(crate::LogicalHeader::decode(&encoded).unwrap(), device);
    let kind_offset = encoded.len() - 9;
    encoded[kind_offset] = 0xff;
    assert!(matches!(
        crate::LogicalHeader::decode(&encoded),
        Err(crate::LogicalHeaderError::InvalidRecord(_))
    ));
}

#[test]
fn pipeline_block_encoding_preserves_compression_bypass_with_encryption() {
    let header = crate::CommitHeader::new(0, [1, 0, 0, 0], 0, 64).unwrap();
    let key = [7; crate::DEK_SIZE];
    let contents = vec![b'x'; 4096];
    let plain = encode_block(1, &contents, 3, true, None).unwrap();
    assert_ne!(plain.flags & crate::BLOCK_FLAG_UNCOMPRESSED, 0);
    assert_eq!(
        plain.uncompressed_at_most(contents.len()).unwrap(),
        contents
    );
    for algorithm in [
        crate::ContentAead::Aes256Gcm,
        crate::ContentAead::ChaCha20Poly1305,
    ] {
        let context =
            EncryptionContext::new(zeroize::Zeroizing::new(key), header.clone(), algorithm);
        let stored = encode_block(2, &contents, 3, true, Some(&context)).unwrap();
        assert_ne!(stored.flags & crate::BLOCK_FLAG_UNCOMPRESSED, 0);
        assert_ne!(stored.flags & crate::BLOCK_FLAG_ENCRYPTED, 0);
        assert_eq!(
            stored
                .uncompressed_encrypted(&header, &key, algorithm, contents.len())
                .unwrap(),
            contents
        );
        let compressed = encode_block(3, &contents, 3, false, Some(&context)).unwrap();
        assert_eq!(compressed.flags & crate::BLOCK_FLAG_UNCOMPRESSED, 0);
        assert_eq!(
            compressed
                .uncompressed_encrypted(&header, &key, algorithm, contents.len())
                .unwrap(),
            contents
        );
    }
}

#[cfg(not(target_os = "wasi"))]
#[test]
fn compression_workers_prioritize_queues_and_stop_when_writer_closes() {
    for (preference, expected) in [
        (WorkerPreference::SmallTinyFirst, [1, 2]),
        (WorkerPreference::BigFileFirst, [2, 1]),
    ] {
        let (small_tx, small_rx) = crossbeam_channel::unbounded();
        let (big_tx, big_rx) = crossbeam_channel::unbounded();
        let (writer_tx, writer_rx) = crossbeam_channel::unbounded();
        small_tx
            .send(BlockWorkItem {
                block_index: 1,
                data: b"small".to_vec(),
                skip_compression: true,
            })
            .unwrap();
        big_tx
            .send(BlockWorkItem {
                block_index: 2,
                data: b"large".to_vec(),
                skip_compression: false,
            })
            .unwrap();
        drop(small_tx);
        drop(big_tx);
        worker_loop(preference, &small_rx, &big_rx, &writer_tx, 3, None);
        let blocks: Vec<_> = writer_rx.try_iter().map(Result::unwrap).collect();
        assert_eq!(
            blocks
                .iter()
                .map(|block| block.block_index)
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            blocks[0].uncompressed_at_most(5).unwrap(),
            if expected[0] == 1 { b"small" } else { b"large" }
        );
    }
    let (small_tx, small_rx) = crossbeam_channel::unbounded();
    let (big_tx, big_rx) = crossbeam_channel::unbounded();
    let (writer_tx, writer_rx) = crossbeam_channel::unbounded();
    for index in 1..=2 {
        small_tx
            .send(BlockWorkItem {
                block_index: index,
                data: b"data".to_vec(),
                skip_compression: true,
            })
            .unwrap();
    }
    drop(writer_rx);
    worker_loop(
        WorkerPreference::SmallTinyFirst,
        &small_rx,
        &big_rx,
        &writer_tx,
        3,
        None,
    );
    assert_eq!(small_rx.recv().unwrap().block_index, 2);
    drop(big_tx);
}

#[test]
fn small_file_packer_reuses_duplicates_and_flushes_incompatible_blocks() {
    let mut packer = SmallFilePacker::new(16);
    let mut next_index = 0;
    let mut allocate = || {
        next_index += 1;
        Ok(next_index)
    };
    let checksum = |contents: &[u8]| xxhash_rust::xxh3::xxh3_64(contents);
    let (index, offset, completed) = packer
        .add(b"abc".to_vec(), checksum(b"abc"), false, &mut allocate)
        .unwrap();
    assert_eq!((index, offset), (1, 0));
    assert!(completed.is_none());
    let (index, offset, completed) = packer
        .add(b"ab".to_vec(), checksum(b"ab"), false, &mut allocate)
        .unwrap();
    assert_eq!((index, offset), (1, 3));
    assert!(completed.is_none());
    let (index, offset, completed) = packer
        .add(b"abc".to_vec(), checksum(b"abc"), false, &mut allocate)
        .unwrap();
    assert_eq!((index, offset), (1, 0));
    assert!(completed.is_none());
    let (index, offset, completed) = packer
        .add(b"raw".to_vec(), checksum(b"raw"), true, &mut allocate)
        .unwrap();
    assert_eq!((index, offset), (2, 0));
    let completed = completed.unwrap();
    assert_eq!(completed.block_index, 1);
    assert_eq!(completed.data, b"abcab");
    assert!(!completed.skip_compression);
    let contents = vec![b'x'; 14];
    let (index, offset, completed) = packer
        .add(contents.clone(), checksum(&contents), true, &mut allocate)
        .unwrap();
    assert_eq!((index, offset), (3, 0));
    assert_eq!(completed.unwrap().data, b"raw");
    assert_eq!(packer.take_block().unwrap().unwrap().data, contents);
    assert!(packer.take_block().unwrap().is_none());
    assert!(packer.add(Vec::new(), 0, false, &mut allocate).is_err());
    assert!(packer.add(vec![0; 16], 0, false, &mut allocate).is_err());
}

#[test]
fn multiblock_extraction_validates_full_and_indexed_batches() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("large.bin");
    let archive_path = directory.path().join("large.zar");
    let workers = std::thread::available_parallelism().map_or(1, usize::from);
    let block_size = 256;
    let contents: Vec<u8> = (0..(workers + 3) * block_size)
        .map(|index| (index.wrapping_mul(31) % 251) as u8)
        .collect();
    std::fs::write(&input, &contents).unwrap();
    create_archive_with_encoding(
        archive_path.to_str().unwrap(),
        &[input],
        false,
        EncodingOptions {
            block_size,
            compression_level: 1,
        },
    )
    .unwrap();
    let archive = archive_path.to_str().unwrap();
    let toc = list_archive(archive, None, None).unwrap();
    assert!(
        matches!(toc.get(&ArchivePath::new("large.bin").unwrap()), Some(TocEntry::File(entry)) if entry.block_indices.len() > workers)
    );
    let extracted = extract_archive_to_memory(archive, None, None, None, 0).unwrap();
    assert_eq!(extracted["large.bin"], contents);
    let output = directory.path().join("single");
    extract_single_file(archive, "large.bin", &output, &PasswordSet::default()).unwrap();
    assert_eq!(std::fs::read(output).unwrap(), contents);
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
fn metadata_values_and_invalid_updates_are_handled_without_mutation() {
    use crate::MetadataType;

    for (input, expected) in [
        ("42", MetadataType::I64(42)),
        ("-1.5", MetadataType::F64(-1.5)),
        ("NaN", MetadataType::String("NaN".into())),
        ("inf", MetadataType::String("inf".into())),
        ("text", MetadataType::String("text".into())),
    ] {
        assert_eq!(parse_metadata_value(input), expected);
        assert_eq!(format_metadata_value(&expected), input);
    }
    let fixture = Fixture::new();
    let before = std::fs::read(&fixture.archive).unwrap();
    let value = MetadataType::String("value".into());
    for (set, remove, message) in [
        (vec![], vec![], "nothing to change"),
        (
            vec![("".to_owned(), value.clone())],
            vec![],
            "must not be empty",
        ),
        (
            vec![("zar.invalid".to_owned(), value.clone())],
            vec![],
            "must not start",
        ),
        (
            vec![("key".to_owned(), value.clone())],
            vec!["key".to_owned()],
            "both set and removed",
        ),
        (
            vec![
                ("key".to_owned(), value.clone()),
                ("key".to_owned(), value.clone()),
            ],
            vec![],
            "set more than once",
        ),
        (vec![], vec!["missing".to_owned()], "not found"),
    ] {
        let error = update_custom_metadata(fixture.archive(), &set, &remove).unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
        assert_eq!(std::fs::read(&fixture.archive).unwrap(), before);
    }
}

#[test]
fn input_summary_classifies_boundaries_and_detects_overflow() {
    use super::pipeline::{TreeEntry, TreeEntryKind, summarize_tree_entries};

    let entry = |kind, size| TreeEntry {
        archive_path: "entry".into(),
        source_path: PathBuf::from("entry"),
        kind,
        size,
        modified_unix_millis: None,
        created_unix_millis: None,
        accessed_unix_millis: None,
    };
    let entries = vec![
        entry(TreeEntryKind::Directory, 0),
        entry(TreeEntryKind::File, 0),
        entry(TreeEntryKind::File, 8),
        entry(TreeEntryKind::File, 9),
        entry(TreeEntryKind::File, 8 * 1024 * 1024 - 1),
        entry(TreeEntryKind::File, 8 * 1024 * 1024),
        entry(
            TreeEntryKind::Symlink {
                target: "file".into(),
            },
            0,
        ),
        entry(TreeEntryKind::NamedPipe, 0),
        entry(
            TreeEntryKind::Device {
                kind: crate::DeviceKind::Block,
                major: 1,
                minor: 2,
            },
            0,
        ),
        entry(TreeEntryKind::Unsupported, 0),
    ];
    let summary = summarize_tree_entries(&entries).unwrap();
    assert_eq!(
        (summary.files, summary.directories, summary.total_bytes),
        (5, 1, 16 * 1024 * 1024 + 16)
    );
    assert_eq!(
        (
            summary.empty_files,
            summary.tiny_files,
            summary.small_files,
            summary.big_files
        ),
        (1, 1, 2, 1)
    );
    assert_eq!(
        (
            summary.symlinks,
            summary.named_pipes,
            summary.devices,
            summary.other_special
        ),
        (1, 1, 1, 1)
    );
    assert!(
        summarize_tree_entries(&[
            entry(TreeEntryKind::File, u64::MAX),
            entry(TreeEntryKind::File, 1)
        ])
        .is_err()
    );
    assert!(archive_prefix(Path::new("../outside"), Path::new(".")).is_err());
    assert_eq!(
        archive_prefix(Path::new("./nested/file"), Path::new(".")).unwrap(),
        "nested/file"
    );
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
fn check_archive_honors_progress_cancellation() {
    struct CancelAfterStart(AtomicUsize);

    impl Progress for CancelAfterStart {
        fn message(&self, _: &str) {}
        fn advance(&self, _: u64) {}

        fn is_cancelled(&self) -> bool {
            self.0.fetch_add(1, Ordering::Relaxed) >= 1
        }
    }

    let fixture = Fixture::new();
    let progress = CancelAfterStart(AtomicUsize::new(0));
    let error =
        check_archive_with_recovery_progress(fixture.archive(), false, None, Some(&progress))
            .unwrap_err();
    assert!(error.to_string().contains("operation cancelled"));
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
    assert!(!report.logical_state_readable);
}

#[test]
fn check_recovers_next_valid_commit_after_damaged_data() {
    let fixture = Fixture::new();
    let second = fixture.dir.path().join("second.txt");
    std::fs::write(&second, b"independent second commit").unwrap();
    add_archive(fixture.archive(), &[second]).unwrap();
    let third = fixture.dir.path().join("third.txt");
    std::fs::write(&third, b"independent third commit").unwrap();
    add_archive(fixture.archive(), &[third]).unwrap();
    let mut file = std::fs::File::open(&fixture.archive).unwrap();
    let offset = crate::ContainerHeader::read_from(&mut file)
        .unwrap()
        .first_header_offset;
    let (_, data_start, next_offset) =
        read_commit_at_offset(&mut file, offset, crate::CommitValidationLimits::default()).unwrap();
    let (_, next_data_start, _) = read_commit_at_offset(
        &mut file,
        next_offset,
        crate::CommitValidationLimits::default(),
    )
    .unwrap();
    drop(file);
    let mut bytes = std::fs::read(&fixture.archive).unwrap();
    bytes[data_start as usize..data_start as usize + 4].copy_from_slice(b"BAD!");
    let damaged = fixture.dir.path().join("damaged-data.zar");
    std::fs::write(&damaged, bytes).unwrap();
    let path = damaged.to_str().unwrap();
    assert!(check_archive_with_recovery(path, false, None).is_err());
    let report = check_archive_with_recovery(path, true, None).unwrap();
    assert_eq!(report.recoveries, 1);
    assert_eq!(report.commits, 2);
    assert_eq!(report.checked_blocks, 2);
    assert!(!report.logical_state_readable);
    let mut cyclic_bytes = std::fs::read(&damaged).unwrap();
    cyclic_bytes[(next_data_start as usize - 8)..next_data_start as usize]
        .copy_from_slice(&next_offset.to_le_bytes());
    std::fs::write(&damaged, cyclic_bytes).unwrap();
    let error = check_archive_with_recovery(path, true, None).unwrap_err();
    assert!(error.to_string().contains("commit chain contains a cycle"));
}

#[test]
fn archive_cursor_helpers_reject_invalid_offsets_and_truncated_data() {
    use std::io::{Read, Seek, SeekFrom};

    let fixture = Fixture::new();
    let limits = crate::CommitValidationLimits::default();
    let mut archive = std::fs::File::open(&fixture.archive).unwrap();
    let (offset, header) = find_last_commit(&mut archive, limits).unwrap();
    assert_eq!(header.commit_id, [1, 0, 0, 0]);
    assert_eq!(
        read_commit_at_offset(&mut archive, offset, limits)
            .unwrap()
            .0
            .commit_id,
        header.commit_id
    );
    assert!(read_commit_at_offset(&mut archive, 0, limits).is_err());

    let path = fixture.dir.path().join("bytes");
    std::fs::write(&path, vec![7; 10_000]).unwrap();
    let mut file = std::fs::File::open(&path).unwrap();
    skip_exact(&mut file, 9_000).unwrap();
    assert_eq!(file.stream_position().unwrap(), 9_000);
    let mut byte = [0];
    file.read_exact(&mut byte).unwrap();
    assert_eq!(byte, [7]);
    assert_eq!(
        skip_exact(&mut file, 1_000).unwrap_err().kind(),
        std::io::ErrorKind::UnexpectedEof
    );
    file.seek(SeekFrom::Start(0)).unwrap();
    skip_exact(&mut file, 0).unwrap();
    assert_eq!(file.stream_position().unwrap(), 0);

    let damaged = fixture.dir.path().join("damaged-tail.zar");
    std::fs::write(&damaged, b"short").unwrap();
    assert!(find_last_commit(&mut std::fs::File::open(&damaged).unwrap(), limits).is_err());
    let mut bytes = std::fs::read(&fixture.archive).unwrap();
    let end = bytes.len();
    bytes[end - 8..].copy_from_slice(&0_u64.to_le_bytes());
    std::fs::write(&damaged, bytes).unwrap();
    let error = find_last_commit(&mut std::fs::File::open(&damaged).unwrap(), limits).unwrap_err();
    assert!(error.to_string().contains("no valid last commit"));
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
    let (locked_toc, locked_details) = list_archive_with_details(archive, None, None).unwrap();
    assert!(locked_toc.is_empty());
    assert!(locked_details.is_empty());
    let progress = TestProgress::default();
    let reporter = ProgressReporter(Some(&progress));
    let (skipped, sources, _) =
        load_extraction_state_with_progress(archive, Some("00000001"), None, Some(&reporter))
            .unwrap();
    assert!(skipped.is_empty());
    assert!(sources.is_empty());
    assert!(!progress.advances.lock().unwrap().is_empty());
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
    let error = load_extraction_state(data_only.to_str().unwrap(), None, None)
        .err()
        .unwrap();
    assert!(
        error
            .to_string()
            .contains("data-encrypted commit without a password")
    );
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
fn special_entries_can_be_renamed_and_removed_from_archive_history() {
    use std::os::unix::ffi::OsStrExt;
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    std::fs::create_dir(&input).unwrap();
    std::os::unix::fs::symlink("target", input.join("link")).unwrap();
    let pipe = std::ffi::CString::new(input.join("pipe").as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(pipe.as_ptr(), 0o600) }, 0);
    let archive = dir.path().join("special.zar");
    create_archive_with_password_progress_and_special_files(
        archive.to_str().unwrap(),
        &[input],
        false,
        EncodingOptions::default(),
        None,
        None,
        CreateInputOptions {
            include_special_files: true,
            ..Default::default()
        },
    )
    .unwrap();
    let path = archive.to_str().unwrap();
    let original = ArchivePath::new("input/link").unwrap();
    let renamed = ArchivePath::new("input/renamed").unwrap();
    rename_archive_entry(path, &original, &renamed).unwrap();
    let pipe = ArchivePath::new("input/pipe").unwrap();
    remove_archive_entries_with_progress(path, std::slice::from_ref(&pipe), None).unwrap();
    let toc = list_archive(path, None, None).unwrap();
    assert_eq!(
        toc.get(&renamed),
        Some(&TocEntry::Symlink {
            target: "target".into()
        })
    );
    assert!(toc.get(&original).is_none());
    assert!(toc.get(&pipe).is_none());
    let historical = list_archive(path, Some("00000001"), None).unwrap();
    assert_eq!(historical.get(&pipe), Some(&TocEntry::NamedPipe));
    assert!(historical.get(&original).is_some());
}

#[test]
fn special_entry_extraction_plans_respect_restore_and_existing_policies() {
    use std::io::Write;

    let directory = tempfile::tempdir().unwrap();
    let archive_path = directory.path().join("special.zar");
    let logical = crate::LogicalHeader::new(vec![
        crate::TocChange::PutSymlink {
            path: ArchivePath::new("link").unwrap(),
            target: "target".into(),
        },
        crate::TocChange::PutNamedPipe {
            path: ArchivePath::new("pipe").unwrap(),
        },
        crate::TocChange::PutDevice {
            path: ArchivePath::new("device").unwrap(),
            kind: crate::DeviceKind::Character,
            major: 2,
            minor: 3,
        },
    ]);
    let bytes = logical.encode().unwrap();
    let provisional = crate::CommitHeader::new(0, [1, 0, 0, 0], 0, 0).unwrap();
    let probe = crate::HeaderFrame::from_uncompressed(
        provisional,
        &bytes,
        crate::UNENCRYPTED_DESCRIPTOR.to_vec(),
        1,
    )
    .unwrap();
    let header =
        crate::CommitHeader::new(0, [1, 0, 0, 0], 0, probe.encoded_size().unwrap() as u32).unwrap();
    let frame = crate::HeaderFrame::from_uncompressed(
        header,
        &bytes,
        crate::UNENCRYPTED_DESCRIPTOR.to_vec(),
        1,
    )
    .unwrap();
    let offset = crate::CONTAINER_HEADER_SIZE + crate::HEADER_OFFSET_SIZE;
    let mut archive = std::fs::File::create(&archive_path).unwrap();
    crate::ContainerHeader::new(offset)
        .write_to(&mut archive)
        .unwrap();
    archive.write_all(&0_u64.to_le_bytes()).unwrap();
    frame.write_to(&mut archive).unwrap();
    archive.write_all(&offset.to_le_bytes()).unwrap();
    drop(archive);

    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();
    let mut settings = options(CollisionPolicy::Error, ExistingPolicy::Error);
    settings.dry_run = true;
    let archive = archive_path.to_str().unwrap();
    let plan = extract_archive_with_progress(archive, &destination, settings, None, None).unwrap();
    assert_eq!(plan.files.len(), 1);
    assert!(matches!(plan.files[0].2, ExtractedKind::NamedPipe));

    settings.restore_symlinks = true;
    settings.restore_devices = true;
    let plan = extract_archive_with_progress(archive, &destination, settings, None, None).unwrap();
    assert_eq!(plan.files.len(), 3);
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;

        let output = directory.path().join("unix-specials");
        let mut executable = settings;
        executable.dry_run = false;
        executable.restore_devices = false;
        extract_archive_with_progress(archive, &output, executable, None, None).unwrap();
        assert_eq!(
            std::fs::read_link(output.join("link")).unwrap(),
            Path::new("target")
        );
        assert!(
            std::fs::symlink_metadata(output.join("pipe"))
                .unwrap()
                .file_type()
                .is_fifo()
        );
        assert!(!output.join("device").exists());
    }
    #[cfg(not(unix))]
    {
        let mut executable = settings;
        executable.dry_run = false;
        let error = extract_archive_with_progress(archive, &destination, executable, None, None)
            .err()
            .unwrap();
        assert!(error.to_string().contains("unsupported"));
        assert!(!destination.join("pipe").exists());
    }
    for name in ["link", "pipe", "device"] {
        std::fs::write(destination.join(name), b"existing").unwrap();
    }
    let error = extract_archive_with_progress(archive, &destination, settings, None, None)
        .err()
        .unwrap();
    assert!(
        error
            .to_string()
            .contains("destination path already exists")
    );
    settings.existing_policy = ExistingPolicy::Skip;
    assert!(
        extract_archive_with_progress(archive, &destination, settings, None, None)
            .unwrap()
            .files
            .is_empty()
    );
    settings.existing_policy = ExistingPolicy::Overwrite;
    assert_eq!(
        extract_archive_with_progress(archive, &destination, settings, None, None)
            .unwrap()
            .files
            .len(),
        3
    );
    settings.dry_run = false;
    let error = extract_archive_with_progress(archive, &destination, settings, None, None)
        .err()
        .unwrap();
    assert!(
        error
            .to_string()
            .contains("destination type does not match")
    );
    settings.dry_run = true;

    settings.existing_policy = ExistingPolicy::Ask;
    for (decision, expected) in [
        (OverwriteDecision::Yes, 3),
        (OverwriteDecision::No, 0),
        (OverwriteDecision::Always, 3),
        (OverwriteDecision::Never, 0),
    ] {
        let progress = TestProgress::default();
        *progress.decision.lock().unwrap() = Some(decision);
        let plan =
            extract_archive_with_progress(archive, &destination, settings, None, Some(&progress))
                .unwrap();
        assert_eq!(plan.files.len(), expected);
    }
    for name in ["link", "pipe", "device"] {
        assert_eq!(std::fs::read(destination.join(name)).unwrap(), b"existing");
    }

    let paths = ["link", "pipe", "device"].map(|name| {
        (
            ArchivePath::new(name).unwrap(),
            ArchivePath::new(format!("moved-{name}")).unwrap(),
        )
    });
    rename_archive_entries_with_encryption(archive, &paths, None, &PasswordSet::default()).unwrap();
    let renamed = list_archive(archive, None, None).unwrap();
    for (old, new) in &paths {
        assert!(renamed.get(old).is_none());
        assert!(renamed.get(new).is_some());
    }
    assert_eq!(
        renamed.get(&paths[0].1),
        Some(&TocEntry::Symlink {
            target: "target".into()
        })
    );
    assert_eq!(renamed.get(&paths[1].1), Some(&TocEntry::NamedPipe));
    assert_eq!(
        renamed.get(&paths[2].1),
        Some(&TocEntry::Device {
            kind: crate::DeviceKind::Character,
            major: 2,
            minor: 3
        })
    );
    join_archive_with_encoding(archive, "00000001", "00000002", EncodingOptions::default())
        .unwrap();
    let joined = list_archive(archive, None, None).unwrap();
    for (_, new) in &paths {
        assert_eq!(joined.get(new), renamed.get(new));
    }
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

#[cfg(unix)]
#[test]
fn tar_extraction_restores_symlinks_only_when_enabled() {
    let directory = tempfile::tempdir().unwrap();
    let archive_path = directory.path().join("links.tar");
    let mut builder = tar::Builder::new(std::fs::File::create(&archive_path).unwrap());
    let mut header = tar::Header::new_gnu();
    header.set_mode(0o644);
    header.set_size(4);
    builder
        .append_data(&mut header, "file", &b"data"[..])
        .unwrap();
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_mode(0o777);
    header.set_size(0);
    header.set_link_name("file").unwrap();
    builder.append_data(&mut header, "link", &[][..]).unwrap();
    builder.finish().unwrap();
    drop(builder);
    for restore in [false, true] {
        let output = directory
            .path()
            .join(if restore { "restored" } else { "skipped" });
        let mut settings = options(CollisionPolicy::Error, ExistingPolicy::Error);
        settings.restore_symlinks = restore;
        extract_archive_with_progress(
            archive_path.to_str().unwrap(),
            &output,
            settings,
            None,
            None,
        )
        .unwrap();
        assert_eq!(std::fs::read(output.join("file")).unwrap(), b"data");
        if restore {
            assert_eq!(
                std::fs::read_link(output.join("link")).unwrap(),
                Path::new("file")
            );
            assert_eq!(std::fs::read(output.join("link")).unwrap(), b"data");
        } else {
            assert!(!output.join("link").exists());
        }
    }
}

#[cfg(unix)]
#[test]
fn archive_creation_scans_and_restores_unix_links_and_fifo() {
    use std::os::unix::fs::{FileTypeExt, symlink};

    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input");
    std::fs::create_dir(&input).unwrap();
    std::fs::write(input.join("file"), b"content").unwrap();
    symlink("file", input.join("link")).unwrap();
    assert!(
        std::process::Command::new("mkfifo")
            .arg(input.join("pipe"))
            .status()
            .unwrap()
            .success()
    );
    for save_metadata in [false, true] {
        let archive = directory.path().join(if save_metadata {
            "metadata.zar"
        } else {
            "no-metadata.zar"
        });
        create_archive_with_password_progress_and_special_files(
            archive.to_str().unwrap(),
            std::slice::from_ref(&input),
            false,
            EncodingOptions::default(),
            None,
            None,
            CreateInputOptions {
                save_metadata,
                include_special_files: true,
                ..Default::default()
            },
        )
        .unwrap();
        let toc = list_archive(archive.to_str().unwrap(), None, None).unwrap();
        assert!(
            matches!(toc.get(&ArchivePath::new("input/link").unwrap()), Some(TocEntry::Symlink { target }) if target == "file")
        );
        assert_eq!(
            toc.get(&ArchivePath::new("input/pipe").unwrap()),
            Some(&TocEntry::NamedPipe)
        );
        let output = directory.path().join(if save_metadata {
            "metadata-output"
        } else {
            "plain-output"
        });
        let mut settings = options(CollisionPolicy::Error, ExistingPolicy::Error);
        settings.restore_symlinks = true;
        settings.restore_metadata = false;
        extract_archive_with_progress(archive.to_str().unwrap(), &output, settings, None, None)
            .unwrap();
        assert_eq!(
            std::fs::read(output.join("input/link")).unwrap(),
            b"content"
        );
        assert!(
            std::fs::symlink_metadata(output.join("input/pipe"))
                .unwrap()
                .file_type()
                .is_fifo()
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn archive_creation_records_device_without_reading_it() {
    use std::os::unix::fs::MetadataExt;

    let device = PathBuf::from("/dev/null");
    let raw_device = std::fs::symlink_metadata(&device).unwrap().rdev();
    let directory = tempfile::tempdir().unwrap();
    for save_metadata in [false, true] {
        let archive = directory.path().join(if save_metadata {
            "metadata.zar"
        } else {
            "plain.zar"
        });
        create_archive_with_password_progress_and_special_files(
            archive.to_str().unwrap(),
            std::slice::from_ref(&device),
            false,
            EncodingOptions::default(),
            None,
            None,
            CreateInputOptions {
                save_metadata,
                include_special_files: true,
                ..Default::default()
            },
        )
        .unwrap();
        let toc = list_archive(archive.to_str().unwrap(), None, None).unwrap();
        assert_eq!(
            toc.get(&ArchivePath::new("null").unwrap()),
            Some(&TocEntry::Device {
                kind: crate::DeviceKind::Character,
                major: libc::major(raw_device),
                minor: libc::minor(raw_device),
            })
        );
    }
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
    assert_eq!(
        select_automatic_encoding(1024 * 1024 * 1024, usize::MAX),
        select_automatic_encoding(1024 * 1024 * 1024, 1024)
    );
    let mut opts = EncodingOptions {
        block_size: 0,
        ..Default::default()
    };
    resolve_encoding_options(&mut opts);
    assert!(opts.block_size >= AUTO_MIN_BLOCK_SIZE);
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
        finalize_entry_metadata(
            vec![entry_date_metadata(&path, Some(1), None, None)],
            false,
            false,
            false
        )
        .len(),
        0
    );
    assert_eq!(
        finalize_entry_metadata(vec![captured], true, false, false).len(),
        1
    );
}

#[test]
fn filesystem_input_summary_classifies_sizes_and_relative_paths() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("files");
    std::fs::create_dir(&input).unwrap();
    std::fs::write(input.join("empty"), []).unwrap();
    std::fs::write(input.join("tiny"), b"12345678").unwrap();
    std::fs::write(input.join("small"), b"123456789").unwrap();
    let big = std::fs::File::create(input.join("big")).unwrap();
    big.set_len(8 * 1024 * 1024).unwrap();
    drop(big);
    let summary = scan_input_summary(&[PathBuf::from("files")], directory.path()).unwrap();
    assert_eq!(summary.directories, 1);
    assert_eq!(summary.files, 4);
    assert_eq!(
        (
            summary.empty_files,
            summary.tiny_files,
            summary.small_files,
            summary.big_files
        ),
        (1, 1, 1, 1)
    );
    assert_eq!(summary.total_bytes, 8 * 1024 * 1024 + 17);
    assert!(scan_input_summary(&[PathBuf::from("missing")], directory.path()).is_err());
}

#[test]
fn timestamps_are_independent_for_create_and_add() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.txt");
    let second = dir.path().join("second.txt");
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    let creation_available = std::fs::metadata(&first).unwrap().created().is_ok()
        && std::fs::metadata(&second).unwrap().created().is_ok();
    let access_available = std::fs::metadata(&first).unwrap().accessed().is_ok()
        && std::fs::metadata(&second).unwrap().accessed().is_ok();

    for (modified, created, accessed) in [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (false, false, true),
        (true, true, false),
        (true, false, true),
        (false, true, true),
        (true, true, true),
    ] {
        if (created && !creation_available) || (accessed && !access_available) {
            continue;
        }
        let archive = dir
            .path()
            .join(format!("{modified}-{created}-{accessed}.zar"));
        let archive = archive.to_str().unwrap();
        create_archive_with_password_progress_and_special_files(
            archive,
            std::slice::from_ref(&first),
            false,
            EncodingOptions::default(),
            None,
            None,
            CreateInputOptions {
                save_modification_time: modified,
                save_creation_time: created,
                save_access_time: accessed,
                ..Default::default()
            },
        )
        .unwrap();
        add_archive_with_encoding_progress_and_options(
            archive,
            std::slice::from_ref(&second),
            EncodingOptions::default(),
            None,
            AddInputOptions {
                save_modification_time: modified,
                save_creation_time: created,
                save_access_time: accessed,
                ..Default::default()
            },
        )
        .unwrap();
        let (_, details) = list_archive_with_details(archive, None, None).unwrap();
        for path in ["first.txt", "second.txt"] {
            let entry = &details[&ArchivePath::new(path).unwrap()];
            assert_eq!(entry.modified_unix_millis.is_some(), modified, "{path}");
            assert_eq!(entry.created_unix_millis.is_some(), created, "{path}");
            assert_eq!(entry.accessed_unix_millis.is_some(), accessed, "{path}");
        }
    }
}

#[test]
fn missing_creation_time_is_reported() {
    let path = ArchivePath::new("missing.txt").unwrap();
    let entry = entry_date_metadata(&path, Some(1), None, None);
    assert!(
        require_creation_times(std::slice::from_ref(&entry))
            .unwrap_err()
            .to_string()
            .contains("missing.txt")
    );
    assert!(
        require_access_times(&[entry])
            .unwrap_err()
            .to_string()
            .contains("missing.txt")
    );
}

#[cfg(unix)]
#[test]
fn access_time_is_captured_before_file_contents_are_read() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.txt");
    let archive = dir.path().join("archive.zar");
    std::fs::write(&input, b"contents").unwrap();
    let timestamp = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000);
    std::fs::File::open(&input)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_accessed(timestamp))
        .unwrap();
    create_archive_with_password_progress_and_special_files(
        archive.to_str().unwrap(),
        std::slice::from_ref(&input),
        false,
        EncodingOptions::default(),
        None,
        None,
        CreateInputOptions {
            save_access_time: true,
            ..Default::default()
        },
    )
    .unwrap();
    let (_, details) = list_archive_with_details(archive.to_str().unwrap(), None, None).unwrap();
    assert_eq!(
        details[&ArchivePath::new("input.txt").unwrap()].accessed_unix_millis,
        Some(1_600_000_000_000)
    );
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
    std::fs::write(&target, b"existing contents are longer than the archive").unwrap();
    assert!(extract_archive_with_progress(fixture.archive(), &dest, opts, None, None).is_err());
    opts.existing_policy = ExistingPolicy::Skip;
    extract_archive_with_progress(fixture.archive(), &dest, opts, None, None).unwrap();
    assert_eq!(
        std::fs::read(&target).unwrap(),
        b"existing contents are longer than the archive"
    );
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
fn default_confirmation_callback_controls_existing_file_extraction() {
    struct Confirm(bool);
    impl Progress for Confirm {
        fn message(&self, _: &str) {}
        fn advance(&self, _: u64) {}
        fn is_interactive(&self) -> bool {
            true
        }
        fn confirm(&self, _: &str) -> Result<bool, Box<dyn std::error::Error>> {
            Ok(self.0)
        }
    }

    let fixture = Fixture::new();
    let dest = fixture.dir.path().join("confirm-output");
    let mut opts = options(CollisionPolicy::Error, ExistingPolicy::Error);
    extract_archive_with_progress(fixture.archive(), &dest, opts, None, None).unwrap();
    let target = dest.join("input/a.txt");
    opts.existing_policy = ExistingPolicy::Ask;
    for (answer, expected) in [
        (false, b"keep".as_slice()),
        (true, b"alpha content".as_slice()),
    ] {
        std::fs::write(&target, b"keep").unwrap();
        extract_archive_with_progress(fixture.archive(), &dest, opts, None, Some(&Confirm(answer)))
            .unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), expected);
    }
}

#[cfg(unix)]
#[test]
fn external_tar_symlinks_are_restored_only_when_requested() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("links.tar");
    let mut builder = tar::Builder::new(std::fs::File::create(&archive).unwrap());
    let mut file_header = tar::Header::new_gnu();
    file_header.set_size(7);
    file_header.set_mode(0o644);
    file_header.set_cksum();
    builder
        .append_data(&mut file_header, "root/file", &b"content"[..])
        .unwrap();
    let mut link_header = tar::Header::new_gnu();
    link_header.set_entry_type(tar::EntryType::Symlink);
    link_header.set_size(0);
    link_header.set_mode(0o777);
    link_header.set_cksum();
    builder
        .append_link(&mut link_header, "root/link", "file")
        .unwrap();
    builder.finish().unwrap();
    drop(builder);

    for restore in [false, true] {
        let destination = dir
            .path()
            .join(if restore { "with-link" } else { "without-link" });
        let mut opts = options(CollisionPolicy::Error, ExistingPolicy::Error);
        opts.restore_symlinks = restore;
        extract_archive_with_progress(archive.to_str().unwrap(), &destination, opts, None, None)
            .unwrap();
        assert_eq!(
            std::fs::read(destination.join("root/file")).unwrap(),
            b"content"
        );
        let link = destination.join("root/link");
        if restore {
            assert_eq!(std::fs::read_link(link).unwrap(), Path::new("file"));
        } else {
            assert!(!link.exists());
        }
    }
}

#[test]
fn compressed_tar_input_is_imported_and_extracted() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.tar.gz");
    let gzip = flate2::write::GzEncoder::new(
        std::fs::File::create(&input).unwrap(),
        flate2::Compression::default(),
    );
    let mut tar = tar::Builder::new(gzip);
    let mut header = tar::Header::new_gnu();
    header.set_size(7);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, "nested/file.txt", &b"content"[..])
        .unwrap();
    tar.into_inner().unwrap().finish().unwrap();

    let archive = dir.path().join("imported.zar");
    create_archive(archive.to_str().unwrap(), std::slice::from_ref(&input)).unwrap();
    let files = extract_archive_to_memory(archive.to_str().unwrap(), None, None, None, 0).unwrap();
    assert_eq!(files["nested/file.txt"], b"content");
    let destination = dir.path().join("direct");
    extract_archive_with_progress(
        input.to_str().unwrap(),
        &destination,
        options(CollisionPolicy::Error, ExistingPolicy::Error),
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        std::fs::read(destination.join("nested/file.txt")).unwrap(),
        b"content"
    );
}

#[test]
fn external_tar_extraction_respects_existing_policies_and_strip() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("input.tar");
    let mut builder = tar::Builder::new(std::fs::File::create(&archive).unwrap());
    for (path, data) in [("root/a.txt", b"alpha".as_slice()), ("root/b.txt", b"beta")] {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, path, data).unwrap();
    }
    builder.finish().unwrap();
    drop(builder);

    let archive = archive.to_str().unwrap();
    let destination = dir.path().join("output");
    let mut opts = options(CollisionPolicy::Error, ExistingPolicy::Error);
    opts.strip_top_level = true;
    opts.dry_run = true;
    let plan = extract_archive_with_progress(archive, &destination, opts, None, None).unwrap();
    assert_eq!(plan.files.len(), 2);
    assert!(!destination.exists());

    opts.dry_run = false;
    extract_archive_with_progress(archive, &destination, opts, None, None).unwrap();
    let first = destination.join("a.txt");
    let second = destination.join("b.txt");
    assert_eq!(std::fs::read(&first).unwrap(), b"alpha");
    assert_eq!(std::fs::read(&second).unwrap(), b"beta");
    std::fs::write(&first, b"existing-a").unwrap();
    std::fs::write(&second, b"existing-b").unwrap();
    assert!(extract_archive_with_progress(archive, &destination, opts, None, None).is_err());
    opts.existing_policy = ExistingPolicy::Skip;
    extract_archive_with_progress(archive, &destination, opts, None, None).unwrap();
    assert_eq!(std::fs::read(&first).unwrap(), b"existing-a");
    assert_eq!(std::fs::read(&second).unwrap(), b"existing-b");

    opts.existing_policy = ExistingPolicy::Ask;
    assert!(extract_archive_with_progress(archive, &destination, opts, None, None).is_err());
    let progress = TestProgress::default();
    for (decision, expected_first, expected_second) in [
        (
            OverwriteDecision::No,
            b"existing-a".as_slice(),
            b"existing-b".as_slice(),
        ),
        (
            OverwriteDecision::Never,
            b"existing-a".as_slice(),
            b"existing-b".as_slice(),
        ),
        (
            OverwriteDecision::Yes,
            b"alpha".as_slice(),
            b"beta".as_slice(),
        ),
        (
            OverwriteDecision::Always,
            b"alpha".as_slice(),
            b"beta".as_slice(),
        ),
    ] {
        std::fs::write(&first, b"existing-a").unwrap();
        std::fs::write(&second, b"existing-b").unwrap();
        *progress.decision.lock().unwrap() = Some(decision);
        extract_archive_with_progress(archive, &destination, opts, None, Some(&progress)).unwrap();
        assert_eq!(std::fs::read(&first).unwrap(), expected_first);
        assert_eq!(std::fs::read(&second).unwrap(), expected_second);
    }
    opts.existing_policy = ExistingPolicy::Overwrite;
    extract_archive_with_progress(archive, &destination, opts, None, None).unwrap();
    assert_eq!(std::fs::read(&first).unwrap(), b"alpha");
    assert_eq!(std::fs::read(&second).unwrap(), b"beta");

    let filters =
        FilterSet::from_patterns(&["root/a.txt".to_owned()], &[], &[], &[], &[] as &[PathBuf])
            .unwrap();
    let filtered = dir.path().join("filtered");
    let plan = extract_archive_with_passwords_progress_and_filters(
        archive,
        &filtered,
        opts,
        &PasswordSet::default(),
        None,
        &filters,
    )
    .unwrap();
    assert_eq!(plan.files.len(), 1);
    assert_eq!(std::fs::read(filtered.join("a.txt")).unwrap(), b"alpha");
    assert!(!filtered.join("b.txt").exists());

    let missing = dir.path().join("missing.tar");
    let error =
        extract_archive_with_progress(missing.to_str().unwrap(), &filtered, opts, None, None)
            .err()
            .unwrap();
    assert!(error.to_string().contains("not a file"));
}

#[test]
fn extraction_collision_policies_and_strip_top_level() {
    let fixture = Fixture::new();
    let other_input = fixture.dir.path().join("other/input");
    std::fs::create_dir_all(&other_input).unwrap();
    std::fs::write(other_input.join("A.txt"), b"upper").unwrap();
    add_archive(fixture.archive(), &[other_input]).unwrap();
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
        decision: Mutex::new(Some(OverwriteDecision::Yes)),
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

#[test]
fn extraction_ask_choices_apply_to_each_conflict_or_all_conflicts() {
    struct DecisionProgress {
        decision: OverwriteDecision,
        prompts: std::sync::atomic::AtomicUsize,
    }

    impl Progress for DecisionProgress {
        fn message(&self, _: &str) {}
        fn advance(&self, _: u64) {}
        fn is_interactive(&self) -> bool {
            true
        }
        fn confirm_overwrite(
            &self,
            _: &str,
        ) -> Result<OverwriteDecision, Box<dyn std::error::Error>> {
            self.prompts.fetch_add(1, Ordering::Relaxed);
            Ok(self.decision)
        }
    }

    let fixture = Fixture::new();
    let other_input = fixture.dir.path().join("other/input");
    std::fs::create_dir_all(other_input.join("nested")).unwrap();
    std::fs::write(other_input.join("A.txt"), b"upper").unwrap();
    std::fs::write(other_input.join("nested/B.txt"), b"upper nested").unwrap();
    add_archive(fixture.archive(), &[other_input]).unwrap();

    let mut plans = Vec::new();
    for (decision, expected_prompts) in [
        (OverwriteDecision::Yes, 2),
        (OverwriteDecision::No, 2),
        (OverwriteDecision::Always, 1),
        (OverwriteDecision::Never, 1),
    ] {
        let progress = DecisionProgress {
            decision,
            prompts: std::sync::atomic::AtomicUsize::new(0),
        };
        let mut opts = options(CollisionPolicy::Ask, ExistingPolicy::Error);
        opts.dry_run = true;
        let destination = fixture.dir.path().join("collisions");
        let plan = extract_archive_with_progress(
            fixture.archive(),
            &destination,
            opts,
            None,
            Some(&progress),
        )
        .unwrap();
        assert_eq!(progress.prompts.load(Ordering::Relaxed), expected_prompts);
        plans.push(
            plan.files
                .into_iter()
                .map(|(path, _, _)| path)
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(plans[0], plans[2]);
    assert_eq!(plans[1], plans[3]);
    assert_ne!(plans[0], plans[1]);

    let fixture = Fixture::new();
    let destination = fixture.dir.path().join("existing");
    std::fs::create_dir_all(destination.join("input/nested")).unwrap();
    std::fs::write(destination.join("input/a.txt"), b"old a").unwrap();
    std::fs::write(destination.join("input/nested/b.txt"), b"old b").unwrap();
    for (decision, expected_prompts, expected_files) in [
        (OverwriteDecision::Yes, 2, 2),
        (OverwriteDecision::No, 2, 0),
        (OverwriteDecision::Always, 1, 2),
        (OverwriteDecision::Never, 1, 0),
    ] {
        let progress = DecisionProgress {
            decision,
            prompts: std::sync::atomic::AtomicUsize::new(0),
        };
        let mut opts = options(CollisionPolicy::Error, ExistingPolicy::Ask);
        opts.dry_run = true;
        let plan = extract_archive_with_progress(
            fixture.archive(),
            &destination,
            opts,
            None,
            Some(&progress),
        )
        .unwrap();
        assert_eq!(progress.prompts.load(Ordering::Relaxed), expected_prompts);
        assert_eq!(plan.files.len(), expected_files);
    }
}
