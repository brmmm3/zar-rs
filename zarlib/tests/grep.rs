use std::io::Cursor;
use std::path::Path;

use zarlib::operations::{
    EncodingOptions, GrepOptions, PasswordSet, StreamEncryption, StreamInput, add_archive,
    create_archive, create_archive_from_readers, grep_archive,
};
use zarlib::{ContentAead, EncryptionScope};

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
        r"(?-u:\xFF)match|^$|end",
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
