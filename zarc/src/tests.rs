use super::*;
use clap::Parser;
use std::io::Seek;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use zarlib::{
    ContentAead, EncryptionDescriptor, EncryptionScope, FileEntry, HeaderFrame, LogicalHeader,
    UNENCRYPTED_DESCRIPTOR, unwrap_dek_with_password, write_container_magic,
};

#[test]
fn extraction_summary_is_formatted_for_cli_before_work_begins() {
    let summary = InputSummary {
        files: 2,
        small_files: 1,
        empty_files: 1,
        directories: 1,
        total_bytes: 1536,
        symlinks: 1,
        ..InputSummary::default()
    };
    let lines = format_summary_lines("Extract", &summary);
    assert_eq!(
        lines[0],
        "Extract: 2 file(s) and 1 dir(s) with 1.50 KiB total"
    );
    assert_eq!(lines[1], "File Groups: 1 empty, 0 tiny, 1 small, 0 big");
    assert_eq!(lines[2], "Special Files: 1 symlinks, 0 pipes, 0 devices");
}

#[test]
fn info_json_includes_commit_ids_and_framing_sizes() {
    let json = render_info_json(&[InfoJsonCommit {
        id: "00000001".to_owned(),
        size: 128,
        header_size: 64,
        data_size: 64,
        header_encrypted: false,
        data_encrypted: false,
        encryption_algorithm: None,
    }]);

    assert_eq!(
        json,
        "{\"commit_count\":1,\"commits\":[{\"id\":\"00000001\",\"size\":128,\"header_size\":64,\"data_size\":64,\"header_encrypted\":false,\"data_encrypted\":false,\"encryption_algorithm\":null}]}"
    );
    assert_eq!(render_info_json(&[]), "{\"commit_count\":0,\"commits\":[]}");
}

#[test]
fn info_commit_line_includes_uncompressed_size_and_ratio() {
    let directory = TestDirectory::new();
    let input = directory.path().join("input.txt");
    std::fs::write(&input, b"hello world").unwrap();
    let archive = directory.path().join("info-size.zar");
    create_archive(archive.to_str().unwrap(), std::slice::from_ref(&input)).unwrap();

    let commits = info_archive(archive.to_str().unwrap(), false).unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].uncompressed_size, Some(11));
    assert_eq!(
        commits[0].ratio_percent,
        Some(commits[0].size as f64 * 100.0 / 11.0)
    );
    assert!(format_commit_summary(&commits[0]).contains("uncompressed=11 B ratio="));
}

#[test]
fn verbose_info_renders_stored_special_entry_counts() {
    let stats = CommitFileStats {
        symlinks: 2,
        named_pipes: 3,
        devices: 4,
        ..CommitFileStats::default()
    };

    assert!(
        render_commit_stats(&stats).contains("special entries: 2 symlinks, 3 pipes, 4 devices")
    );
}

#[test]
fn verbose_info_renders_saved_entry_metadata() {
    assert!(render_commit_stats(&CommitFileStats::default()).contains("saved metadata: none"));
    let stats = CommitFileStats {
        entry_metadata: zarlib::operations::EntryMetadataStats {
            owner: 2,
            group: 2,
            permissions: 2,
            dates: 1,
        },
        ..CommitFileStats::default()
    };
    assert!(
        render_commit_stats(&stats)
            .contains("saved metadata: owner (2), group (2), permissions (2), date (1)")
    );
}

#[test]
fn verbose_info_only_renders_deduplication_when_files_were_deduplicated() {
    let without_deduplication = render_commit_stats(&CommitFileStats::default());
    assert!(!without_deduplication.contains("deduplicated files:"));

    let with_deduplication = render_commit_stats(&CommitFileStats {
        deduplicated_files: 1,
        deduplicated_uncompressed_size: 128,
        ..CommitFileStats::default()
    });
    assert!(with_deduplication.contains("deduplicated files: 1, saved uncompressed space: 128 B"));
}

type TestCommit = (Vec<TocChange>, Vec<(u32, Vec<u8>)>);

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

#[test]
fn verbose_commit_summary_reports_group_sizes_and_deduplication() {
    let header = LogicalHeader::with_block_size(
        vec![
            TocChange::PutFile {
                path: archive_path("one.txt"),
                entry: FileEntry::new(5, vec![1]).unwrap(),
            },
            TocChange::PutFile {
                path: archive_path("copy.txt"),
                entry: FileEntry::new(5, vec![1]).unwrap(),
            },
            TocChange::PutFile {
                path: archive_path("empty"),
                entry: FileEntry::new(0, vec![]).unwrap(),
            },
        ],
        64 * 1024,
    );
    let blocks = HashMap::from([(1, (16, Some(5)))]);
    let stats = summarize_commit_files(&header, &blocks).unwrap();

    assert_eq!(stats.groups["tiny files"].files, 2);
    assert_eq!(stats.groups["tiny files"].compressed_size, 16);
    assert_eq!(stats.groups["tiny files"].uncompressed_size, 5);
    assert_eq!(stats.groups["empty files"].files, 1);
    assert_eq!(stats.groups["empty files"].compressed_size, 0);
    assert_eq!(stats.deduplicated_files, 1);
    assert_eq!(stats.deduplicated_uncompressed_size, 5);
    assert_eq!(
        (stats.symlinks, stats.named_pipes, stats.devices),
        (0, 0, 0)
    );
}

#[test]
fn info_reports_commit_header_compressed_and_uncompressed_sizes() {
    let directory = TestDirectory::new();
    let file_path = directory.path().join("payload.bin");
    std::fs::write(&file_path, vec![b'a'; 64 * 1024]).unwrap();
    let archive_path = directory.path().join("header-info.zar");
    create_archive_with_encoding(
        archive_path.to_str().unwrap(),
        std::slice::from_ref(&file_path),
        false,
        EncodingOptions {
            block_size: 64 * 1024,
            compression_level: 3,
        },
    )
    .unwrap();

    let commits = info_archive(archive_path.to_str().unwrap(), true).unwrap();
    let header = commits[0].header_compression.as_ref().unwrap();
    assert!(header.compressed_size > 0);
    assert!(header.uncompressed_size.unwrap() > 0);
    assert!(header.ratio_percent.unwrap() > 0.0);
}

#[test]
fn clap_parser_accepts_cli_options_and_rejects_invalid_usage() {
    let cli = Cli::try_parse_from([
        "zarc",
        "create",
        "archive.zar",
        "input.txt",
        "--strict",
        "--metadata",
        "--password",
        "--overwrite",
        "--block-size",
        "64K",
        "--compression-level",
        "7",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Command::Create {
            strict: true,
            metadata: true,
            password: true,
            overwrite: true,
            ref paths,
            ref block_size,
            ref compression_level,
            ..
        } if paths == &[PathBuf::from("input.txt")]
            && block_size.as_deref() == Some("64K")
            && compression_level.as_deref() == Some("7")
    ));

    assert_eq!(
        Cli::try_parse_from(["zarc", "create", "archive.zar"])
            .unwrap_err()
            .kind(),
        clap::error::ErrorKind::MissingRequiredArgument
    );
    assert_eq!(
        Cli::try_parse_from(["zarc", "info", "archive.zar", "--unknown"])
            .unwrap_err()
            .kind(),
        clap::error::ErrorKind::UnknownArgument
    );
    assert!(Cli::try_parse_from(["zarc", "create", "archive.zar", "input", "--metadata"]).is_ok());
    assert!(matches!(
        Cli::try_parse_from(["zarc", "create", "archive.zar", "input", "--overwrite"])
            .unwrap()
            .command,
        Command::Create {
            overwrite: true,
            ..
        }
    ));
    assert!(matches!(
        Cli::try_parse_from(["zarc", "create", "archive.zar", "input", "--special-files"])
            .unwrap()
            .command,
        Command::Create {
            special_files: true,
            ..
        }
    ));
    assert!(matches!(
        Cli::try_parse_from(["zarc", "create", "archive.zar", "input", "--verbose"])
            .unwrap()
            .command,
        Command::Create { verbose: true, .. }
    ));
    assert!(Cli::try_parse_from(["zarc", "add", "archive.zar", "input", "--metadata"]).is_ok());
    assert!(matches!(
        Cli::try_parse_from(["zarc", "add", "archive.zar", "input", "--special-files"])
            .unwrap()
            .command,
        Command::Add {
            special_files: true,
            ..
        }
    ));
    assert!(matches!(
        Cli::try_parse_from(["zarc", "add", "archive.zar", "input", "--strict"])
            .unwrap()
            .command,
        Command::Add { strict: true, .. }
    ));
    assert!(matches!(
        Cli::try_parse_from(["zarc", "add", "archive.zar", "input", "--verbose"])
            .unwrap()
            .command,
        Command::Add { verbose: true, .. }
    ));
    assert!(matches!(
        Cli::try_parse_from([
            "zarc",
            "create",
            "archive.zar",
            "input",
            "--include-file",
            "*.rs",
            "--include-file",
            "*.toml",
            "--exclude-folder",
            "target",
            "--filelist",
            "filters.txt",
        ])
        .unwrap()
        .command,
        Command::Create {
            ref include_files,
            ref exclude_folders,
            ref filelist,
            ..
        } if include_files == &["*.rs", "*.toml"]
            && exclude_folders == &["target"]
            && filelist == &[PathBuf::from("filters.txt")]
    ));
    assert!(matches!(
        Cli::try_parse_from([
            "zarc",
            "extract",
            "archive.zar",
            "--include-folder",
            "docs",
            "--exclude-file",
            "*.tmp",
        ])
        .unwrap()
        .command,
        Command::Extract {
            ref include_folders,
            ref exclude_files,
            ..
        } if include_folders == &["docs"] && exclude_files == &["*.tmp"]
    ));
    assert!(Cli::try_parse_from(["zarc", "extract", "archive.zar", "--metadata"]).is_ok());
    assert!(Cli::try_parse_from(["zarc", "info", "archive.zar", "-v"]).is_ok());
    assert!(Cli::try_parse_from(["zarc", "info", "archive.zar", "--verbose"]).is_ok());
    assert!(Cli::try_parse_from(["zarc", "info", "archive.zar", "--json", "--verbose"]).is_err());
}

#[test]
fn clap_parser_accepts_short_subcommand_aliases() {
    let aliases = [
        ("c", "create"),
        ("a", "add"),
        ("r", "remove"),
        ("j", "join"),
        ("k", "check"),
        ("i", "info"),
        ("l", "list"),
        ("e", "extract"),
    ];

    for (alias, command) in aliases {
        let alias_help = Cli::try_parse_from(["zarc", alias, "--help"]).unwrap_err();
        let command_help = Cli::try_parse_from(["zarc", command, "--help"]).unwrap_err();
        assert_eq!(
            alias_help.kind(),
            clap::error::ErrorKind::DisplayHelp,
            "alias {alias} should be recognized"
        );
        assert_eq!(
            alias_help.to_string(),
            command_help.to_string(),
            "alias {alias} should resolve to {command}"
        );
    }
}

#[test]
fn create_parser_accepts_both_password_aead_algorithms() {
    for (algorithm, expected) in [
        ("aes-256-gcm", "aes-256-gcm"),
        ("chacha20-poly1305", "chacha20-poly1305"),
    ] {
        let cli = Cli::try_parse_from([
            "zarc",
            "create",
            "--password",
            "--aead",
            algorithm,
            "encrypted.zar",
            "input",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Create { aead, encryption_scope, .. }
                if aead == expected && encryption_scope == "header-and-data"
        ));
    }

    assert!(
        Cli::try_parse_from([
            "zarc",
            "create",
            "--aead",
            "unsupported",
            "encrypted.zar",
            "input",
        ])
        .is_err()
    );
}

#[test]
fn cli_accepts_encryption_scope_and_repeated_commit_password_prompts() {
    let cli = Cli::try_parse_from([
        "zarc",
        "create",
        "--password",
        "--encryption-scope",
        "data",
        "archive.zar",
        "input",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Command::Create {
            encryption_scope,
            ..
        } if encryption_scope == "data"
    ));

    let cli = Cli::try_parse_from([
        "zarc",
        "extract",
        "--password-for",
        "00000001",
        "--password-for",
        "00000002",
        "archive.zar",
    ])
    .unwrap();
    assert!(matches!(
        cli.command,
        Command::Extract { password_for, .. }
            if password_for == ["00000001", "00000002"]
    ));
}

#[test]
fn create_help_flag_displays_detailed_help_and_usage_examples() {
    let err = Cli::try_parse_from(["zarc", "create", "-h"]).unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    let output = err.to_string();
    assert!(output.contains("Create a new single-commit ZAR archive"));
    assert!(output.contains("USAGE EXAMPLES:"));
    assert!(output.contains("zarc create archive.zar"));
    assert!(output.contains("--block-size"));
    assert!(output.contains("--compression-level"));
    assert!(output.contains("--strict"));
}

#[test]
fn all_subcommands_have_detailed_help_messages() {
    for cmd in ["add", "remove", "join", "check", "info", "list", "extract"] {
        let err = Cli::try_parse_from(["zarc", cmd, "-h"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
        let output = err.to_string();
        assert!(
            output.contains("USAGE EXAMPLES:"),
            "subcommand {cmd} missing usage examples"
        );
    }
}

#[test]
fn duration_and_throughput_formatting_works_correctly() {
    assert_eq!(
        format_duration(std::time::Duration::from_millis(50)),
        "0.05s"
    );
    assert_eq!(
        format_duration(std::time::Duration::from_millis(1500)),
        "1.50s"
    );
    assert_eq!(
        format_duration(std::time::Duration::from_secs(65)),
        "1m 5.00s"
    );
    assert_eq!(format_compression_ratio(50, 100), "50.00%");
    assert_eq!(format_compression_ratio(10, 100), "10.00%");
    assert_eq!(format_compression_ratio(0, 0), "n/a");
}

struct RecordingProgress {
    messages: Mutex<Vec<String>>,
    increments: Mutex<Vec<u64>>,
    total: Mutex<Option<u64>>,
    message_times: Mutex<Vec<Instant>>,
    input_summary: Mutex<Option<InputSummary>>,
    extraction_summary: Mutex<Option<InputSummary>>,
    events: Mutex<Vec<String>>,
}

impl Progress for RecordingProgress {
    fn message(&self, message: &str) {
        self.messages.lock().unwrap().push(message.to_owned());
        self.message_times.lock().unwrap().push(Instant::now());
        self.events
            .lock()
            .unwrap()
            .push(format!("message:{message}"));
    }
    fn advance(&self, amount: u64) {
        self.increments.lock().unwrap().push(amount);
    }
    fn set_total(&self, total: u64) {
        *self.total.lock().unwrap() = Some(total);
        self.events.lock().unwrap().push(format!("total:{total}"));
    }
    fn start_phase(&self, message: &str, total: u64) {
        self.events.lock().unwrap().push(format!("start:{message}"));
        self.set_total(total);
    }
    fn reset_phase(&self, message: &str, total: u64) {
        self.events.lock().unwrap().push(format!("reset:{message}"));
        self.set_total(total);
    }
    fn add_summary(&self, summary: &InputSummary) {
        *self.input_summary.lock().unwrap() = Some(summary.clone());
    }
    fn extract_summary(&self, summary: &InputSummary) {
        *self.extraction_summary.lock().unwrap() = Some(summary.clone());
        self.events.lock().unwrap().push("summary".to_owned());
    }
}

#[test]
fn progress_labels_every_input_type_and_normalizes_sizes_to_bytes() {
    let directory = TestDirectory::new();
    let source = directory.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("data.bin"), vec![3_u8; 1536]).unwrap();
    let progress = RecordingProgress {
        messages: Mutex::new(Vec::new()),
        increments: Mutex::new(Vec::new()),
        total: Mutex::new(None),
        message_times: Mutex::new(Vec::new()),
        input_summary: Mutex::new(None),
        extraction_summary: Mutex::new(None),
        events: Mutex::new(Vec::new()),
    };
    let archive = directory.path().join("progress-create.zar");
    create_archive_with_password_progress(
        archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        false,
        EncodingOptions {
            block_size: 64 * 1024,
            compression_level: 1,
        },
        None,
        Some(&progress),
        false,
    )
    .unwrap();
    assert_eq!(*progress.total.lock().unwrap(), Some(1536));
    let summary = progress.input_summary.lock().unwrap().clone().unwrap();
    assert_eq!(summary.files, 1);
    assert_eq!(summary.directories, 1);
    assert_eq!(summary.small_files, 1);
    assert_eq!(summary.empty_files, 0);
    assert_eq!(summary.total_bytes, 1536);
    assert!(
        progress
            .messages
            .lock()
            .unwrap()
            .iter()
            .any(|message| message.starts_with("Creating "))
    );
    assert_eq!(
        progress.increments.lock().unwrap().iter().sum::<u64>(),
        1536
    );
}

#[test]
fn extract_progress_reports_summary_before_nonzero_work_and_counts_bytes() {
    let directory = TestDirectory::new();
    let source = directory.path().join("payload.bin");
    let payload = vec![b'x'; 4096];
    std::fs::write(&source, &payload).unwrap();
    let archive = directory.path().join("extract-progress.zar");
    create_archive_with_encoding(
        archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        false,
        EncodingOptions {
            block_size: 64 * 1024,
            compression_level: 1,
        },
    )
    .unwrap();
    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("payload.bin"), b"existing").unwrap();
    let progress = RecordingProgress {
        messages: Mutex::new(Vec::new()),
        increments: Mutex::new(Vec::new()),
        total: Mutex::new(None),
        message_times: Mutex::new(Vec::new()),
        input_summary: Mutex::new(None),
        extraction_summary: Mutex::new(None),
        events: Mutex::new(Vec::new()),
    };

    assert!(
        extract_archive_with_progress(
            archive.to_str().unwrap(),
            &destination,
            ExtractionOptions {
                dry_run: false,
                through: None,
                collision_policy: CollisionPolicy::Error,
                existing_policy: ExistingPolicy::Error,
                restore_metadata: false,
                restore_symlinks: false,
                restore_devices: false,
                strip_top_level: false,
            },
            None,
            Some(&progress),
        )
        .is_err()
    );

    assert!(
        progress
            .total
            .lock()
            .unwrap()
            .is_some_and(|total| total > 0)
    );
    let summary = progress.extraction_summary.lock().unwrap().clone().unwrap();
    assert_eq!(summary.files, 1);
    assert_eq!(summary.small_files, 1);
    assert_eq!(summary.total_bytes, payload.len() as u64);
    let events = progress.events.lock().unwrap();
    assert_eq!(
        events.first().map(String::as_str),
        Some("start:Reading archive headers")
    );
    assert!(events.iter().any(|event| event == "summary"));
    assert!(
        events
            .iter()
            .any(|event| event == "reset:Preparing extraction")
    );
    assert!(events.iter().any(|event| event.starts_with("total:")));
    let progressed = progress.increments.lock().unwrap().iter().sum::<u64>();
    assert!(progressed > 0);
    assert_eq!(
        std::fs::read(destination.join("payload.bin")).unwrap(),
        b"existing"
    );
}

#[test]
fn extract_summary_precedes_data_frame_validation() {
    let directory = TestDirectory::new();
    let archive = make_archive(vec![(
        vec![TocChange::PutFile {
            path: archive_path("broken.bin"),
            entry: FileEntry::new(3, vec![0]).unwrap(),
        }],
        vec![(0, b"bad".to_vec())],
    )]);
    let archive = write_archive(&directory, &archive);
    // Corrupt a byte in the data-block region (after container header +
    // next_header_offset, before the header frame) so the TOC is still
    // readable but block validation fails during extraction.
    let mut bytes = std::fs::read(&archive).unwrap();
    let corrupt_offset =
        zarlib::CONTAINER_HEADER_SIZE as usize + zarlib::HEADER_OFFSET_SIZE as usize + 2;
    bytes[corrupt_offset] ^= 0xff;
    std::fs::write(&archive, &bytes).unwrap();

    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();
    let progress = RecordingProgress {
        messages: Mutex::new(Vec::new()),
        increments: Mutex::new(Vec::new()),
        total: Mutex::new(None),
        message_times: Mutex::new(Vec::new()),
        input_summary: Mutex::new(None),
        extraction_summary: Mutex::new(None),
        events: Mutex::new(Vec::new()),
    };
    assert!(
        extract_archive_with_progress(
            archive.to_str().unwrap(),
            &destination,
            ExtractionOptions {
                dry_run: false,
                through: None,
                collision_policy: CollisionPolicy::Error,
                existing_policy: ExistingPolicy::Error,
                restore_metadata: false,
                restore_symlinks: false,
                restore_devices: false,
                strip_top_level: false,
            },
            None,
            Some(&progress),
        )
        .is_err()
    );

    let summary = progress.extraction_summary.lock().unwrap().clone().unwrap();
    assert_eq!(summary.files, 1);
    assert_eq!(summary.total_bytes, 3);
    assert!(
        progress
            .total
            .lock()
            .unwrap()
            .is_some_and(|total| total > 0)
    );
    let events = progress.events.lock().unwrap();
    assert_eq!(events[0], "start:Reading archive headers");
    assert!(events.iter().any(|event| event == "summary"));
    assert!(
        events
            .iter()
            .any(|event| event == "reset:Preparing extraction")
    );
}

#[test]
fn add_progress_uses_adding_message_byte_counts_and_total() {
    let directory = TestDirectory::new();
    let initial = directory.path().join("initial.bin");
    let added = directory.path().join("added.bin");
    let bytes = vec![b'a'; 4096];
    std::fs::write(&initial, b"start").unwrap();
    std::fs::write(&added, &bytes).unwrap();
    let archive = directory.path().join("progress-add.zar");
    create_archive(archive.to_str().unwrap(), std::slice::from_ref(&initial)).unwrap();
    let reporter = RecordingProgress {
        messages: Mutex::new(Vec::new()),
        increments: Mutex::new(Vec::new()),
        total: Mutex::new(None),
        message_times: Mutex::new(Vec::new()),
        input_summary: Mutex::new(None),
        extraction_summary: Mutex::new(None),
        events: Mutex::new(Vec::new()),
    };

    add_archive_with_encoding_progress(
        archive.to_str().unwrap(),
        std::slice::from_ref(&added),
        EncodingOptions {
            block_size: 64 * 1024,
            compression_level: 1,
        },
        Some(&reporter),
        false,
    )
    .unwrap();

    assert_eq!(*reporter.total.lock().unwrap(), Some(bytes.len() as u64));
    assert!(
        reporter
            .messages
            .lock()
            .unwrap()
            .iter()
            .any(|message| message.starts_with("Adding "))
    );
    assert_eq!(
        reporter.increments.lock().unwrap().iter().sum::<u64>(),
        bytes.len() as u64
    );
    let times = reporter.message_times.lock().unwrap();
    assert!(
        times
            .windows(2)
            .all(|pair| pair[1].duration_since(pair[0]) >= std::time::Duration::from_millis(200))
    );
}

#[test]
fn progress_flags_are_available_for_archive_operations() {
    let commands: &[&[&str]] = &[
        &["create", "archive.zar", "input.txt"],
        &["add", "archive.zar", "input.txt"],
        &["remove", "archive.zar", "input.txt"],
        &[
            "join",
            "archive.zar",
            "--from",
            "00000001",
            "--through",
            "00000001",
        ],
        &["check", "archive.zar"],
        &["extract", "archive.zar"],
    ];

    for command in commands {
        for flag in ["-p", "--progress"] {
            let args = std::iter::once("zarc")
                .chain(command.iter().copied())
                .chain(std::iter::once(flag));
            assert!(Cli::try_parse_from(args).is_ok(), "{command:?} {flag}");
        }
    }
}

#[test]
fn commit_ids_use_eight_hex_digits_and_little_endian_counters() {
    assert_eq!(parse_commit_id("00000001").unwrap(), 1_u32.to_le_bytes());
    assert_eq!(encode_hex(&2_u32.to_le_bytes()), "02000000");
    assert!(parse_commit_id("0000001").is_err());
    assert!(parse_commit_id("0000000g").is_err());
}

#[test]
fn encoding_options_parse_and_enforce_limits() {
    assert_eq!(parse_block_size("64K").unwrap(), MIN_CREATE_BLOCK_SIZE);
    assert_eq!(parse_block_size("2M").unwrap(), 2 * 1024 * 1024);
    assert_eq!(parse_block_size("32M").unwrap(), MAX_CREATE_BLOCK_SIZE);
    assert_eq!(parse_block_size("auto").unwrap(), 0);
    let automatic = select_automatic_encoding(64 * 1024 * 1024, 1);
    assert_eq!(automatic.block_size, 6 * AUTO_MIN_BLOCK_SIZE);
    assert_eq!(automatic.workers, 1);
    assert!(!automatic.warning);
    let reduced_workers = select_automatic_encoding(64 * 1024 * 1024, 8);
    assert_eq!(reduced_workers.workers, 5);
    assert_eq!(reduced_workers.block_size, AUTO_MIN_BLOCK_SIZE);
    assert!(reduced_workers.estimated_bytes <= reduced_workers.budget_bytes);
    let low_memory = select_automatic_encoding(1024, 8);
    assert_eq!(low_memory.block_size, AUTO_MIN_BLOCK_SIZE);
    assert_eq!(low_memory.workers, 1);
    assert!(low_memory.warning);
    assert!(parse_block_size("0").is_err());
    assert!(parse_block_size("1").is_err());
    assert!(parse_block_size("63K").is_err());
    assert!(parse_block_size("33M").is_err());
    assert!(parse_block_size("2KiB").is_err());
    assert_eq!(parse_compression_level("-7").unwrap(), -7);
    assert_eq!(parse_compression_level("22").unwrap(), 22);
    assert!(parse_compression_level("-8").is_err());
    assert!(parse_compression_level("23").is_err());
}

#[test]
fn create_and_add_honor_block_size_and_compression_level() {
    let directory = TestDirectory::new();
    let first_source = directory.path().join("first.txt");
    std::fs::write(&first_source, b"abcdefghij").unwrap();
    let archive = directory.path().join("options.zar");

    create_archive_with_encoding(
        archive.to_str().unwrap(),
        std::slice::from_ref(&first_source),
        false,
        EncodingOptions {
            block_size: 3,
            compression_level: 1,
        },
    )
    .unwrap();

    let second_source = directory.path().join("second.txt");
    std::fs::write(&second_source, b"klmnop").unwrap();
    add_archive_with_encoding(
        archive.to_str().unwrap(),
        std::slice::from_ref(&second_source),
        EncodingOptions {
            block_size: 2,
            compression_level: 5,
        },
    )
    .unwrap();

    let mut archive_reader = File::open(&archive).unwrap();
    let first_header_offset = read_first_header_offset(&mut archive_reader);
    let (first_ch, _first_data_start, second_header_offset) =
        read_commit_at(&mut archive_reader, first_header_offset);
    assert_eq!(first_ch.commit_id, 1_u32.to_le_bytes());
    let first_commit = ValidatedCommit::read_body(
        &mut archive_reader,
        first_ch,
        CommitValidationLimits::default(),
    )
    .unwrap();
    let (second_ch, _second_data_start, _) =
        read_commit_at(&mut archive_reader, second_header_offset);
    assert_eq!(second_ch.commit_id, 2_u32.to_le_bytes());
    let second_commit = ValidatedCommit::read_body(
        &mut archive_reader,
        second_ch,
        CommitValidationLimits::default(),
    )
    .unwrap();
    assert_eq!(first_commit.block_indices.len(), 4);
    assert_eq!(second_commit.block_indices.len(), 3);
    assert_eq!(first_commit.logical_header.unwrap().block_size, Some(3));
    assert_eq!(second_commit.logical_header.unwrap().block_size, Some(2));

    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();
    extract_archive(archive.to_str().unwrap(), &destination, false).unwrap();
    assert_eq!(
        std::fs::read(destination.join("first.txt")).unwrap(),
        b"abcdefghij"
    );
    assert_eq!(
        std::fs::read(destination.join("second.txt")).unwrap(),
        b"klmnop"
    );
}

#[test]
fn encrypted_creation_authenticates_with_password_and_rejects_wrong_password() {
    let directory = TestDirectory::new();
    let source = directory.path().join("secret.txt");
    std::fs::write(&source, b"encrypted archive contents").unwrap();
    let archive = directory.path().join("encrypted.zar");

    create_archive_with_password(
        archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        false,
        EncodingOptions {
            block_size: 8,
            compression_level: 1,
        },
        Some(b"correct horse battery staple"),
    )
    .unwrap();

    let mut input = File::open(&archive).unwrap();
    let first_header_offset = read_first_header_offset(&mut input);
    let (commit_header, _, _) = read_commit_at(&mut input, first_header_offset);
    let unkeyed = ValidatedCommit::read_body(
        &mut input,
        commit_header.clone(),
        CommitValidationLimits::default(),
    )
    .unwrap();
    assert!(unkeyed.logical_header.is_none());
    assert!(unkeyed.encryption.is_header_encrypted());
    assert!(unkeyed.encryption.is_data_encrypted());

    let dek = match &unkeyed.encryption {
        EncryptionDescriptor::Password { key_wrap, .. } => {
            let dek = unwrap_dek_with_password(b"correct horse battery staple", key_wrap).unwrap();
            assert!(unwrap_dek_with_password(b"wrong password", key_wrap).is_err());
            dek
        }
        descriptor => panic!("expected password encryption descriptor, got {descriptor:?}"),
    };

    let mut keyed_input = File::open(&archive).unwrap();
    let keyed_header_offset = read_first_header_offset(&mut keyed_input);
    let (keyed_ch, _, _) = read_commit_at(&mut keyed_input, keyed_header_offset);
    let validated = ValidatedCommit::read_body_with_key(
        &mut keyed_input,
        keyed_ch.clone(),
        CommitValidationLimits::default(),
        &dek,
    )
    .unwrap();
    assert!(validated.logical_header.is_some());

    let mut wrong_key_input = File::open(&archive).unwrap();
    let wrong_header_offset = read_first_header_offset(&mut wrong_key_input);
    let (wrong_ch, _, _) = read_commit_at(&mut wrong_key_input, wrong_header_offset);
    let mut wrong_dek = dek;
    wrong_dek[0] ^= 1;
    assert!(
        ValidatedCommit::read_body_with_key(
            &mut wrong_key_input,
            wrong_ch,
            CommitValidationLimits::default(),
            &wrong_dek,
        )
        .is_err()
    );
}

#[test]
fn encrypted_read_flows_require_and_accept_passwords() {
    let directory = TestDirectory::new();
    let source = directory.path().join("secret.txt");
    std::fs::write(&source, b"authenticated read flows").unwrap();
    let archive = directory.path().join("encrypted-reads.zar");
    create_archive_with_password(
        archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        false,
        EncodingOptions {
            block_size: 8,
            compression_level: 1,
        },
        Some(b"correct password"),
    )
    .unwrap();

    let archive_path = archive.to_str().unwrap();
    assert!(check_archive_with_recovery(archive_path, false, None).is_ok());
    assert!(check_archive_with_recovery(archive_path, false, Some(b"wrong password")).is_err());
    assert!(check_archive_with_recovery(archive_path, false, Some(b"correct password")).is_ok());

    assert!(load_toc_through(archive_path, None, Some(b"wrong password")).is_err());
    let toc = load_toc_through(archive_path, None, Some(b"correct password")).unwrap();
    assert!(toc.get(&ArchivePath::new("secret.txt").unwrap()).is_some());

    let rejected_destination = directory.path().join("rejected");
    std::fs::create_dir(&rejected_destination).unwrap();
    assert!(
        extract_archive_with_options(
            archive_path,
            &rejected_destination,
            false,
            None,
            Some(b"wrong password"),
            CollisionPolicy::Error,
            ExistingPolicy::Error,
        )
        .is_err()
    );
    assert!(
        std::fs::read_dir(&rejected_destination)
            .unwrap()
            .next()
            .is_none()
    );

    let destination = directory.path().join("accepted");
    std::fs::create_dir(&destination).unwrap();
    extract_archive_with_options(
        archive_path,
        &destination,
        false,
        None,
        Some(b"correct password"),
        CollisionPolicy::Error,
        ExistingPolicy::Error,
    )
    .unwrap();
    assert_eq!(
        std::fs::read(destination.join("secret.txt")).unwrap(),
        b"authenticated read flows"
    );
}

#[test]
fn encrypted_commits_support_scope_and_commit_specific_passwords() {
    let directory = TestDirectory::new();
    let first_input = directory.path().join("first.txt");
    let second_input = directory.path().join("second.txt");
    std::fs::write(&first_input, b"first encrypted commit").unwrap();
    std::fs::write(&second_input, b"second encrypted commit").unwrap();
    let archive = directory.path().join("mixed-encryption.zar");
    let archive_name = archive.to_str().unwrap();
    let options = EncodingOptions {
        block_size: 64 * 1024,
        compression_level: 1,
    };
    let filters = FilterSet::default();
    let create_options = CreateFilterOptions {
        input: CreateInputOptions::default(),
        filters: &filters,
    };

    create_archive_with_password_progress_and_filters_and_encryption(
        archive_name,
        std::slice::from_ref(&first_input),
        false,
        options,
        Some((
            b"first-password",
            ContentAead::Aes256Gcm,
            EncryptionScope::Data,
        )),
        None,
        create_options,
    )
    .unwrap();
    let first_info = info_archive(archive_name, false).unwrap();
    assert_eq!(first_info.len(), 1);
    assert!(!first_info[0].header_encrypted);
    assert!(first_info[0].uncompressed_size.is_some());
    assert!(extract_archive_to_memory(archive_name, None, None, None, 0).is_err());

    let mut existing_passwords = PasswordSet::new(Some(b"second-password".to_vec()));
    existing_passwords.insert_for_commit(*b"\x01\0\0\0", b"first-password".to_vec());
    add_archive_with_encoding_progress_and_filters_and_encryption(
        archive_name,
        std::slice::from_ref(&second_input),
        options,
        None,
        AddInputOptions::default(),
        &filters,
        Some((
            b"second-password",
            ContentAead::ChaCha20Poly1305,
            EncryptionScope::HeaderAndData,
        )),
        &existing_passwords,
    )
    .unwrap();

    let mut passwords = PasswordSet::default();
    passwords.insert_for_commit(*b"\x01\0\0\0", b"first-password".to_vec());
    passwords.insert_for_commit(*b"\x02\0\0\0", b"second-password".to_vec());
    assert!(list_archive_with_passwords(archive_name, None, &passwords).is_ok());
    assert!(check_archive_with_recovery_and_passwords(archive_name, false, &passwords).is_ok());
    let files =
        extract_archive_to_memory_with_passwords(archive_name, None, &passwords, None, 0).unwrap();
    assert_eq!(files["first.txt"], b"first encrypted commit");
    assert_eq!(files["second.txt"], b"second encrypted commit");

    let fallback = PasswordSet::new(Some(b"first-password".to_vec()));
    assert!(list_archive_with_passwords(archive_name, None, &fallback).is_err());
}

#[test]
fn exit_codes_distinguish_usage_io_container_and_strict_warnings() {
    let usage: Box<dyn std::error::Error> = "usage: zarc create <archive> <paths...>".into();
    let io_error: Box<dyn std::error::Error> = io::Error::other("disk failure").into();
    let framing_error: Box<dyn std::error::Error> = zarlib::FramingError::BadMagic.into();
    let archive_error: Box<dyn std::error::Error> = "archive validation failed".into();
    let strict_warning: Box<dyn std::error::Error> = StrictModeWarning.into();

    assert_eq!(classify_exit_code(usage.as_ref()), 2);
    assert_eq!(classify_exit_code(io_error.as_ref()), 3);
    assert_eq!(classify_exit_code(framing_error.as_ref()), 4);
    assert_eq!(classify_exit_code(archive_error.as_ref()), 4);
    assert_eq!(classify_exit_code(strict_warning.as_ref()), 6);
}

impl TestDirectory {
    fn new() -> Self {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("zarc-test-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn archive_path(value: &str) -> ArchivePath {
    ArchivePath::try_from(value).unwrap()
}

/// Reads the first_header_offset from the container header.
fn read_first_header_offset<R: std::io::Read + std::io::Seek>(file: &mut R) -> u64 {
    file.seek(std::io::SeekFrom::Start(4)).unwrap();
    let mut bytes = [0u8; 8];
    file.read_exact(&mut bytes).unwrap();
    u64::from_le_bytes(bytes)
}

/// Reads a commit at the given header offset. Returns (commit_header, data_start, next_header_offset).
/// Leaves the reader positioned at data_start.
fn read_commit_at<R: std::io::Read + std::io::Seek>(
    file: &mut R,
    header_offset: u64,
) -> (zarlib::CommitHeader, u64, u64) {
    file.seek(std::io::SeekFrom::Start(header_offset)).unwrap();
    let header_frame = HeaderFrame::read_from(
        file,
        CommitValidationLimits::default().max_frame_payload_size,
    )
    .unwrap();
    let commit_header = header_frame.commit_header.clone();
    let data_start = header_offset - commit_header.data_section_size;
    let next_offset_pos = data_start - zarlib::HEADER_OFFSET_SIZE;
    file.seek(std::io::SeekFrom::Start(next_offset_pos))
        .unwrap();
    let mut bytes = [0u8; 8];
    file.read_exact(&mut bytes).unwrap();
    let next_header_offset = u64::from_le_bytes(bytes);
    file.seek(std::io::SeekFrom::Start(data_start)).unwrap();
    (commit_header, data_start, next_header_offset)
}

fn make_archive(commits: Vec<TestCommit>) -> Vec<u8> {
    let mut archive = Vec::new();
    // Container header placeholder: magic + first_header_offset (filled later).
    write_container_magic(&mut archive).unwrap();
    archive.extend_from_slice(&0u64.to_le_bytes());
    let mut header_offsets: Vec<(usize, u64)> = Vec::new();
    for (commit_index, (changes, blocks)) in commits.into_iter().enumerate() {
        let logical_header = LogicalHeader::new(changes).encode().unwrap();
        let mut data_bytes = Vec::new();
        for (index, contents) in blocks {
            BlockFrame::from_uncompressed(index, &contents, 1)
                .unwrap()
                .write_to(&mut data_bytes)
                .unwrap();
        }
        let data_section_size = data_bytes.len() as u64;
        let commit_id = (commit_index as u32 + 1).to_le_bytes();
        // Build a temporary commit header to estimate header_frame_size.
        let temp_ch = zarlib::CommitHeader::new(0, commit_id, data_section_size, 0).unwrap();
        let staged = HeaderFrame::from_uncompressed(
            temp_ch,
            &logical_header,
            UNENCRYPTED_DESCRIPTOR.to_vec(),
            1,
        )
        .unwrap();
        let header_frame_size = u32::try_from(staged.encoded_size().unwrap()).unwrap();
        let commit_header =
            zarlib::CommitHeader::new(0, commit_id, data_section_size, header_frame_size).unwrap();
        let header = HeaderFrame::from_uncompressed(
            commit_header,
            &logical_header,
            UNENCRYPTED_DESCRIPTOR.to_vec(),
            1,
        )
        .unwrap();
        let mut header_bytes = Vec::new();
        header.write_to(&mut header_bytes).unwrap();
        // next_header_offset placeholder (filled later).
        let next_offset_pos = archive.len();
        archive.extend_from_slice(&0u64.to_le_bytes());
        // Data blocks.
        archive.extend_from_slice(&data_bytes);
        // Header frame.
        let header_offset = archive.len() as u64;
        archive.extend_from_slice(&header_bytes);
        // this_header_offset.
        archive.extend_from_slice(&header_offset.to_le_bytes());
        header_offsets.push((next_offset_pos, header_offset));
    }
    // Backfill first_header_offset in container header.
    if let Some((_, first_offset)) = header_offsets.first() {
        archive[4..12].copy_from_slice(&first_offset.to_le_bytes());
    }
    // Backfill next_header_offset for each commit.
    for window in header_offsets.windows(2) {
        let (cur_pos, _) = window[0];
        let (_, next_offset) = window[1];
        archive[cur_pos..cur_pos + 8].copy_from_slice(&next_offset.to_le_bytes());
    }
    archive
}

fn write_archive(directory: &TestDirectory, bytes: &[u8]) -> PathBuf {
    let archive_path = directory.path().join("input.zar");
    std::fs::write(&archive_path, bytes).unwrap();
    archive_path
}

fn make_encrypted_empty_commit(commit_id: [u8; 4]) -> Vec<u8> {
    let descriptor = EncryptionDescriptor::Symmetric {
        scope: EncryptionScope::HeaderAndData,
        algorithm: ContentAead::Aes256Gcm,
        nonce: [8; 12],
    }
    .encode()
    .unwrap();
    // Create encrypted header frame with a temporary commit header, then compute the correct size.
    let temp_ch = zarlib::CommitHeader::new(0, commit_id, 0, 0).unwrap();
    let temp_header = HeaderFrame::from_uncompressed_encrypted(
        temp_ch,
        &[],
        descriptor.clone(),
        1,
        &[9; zarlib::DEK_SIZE],
    )
    .unwrap();
    let header_frame_size = u32::try_from(temp_header.encoded_size().unwrap()).unwrap();
    let commit_header = zarlib::CommitHeader::new(0, commit_id, 0, header_frame_size).unwrap();
    let header = HeaderFrame::from_uncompressed_encrypted(
        commit_header.clone(),
        &[],
        descriptor,
        1,
        &[9; zarlib::DEK_SIZE],
    )
    .unwrap();
    let mut header_bytes = Vec::new();
    header.write_to(&mut header_bytes).unwrap();
    let mut commit = Vec::new();
    // next_header_offset (0 = last commit).
    commit.extend_from_slice(&0u64.to_le_bytes());
    // Header frame.
    let header_offset = commit.len() as u64;
    commit.extend_from_slice(&header_bytes);
    // this_header_offset.
    commit.extend_from_slice(&header_offset.to_le_bytes());
    commit
}

fn make_data_encrypted_empty_commit(commit_id: [u8; 4]) -> Vec<u8> {
    let descriptor = EncryptionDescriptor::Symmetric {
        scope: EncryptionScope::Data,
        algorithm: ContentAead::Aes256Gcm,
        nonce: [7; 12],
    }
    .encode()
    .unwrap();
    let logical_header = LogicalHeader::default().encode().unwrap();
    let temp_ch = zarlib::CommitHeader::new(0, commit_id, 0, 0).unwrap();
    let staged =
        HeaderFrame::from_uncompressed(temp_ch, &logical_header, descriptor.clone(), 1).unwrap();
    let header_frame_size = u32::try_from(staged.encoded_size().unwrap()).unwrap();
    let commit_header = zarlib::CommitHeader::new(0, commit_id, 0, header_frame_size).unwrap();
    let header =
        HeaderFrame::from_uncompressed(commit_header, &logical_header, descriptor, 1).unwrap();
    let mut header_bytes = Vec::new();
    header.write_to(&mut header_bytes).unwrap();
    let mut commit = Vec::new();
    // next_header_offset (0 = last commit).
    commit.extend_from_slice(&0u64.to_le_bytes());
    // Header frame.
    let header_offset = commit.len() as u64;
    commit.extend_from_slice(&header_bytes);
    // this_header_offset.
    commit.extend_from_slice(&header_offset.to_le_bytes());
    commit
}

fn commit_id_at(path: &Path, index: usize) -> String {
    let commits = info_archive(path.to_str().unwrap(), false).unwrap();
    format!("{:08x}", u32::from_le_bytes(commits[index].id))
}

#[test]
fn mutations_reject_encrypted_archives_without_changing_them() {
    let directory = TestDirectory::new();
    let mut original = zarlib::CONTAINER_MAGIC.to_vec();
    original.extend_from_slice(&0u64.to_le_bytes()); // first_header_offset placeholder
    let encrypted_commit = make_encrypted_empty_commit([8; 4]);
    let header_offset = original.len() as u64;
    original.extend_from_slice(&encrypted_commit);
    // Backfill first_header_offset.
    original[4..12].copy_from_slice(&header_offset.to_le_bytes());
    let archive = write_archive(&directory, &original);
    let path = archive.to_str().unwrap();

    assert!(
        add_archive_with_encoding(
            path,
            &[PathBuf::from("missing.txt")],
            EncodingOptions::default(),
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&archive).unwrap(), original);

    assert!(remove_archive(path, &[PathBuf::from("missing.txt")]).is_err());
    assert_eq!(std::fs::read(&archive).unwrap(), original);

    assert!(
        join_archive_with_encoding(
            path,
            &encode_hex(&[8; 4]),
            &encode_hex(&[8; 4]),
            EncodingOptions::default(),
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&archive).unwrap(), original);
}

#[test]
fn join_merges_contiguous_commits_and_preserves_following_overrides() {
    let directory = TestDirectory::new();
    let original = make_archive(vec![
        (
            vec![TocChange::PutFile {
                path: archive_path("before.txt"),
                entry: FileEntry::new(3, vec![0]).unwrap(),
            }],
            vec![(0, b"one".to_vec())],
        ),
        (
            vec![
                TocChange::PutFile {
                    path: archive_path("stable.txt"),
                    entry: FileEntry::new(5, vec![0]).unwrap(),
                },
                TocChange::PutFile {
                    path: archive_path("changed.txt"),
                    entry: FileEntry::new(3, vec![1]).unwrap(),
                },
            ],
            vec![(0, b"first".to_vec()), (1, b"old".to_vec())],
        ),
        (
            vec![
                TocChange::RemoveFile {
                    path: archive_path("changed.txt"),
                },
                TocChange::PutFile {
                    path: archive_path("changed.txt"),
                    entry: FileEntry::new(65_537, vec![0]).unwrap(),
                },
            ],
            vec![(0, vec![b'n'; 65_537])],
        ),
        (
            vec![TocChange::PutFile {
                path: archive_path("after.txt"),
                entry: FileEntry::new(3, vec![0]).unwrap(),
            }],
            vec![(0, b"end".to_vec())],
        ),
    ]);
    let archive = write_archive(&directory, &original);
    let prefix_id = commit_id_at(&archive, 0);
    let from = commit_id_at(&archive, 1);
    let through = commit_id_at(&archive, 2);
    let suffix_id = commit_id_at(&archive, 3);

    run([
        "join".to_owned(),
        archive.to_str().unwrap().to_owned(),
        "--from".to_owned(),
        from,
        "--through".to_owned(),
        through,
        "--block-size".to_owned(),
        "64K".to_owned(),
        "--compression-level".to_owned(),
        "7".to_owned(),
    ]
    .into_iter())
    .unwrap();

    let mut validation = File::open(&archive).unwrap();
    let mut header_offset = read_first_header_offset(&mut validation);
    let mut ids = Vec::new();
    let mut commits = Vec::new();
    while header_offset != 0 {
        let (commit_header, _, next_header_offset) = read_commit_at(&mut validation, header_offset);
        ids.push(format!(
            "{:08x}",
            u32::from_le_bytes(commit_header.commit_id)
        ));
        commits.push(
            ValidatedCommit::read_body(
                &mut validation,
                commit_header,
                CommitValidationLimits::default(),
            )
            .unwrap(),
        );
        header_offset = next_header_offset;
    }
    assert_eq!(ids.len(), 3);
    assert_eq!(ids[0], prefix_id);
    assert_eq!(ids[2], suffix_id);
    assert_eq!(commits[1].block_indices.len(), 3);

    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();
    extract_archive(archive.to_str().unwrap(), &destination, false).unwrap();
    assert_eq!(
        std::fs::read(destination.join("before.txt")).unwrap(),
        b"one"
    );
    assert_eq!(
        std::fs::read(destination.join("stable.txt")).unwrap(),
        b"first"
    );
    assert_eq!(
        std::fs::read(destination.join("changed.txt")).unwrap(),
        vec![b'n'; 65_537]
    );
    assert_eq!(
        std::fs::read(destination.join("after.txt")).unwrap(),
        b"end"
    );
}

#[test]
fn join_rejects_invalid_ranges_and_preserves_the_original_archive() {
    let directory = TestDirectory::new();
    let original = make_archive(vec![
        (
            vec![TocChange::PutFile {
                path: archive_path("one.txt"),
                entry: FileEntry::new(1, vec![0]).unwrap(),
            }],
            vec![(0, b"1".to_vec())],
        ),
        (
            vec![TocChange::PutFile {
                path: archive_path("two.txt"),
                entry: FileEntry::new(1, vec![0]).unwrap(),
            }],
            vec![(0, b"2".to_vec())],
        ),
        (
            vec![TocChange::PutFile {
                path: archive_path("three.txt"),
                entry: FileEntry::new(1, vec![0]).unwrap(),
            }],
            vec![(0, b"3".to_vec())],
        ),
    ]);
    let archive = write_archive(&directory, &original);
    let first = commit_id_at(&archive, 0);
    let third = commit_id_at(&archive, 2);
    let default_options = EncodingOptions::default();
    assert!(
        join_archive_with_encoding(archive.to_str().unwrap(), &third, &first, default_options)
            .is_err()
    );
    assert!(
        join_archive_with_encoding(
            archive.to_str().unwrap(),
            &first,
            "missing",
            default_options
        )
        .is_err()
    );
    assert!(
        join_archive_with_encoding(
            archive.to_str().unwrap(),
            &first,
            "not-a-commit-id",
            default_options
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&archive).unwrap(), original);
}

#[test]
fn dedup_candidates_are_searched_only_within_the_same_file_size() {
    let candidates = HashMap::from([
        (
            4,
            vec![DedupCandidate {
                checksum: Some(17),
                tiny_contents: Vec::new(),
                block_indices: vec![3],
            }],
        ),
        (
            5,
            vec![DedupCandidate {
                checksum: Some(17),
                tiny_contents: Vec::new(),
                block_indices: vec![8],
            }],
        ),
    ]);

    assert_eq!(
        duplicate_blocks(&candidates, 4, Some(17), &[]),
        Some(vec![3])
    );
    assert_eq!(
        duplicate_blocks(&candidates, 5, Some(17), &[]),
        Some(vec![8])
    );
    assert_eq!(duplicate_blocks(&candidates, 6, Some(17), &[]), None);
}

#[test]
fn create_deduplicates_equal_files_but_never_compares_different_sizes() {
    let directory = TestDirectory::new();
    let first = directory.path().join("first.bin");
    let same = directory.path().join("same.bin");
    let different_contents = directory.path().join("different-contents.bin");
    std::fs::write(&first, b"duplicate payload").unwrap();
    std::fs::write(&same, b"duplicate payload").unwrap();
    std::fs::write(&different_contents, b"different content").unwrap();
    let archive = directory.path().join("deduplicated.zar");

    create_archive_with_encoding(
        archive.to_str().unwrap(),
        &[first, same, different_contents],
        false,
        EncodingOptions {
            block_size: 64 * 1024,
            compression_level: 1,
        },
    )
    .unwrap();

    let mut input = File::open(&archive).unwrap();
    let first_header_offset = read_first_header_offset(&mut input);
    let (commit_header, _, _) = read_commit_at(&mut input, first_header_offset);
    let commit =
        ValidatedCommit::read_body(&mut input, commit_header, CommitValidationLimits::default())
            .unwrap();
    let entries = commit
        .logical_header
        .unwrap()
        .changes
        .into_iter()
        .filter_map(|change| match change {
            TocChange::PutFile { path, entry } => Some((path.to_string(), entry)),
            _ => None,
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let first_entry = entries.get("first.bin").unwrap();
    let same_entry = entries.get("same.bin").unwrap();
    let different_entry = entries.get("different-contents.bin").unwrap();

    assert_eq!(first_entry.block_indices, same_entry.block_indices);
    assert_ne!(first_entry.block_indices, different_entry.block_indices);
    assert_eq!(commit.block_count, 2);
}

#[test]
fn create_and_add_skip_compression_for_known_extensions_case_insensitively() {
    let directory = TestDirectory::new();
    let compressed = directory.path().join("archive.rar");
    let ordinary = directory.path().join("archive.txt");
    std::fs::write(&compressed, vec![b'x'; 4096]).unwrap();
    std::fs::write(&ordinary, vec![b'y'; 4096]).unwrap();
    let archive = directory.path().join("skip-compression.zar");

    create_archive_with_encoding(
        archive.to_str().unwrap(),
        &[compressed, ordinary],
        false,
        EncodingOptions {
            block_size: 64 * 1024,
            compression_level: 3,
        },
    )
    .unwrap();

    let mut input = File::open(&archive).unwrap();
    let first_header_offset = read_first_header_offset(&mut input);
    let (commit_header, _, _) = read_commit_at(&mut input, first_header_offset);
    let commit = ValidatedCommit::read_body(
        &mut input,
        commit_header.clone(),
        CommitValidationLimits::default(),
    )
    .unwrap();
    let entries = commit
        .logical_header
        .unwrap()
        .changes
        .into_iter()
        .filter_map(|change| match change {
            TocChange::PutFile { path, entry } => Some((path.to_string(), entry)),
            _ => None,
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let data_start = zarlib::CONTAINER_HEADER_SIZE + zarlib::HEADER_OFFSET_SIZE;
    input.seek(std::io::SeekFrom::Start(data_start)).unwrap();
    let mut flags_by_index = HashMap::new();
    let mut remaining = commit_header.data_section_size;
    while remaining > 0 {
        let before = input.stream_position().unwrap();
        let frame = BlockFrame::read_from(&mut input, 64 * 1024).unwrap();
        remaining -= input.stream_position().unwrap() - before;
        flags_by_index.insert(frame.block_index, frame.flags);
    }

    let compressed_entry = entries.get("archive.rar").unwrap();
    let ordinary_entry = entries.get("archive.txt").unwrap();
    assert_eq!(compressed_entry.block_indices.len(), 1);
    assert_eq!(ordinary_entry.block_indices.len(), 1);
    assert_ne!(
        flags_by_index[&compressed_entry.block_indices[0]] & zarlib::BLOCK_FLAG_UNCOMPRESSED,
        0
    );
    assert_eq!(
        flags_by_index[&ordinary_entry.block_indices[0]] & zarlib::BLOCK_FLAG_UNCOMPRESSED,
        0
    );

    let appended = directory.path().join("added.rar");
    let appended_payload = vec![b'z'; 4096];
    std::fs::write(&appended, &appended_payload).unwrap();
    add_archive_with_encoding(
        archive.to_str().unwrap(),
        std::slice::from_ref(&appended),
        EncodingOptions {
            block_size: 64 * 1024,
            compression_level: 3,
        },
    )
    .unwrap();
    let commits = info_archive(archive.to_str().unwrap(), false).unwrap();
    assert_eq!(commits.len(), 2);
    let mut input = File::open(&archive).unwrap();
    let first_header_offset = read_first_header_offset(&mut input);
    let (first_ch, _, second_header_offset) = read_commit_at(&mut input, first_header_offset);
    ValidatedCommit::read_body(&mut input, first_ch, CommitValidationLimits::default()).unwrap();
    let (second_ch, _, _) = read_commit_at(&mut input, second_header_offset);
    let second =
        ValidatedCommit::read_body(&mut input, second_ch, CommitValidationLimits::default())
            .unwrap();
    let appended_entry = second
        .logical_header
        .unwrap()
        .changes
        .into_iter()
        .find_map(|change| match change {
            TocChange::PutFile { path, entry } if path.as_str() == "added.rar" => Some(entry),
            _ => None,
        })
        .unwrap();
    let mut input = File::open(&archive).unwrap();
    let first_header_offset = read_first_header_offset(&mut input);
    let (_first_ch, _first_data_start, second_header_offset) =
        read_commit_at(&mut input, first_header_offset);
    // Seek to the second commit's data section.
    let (_second_ch, second_data_start, _) = read_commit_at(&mut input, second_header_offset);
    input
        .seek(std::io::SeekFrom::Start(second_data_start))
        .unwrap();
    let frame = BlockFrame::read_from(&mut input, 64 * 1024).unwrap();
    assert_eq!(frame.block_index, appended_entry.block_indices[0]);
    assert_ne!(frame.flags & zarlib::BLOCK_FLAG_UNCOMPRESSED, 0);
}

#[test]
fn create_stores_incompressible_blocks_uncompressed_and_compressible_blocks_compressed() {
    let directory = TestDirectory::new();
    let incompressible = directory.path().join("raw.bin");
    let compressible = directory.path().join("repeat.bin");
    let mut state = 0x9e37_79b9_u32;
    let raw = (0..4096)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect::<Vec<_>>();
    std::fs::write(&incompressible, raw).unwrap();
    std::fs::write(&compressible, vec![b'x'; 4096]).unwrap();
    let archive = directory.path().join("block-fallback.zar");
    create_archive_with_encoding(
        archive.to_str().unwrap(),
        &[incompressible, compressible],
        false,
        EncodingOptions {
            block_size: 64 * 1024,
            compression_level: 3,
        },
    )
    .unwrap();

    let mut input = File::open(&archive).unwrap();
    let first_header_offset = read_first_header_offset(&mut input);
    let (ch, data_start, _) = read_commit_at(&mut input, first_header_offset);
    let _commit =
        ValidatedCommit::read_body(&mut input, ch.clone(), CommitValidationLimits::default())
            .unwrap();
    input.seek(std::io::SeekFrom::Start(data_start)).unwrap();
    let mut raw_flags = None;
    let mut compressed_flags = None;
    let mut remaining = ch.data_section_size;
    while remaining > 0 {
        let before = input.stream_position().unwrap();
        let block = BlockFrame::read_from(&mut input, 64 * 1024).unwrap();
        let consumed = input.stream_position().unwrap() - before;
        remaining -= consumed;
        let content = block.uncompressed_at_most(64 * 1024).unwrap();
        if content.iter().all(|byte| *byte == b'x') {
            compressed_flags = Some(block.flags);
            assert!(block.payload.len() < content.len());
        } else {
            raw_flags = Some(block.flags);
            assert_ne!(block.flags & zarlib::BLOCK_FLAG_UNCOMPRESSED, 0);
            assert_eq!(block.payload.len(), content.len());
        }
    }
    assert_ne!(raw_flags.unwrap() & zarlib::BLOCK_FLAG_UNCOMPRESSED, 0);
    assert_eq!(
        compressed_flags.unwrap() & zarlib::BLOCK_FLAG_UNCOMPRESSED,
        0
    );
}

#[test]
fn create_omits_file_checksums_for_tiny_files_only() {
    let directory = TestDirectory::new();
    let tiny = directory.path().join("tiny.bin");
    let checksummed = directory.path().join("checksummed.bin");
    std::fs::write(&tiny, b"12345678").unwrap();
    std::fs::write(&checksummed, b"123456789").unwrap();
    let archive = directory.path().join("tiny-checksums.zar");

    create_archive_with_encoding(
        archive.to_str().unwrap(),
        &[tiny, checksummed],
        false,
        EncodingOptions {
            block_size: 64 * 1024,
            compression_level: 1,
        },
    )
    .unwrap();

    let mut input = File::open(&archive).unwrap();
    let first_header_offset = read_first_header_offset(&mut input);
    let (ch, _, _) = read_commit_at(&mut input, first_header_offset);
    let commit =
        ValidatedCommit::read_body(&mut input, ch, CommitValidationLimits::default()).unwrap();
    let changes = commit.logical_header.unwrap().changes;
    let checksums = changes
        .into_iter()
        .filter_map(|change| match change {
            TocChange::PutFile { path, entry } => Some((path.to_string(), entry.checksum)),
            _ => None,
        })
        .collect::<std::collections::BTreeMap<_, _>>();

    assert_eq!(checksums.get("tiny.bin"), Some(&None));
    assert!(
        checksums
            .get("checksummed.bin")
            .is_some_and(Option::is_some)
    );
}

#[cfg(unix)]
#[test]
fn add_metadata_option_saves_metadata_for_appended_entries() {
    let directory = TestDirectory::new();
    let initial = directory.path().join("initial.txt");
    let added = directory.path().join("added.txt");
    std::fs::write(&initial, b"initial").unwrap();
    std::fs::write(&added, b"added").unwrap();
    let archive = directory.path().join("add-metadata.zar");
    create_archive(archive.to_str().unwrap(), std::slice::from_ref(&initial)).unwrap();

    add_archive_with_encoding_progress(
        archive.to_str().unwrap(),
        std::slice::from_ref(&added),
        EncodingOptions {
            block_size: 64 * 1024,
            compression_level: 1,
        },
        None,
        true,
    )
    .unwrap();

    let mut input = File::open(&archive).unwrap();
    read_container_magic(&mut input).unwrap();
    let first_offset = read_first_header_offset(&mut input);
    let (first_header, first_data_start, next_offset) = read_commit_at(&mut input, first_offset);
    input
        .seek(std::io::SeekFrom::Start(first_data_start))
        .unwrap();
    let _first =
        ValidatedCommit::read_body(&mut input, first_header, CommitValidationLimits::default())
            .unwrap();
    let (second_header, second_data_start, _) = read_commit_at(&mut input, next_offset);
    input
        .seek(std::io::SeekFrom::Start(second_data_start))
        .unwrap();
    let second =
        ValidatedCommit::read_body(&mut input, second_header, CommitValidationLimits::default())
            .unwrap();
    assert_eq!(second.logical_header.unwrap().entry_metadata.len(), 1);
}

#[cfg(unix)]
#[test]
fn metadata_option_saves_and_restores_file_and_directory_metadata() {
    use std::os::unix::fs::PermissionsExt;

    let directory = TestDirectory::new();
    let source = directory.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("file.txt"), b"metadata test").unwrap();
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o751)).unwrap();
    std::fs::set_permissions(
        source.join("file.txt"),
        std::fs::Permissions::from_mode(0o640),
    )
    .unwrap();
    let archive = directory.path().join("metadata.zar");
    let result = create_archive_with_password_progress(
        archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        false,
        EncodingOptions {
            block_size: 64 * 1024,
            compression_level: 1,
        },
        None,
        None,
        true,
    );
    result.unwrap();

    let mut input = File::open(&archive).unwrap();
    let first_offset = read_first_header_offset(&mut input);
    let (commit_header, data_start, _) = read_commit_at(&mut input, first_offset);
    input.seek(std::io::SeekFrom::Start(data_start)).unwrap();
    let commit =
        ValidatedCommit::read_body(&mut input, commit_header, CommitValidationLimits::default())
            .unwrap();
    let header = commit.logical_header.unwrap();
    assert_eq!(header.entry_metadata.len(), 2);
    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();
    extract_archive_with_progress(
        archive.to_str().unwrap(),
        &destination,
        ExtractionOptions {
            dry_run: false,
            through: None,
            collision_policy: CollisionPolicy::Error,
            existing_policy: ExistingPolicy::Error,
            restore_metadata: true,
            restore_symlinks: false,
            restore_devices: false,
            strip_top_level: false,
        },
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        std::fs::metadata(destination.join("source"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o751
    );
    assert_eq!(
        std::fs::metadata(destination.join("source/file.txt"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o640
    );
}

#[cfg(unix)]
#[test]
fn create_and_extract_symlink_named_pipe_and_device_records() {
    use std::os::unix::fs::FileTypeExt;

    let directory = TestDirectory::new();
    let source = directory.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("target.txt"), b"link target").unwrap();
    std::os::unix::fs::symlink("target.txt", source.join("link")).unwrap();
    let fifo = source.join("pipe");
    let fifo_name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) }, 0);
    let device = source.join("char-device");
    let device_name = std::ffi::CString::new(device.as_os_str().as_encoded_bytes()).unwrap();
    let device_number = libc::makedev(1, 7);
    let device_result = unsafe {
        #[allow(clippy::useless_conversion)]
        libc::mknod(
            device_name.as_ptr(),
            libc::S_IFCHR | 0o600,
            device_number.try_into().unwrap(),
        )
    };
    if device_result < 0 {
        std::fs::remove_file(&fifo).unwrap();
        std::fs::remove_file(source.join("link")).unwrap();
        return;
    }

    let archive = directory.path().join("special-files.zar");
    create_archive_with_password_progress(
        archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        false,
        EncodingOptions {
            block_size: 64 * 1024,
            compression_level: 1,
        },
        None,
        None,
        false,
    )
    .unwrap();
    let mut archive_file = File::open(&archive).unwrap();
    let first_offset = ContainerHeader::read_from(&mut archive_file)
        .unwrap()
        .first_header_offset;
    let (commit_header, _, _) = read_commit_at_offset(
        &mut archive_file,
        first_offset,
        CommitValidationLimits::default(),
    )
    .unwrap();
    let commit = ValidatedCommit::read_body(
        &mut archive_file,
        commit_header,
        CommitValidationLimits::default(),
    )
    .unwrap();
    let header = commit.logical_header.unwrap();
    assert!(
        header
            .changes
            .iter()
            .any(|change| matches!(change, TocChange::PutSymlink { .. }))
    );
    assert!(
        header
            .changes
            .iter()
            .any(|change| matches!(change, TocChange::PutNamedPipe { .. }))
    );
    assert!(
        header
            .changes
            .iter()
            .any(|change| matches!(change, TocChange::PutDevice { .. }))
    );
    let stats = info_archive(archive.to_str().unwrap(), true).unwrap();
    let details = stats[0].details.as_ref().unwrap();
    assert_eq!(
        (details.symlinks, details.named_pipes, details.devices),
        (1, 1, 1)
    );

    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();
    extract_archive_with_progress(
        archive.to_str().unwrap(),
        &destination,
        ExtractionOptions {
            dry_run: false,
            through: None,
            collision_policy: CollisionPolicy::Error,
            existing_policy: ExistingPolicy::Error,
            restore_metadata: false,
            restore_symlinks: true,
            restore_devices: true,
            strip_top_level: false,
        },
        None,
        None,
    )
    .unwrap();
    let escaped_destination = directory.path().join("escaped-output");
    std::fs::create_dir(&escaped_destination).unwrap();
    let escaping_archive = write_archive(
        &directory,
        &make_archive(vec![(
            vec![TocChange::PutSymlink {
                path: archive_path("escape"),
                target: "../../outside".to_owned(),
            }],
            vec![],
        )]),
    );
    assert!(
        extract_archive_with_progress(
            escaping_archive.to_str().unwrap(),
            &escaped_destination,
            ExtractionOptions {
                dry_run: false,
                through: None,
                collision_policy: CollisionPolicy::Error,
                existing_policy: ExistingPolicy::Error,
                restore_metadata: false,
                restore_symlinks: true,
                restore_devices: false,
                strip_top_level: false,
            },
            None,
            None,
        )
        .is_err()
    );
    assert!(!escaped_destination.join("escape").exists());
    assert!(
        std::fs::symlink_metadata(destination.join("source/link"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        std::fs::read_link(destination.join("source/link")).unwrap(),
        PathBuf::from("target.txt")
    );
    assert!(
        std::fs::symlink_metadata(destination.join("source/pipe"))
            .unwrap()
            .file_type()
            .is_fifo()
    );
    assert!(
        std::fs::symlink_metadata(destination.join("source/char-device"))
            .unwrap()
            .file_type()
            .is_char_device()
    );
}

#[test]
fn create_writes_a_readable_archive_from_files_and_directories() {
    let directory = TestDirectory::new();
    let source = directory.path().join("source");
    std::fs::create_dir_all(source.join("nested")).unwrap();
    std::fs::write(source.join("nested/file.txt"), b"hello world").unwrap();
    std::fs::write(source.join("empty.txt"), b"").unwrap();
    let archive = directory.path().join("created.zar");
    let relative_source = source
        .strip_prefix(std::env::current_dir().unwrap())
        .unwrap_or(&source);
    let source_arg = if relative_source == source {
        source.as_path()
    } else {
        relative_source
    };
    create_archive(archive.to_str().unwrap(), &[source_arg.to_path_buf()]).unwrap();

    let mut file = File::open(&archive).unwrap();
    let toc = load_toc(&mut file).unwrap();
    assert!(matches!(
        toc.get(&archive_path("source/nested")),
        Some(TocEntry::Directory)
    ));
    assert!(
        matches!(toc.get(&archive_path("source/nested/file.txt")), Some(TocEntry::File(entry)) if entry.size == 11)
    );
    assert!(
        matches!(toc.get(&archive_path("source/empty.txt")), Some(TocEntry::File(entry)) if entry.size == 0)
    );
    assert!(
        check_archive_with_recovery(archive.to_str().unwrap(), false, None)
            .map(|_| ())
            .is_ok()
    );

    let output = directory.path().join("output");
    std::fs::create_dir(&output).unwrap();
    extract_archive(archive.to_str().unwrap(), &output, false).unwrap();
    assert_eq!(
        std::fs::read(output.join("source/nested/file.txt")).unwrap(),
        b"hello world"
    );
}

#[test]
fn create_and_extract_apply_file_and_folder_filters() {
    let directory = TestDirectory::new();
    let source = directory.path().join("source");
    std::fs::create_dir_all(source.join("private")).unwrap();
    std::fs::create_dir_all(source.join("empty")).unwrap();
    std::fs::write(source.join("readme.txt"), b"read me").unwrap();
    std::fs::write(source.join("data.bin"), b"binary").unwrap();
    std::fs::write(source.join("private/secret.txt"), b"secret").unwrap();

    let archive = directory.path().join("filtered.zar");
    let create_filters = FilterSet::from_patterns(
        &[],
        &[],
        &[],
        &["source/private".to_owned()],
        &[] as &[&Path],
    )
    .unwrap();
    create_archive_with_password_progress_and_filters(
        archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        false,
        EncodingOptions::default(),
        None,
        None,
        CreateFilterOptions {
            input: CreateInputOptions::default(),
            filters: &create_filters,
        },
    )
    .unwrap();

    let mut archive_file = File::open(&archive).unwrap();
    let toc = load_toc(&mut archive_file).unwrap();
    assert!(toc.get(&archive_path("source/readme.txt")).is_some());
    assert!(toc.get(&archive_path("source/data.bin")).is_some());
    assert!(
        toc.get(&archive_path("source/private/secret.txt"))
            .is_none()
    );
    assert!(toc.get(&archive_path("source/empty")).is_some());

    let add_filters =
        FilterSet::from_patterns(&["*.bin".to_owned()], &[], &[], &[], &[] as &[&Path]).unwrap();
    add_archive_with_encoding_progress_and_options_and_filters(
        archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        EncodingOptions::default(),
        None,
        AddInputOptions::default(),
        &add_filters,
    )
    .unwrap();
    let mut archive_file = File::open(&archive).unwrap();
    let toc = load_toc(&mut archive_file).unwrap();
    assert!(toc.get(&archive_path("source/readme.txt")).is_some());
    assert!(toc.get(&archive_path("source/data.bin")).is_some());
    assert!(
        toc.get(&archive_path("source/private/secret.txt"))
            .is_none()
    );

    let extract_filters =
        FilterSet::from_patterns(&["*.txt".to_owned()], &[], &[], &[], &[] as &[&Path]).unwrap();
    let destination = directory.path().join("output");
    let plan = extract_archive_with_progress_and_filters(
        archive.to_str().unwrap(),
        &destination,
        ExtractionOptions {
            dry_run: false,
            through: None,
            collision_policy: CollisionPolicy::Error,
            existing_policy: ExistingPolicy::Error,
            restore_metadata: false,
            restore_symlinks: false,
            restore_devices: false,
            strip_top_level: false,
        },
        None,
        None,
        &extract_filters,
    )
    .unwrap();
    assert_eq!(plan.files.len(), 1);
    assert_eq!(plan.files[0].0, destination.join("source/readme.txt"));
    assert_eq!(
        std::fs::read(destination.join("source/readme.txt")).unwrap(),
        b"read me"
    );
    assert!(!destination.join("source/data.bin").exists());
}

#[test]
fn create_archives_symlinks_and_strict_mode_fails_on_them() {
    let directory = TestDirectory::new();
    let source = directory.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("file.txt"), b"kept").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(source.join("file.txt"), source.join("link.txt")).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(source.join("file.txt"), source.join("link.txt")).unwrap();

    let archive = directory.path().join("warning.zar");
    let report = create_archive_with_options(
        archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        false,
    )
    .unwrap();
    assert!(
        report
            .warnings
            .iter()
            .all(|warning| !warning.contains("W_SYMLINK_STORED"))
    );
    assert!(check_archive_with_recovery(archive.to_str().unwrap(), false, None).is_ok());
    let mut archive_file = File::open(&archive).unwrap();
    let first_header_offset = read_first_header_offset(&mut archive_file);
    let (ch, _, _) = read_commit_at_offset(
        &mut archive_file,
        first_header_offset,
        CommitValidationLimits::default(),
    )
    .unwrap();
    let commit =
        ValidatedCommit::read_body(&mut archive_file, ch, CommitValidationLimits::default())
            .unwrap();
    assert!(
        commit
            .logical_header
            .unwrap()
            .changes
            .iter()
            .any(|change| matches!(change, TocChange::PutSymlink { .. }))
    );

    let verbose_archive = directory.path().join("verbose.zar");
    let report = create_archive_with_password_progress_and_special_files(
        verbose_archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        false,
        EncodingOptions::default(),
        None,
        None,
        CreateInputOptions {
            verbose: true,
            ..CreateInputOptions::default()
        },
    )
    .unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("W_SYMLINK_STORED"))
    );

    let strict_archive = directory.path().join("strict.zar");
    let error = create_archive_with_options(
        strict_archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        true,
    )
    .unwrap_err();
    assert_eq!(classify_exit_code(error.as_ref()), 6);
    assert!(!strict_archive.exists());
}

#[cfg(unix)]
#[test]
fn create_skips_backslash_paths_with_warning_and_strict_mode_rejects_them() {
    let directory = TestDirectory::new();
    let source = directory.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("supported.txt"), b"kept").unwrap();
    let unsupported_name = std::ffi::OsString::from("unsupported\\name.txt");
    std::fs::write(source.join(unsupported_name), b"not archived").unwrap();

    let archive = directory.path().join("backslash.zar");
    let report = create_archive_with_options(
        archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        false,
    )
    .unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("W_INVALID_ARCHIVE_PATH"))
    );
    let toc = list_archive(archive.to_str().unwrap(), None, None).unwrap();
    assert!(
        toc.iter()
            .any(|(path, _)| path.as_str().ends_with("source/supported.txt"))
    );
    assert!(toc.iter().all(|(path, _)| !path.as_str().contains('\\')));

    let strict_archive = directory.path().join("backslash-strict.zar");
    let error = create_archive_with_options(
        strict_archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        true,
    )
    .unwrap_err();
    assert_eq!(classify_exit_code(error.as_ref()), 6);
    assert!(!strict_archive.exists());
}

#[cfg(unix)]
#[test]
fn create_includes_named_pipes_only_when_special_files_are_enabled() {
    let directory = TestDirectory::new();
    let source = directory.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("regular.txt"), b"regular").unwrap();
    let pipe = source.join("pipe");
    let pipe_name = std::ffi::CString::new(pipe.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(pipe_name.as_ptr(), 0o600) }, 0);

    let skipped_archive = directory.path().join("skipped-special.zar");
    let report = create_archive_with_password_progress_and_special_files(
        skipped_archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        false,
        EncodingOptions::default(),
        None,
        None,
        CreateInputOptions {
            save_metadata: false,
            save_dates: false,
            include_special_files: false,
            overwrite_existing: false,
            verbose: false,
        },
    )
    .unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("W_SPECIAL_FILE_SKIPPED"))
    );
    let skipped_toc = list_archive(skipped_archive.to_str().unwrap(), None, None).unwrap();
    assert!(
        skipped_toc
            .iter()
            .all(|(path, _)| !path.as_str().ends_with("source/pipe"))
    );

    let included_archive = directory.path().join("included-special.zar");
    create_archive_with_password_progress_and_special_files(
        included_archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        true,
        EncodingOptions::default(),
        None,
        None,
        CreateInputOptions {
            save_metadata: false,
            save_dates: false,
            include_special_files: true,
            overwrite_existing: false,
            verbose: false,
        },
    )
    .unwrap();
    let included_toc = list_archive(included_archive.to_str().unwrap(), None, None).unwrap();
    assert!(
        included_toc
            .iter()
            .any(|(path, entry)| path.as_str().ends_with("source/pipe")
                && matches!(entry, TocEntry::NamedPipe))
    );
}

#[test]
fn create_refuses_existing_archive_without_modifying_it() {
    let directory = TestDirectory::new();
    let source = directory.path().join("input.txt");
    std::fs::write(&source, b"data").unwrap();
    let archive = directory.path().join("already.zar");
    std::fs::write(&archive, b"keep").unwrap();
    assert!(create_archive(archive.to_str().unwrap(), &[source]).is_err());
    assert_eq!(std::fs::read(archive).unwrap(), b"keep");
}

#[test]
fn create_overwrite_replaces_existing_archive_only_after_successful_creation() {
    let directory = TestDirectory::new();
    let source = directory.path().join("input.txt");
    std::fs::write(&source, b"new archive contents").unwrap();
    let archive = directory.path().join("replace.zar");
    std::fs::write(&archive, b"old archive contents").unwrap();

    create_archive_with_password_progress_and_special_files(
        archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        false,
        EncodingOptions::default(),
        None,
        None,
        CreateInputOptions {
            save_metadata: false,
            save_dates: false,
            include_special_files: true,
            overwrite_existing: true,
            verbose: false,
        },
    )
    .unwrap();

    assert!(check_archive_with_recovery(archive.to_str().unwrap(), false, None).is_ok());
    let toc = list_archive(archive.to_str().unwrap(), None, None).unwrap();
    assert!(
        toc.iter()
            .any(|(path, _)| path.as_str().ends_with("input.txt"))
    );
}

#[test]
fn list_archive_details_reports_file_sizes_and_saved_metadata() {
    let directory = TestDirectory::new();
    let source = directory.path().join("metadata-input.txt");
    std::fs::write(&source, vec![b'x'; 1024]).unwrap();
    let archive = directory.path().join("entry-details.zar");
    create_archive_with_password_progress_and_special_files(
        archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        false,
        EncodingOptions::default(),
        None,
        None,
        CreateInputOptions {
            save_metadata: true,
            save_dates: false,
            ..CreateInputOptions::default()
        },
    )
    .unwrap();

    let (toc, details) = list_archive_with_details(archive.to_str().unwrap(), None, None).unwrap();
    let (path, entry) = toc
        .iter()
        .find(|(path, _)| path.as_str().ends_with("metadata-input.txt"))
        .unwrap();
    let TocEntry::File(file) = entry else {
        panic!("expected a file entry");
    };
    assert_eq!(file.size, 1024);
    let details = details.get(path).unwrap();
    assert!(details.compressed_size.is_some_and(|size| size > 0));
    assert!(details.permissions.is_some());
    #[cfg(unix)]
    {
        assert!(details.owner.is_some());
        assert!(details.group.is_some());
        assert_eq!(details.metadata_platform.as_deref(), Some("unix"));
    }
}

#[test]
fn add_appends_a_commit_and_extracts_new_files() {
    let directory = TestDirectory::new();
    let original = make_archive(vec![(
        vec![TocChange::PutFile {
            path: archive_path("original.txt"),
            entry: FileEntry::new(3, vec![0]).unwrap(),
        }],
        vec![(0, b"old".to_vec())],
    )]);
    let archive = write_archive(&directory, &original);
    let added = directory.path().join("added.txt");
    std::fs::write(&added, b"new content").unwrap();

    add_archive(archive.to_str().unwrap(), &[added]).unwrap();

    let mut file = File::open(&archive).unwrap();
    let toc = load_toc(&mut file).unwrap();
    assert!(matches!(
        toc.get(&archive_path("original.txt")),
        Some(TocEntry::File(entry)) if entry.size == 3
    ));
    assert!(matches!(
        toc.get(&archive_path("added.txt")),
        Some(TocEntry::File(entry)) if entry.size == 11
    ));
    assert!(check_archive_with_recovery(archive.to_str().unwrap(), false, None).is_ok());

    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();
    extract_archive(archive.to_str().unwrap(), &destination, false).unwrap();
    assert_eq!(
        std::fs::read(destination.join("original.txt")).unwrap(),
        b"old"
    );
    assert_eq!(
        std::fs::read(destination.join("added.txt")).unwrap(),
        b"new content"
    );
}

#[test]
fn add_replaces_existing_file_with_new_commit_contents() {
    let directory = TestDirectory::new();
    let original = make_archive(vec![(
        vec![TocChange::PutFile {
            path: archive_path("same.txt"),
            entry: FileEntry::new(3, vec![0]).unwrap(),
        }],
        vec![(0, b"old".to_vec())],
    )]);
    let archive = write_archive(&directory, &original);
    let replacement = directory.path().join("same.txt");
    std::fs::write(&replacement, b"replacement").unwrap();

    add_archive(archive.to_str().unwrap(), &[replacement]).unwrap();

    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();
    extract_archive(archive.to_str().unwrap(), &destination, false).unwrap();
    assert_eq!(
        std::fs::read(destination.join("same.txt")).unwrap(),
        b"replacement"
    );
}

#[cfg(unix)]
#[test]
fn add_includes_named_pipes_only_when_special_files_are_enabled() {
    let directory = TestDirectory::new();
    let archive_data = make_archive(vec![(
        vec![TocChange::PutFile {
            path: archive_path("existing.txt"),
            entry: FileEntry::new(1, vec![0]).unwrap(),
        }],
        vec![(0, b"x".to_vec())],
    )]);
    let archive = write_archive(&directory, &archive_data);
    let source = directory.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("regular.txt"), b"regular").unwrap();
    let pipe = source.join("pipe");
    let pipe_name = std::ffi::CString::new(pipe.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(pipe_name.as_ptr(), 0o600) }, 0);

    add_archive_with_encoding_progress_and_special_files(
        archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        EncodingOptions::default(),
        None,
        false,
        false,
    )
    .unwrap();
    let mut archive_file = File::open(&archive).unwrap();
    let toc = load_toc(&mut archive_file).unwrap();
    assert!(
        toc.iter()
            .all(|(path, _)| !path.as_str().ends_with("source/pipe"))
    );

    add_archive_with_encoding_progress_and_special_files(
        archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        EncodingOptions::default(),
        None,
        false,
        true,
    )
    .unwrap();
    let mut archive_file = File::open(&archive).unwrap();
    let toc = load_toc(&mut archive_file).unwrap();
    assert!(
        toc.iter()
            .any(|(path, entry)| path.as_str().ends_with("source/pipe")
                && matches!(entry, TocEntry::NamedPipe))
    );
}

#[test]
fn add_failure_preserves_archive_and_cleans_staged_files() {
    let directory = TestDirectory::new();
    let original = make_archive(vec![(
        vec![TocChange::PutFile {
            path: archive_path("original.txt"),
            entry: FileEntry::new(3, vec![0]).unwrap(),
        }],
        vec![(0, b"old".to_vec())],
    )]);
    let archive = write_archive(&directory, &original);
    let missing = directory.path().join("missing.txt");

    assert!(add_archive(archive.to_str().unwrap(), &[missing]).is_err());

    assert_eq!(std::fs::read(&archive).unwrap(), original);
    let staged_prefix = ".input.zar.zarc-add-";
    assert!(std::fs::read_dir(directory.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(staged_prefix)
    }));
}

#[test]
fn remove_appends_a_tombstone_for_a_file() {
    let directory = TestDirectory::new();
    let original = make_archive(vec![(
        vec![TocChange::PutFile {
            path: archive_path("file.txt"),
            entry: FileEntry::new(3, vec![0]).unwrap(),
        }],
        vec![(0, b"old".to_vec())],
    )]);
    let archive = write_archive(&directory, &original);
    let input = directory.path().join("file.txt");
    std::fs::write(&input, b"ignored").unwrap();

    remove_archive(archive.to_str().unwrap(), &[input]).unwrap();

    let toc = load_toc(&mut File::open(&archive).unwrap()).unwrap();
    assert!(toc.is_empty());
    assert!(check_archive_with_recovery(archive.to_str().unwrap(), false, None).is_ok());
}

#[test]
fn remove_directory_removes_its_descendants() {
    let directory = TestDirectory::new();
    let original = make_archive(vec![(
        vec![
            TocChange::PutDirectory {
                path: archive_path("docs"),
            },
            TocChange::PutFile {
                path: archive_path("docs/a.txt"),
                entry: FileEntry::new(1, vec![0]).unwrap(),
            },
            TocChange::PutDirectory {
                path: archive_path("docs/sub"),
            },
            TocChange::PutFile {
                path: archive_path("docs/sub/b.txt"),
                entry: FileEntry::new(1, vec![1]).unwrap(),
            },
        ],
        vec![(0, b"a".to_vec()), (1, b"b".to_vec())],
    )]);
    let archive = write_archive(&directory, &original);
    let input = directory.path().join("docs");
    std::fs::create_dir(&input).unwrap();

    remove_archive(archive.to_str().unwrap(), &[input]).unwrap();

    let toc = load_toc(&mut File::open(&archive).unwrap()).unwrap();
    assert!(toc.is_empty());
    assert!(check_archive_with_recovery(archive.to_str().unwrap(), false, None).is_ok());
}

#[test]
fn remove_missing_path_preserves_archive() {
    let directory = TestDirectory::new();
    let original = make_archive(vec![(
        vec![TocChange::PutFile {
            path: archive_path("present.txt"),
            entry: FileEntry::new(3, vec![0]).unwrap(),
        }],
        vec![(0, b"old".to_vec())],
    )]);
    let archive = write_archive(&directory, &original);
    let input = directory.path().join("missing.txt");

    assert!(remove_archive(archive.to_str().unwrap(), &[input]).is_err());

    assert_eq!(std::fs::read(&archive).unwrap(), original);
}

#[test]
fn extracts_files_into_nested_directories() {
    let directory = TestDirectory::new();
    let archive = make_archive(vec![(
        vec![TocChange::PutFile {
            path: archive_path("nested/deeper/file.txt"),
            entry: FileEntry::new(5, vec![3]).unwrap(),
        }],
        vec![(3, b"hello".to_vec())],
    )]);
    let archive_path = write_archive(&directory, &archive);
    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();

    extract_archive(archive_path.to_str().unwrap(), &destination, false).unwrap();

    assert_eq!(
        std::fs::read(destination.join("nested/deeper/file.txt")).unwrap(),
        b"hello"
    );
}

#[test]
fn dry_run_plan_lists_toc_and_planned_actions() {
    let directory = TestDirectory::new();
    let archive = make_archive(vec![(
        vec![
            TocChange::PutDirectory {
                path: archive_path("nested"),
            },
            TocChange::PutFile {
                path: archive_path("nested/file.txt"),
                entry: FileEntry::new(5, vec![0]).unwrap(),
            },
        ],
        vec![(0, b"hello".to_vec())],
    )]);
    let archive = write_archive(&directory, &archive);
    let destination = directory.path().join("output");
    let toc = load_toc_through(archive.to_str().unwrap(), None, None).unwrap();
    let directories = vec![destination.join("nested")];
    let files = [(
        destination.join("nested/file.txt"),
        ExtractedSource {
            entry: FileEntry::new(5, vec![0]).unwrap(),
            blocks: vec![ExtractedBlock {
                offset: 0,
                block_index: 0,
                encoded_size: 0,
                uncompressed_size: 5,
                commit_header: zarlib::CommitHeader::new(0, [0; 4], 1, 0).unwrap(),
                algorithm: None,
                dek: None,
            }],
            metadata: None,
            kind: ExtractedKind::File,
        },
    )];

    let plan = ExtractionPlan {
        toc,
        directories: directories.clone(),
        files: files
            .iter()
            .map(|(path, source)| (path.clone(), source.entry.clone(), source.kind.clone()))
            .collect(),
    };
    let plan_lines = format_extraction_plan(&plan);

    assert!(plan_lines.iter().any(|line| line == "final TOC:"));
    assert!(plan_lines.iter().any(|line| line == "  nested/"));
    assert!(
        plan_lines
            .iter()
            .any(|line| line == "  nested/file.txt\t5 bytes")
    );
    assert!(plan_lines.iter().any(|line| {
        line == &format!("create directory {}", destination.join("nested").display())
    }));
    assert!(plan_lines.iter().any(|line| {
        line == &format!(
            "write {} (5 bytes)",
            destination.join("nested/file.txt").display()
        )
    }));
}

#[test]
fn dry_run_plans_extraction_without_writing_files() {
    let directory = TestDirectory::new();
    let archive = make_archive(vec![(
        vec![TocChange::PutFile {
            path: archive_path("file.txt"),
            entry: FileEntry::new(5, vec![0]).unwrap(),
        }],
        vec![(0, b"hello".to_vec())],
    )]);
    let archive_path = write_archive(&directory, &archive);
    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();

    extract_archive(archive_path.to_str().unwrap(), &destination, true).unwrap();

    assert!(!destination.join("file.txt").exists());
}

#[test]
fn extraction_through_stops_before_later_data_encrypted_commits() {
    let directory = TestDirectory::new();
    let mut archive = make_archive(vec![(
        vec![TocChange::PutFile {
            path: archive_path("visible.txt"),
            entry: FileEntry::new(3, vec![0]).unwrap(),
        }],
        vec![(0, b"one".to_vec())],
    )]);
    let selected_id = {
        let path = write_archive(&directory, &archive);
        commit_id_at(&path, 0)
    };
    archive.extend_from_slice(&make_data_encrypted_empty_commit([9; 4]));
    let archive = write_archive(&directory, &archive);
    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();

    extract_archive_with_options(
        archive.to_str().unwrap(),
        &destination,
        false,
        Some(&selected_id),
        None,
        CollisionPolicy::Error,
        ExistingPolicy::Error,
    )
    .unwrap();

    assert_eq!(
        std::fs::read(destination.join("visible.txt")).unwrap(),
        b"one"
    );
}

#[test]
fn extraction_through_rejects_malformed_missing_and_duplicate_ids() {
    let directory = TestDirectory::new();
    let commit = make_archive(vec![(Vec::new(), Vec::new())]);
    let selected_path = write_archive(&directory, &commit);
    let selected_id = commit_id_at(&selected_path, 0);
    let missing_id = "ff".repeat(16);
    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();

    assert!(
        load_extraction_state(selected_path.to_str().unwrap(), Some("not-an-id"), None).is_err()
    );
    assert!(
        load_extraction_state(selected_path.to_str().unwrap(), Some(&missing_id), None).is_err()
    );

    let mut duplicate_archive = commit.clone();
    // Find the first commit's next_header_offset position and data_section_size.
    let mut reader = std::io::Cursor::new(&duplicate_archive);
    let first_header_offset = read_first_header_offset(&mut reader);
    let (first_ch, first_data_start, _) = read_commit_at(&mut reader, first_header_offset);
    let first_next_offset_pos = (first_data_start - zarlib::HEADER_OFFSET_SIZE) as usize;
    // Append a second copy of the commit (from its next_header_offset through its this_header_offset).
    let second_commit_bytes = &commit[first_next_offset_pos..];
    let second_commit_start = duplicate_archive.len() as u64;
    duplicate_archive.extend_from_slice(second_commit_bytes);
    // Backfill the first commit's next_header_offset to point to the second commit's header.
    let second_header_offset =
        second_commit_start + zarlib::HEADER_OFFSET_SIZE + first_ch.data_section_size;
    duplicate_archive[first_next_offset_pos..first_next_offset_pos + 8]
        .copy_from_slice(&second_header_offset.to_le_bytes());
    let duplicate_path = directory.path().join("duplicate.zar");
    std::fs::write(&duplicate_path, duplicate_archive).unwrap();
    assert!(
        load_extraction_state(duplicate_path.to_str().unwrap(), Some(&selected_id), None).is_err()
    );
}

#[test]
fn check_recovery_finds_valid_commit_after_damaged_gap() {
    let directory = TestDirectory::new();
    let archive = make_archive(vec![(Vec::new(), Vec::new()), (Vec::new(), Vec::new())]);
    // Read the first commit's extent using the new offset-chain format.
    let mut reader = std::io::Cursor::new(&archive);
    let first_header_offset = read_first_header_offset(&mut reader);
    let (_first_ch, _, _second_header_offset) = read_commit_at(&mut reader, first_header_offset);
    // Corrupt the first commit's header frame so recovery must scan forward
    // to find the second commit. The first commit's header frame starts at
    // first_header_offset. We overwrite a few bytes there.
    let mut damaged = archive.clone();
    let corrupt_pos = first_header_offset as usize;
    damaged[corrupt_pos..corrupt_pos + 4].copy_from_slice(b"BAD!");
    let archive_path = write_archive(&directory, &damaged);

    assert!(check_archive_with_recovery(archive_path.to_str().unwrap(), false, None).is_err());
    assert_eq!(
        check_archive_with_recovery(archive_path.to_str().unwrap(), true, None)
            .unwrap()
            .recoveries,
        1
    );
}

#[test]
fn check_rejects_file_size_mismatches() {
    let directory = TestDirectory::new();
    let archive = make_archive(vec![(
        vec![TocChange::PutFile {
            path: archive_path("file.txt"),
            entry: FileEntry::new(6, vec![0]).unwrap(),
        }],
        vec![(0, b"hello".to_vec())],
    )]);
    let archive_path = write_archive(&directory, &archive);

    assert!(check_archive_with_recovery(archive_path.to_str().unwrap(), false, None).is_err());
}

#[test]
fn check_applies_toc_changes_across_commits() {
    let directory = TestDirectory::new();
    let archive = make_archive(vec![
        (
            vec![TocChange::PutFile {
                path: archive_path("node"),
                entry: FileEntry::new(1, vec![0]).unwrap(),
            }],
            vec![(0, b"a".to_vec())],
        ),
        (
            vec![TocChange::PutFile {
                path: archive_path("node/child"),
                entry: FileEntry::new(1, vec![0]).unwrap(),
            }],
            vec![(0, b"b".to_vec())],
        ),
    ]);
    let archive_path = write_archive(&directory, &archive);

    assert!(check_archive_with_recovery(archive_path.to_str().unwrap(), false, None).is_err());
}

#[test]
fn check_accepts_reused_block_indices_in_separate_commits() {
    let directory = TestDirectory::new();
    let archive = make_archive(vec![
        (
            vec![TocChange::PutFile {
                path: archive_path("first.txt"),
                entry: FileEntry::new(1, vec![0]).unwrap(),
            }],
            vec![(0, b"a".to_vec())],
        ),
        (
            vec![TocChange::PutFile {
                path: archive_path("second.txt"),
                entry: FileEntry::new(2, vec![0]).unwrap(),
            }],
            vec![(0, b"bc".to_vec())],
        ),
    ]);
    let archive_path = write_archive(&directory, &archive);

    assert!(check_archive_with_recovery(archive_path.to_str().unwrap(), false, None).is_ok());
}

#[test]
fn check_keeps_scanning_when_header_encryption_hides_toc() {
    let directory = TestDirectory::new();
    let descriptor = EncryptionDescriptor::Symmetric {
        scope: EncryptionScope::Header,
        algorithm: ContentAead::Aes256Gcm,
        nonce: [11; 12],
    }
    .encode()
    .unwrap();
    let logical_header = LogicalHeader::default().encode().unwrap();
    let dek = [9u8; zarlib::DEK_SIZE];
    // First create with a dummy frame_size to measure, then re-encode.
    let temp_ch = zarlib::CommitHeader::new(0, [3; 4], 0, 0).unwrap();
    let temp_header = HeaderFrame::from_uncompressed_encrypted(
        temp_ch,
        &logical_header,
        descriptor.clone(),
        1,
        &dek,
    )
    .unwrap();
    let header_frame_size = u32::try_from(temp_header.encoded_size().unwrap()).unwrap();
    let commit_header = zarlib::CommitHeader::new(0, [3; 4], 0, header_frame_size).unwrap();
    let header = HeaderFrame::from_uncompressed_encrypted(
        commit_header.clone(),
        &logical_header,
        descriptor,
        1,
        &dek,
    )
    .unwrap();
    let mut header_bytes = Vec::new();
    header.write_to(&mut header_bytes).unwrap();
    let mut archive = Vec::new();
    write_container_magic(&mut archive).unwrap();
    // first_header_offset placeholder.
    archive.extend_from_slice(&0u64.to_le_bytes());
    // next_header_offset (0 = last commit).
    archive.extend_from_slice(&0u64.to_le_bytes());
    // No data blocks (data_section_size = 0).
    // Header frame.
    let header_offset = archive.len() as u64;
    archive.extend_from_slice(&header_bytes);
    // this_header_offset.
    archive.extend_from_slice(&header_offset.to_le_bytes());
    // Backfill first_header_offset in container header.
    archive[4..12].copy_from_slice(&header_offset.to_le_bytes());
    let archive_path = write_archive(&directory, &archive);

    assert!(check_archive_with_recovery(archive_path.to_str().unwrap(), false, None).is_ok());
}

#[test]
fn rejects_file_size_mismatch_before_writing() {
    let directory = TestDirectory::new();
    let archive = make_archive(vec![(
        vec![TocChange::PutFile {
            path: archive_path("file.txt"),
            entry: FileEntry::new(6, vec![0]).unwrap(),
        }],
        vec![(0, b"hello".to_vec())],
    )]);
    let archive_path = write_archive(&directory, &archive);
    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();

    assert!(extract_archive(archive_path.to_str().unwrap(), &destination, false).is_err());
    assert!(!destination.join("file.txt").exists());
}

#[test]
fn rejects_data_encrypted_commits() {
    let directory = TestDirectory::new();
    let descriptor = EncryptionDescriptor::Symmetric {
        scope: EncryptionScope::Data,
        algorithm: ContentAead::Aes256Gcm,
        nonce: [7; 12],
    }
    .encode()
    .unwrap();
    let logical_header = LogicalHeader::default().encode().unwrap();
    let temp_ch = zarlib::CommitHeader::new(0, [0; 4], 0, 0).unwrap();
    let header = HeaderFrame::from_uncompressed(temp_ch, &logical_header, descriptor, 1).unwrap();
    let header_frame_size = u32::try_from(header.encoded_size().unwrap()).unwrap();
    let commit_header = zarlib::CommitHeader::new(0, [0; 4], 0, header_frame_size).unwrap();
    let header = HeaderFrame::from_uncompressed(
        commit_header.clone(),
        &logical_header,
        header.encryption_descriptor.clone(),
        1,
    )
    .unwrap();
    let mut header_bytes = Vec::new();
    header.write_to(&mut header_bytes).unwrap();
    let mut archive = Vec::new();
    write_container_magic(&mut archive).unwrap();
    archive.extend_from_slice(&0u64.to_le_bytes()); // first_header_offset placeholder
    archive.extend_from_slice(&0u64.to_le_bytes()); // next_header_offset (last)
    let header_offset = archive.len() as u64;
    archive.extend_from_slice(&header_bytes);
    archive.extend_from_slice(&header_offset.to_le_bytes()); // this_header_offset
    archive[4..12].copy_from_slice(&header_offset.to_le_bytes()); // backfill
    let archive_path = write_archive(&directory, &archive);
    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();

    assert!(extract_archive(archive_path.to_str().unwrap(), &destination, false).is_err());
}

#[test]
fn list_and_verbose_info_skip_data_payloads_without_decompressing_them() {
    let directory = TestDirectory::new();
    let archive = make_archive(vec![(
        vec![TocChange::PutFile {
            path: archive_path("visible.txt"),
            entry: FileEntry::new(5, vec![0]).unwrap(),
        }],
        vec![(0, b"hello".to_vec())],
    )]);
    let archive = write_archive(&directory, &archive);
    let mut bytes = std::fs::read(&archive).unwrap();
    let _prefix_reader = File::open(&archive).unwrap();
    let mut prefix_reader = std::io::Cursor::new(&bytes);
    let first_header_offset = read_first_header_offset(&mut prefix_reader);
    let (_ch, data_start, _) = read_commit_at(&mut prefix_reader, first_header_offset);
    let payload_offset = data_start as usize + 20;
    bytes[payload_offset] ^= 0x80;
    std::fs::write(&archive, bytes).unwrap();

    assert_eq!(
        load_toc_through(archive.to_str().unwrap(), None, None)
            .unwrap()
            .len(),
        1
    );
    let (_, details) = list_archive_with_details(archive.to_str().unwrap(), None, None).unwrap();
    assert!(
        details
            .iter()
            .any(|(path, entry)| path.as_str() == "visible.txt" && entry.compressed_size.is_some())
    );
    let info = info_archive(archive.to_str().unwrap(), true).unwrap();
    assert_eq!(info.len(), 1);
    assert!(info[0].details.is_some());
    assert!(check_archive_with_recovery(archive.to_str().unwrap(), false, None).is_err());
}

#[test]
fn list_state_does_not_decode_data_encrypted_content_or_future_commits() {
    let directory = TestDirectory::new();
    let mut archive = make_archive(vec![(
        vec![TocChange::PutFile {
            path: archive_path("visible.txt"),
            entry: FileEntry::new(3, vec![0]).unwrap(),
        }],
        vec![(0, b"one".to_vec())],
    )]);
    let descriptor = EncryptionDescriptor::Symmetric {
        scope: EncryptionScope::Data,
        algorithm: ContentAead::Aes256Gcm,
        nonce: [9; 12],
    }
    .encode()
    .unwrap();
    let logical_header = LogicalHeader::default().encode().unwrap();
    let temp_ch = zarlib::CommitHeader::new(0, [1; 4], 0, 0).unwrap();
    let header = HeaderFrame::from_uncompressed(temp_ch, &logical_header, descriptor, 1).unwrap();
    let header_frame_size = u32::try_from(header.encoded_size().unwrap()).unwrap();
    let commit_header = zarlib::CommitHeader::new(0, [1; 4], 0, header_frame_size).unwrap();
    let header = HeaderFrame::from_uncompressed(
        commit_header.clone(),
        &logical_header,
        header.encryption_descriptor.clone(),
        1,
    )
    .unwrap();
    let mut header_bytes = Vec::new();
    header.write_to(&mut header_bytes).unwrap();
    // Read the first commit's header offset and backfill its next_header_offset.
    let first_header_offset = u64::from_le_bytes(archive[4..12].try_into().unwrap());
    // The first commit's next_header_offset is at data_start - HEADER_OFFSET_SIZE.
    // For make_archive with data_section_size bytes of data, data_start = first_header_offset - data_section_size.
    // Read the first commit's commit_header to find data_section_size.
    let mut reader = std::io::Cursor::new(&archive);
    use std::io::{Seek, SeekFrom};
    reader.seek(SeekFrom::Start(first_header_offset)).unwrap();
    let first_frame = HeaderFrame::read_from(&mut reader, 1024 * 1024).unwrap();
    let first_data_section_size = first_frame.commit_header.data_section_size;
    let first_next_offset_pos =
        first_header_offset - first_data_section_size - zarlib::HEADER_OFFSET_SIZE;
    // Append the encrypted commit after the first commit.
    let new_commit_start = archive.len() as u64;
    archive.extend_from_slice(&0u64.to_le_bytes()); // next_header_offset (last)
    let new_header_offset = archive.len() as u64;
    archive.extend_from_slice(&header_bytes);
    archive.extend_from_slice(&new_header_offset.to_le_bytes()); // this_header_offset
    // Backfill the first commit's next_header_offset to point to the new commit's header.
    archive[first_next_offset_pos as usize..(first_next_offset_pos as usize + 8)]
        .copy_from_slice(&new_header_offset.to_le_bytes());
    let _ = new_commit_start;
    let archive = write_archive(&directory, &archive);

    let toc = load_toc_through(archive.to_str().unwrap(), Some("01010101"), None).unwrap();
    assert_eq!(toc.len(), 1);
    let full_toc = load_toc_through(archive.to_str().unwrap(), None, None).unwrap();
    assert_eq!(full_toc.len(), 1);
}

#[test]
fn collision_policies_choose_first_latest_or_renamed_paths() {
    let directory = TestDirectory::new();
    let archive = make_archive(vec![(
        vec![
            TocChange::PutFile {
                path: archive_path("file.txt"),
                entry: FileEntry::new(3, vec![0]).unwrap(),
            },
            TocChange::PutFile {
                path: archive_path("FILE.txt"),
                entry: FileEntry::new(3, vec![1]).unwrap(),
            },
        ],
        vec![(0, b"old".to_vec()), (1, b"new".to_vec())],
    )]);
    let archive = write_archive(&directory, &archive);

    let first = directory.path().join("first");
    std::fs::create_dir(&first).unwrap();
    extract_archive_with_options(
        archive.to_str().unwrap(),
        &first,
        false,
        None,
        None,
        CollisionPolicy::First,
        ExistingPolicy::Error,
    )
    .unwrap();
    assert_eq!(std::fs::read(first.join("FILE.txt")).unwrap(), b"new");
    assert!(!first.join("file.txt").exists());

    let latest = directory.path().join("latest");
    std::fs::create_dir(&latest).unwrap();
    extract_archive_with_options(
        archive.to_str().unwrap(),
        &latest,
        false,
        None,
        None,
        CollisionPolicy::Latest,
        ExistingPolicy::Error,
    )
    .unwrap();
    assert_eq!(std::fs::read(latest.join("file.txt")).unwrap(), b"old");

    let renamed = directory.path().join("renamed");
    std::fs::create_dir(&renamed).unwrap();
    extract_archive(archive.to_str().unwrap(), &renamed, false).unwrap();
    assert_eq!(std::fs::read(renamed.join("FILE.txt")).unwrap(), b"new");
    assert_eq!(std::fs::read(renamed.join("file (1).txt")).unwrap(), b"old");
}

#[test]
fn explicit_error_collision_policy_rejects_case_folded_paths() {
    let directory = TestDirectory::new();
    let archive = make_archive(vec![(
        vec![
            TocChange::PutFile {
                path: archive_path("file.txt"),
                entry: FileEntry::new(3, vec![0]).unwrap(),
            },
            TocChange::PutFile {
                path: archive_path("FILE.txt"),
                entry: FileEntry::new(3, vec![1]).unwrap(),
            },
        ],
        vec![(0, b"old".to_vec()), (1, b"new".to_vec())],
    )]);
    let archive = write_archive(&directory, &archive);
    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();
    let progress = RecordingProgress {
        messages: Mutex::new(Vec::new()),
        increments: Mutex::new(Vec::new()),
        total: Mutex::new(None),
        message_times: Mutex::new(Vec::new()),
        input_summary: Mutex::new(None),
        extraction_summary: Mutex::new(None),
        events: Mutex::new(Vec::new()),
    };

    assert!(
        extract_archive_with_progress(
            archive.to_str().unwrap(),
            &destination,
            ExtractionOptions {
                dry_run: false,
                through: None,
                collision_policy: CollisionPolicy::Error,
                existing_policy: ExistingPolicy::Error,
                restore_metadata: false,
                restore_symlinks: false,
                restore_devices: false,
                strip_top_level: false,
            },
            None,
            Some(&progress),
        )
        .is_err()
    );

    let summary = progress.extraction_summary.lock().unwrap().clone().unwrap();
    assert_eq!(summary.files, 2);
    assert_eq!(summary.total_bytes, 6);
    assert!(
        progress
            .total
            .lock()
            .unwrap()
            .is_some_and(|total| total > 0)
    );
    let events = progress.events.lock().unwrap();
    assert_eq!(events[0], "start:Reading archive headers");
    assert!(events.iter().any(|event| event == "summary"));
    assert!(!destination.join("file.txt").exists());
    assert!(!destination.join("file.txt").exists());
    assert!(!destination.join("FILE.txt").exists());
}

#[test]
fn default_collision_policy_renames_case_folded_paths() {
    let directory = TestDirectory::new();
    let archive = make_archive(vec![(
        vec![
            TocChange::PutFile {
                path: archive_path("file.txt"),
                entry: FileEntry::new(3, vec![0]).unwrap(),
            },
            TocChange::PutFile {
                path: archive_path("FILE.txt"),
                entry: FileEntry::new(3, vec![1]).unwrap(),
            },
        ],
        vec![(0, b"old".to_vec()), (1, b"new".to_vec())],
    )]);
    let archive = write_archive(&directory, &archive);
    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();

    extract_archive(archive.to_str().unwrap(), &destination, false).unwrap();

    assert_eq!(std::fs::read(destination.join("FILE.txt")).unwrap(), b"new");
    assert_eq!(
        std::fs::read(destination.join("file (1).txt")).unwrap(),
        b"old"
    );
}

#[test]
fn duplicate_archive_path_uses_final_ordered_toc_value_without_collision_error() {
    let directory = TestDirectory::new();
    let archive = make_archive(vec![(
        vec![
            TocChange::PutFile {
                path: archive_path("same.txt"),
                entry: FileEntry::new(3, vec![0]).unwrap(),
            },
            TocChange::PutFile {
                path: archive_path("same.txt"),
                entry: FileEntry::new(3, vec![1]).unwrap(),
            },
        ],
        vec![(0, b"old".to_vec()), (1, b"new".to_vec())],
    )]);
    let archive = write_archive(&directory, &archive);
    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();

    extract_archive_with_options(
        archive.to_str().unwrap(),
        &destination,
        false,
        None,
        None,
        CollisionPolicy::Error,
        ExistingPolicy::Error,
    )
    .unwrap();

    assert_eq!(std::fs::read(destination.join("same.txt")).unwrap(), b"new");
}

#[test]
fn parallel_extraction_preserves_block_order_and_checksums() {
    let directory = TestDirectory::new();
    let source = directory.path().join("large.bin");
    let payload = (0..(64 * 1024 * 4 + 137))
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    std::fs::write(&source, &payload).unwrap();
    let archive = directory.path().join("parallel-extraction.zar");
    create_archive_with_encoding(
        archive.to_str().unwrap(),
        std::slice::from_ref(&source),
        false,
        EncodingOptions {
            block_size: 64 * 1024,
            compression_level: 1,
        },
    )
    .unwrap();
    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();

    extract_archive(archive.to_str().unwrap(), &destination, false).unwrap();

    assert_eq!(
        std::fs::read(destination.join("large.bin")).unwrap(),
        payload
    );
}

#[test]
fn non_interactive_ask_policies_fail_before_extraction() {
    assert!(
        validate_interactive_policies(false, CollisionPolicy::Ask, ExistingPolicy::Error,).is_err()
    );
    assert!(
        validate_interactive_policies(false, CollisionPolicy::Error, ExistingPolicy::Ask,).is_err()
    );
    assert!(
        validate_interactive_policies(false, CollisionPolicy::Error, ExistingPolicy::Error,)
            .is_ok()
    );
}

#[cfg(unix)]
#[test]
fn extraction_file_creation_does_not_follow_symlinks_or_overwrite_races() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::new();
    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();
    let external_file = directory.path().join("external.txt");
    std::fs::write(&external_file, b"keep").unwrap();
    let symlink_target = destination.join("link.txt");
    symlink(&external_file, &symlink_target).unwrap();

    assert!(
        create_extraction_file(&destination, &symlink_target, ExistingPolicy::Overwrite).is_err()
    );
    assert_eq!(std::fs::read(&external_file).unwrap(), b"keep");

    let raced_file = destination.join("raced.txt");
    std::fs::write(&raced_file, b"keep").unwrap();
    assert!(create_extraction_file(&destination, &raced_file, ExistingPolicy::Error).is_err());
    assert_eq!(std::fs::read(&raced_file).unwrap(), b"keep");
    assert!(create_extraction_file(&destination, &raced_file, ExistingPolicy::Skip).is_err());
    assert_eq!(std::fs::read(&raced_file).unwrap(), b"keep");
}

#[test]
fn existing_file_policy_error_preserves_existing_file() {
    let directory = TestDirectory::new();
    let archive = make_archive(vec![(
        vec![TocChange::PutFile {
            path: archive_path("file.txt"),
            entry: FileEntry::new(3, vec![0]).unwrap(),
        }],
        vec![(0, b"new".to_vec())],
    )]);
    let archive = write_archive(&directory, &archive);
    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();
    let target = destination.join("file.txt");
    std::fs::write(&target, b"old").unwrap();

    assert!(
        extract_archive_with_options(
            archive.to_str().unwrap(),
            &destination,
            false,
            None,
            None,
            CollisionPolicy::Error,
            ExistingPolicy::Error,
        )
        .is_err()
    );
    assert_eq!(std::fs::read(target).unwrap(), b"old");
}

#[test]
fn existing_file_policies_skip_and_overwrite() {
    let directory = TestDirectory::new();
    let archive = make_archive(vec![(
        vec![TocChange::PutFile {
            path: archive_path("file.txt"),
            entry: FileEntry::new(3, vec![0]).unwrap(),
        }],
        vec![(0, b"new".to_vec())],
    )]);
    let archive = write_archive(&directory, &archive);
    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(destination.join("file.txt"), b"old").unwrap();

    extract_archive_with_options(
        archive.to_str().unwrap(),
        &destination,
        false,
        None,
        None,
        CollisionPolicy::Error,
        ExistingPolicy::Skip,
    )
    .unwrap();
    assert_eq!(std::fs::read(destination.join("file.txt")).unwrap(), b"old");
    extract_archive_with_options(
        archive.to_str().unwrap(),
        &destination,
        false,
        None,
        None,
        CollisionPolicy::Error,
        ExistingPolicy::Overwrite,
    )
    .unwrap();
    assert_eq!(std::fs::read(destination.join("file.txt")).unwrap(), b"new");
}

#[test]
fn block_indices_can_be_reused_in_later_commits() {
    let directory = TestDirectory::new();
    let archive = make_archive(vec![
        (
            vec![TocChange::PutFile {
                path: archive_path("file.txt"),
                entry: FileEntry::new(3, vec![0]).unwrap(),
            }],
            vec![(0, b"old".to_vec())],
        ),
        (
            vec![TocChange::PutFile {
                path: archive_path("file.txt"),
                entry: FileEntry::new(3, vec![0]).unwrap(),
            }],
            vec![(0, b"new".to_vec())],
        ),
    ]);
    let archive_path = write_archive(&directory, &archive);
    let destination = directory.path().join("output");
    std::fs::create_dir(&destination).unwrap();

    extract_archive(archive_path.to_str().unwrap(), &destination, false).unwrap();

    assert_eq!(std::fs::read(destination.join("file.txt")).unwrap(), b"new");
}
