use std::io::Cursor;
use std::path::Path;

use zarlib::operations::{
    EncodingOptions, GrepOptions, PasswordSet, StreamEncryption, StreamInput, add_archive,
    create_archive, create_archive_from_readers, grep_archive,
};
use zarlib::{ArchivePath, ContentAead, EncryptionScope, TocEntry};

fn archive(path: &Path, files: &[(&str, &[u8])], encryption: Option<StreamEncryption<'_>>) {
    let mut readers: Vec<_> = files.iter().map(|(_, data)| Cursor::new(*data)).collect();
    let mut inputs: Vec<_> = files
        .iter()
        .zip(&mut readers)
        .map(|((path, _), reader)| StreamInput { path, reader })
        .collect();
    let mut output = std::fs::File::create(path).unwrap();
    create_archive_from_readers(
        &mut output,
        &mut inputs,
        EncodingOptions {
            block_size: 64 * 1024,
            ..Default::default()
        },
        encryption,
    )
    .unwrap();
}

#[test]
fn searches_filtered_files_with_deduplicated_context() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.zar");
    archive(
        &path,
        &[
            ("z.txt", b"match\nlast"),
            ("dir/a.txt", b"zero\r\nmatch\r\nbetween\nmatch\nend\nfar\n"),
            ("dir/b.txt", b"nothing\n"),
            ("dir/c.bin", b"match\n"),
            ("empty.txt", b""),
        ],
        None,
    );
    let results = grep_archive(
        path.to_str().unwrap(),
        "*.txt",
        "^match$",
        GrepOptions {
            before: 1,
            after: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].path.as_str(), "dir/a.txt");
    let lines: Vec<_> = results[0]
        .lines
        .iter()
        .map(|line| (line.line_number, line.bytes.as_slice(), line.is_match))
        .collect();
    assert_eq!(
        lines,
        vec![
            (1, b"zero".as_slice(), false),
            (2, b"match".as_slice(), true),
            (3, b"between".as_slice(), false),
            (4, b"match".as_slice(), true),
            (5, b"end".as_slice(), false),
        ]
    );
    assert_eq!(results[1].path.as_str(), "z.txt");
    assert_eq!(results[1].lines[1].bytes, b"last");
    assert!(!results[1].lines[1].is_match);

    let no_context = grep_archive(
        path.to_str().unwrap(),
        "dir/*.txt",
        "match",
        Default::default(),
    )
    .unwrap();
    assert_eq!(no_context.len(), 1);
    assert_eq!(no_context[0].lines.len(), 2);
    assert!(no_context[0].lines.iter().all(|line| line.is_match));
    assert!(
        grep_archive(
            path.to_str().unwrap(),
            "*.txt",
            "absent",
            Default::default()
        )
        .unwrap()
        .is_empty()
    );
    assert!(grep_archive(path.to_str().unwrap(), "[", "x", Default::default()).is_err());
    assert!(grep_archive(path.to_str().unwrap(), "*", "(", Default::default()).is_err());
    let empty_lines =
        grep_archive(path.to_str().unwrap(), "z.txt", "^$", Default::default()).unwrap();
    assert!(empty_lines.is_empty());
}

#[test]
fn searches_lines_spanning_blocks_and_non_utf8_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.zar");
    let mut data = vec![b'a'; 64 * 1024 - 1];
    data.extend_from_slice(b"\r\n\xffmatch\n\nend\r");
    archive(&path, &[("long.txt", &data)], None);
    let results = grep_archive(
        path.to_str().unwrap(),
        "*",
        r"match|^$|end",
        GrepOptions {
            before: 1,
            after: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let lines = &results[0].lines;
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0].bytes.len(), 64 * 1024 - 1);
    assert!(!lines[0].is_match);
    assert_eq!(lines[1].bytes, b"\xffmatch");
    assert!(lines[2].bytes.is_empty());
    assert_eq!(lines[3].bytes, b"end\r");
    assert!(lines[1..].iter().all(|line| line.is_match));
}

#[test]
fn searches_small_files_sharing_an_archive_block() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.txt");
    let second = dir.path().join("second.txt");
    let path = dir.path().join("test.zar");
    std::fs::write(&first, b"first needle\n").unwrap();
    std::fs::write(&second, b"second needle\n").unwrap();
    create_archive(path.to_str().unwrap(), &[first, second]).unwrap();
    let toc = zarlib::operations::list_archive(path.to_str().unwrap(), None, None).unwrap();
    let Some(TocEntry::File(first)) = toc.get(&ArchivePath::new("first.txt").unwrap()) else {
        panic!("missing first file");
    };
    let Some(TocEntry::File(second)) = toc.get(&ArchivePath::new("second.txt").unwrap()) else {
        panic!("missing second file");
    };
    assert_eq!(first.block_indices, second.block_indices);
    assert!(first.offset_in_block.is_some() && second.offset_in_block.is_some());
    assert_ne!(first.offset_in_block, second.offset_in_block);

    let results = grep_archive(path.to_str().unwrap(), "*", "needle", Default::default()).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].lines[0].bytes, b"first needle");
    assert_eq!(results[1].lines[0].bytes, b"second needle");
}

#[test]
fn matches_and_crlf_across_block_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.zar");
    let block_size = 64 * 1024;
    let mut data = b"before\n".to_vec();
    data.resize(block_size - 2, b'a');
    data.extend_from_slice(b"NEEDLE\n");
    data.resize(2 * block_size - 1, b'b');
    data.extend_from_slice(b"\r\nafter\n");
    let mut unicode = vec![b'x'; block_size - 1];
    unicode.extend_from_slice("éTARGET\n".as_bytes());
    archive(
        &path,
        &[("large.txt", &data), ("unicode.txt", &unicode)],
        None,
    );
    let toc = zarlib::operations::list_archive(path.to_str().unwrap(), None, None).unwrap();
    assert!(matches!(
        toc.get(&ArchivePath::new("large.txt").unwrap()),
        Some(TocEntry::File(entry)) if entry.block_indices.len() == 3
    ));
    assert!(matches!(
        toc.get(&ArchivePath::new("unicode.txt").unwrap()),
        Some(TocEntry::File(entry)) if entry.block_indices.len() == 2
    ));

    let results = grep_archive(
        path.to_str().unwrap(),
        "*",
        "NEEDLE|éTARGET",
        GrepOptions {
            before: 1,
            after: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(results.len(), 2);
    let lines = &results[0].lines;
    assert_eq!(lines.len(), 3);
    assert_eq!(
        (
            lines[0].line_number,
            lines[0].bytes.as_slice(),
            lines[0].is_match
        ),
        (1, b"before".as_slice(), false)
    );
    assert_eq!(lines[1].line_number, 2);
    assert!(lines[1].bytes.ends_with(b"NEEDLE"));
    assert!(lines[1].is_match);
    assert_eq!(lines[2].line_number, 3);
    assert!(lines[2].bytes.iter().all(|byte| *byte == b'b'));
    assert!(!lines[2].is_match);
    assert_eq!(results[1].lines[0].bytes, unicode[..unicode.len() - 1]);
    assert!(results[1].lines[0].is_match);
}

#[cfg(not(target_os = "wasi"))]
#[test]
fn grep_works_without_a_temporary_directory() {
    const ARCHIVE_ENV: &str = "ZAR_GREP_NO_TEMP_TEST_ARCHIVE";
    if let Some(path) = std::env::var_os(ARCHIVE_ENV) {
        assert!(tempfile::tempfile().is_err());
        let results = grep_archive(
            Path::new(&path).to_str().unwrap(),
            "*",
            "NEEDLE",
            Default::default(),
        )
        .unwrap();
        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .all(|file| file.lines.len() == 1 && file.lines[0].is_match)
        );
        assert!(results[0].lines[0].bytes.ends_with(b"NEEDLE"));
        assert_eq!(results[1].lines[0].bytes, b"small NEEDLE");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.zar");
    let mut large = vec![b'x'; 64 * 1024 - 2];
    large.extend_from_slice(b"NEEDLE\n");
    archive(
        &path,
        &[("large.txt", &large), ("small.txt", b"small NEEDLE\n")],
        None,
    );
    let unavailable = dir.path().join("not-a-directory");
    std::fs::write(&unavailable, b"not a directory").unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "grep_works_without_a_temporary_directory",
            "--nocapture",
        ])
        .env(ARCHIVE_ENV, &path)
        .env("TMPDIR", &unavailable)
        .env("TMP", &unavailable)
        .env("TEMP", &unavailable)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}

#[test]
fn supports_lookaround_and_backreferences_on_byte_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.zar");
    archive(
        &path,
        &[
            (
                "input.txt",
                b"\xffprefix foo foo!\nprefix foo bar!\n\xffmatch\n",
            ),
            ("unicode.txt", "café café\n".as_bytes()),
        ],
        None,
    );
    let results = grep_archive(
        path.to_str().unwrap(),
        "*",
        r"(?<=prefix )(\w+) \1(?=!)",
        Default::default(),
    )
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].lines.len(), 1);
    assert_eq!(results[0].lines[0].bytes, b"\xffprefix foo foo!");

    let results = grep_archive(
        path.to_str().unwrap(),
        "unicode.txt",
        r"^(\w+) \1$",
        Default::default(),
    )
    .unwrap();
    assert_eq!(results[0].lines[0].bytes, "café café".as_bytes());

    let error = grep_archive(
        path.to_str().unwrap(),
        "*",
        r"(?-u:\xFF)",
        Default::default(),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("Unicode"), "{error}");
}

#[test]
fn reports_regex_backtracking_errors_with_file_and_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.zar");
    archive(
        &path,
        &[(
            "input.txt",
            b"first\nabababababababababababababababababababababababababababab\n",
        )],
        None,
    );
    let error = grep_archive(
        path.to_str().unwrap(),
        "*",
        r"(?i)(a|b|ab)*(?>c)",
        Default::default(),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("input.txt"), "{error}");
    assert!(error.contains("line 2"), "{error}");
    assert!(error.contains("backtrack"), "{error}");
}

#[test]
fn respects_passwords_and_history() {
    let dir = tempfile::tempdir().unwrap();
    let encrypted = dir.path().join("encrypted.zar");
    archive(
        &encrypted,
        &[("secret.txt", b"secret")],
        Some(StreamEncryption {
            password: b"secret",
            algorithm: ContentAead::ChaCha20Poly1305,
            scope: EncryptionScope::HeaderAndData,
        }),
    );
    let encrypted = encrypted.to_str().unwrap();
    assert!(grep_archive(encrypted, "*", "secret", Default::default()).is_err());
    let passwords = PasswordSet::new(Some(b"secret".to_vec()));
    assert_eq!(
        grep_archive(
            encrypted,
            "*",
            "secret",
            GrepOptions {
                passwords: Some(&passwords),
                ..Default::default()
            },
        )
        .unwrap()
        .len(),
        1
    );

    let input = dir.path().join("history.txt");
    let history = dir.path().join("history.zar");
    std::fs::write(&input, b"old").unwrap();
    create_archive(history.to_str().unwrap(), std::slice::from_ref(&input)).unwrap();
    std::fs::write(&input, b"new").unwrap();
    add_archive(history.to_str().unwrap(), &[input]).unwrap();
    assert!(
        grep_archive(history.to_str().unwrap(), "*", "old", Default::default())
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        grep_archive(
            history.to_str().unwrap(),
            "*",
            "old",
            GrepOptions {
                to: Some("00000001"),
                ..Default::default()
            },
        )
        .unwrap()
        .len(),
        1
    );
    assert!(
        grep_archive(
            history.to_str().unwrap(),
            "*",
            "old",
            GrepOptions {
                to: Some("ffffffff"),
                ..Default::default()
            },
        )
        .is_err()
    );
}
