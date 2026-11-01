use super::*;
use std::io::Cursor;

fn sample_commit_header() -> CommitHeader {
    CommitHeader::new(0, [7; 4], 32, 128).unwrap()
}

#[test]
fn commit_header_round_trips_with_little_endian_fields() {
    let ch = sample_commit_header();
    let bytes = ch.fixed_bytes().unwrap();
    assert_eq!(&bytes[..4], &COMMIT_MAGIC);
    assert_eq!(bytes[4], 0); // framing_version
    assert_eq!(bytes[5], 0); // flags
    assert_eq!(&bytes[6..10], &[7; 4]); // commit_id
    assert_eq!(&bytes[10..18], &32u64.to_le_bytes()); // data_section_size
    assert_eq!(&bytes[18..22], &128u32.to_le_bytes()); // header_frame_size
    assert_eq!(bytes.len(), COMMIT_HEADER_FIXED_SIZE);
    let mut reader = Cursor::new(bytes);
    let decoded = CommitHeader::read_fixed(&mut reader).unwrap();
    assert_eq!(decoded, ch);
}

#[test]
fn commit_header_rejects_unknown_version() {
    let mut ch = sample_commit_header();
    ch.framing_version = 1;
    assert_eq!(ch.validate(), Err(FramingError::UnsupportedVersion(1)));
}

#[test]
fn container_header_round_trips() {
    let ch = ContainerHeader::new(42);
    let bytes = ch.encode();
    assert_eq!(&bytes[..4], b"ZAR0");
    assert_eq!(&bytes[4..12], &42u64.to_le_bytes());
    let decoded = ContainerHeader::decode(&bytes).unwrap();
    assert_eq!(decoded, ch);
    let mut reader = Cursor::new(bytes);
    let from_reader = ContainerHeader::read_from(&mut reader).unwrap();
    assert_eq!(from_reader, ch);
}

#[test]
fn container_magic_is_checked() {
    let mut bytes = Vec::new();
    crate::write_container_magic(&mut bytes).unwrap();
    assert_eq!(bytes, b"ZAR0");
    crate::read_container_magic(&mut Cursor::new(bytes)).unwrap();
    assert_eq!(
        crate::read_container_magic(&mut Cursor::new(b"NOPE")),
        Err(FramingError::BadMagic)
    );
}

#[test]
fn pipeline_detects_hard_links_and_reuses_block_indices() {
    let temp_dir = std::env::temp_dir().join(format!("zar_hardlink_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let original = temp_dir.join("original.bin");
    let hardlink = temp_dir.join("hardlink.bin");
    std::fs::write(&original, b"shared content between hard links").unwrap();
    if std::fs::hard_link(&original, &hardlink).is_ok() {
        let archive_path = temp_dir.join("hardlinks.zar");
        crate::operations::create_archive(
            archive_path.to_str().unwrap(),
            &[original.clone(), hardlink.clone()],
        )
        .unwrap();

        let toc = crate::operations::load_toc_through(archive_path.to_str().unwrap(), None, None)
            .unwrap();
        let orig_entry = match toc.get(&crate::ArchivePath::new("original.bin").unwrap()) {
            Some(crate::TocEntry::File(f)) => f.clone(),
            other => panic!("expected File entry, got {other:?}"),
        };
        let link_entry = match toc.get(&crate::ArchivePath::new("hardlink.bin").unwrap()) {
            Some(crate::TocEntry::File(f)) => f.clone(),
            other => panic!("expected File entry, got {other:?}"),
        };

        assert_eq!(orig_entry.block_indices, link_entry.block_indices);
        assert_eq!(orig_entry.size, link_entry.size);
        assert_eq!(orig_entry.checksum, link_entry.checksum);
    }
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn pipeline_groups_empty_tiny_small_and_big_files_properly() {
    let temp_dir = std::env::temp_dir().join(format!("zar_grouping_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir).unwrap();

    let empty = temp_dir.join("empty.bin");
    let tiny1 = temp_dir.join("tiny1.bin");
    let tiny2 = temp_dir.join("tiny2.bin");
    let small = temp_dir.join("small.bin");
    let big = temp_dir.join("big.bin");

    std::fs::write(&empty, b"").unwrap();
    std::fs::write(&tiny1, b"1234").unwrap();
    std::fs::write(&tiny2, b"1234").unwrap();
    std::fs::write(&small, b"hello small world").unwrap(); // 17 bytes, block size 32 => SmallFile
    std::fs::write(&big, vec![b'B'; 40]).unwrap(); // block size 16 => 3 blocks

    let archive_path = temp_dir.join("groups.zar");
    crate::operations::create_archive_with_encoding(
        archive_path.to_str().unwrap(),
        &[empty, tiny1, tiny2, small, big],
        false,
        crate::operations::EncodingOptions {
            block_size: 32,
            compression_level: 1,
        },
    )
    .unwrap();

    let toc =
        crate::operations::load_toc_through(archive_path.to_str().unwrap(), None, None).unwrap();
    let empty_entry = match toc.get(&crate::ArchivePath::new("empty.bin").unwrap()) {
        Some(crate::TocEntry::File(f)) => f.clone(),
        _ => panic!("missing empty entry"),
    };
    assert_eq!(empty_entry.size, 0);
    assert_eq!(empty_entry.block_indices.len(), 0);
    assert_eq!(empty_entry.checksum, None);

    let tiny1_entry = match toc.get(&crate::ArchivePath::new("tiny1.bin").unwrap()) {
        Some(crate::TocEntry::File(f)) => f.clone(),
        _ => panic!("missing tiny1 entry"),
    };
    let tiny2_entry = match toc.get(&crate::ArchivePath::new("tiny2.bin").unwrap()) {
        Some(crate::TocEntry::File(f)) => f.clone(),
        _ => panic!("missing tiny2 entry"),
    };
    assert_eq!(tiny1_entry.size, 4);
    assert_eq!(tiny1_entry.block_indices.len(), 1);
    assert_eq!(tiny1_entry.checksum, None);
    assert_eq!(tiny1_entry.block_indices, tiny2_entry.block_indices);

    let small_entry = match toc.get(&crate::ArchivePath::new("small.bin").unwrap()) {
        Some(crate::TocEntry::File(f)) => f.clone(),
        _ => panic!("missing small entry"),
    };
    assert_eq!(small_entry.size, 17);
    assert_eq!(small_entry.block_indices.len(), 1);
    assert!(small_entry.checksum.is_some());

    let big_entry = match toc.get(&crate::ArchivePath::new("big.bin").unwrap()) {
        Some(crate::TocEntry::File(f)) => f.clone(),
        _ => panic!("missing big entry"),
    };
    assert_eq!(big_entry.size, 40);
    assert_eq!(big_entry.block_indices.len(), 2); // ceil(40 / 32) = 2 blocks
    assert!(big_entry.checksum.is_some());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn extract_single_file_writes_only_the_requested_file() {
    let temp_dir = std::env::temp_dir().join(format!(
        "zar_extract_single_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let wanted = temp_dir.join("wa[n]ted.bin");
    let other = temp_dir.join("other.bin");
    let expected = (0_u8..200).collect::<Vec<_>>();
    std::fs::write(&wanted, &expected).unwrap();
    std::fs::write(&other, [7_u8; 64]).unwrap();
    let archive_path = temp_dir.join("single.zar");
    let archive = archive_path.to_str().unwrap();
    crate::operations::create_archive_with_encoding(
        archive,
        &[wanted.clone(), other],
        false,
        crate::operations::EncodingOptions {
            block_size: 16,
            compression_level: 1,
        },
    )
    .unwrap();
    let archived = "wa[n]ted.bin";
    let output = temp_dir.join("out.bin");
    let written = crate::operations::extract_single_file(
        archive,
        archived,
        &output,
        &crate::operations::PasswordSet::default(),
    )
    .unwrap();
    assert_eq!(written, 200);
    assert_eq!(std::fs::read(&output).unwrap(), expected);
    assert!(
        crate::operations::extract_single_file(
            archive,
            "missing.bin",
            &output,
            &crate::operations::PasswordSet::default()
        )
        .is_err()
    );
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn extraction_to_memory_returns_file_contents_and_selected_slices() {
    let temp_dir = std::env::temp_dir().join(format!(
        "zar_extract_memory_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let payload = temp_dir.join("payload.bin");
    let empty = temp_dir.join("empty.bin");
    let expected = (0_u8..64).collect::<Vec<_>>();
    std::fs::write(&payload, &expected).unwrap();
    std::fs::write(&empty, []).unwrap();

    let archive_path = temp_dir.join("memory.zar");
    crate::operations::create_archive_with_encoding(
        archive_path.to_str().unwrap(),
        &[payload, empty],
        false,
        crate::operations::EncodingOptions {
            block_size: 16,
            compression_level: 1,
        },
    )
    .unwrap();

    let all_files = crate::operations::extract_archive_to_memory(
        archive_path.to_str().unwrap(),
        None,
        None,
        None,
        0,
    )
    .unwrap();
    assert_eq!(all_files.get("payload.bin"), Some(&expected));
    assert_eq!(all_files.get("empty.bin"), Some(&Vec::new()));

    let partial = crate::operations::extract_archive_to_memory(
        archive_path.to_str().unwrap(),
        None,
        None,
        Some(9),
        13,
    )
    .unwrap();
    assert_eq!(partial.get("payload.bin"), Some(&expected[13..22].to_vec()));
    assert_eq!(partial.get("empty.bin"), Some(&Vec::new()));

    let beyond_end = crate::operations::extract_archive_to_memory(
        archive_path.to_str().unwrap(),
        None,
        None,
        None,
        100,
    )
    .unwrap();
    assert_eq!(beyond_end.get("payload.bin"), Some(&Vec::new()));

    let include_payload = crate::operations::FilterSet::from_patterns(
        &["payload.bin".to_owned()],
        &[],
        &[],
        &[],
        &[] as &[std::path::PathBuf],
    )
    .unwrap();
    let filtered = crate::operations::extract_archive_to_memory_with_filters(
        archive_path.to_str().unwrap(),
        None,
        None,
        Some(0),
        0,
        &include_payload,
    )
    .unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered.get("payload.bin"), Some(&Vec::new()));

    std::fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
fn extraction_to_disk_creates_directories_and_files_properly() {
    let temp_dir = std::env::temp_dir().join(format!(
        "zar-extract-disk-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let src_dir = temp_dir.join("src");
    let dest_dir = temp_dir.join("dest");
    let sub_dir = src_dir.join("nested").join("deep");
    std::fs::create_dir_all(&sub_dir).unwrap();

    let file1 = src_dir.join("file1.txt");
    let file2 = sub_dir.join("file2.txt");
    let empty = sub_dir.join("empty.txt");
    std::fs::write(&file1, b"hello world from file1").unwrap();
    std::fs::write(&file2, b"deep nested file contents").unwrap();
    std::fs::write(&empty, b"").unwrap();

    let archive_path = temp_dir.join("test.zar");
    crate::operations::create_archive(
        archive_path.to_str().unwrap(),
        std::slice::from_ref(&src_dir),
    )
    .unwrap();

    let plan = crate::operations::extract_archive(archive_path.to_str().unwrap(), &dest_dir, false)
        .unwrap();

    assert_eq!(plan.files.len(), 3);
    assert_eq!(
        std::fs::read(dest_dir.join("src").join("file1.txt")).unwrap(),
        b"hello world from file1"
    );
    assert_eq!(
        std::fs::read(
            dest_dir
                .join("src")
                .join("nested")
                .join("deep")
                .join("file2.txt")
        )
        .unwrap(),
        b"deep nested file contents"
    );
    assert_eq!(
        std::fs::read(
            dest_dir
                .join("src")
                .join("nested")
                .join("deep")
                .join("empty.txt")
        )
        .unwrap(),
        b""
    );

    std::fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
fn custom_metadata_can_be_set_removed_and_survives_join() {
    use crate::MetadataType;
    use crate::operations::{EncodingOptions, read_custom_metadata, update_custom_metadata};
    let temp_dir =
        std::env::temp_dir().join(format!("zar_custom_meta_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let file = temp_dir.join("a.txt");
    std::fs::write(&file, b"hello").unwrap();
    let archive_path = temp_dir.join("meta.zar");
    let archive = archive_path.to_str().unwrap();
    let options = || EncodingOptions {
        block_size: 64,
        compression_level: 1,
    };
    crate::operations::create_archive_with_encoding(archive, &[file], false, options()).unwrap();
    assert!(read_custom_metadata(archive).unwrap().is_empty());

    update_custom_metadata(
        archive,
        &[
            ("author".into(), MetadataType::String("me".into())),
            ("rev".into(), MetadataType::I64(3)),
        ],
        &[],
    )
    .unwrap();
    update_custom_metadata(archive, &[("rev".into(), MetadataType::I64(4))], &[]).unwrap();
    update_custom_metadata(archive, &[], &["author".into()]).unwrap();
    let meta = read_custom_metadata(archive).unwrap();
    assert_eq!(meta.len(), 1);
    assert_eq!(meta["rev"], MetadataType::I64(4));

    assert!(update_custom_metadata(archive, &[], &["author".into()]).is_err());
    assert!(
        update_custom_metadata(archive, &[("zar.x".into(), MetadataType::I64(1))], &[]).is_err()
    );

    update_custom_metadata(
        archive,
        &[("author".into(), MetadataType::String("you".into()))],
        &[],
    )
    .unwrap();
    let ids = crate::operations::info_archive(archive, false)
        .unwrap()
        .iter()
        .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
        .collect::<Vec<_>>();
    crate::operations::join_archive_with_encoding(archive, &ids[1], &ids[ids.len() - 1], options())
        .unwrap();
    let meta = read_custom_metadata(archive).unwrap();
    assert_eq!(meta["author"], MetadataType::String("you".into()));
    assert_eq!(meta["rev"], MetadataType::I64(4));
    let _ = std::fs::remove_dir_all(&temp_dir);
}
