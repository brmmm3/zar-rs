use super::*;
use std::io::{Cursor, Write};

fn append_tar_entries<W: Write>(archive: &mut tar::Builder<W>, entries: &[(&str, &[u8])]) {
    for (name, contents) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        archive
            .append_data(&mut header, name, Cursor::new(*contents))
            .unwrap();
    }
}

fn write_tar(path: &std::path::Path, entries: &[(&str, &[u8])], gzip: bool) {
    if gzip {
        let encoder = flate2::write::GzEncoder::new(
            std::fs::File::create(path).unwrap(),
            flate2::Compression::default(),
        );
        let mut archive = tar::Builder::new(encoder);
        append_tar_entries(&mut archive, entries);
        archive.into_inner().unwrap().finish().unwrap();
    } else {
        let mut archive = tar::Builder::new(std::fs::File::create(path).unwrap());
        append_tar_entries(&mut archive, entries);
        archive.into_inner().unwrap();
    }
}

fn write_tar_gz_with_global_pax_header(path: &std::path::Path, entries: &[(&str, &[u8])]) {
    let encoder = flate2::write::GzEncoder::new(
        std::fs::File::create(path).unwrap(),
        flate2::Compression::default(),
    );
    let mut archive = tar::Builder::new(encoder);
    let pax_data = b"11 mtime=1\n";
    let mut header = tar::Header::new_ustar();
    header.set_path("pax_global_header").unwrap();
    header.set_entry_type(tar::EntryType::XGlobalHeader);
    header.set_size(pax_data.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    archive.append(&header, Cursor::new(pax_data)).unwrap();
    append_tar_entries(&mut archive, entries);
    archive.into_inner().unwrap().finish().unwrap();
}

#[test]
fn supported_archive_import_types_are_detected_case_insensitively() {
    for path in [
        "backup.zip",
        "backup.7z",
        "backup.RAR",
        "backup.tar",
        "backup.tar.gz",
        "backup.TGZ",
        "backup.txt.gz",
    ] {
        assert!(crate::operations::is_supported_import_archive(
            std::path::Path::new(path)
        ));
    }
    assert!(!crate::operations::is_supported_import_archive(
        std::path::Path::new("backup.rarx")
    ));
}

fn write_rar(path: &std::path::Path, version: rars::ArchiveVersion, solid: bool) {
    let mut builder = rars::Builder::new(version).solid(solid);
    builder
        .add_directory(b"nested".to_vec(), None, None)
        .unwrap();
    builder
        .add_bytes(
            b"nested/hello.txt".to_vec(),
            b"hello from rar".to_vec(),
            None,
            None,
        )
        .unwrap();
    builder
        .add_bytes(b"empty.txt".to_vec(), Vec::new(), None, None)
        .unwrap();
    builder
        .add_bytes(
            "nested/café.txt".as_bytes().to_vec(),
            b"unicode".to_vec(),
            None,
            None,
        )
        .unwrap();
    std::fs::write(path, builder.to_bytes().unwrap()).unwrap();
}

#[test]
fn rar_imports_create_add_and_extract_compressed_and_solid_archives() {
    for version in [rars::ArchiveVersion::Rar29, rars::ArchiveVersion::Rar50] {
        for solid in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let rar = directory.path().join("input.RAR");
            let archive = directory.path().join("archive.zar");
            let original = directory.path().join("original.txt");
            write_rar(&rar, version, solid);
            crate::operations::create_archive(
                archive.to_str().unwrap(),
                std::slice::from_ref(&rar),
            )
            .unwrap();
            let contents = crate::operations::extract_archive_to_memory(
                archive.to_str().unwrap(),
                None,
                None,
                None,
                0,
            )
            .unwrap();
            assert_eq!(contents["nested/hello.txt"], b"hello from rar");
            assert_eq!(contents["empty.txt"], b"");
            assert_eq!(contents["nested/café.txt"], b"unicode");
            let output = directory.path().join("output");
            crate::operations::extract_archive(rar.to_str().unwrap(), &output, false).unwrap();
            assert_eq!(
                std::fs::read(output.join("nested/hello.txt")).unwrap(),
                b"hello from rar"
            );
            std::fs::write(&original, b"existing").unwrap();
            let added_archive = directory.path().join("added.zar");
            crate::operations::create_archive(
                added_archive.to_str().unwrap(),
                std::slice::from_ref(&original),
            )
            .unwrap();
            crate::operations::add_archive(
                added_archive.to_str().unwrap(),
                std::slice::from_ref(&rar),
            )
            .unwrap();
            let contents = crate::operations::extract_archive_to_memory(
                added_archive.to_str().unwrap(),
                None,
                None,
                None,
                0,
            )
            .unwrap();
            assert_eq!(contents["original.txt"], b"existing");
            assert_eq!(contents["nested/hello.txt"], b"hello from rar");
        }
    }
}

#[test]
fn rar_import_rejects_links_and_incomplete_volumes() {
    let directory = tempfile::tempdir().unwrap();
    for version in [rars::ArchiveVersion::Rar29, rars::ArchiveVersion::Rar50] {
        let rar = directory.path().join("unsafe.rar");
        let target = directory.path().join("unsafe.zar");
        let mut builder = rars::Builder::new(version).store(true);
        builder
            .add_unix_symlink(b"link".to_vec(), b"target".to_vec(), false, None, None)
            .unwrap();
        std::fs::write(&rar, builder.to_bytes().unwrap()).unwrap();
        assert!(
            crate::operations::create_archive(target.to_str().unwrap(), std::slice::from_ref(&rar))
                .is_err()
        );
        assert!(!target.exists());

        let mut builder = rars::Builder::new(version)
            .store(true)
            .volume_size(Some(128));
        builder
            .add_bytes(b"file".to_vec(), vec![42; 256], None, None)
            .unwrap();
        let volumes = builder.build_volumes(None).unwrap();
        assert!(volumes.len() > 1);
        std::fs::write(&rar, &volumes[0]).unwrap();
        let error =
            crate::operations::create_archive(target.to_str().unwrap(), std::slice::from_ref(&rar))
                .unwrap_err();
        assert!(error.to_string().contains("multi-volume RAR"));
        assert!(!target.exists());
    }
}

#[test]
fn rar_import_rejects_encrypted_and_corrupt_entries_without_changing_archive() {
    let directory = tempfile::tempdir().unwrap();
    let rar = directory.path().join("bad.rar");
    let original = directory.path().join("original.txt");
    let archive = directory.path().join("archive.zar");
    std::fs::write(&original, b"existing").unwrap();
    crate::operations::create_archive(archive.to_str().unwrap(), std::slice::from_ref(&original))
        .unwrap();
    let before = std::fs::read(&archive).unwrap();
    for version in [rars::ArchiveVersion::Rar29, rars::ArchiveVersion::Rar50] {
        let mut builder = rars::Builder::new(version).store(true);
        builder
            .add_bytes(b"file".to_vec(), b"payload text".to_vec(), None, None)
            .unwrap();
        builder
            .set_entry_encryption(b"file", Some(b"secret".to_vec()), None)
            .unwrap();
        std::fs::write(&rar, builder.to_bytes().unwrap()).unwrap();
        let error =
            crate::operations::add_archive(archive.to_str().unwrap(), std::slice::from_ref(&rar))
                .unwrap_err();
        assert!(error.to_string().contains("encrypted RAR"));
        assert_eq!(std::fs::read(&archive).unwrap(), before);

        let mut builder = rars::Builder::new(version).store(true);
        builder
            .add_bytes(b"file".to_vec(), b"payload text".to_vec(), None, None)
            .unwrap();
        let mut bytes = builder.to_bytes().unwrap();
        let offset = bytes
            .windows(b"payload text".len())
            .position(|window| window == b"payload text")
            .unwrap();
        bytes[offset] ^= 1;
        std::fs::write(&rar, bytes).unwrap();
        assert!(
            crate::operations::add_archive(archive.to_str().unwrap(), std::slice::from_ref(&rar))
                .is_err()
        );
        assert_eq!(std::fs::read(&archive).unwrap(), before);
    }
}

#[test]
fn archive_operations_reject_older_logical_header_versions() {
    let directory = tempfile::tempdir().unwrap();
    for version in 0u16..=8 {
        let archive = directory.path().join(format!("version-{version}.zar"));
        let mut logical = b"ZHDR".to_vec();
        logical.extend_from_slice(&version.to_le_bytes());
        if matches!(version, 1 | 3) {
            logical.extend_from_slice(&65536u32.to_le_bytes());
        } else if version >= 4 {
            logical.extend_from_slice(&0u32.to_le_bytes());
            logical.extend_from_slice(&0u32.to_le_bytes());
            if version >= 5 {
                logical.extend_from_slice(&0u32.to_le_bytes());
            }
        }
        logical.extend_from_slice(&0u32.to_le_bytes());
        let staged = HeaderFrame::from_uncompressed(
            CommitHeader::new(0, [1, 0, 0, 0], 0, 0).unwrap(),
            &logical,
            UNENCRYPTED_DESCRIPTOR.to_vec(),
            3,
        )
        .unwrap();
        let frame = HeaderFrame::from_uncompressed(
            CommitHeader::new(
                0,
                [1, 0, 0, 0],
                0,
                u32::try_from(staged.encoded_size().unwrap()).unwrap(),
            )
            .unwrap(),
            &logical,
            UNENCRYPTED_DESCRIPTOR.to_vec(),
            3,
        )
        .unwrap();
        let offset = CONTAINER_HEADER_SIZE + HEADER_OFFSET_SIZE;
        let mut output = std::fs::File::create(&archive).unwrap();
        ContainerHeader::new(offset).write_to(&mut output).unwrap();
        output.write_all(&0u64.to_le_bytes()).unwrap();
        frame.write_to(&mut output).unwrap();
        output.write_all(&offset.to_le_bytes()).unwrap();
        drop(output);

        let archive = archive.to_str().unwrap();
        let listing = crate::operations::list_archive(archive, None, None);
        let info = crate::operations::info_archive(archive, false);
        let check = crate::operations::check_archive_with_recovery(archive, false, None);
        let extracted = crate::operations::extract_archive_to_memory(archive, None, None, None, 0);
        if version == 8 {
            assert!(listing.unwrap().is_empty());
            assert_eq!(info.unwrap().len(), 1);
            assert_eq!(check.unwrap().commits, 1);
            assert!(extracted.unwrap().is_empty());
        } else {
            let expected = format!("unsupported logical-header version {version}");
            for error in [
                listing.unwrap_err().to_string(),
                info.unwrap_err().to_string(),
                check.unwrap_err().to_string(),
                extracted.unwrap_err().to_string(),
            ] {
                assert!(error.contains(&expected), "{error}");
            }
        }
    }
}

#[test]
fn commits_record_creation_time_and_optional_user() {
    let directory = tempfile::tempdir().unwrap();
    let first_input = directory.path().join("first.txt");
    let second_input = directory.path().join("second.txt");
    std::fs::write(&first_input, b"first").unwrap();
    std::fs::write(&second_input, b"second").unwrap();
    let archive = directory.path().join("metadata.zar");
    let archive_name = archive.to_str().unwrap();

    crate::operations::create_archive_with_password_progress_and_special_files(
        archive_name,
        std::slice::from_ref(&first_input),
        false,
        crate::operations::EncodingOptions::default(),
        None,
        None,
        crate::operations::CreateInputOptions::default(),
    )
    .unwrap();
    let initial_commit = crate::operations::info_archive(archive_name, false).unwrap()[0].clone();
    assert!(initial_commit.created_unix_millis.is_some());
    assert!(initial_commit.user.is_none());

    let username = crate::operations::common::current_username().unwrap();
    crate::operations::add_archive_with_encoding_progress_and_options(
        archive_name,
        std::slice::from_ref(&second_input),
        crate::operations::EncodingOptions::default(),
        None,
        crate::operations::AddInputOptions {
            save_user: true,
            ..crate::operations::AddInputOptions::default()
        },
    )
    .unwrap();
    let commits = crate::operations::info_archive(archive_name, false).unwrap();
    assert_eq!(commits.len(), 2);
    assert!(commits[1].created_unix_millis.is_some());
    assert_eq!(commits[1].user.as_deref(), Some(username.as_str()));
}

#[test]
fn add_reuses_an_empty_root_commit() {
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("empty.zar");
    let input = directory.path().join("added.txt");
    std::fs::write(&input, b"added").unwrap();
    crate::operations::create_archive(archive.to_str().unwrap(), &[]).unwrap();
    let original_id =
        crate::operations::info_archive(archive.to_str().unwrap(), false).unwrap()[0].id;

    crate::operations::add_archive(archive.to_str().unwrap(), &[input]).unwrap();

    let commits = crate::operations::info_archive(archive.to_str().unwrap(), false).unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].id, original_id);
    assert_eq!(
        crate::operations::list_archive(archive.to_str().unwrap(), None, None)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn add_reuses_an_empty_non_root_tail_commit() {
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("archive.zar");
    let existing = directory.path().join("existing.txt");
    let added = directory.path().join("added.txt");
    std::fs::write(&existing, b"existing").unwrap();
    std::fs::write(&added, b"added").unwrap();
    crate::operations::create_archive(archive.to_str().unwrap(), std::slice::from_ref(&existing))
        .unwrap();
    crate::operations::remove_archive_entries_with_progress(archive.to_str().unwrap(), &[], None)
        .unwrap();
    let original_commits =
        crate::operations::info_archive(archive.to_str().unwrap(), false).unwrap();
    let original_tail_id = original_commits[1].id;

    crate::operations::add_archive(archive.to_str().unwrap(), &[added]).unwrap();

    let commits = crate::operations::info_archive(archive.to_str().unwrap(), false).unwrap();
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[1].id, original_tail_id);
    assert_eq!(
        crate::operations::list_archive(archive.to_str().unwrap(), None, None)
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn remove_reuses_an_empty_tail_commit() {
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("archive.zar");
    let input = directory.path().join("existing.txt");
    std::fs::write(&input, b"existing").unwrap();
    crate::operations::create_archive(archive.to_str().unwrap(), std::slice::from_ref(&input))
        .unwrap();
    crate::operations::remove_archive_entries_with_progress(archive.to_str().unwrap(), &[], None)
        .unwrap();
    let original_commits =
        crate::operations::info_archive(archive.to_str().unwrap(), false).unwrap();
    let original_tail_id = original_commits[1].id;
    let path = crate::ArchivePath::new("existing.txt").unwrap();

    crate::operations::remove_archive_entries_with_progress(
        archive.to_str().unwrap(),
        &[path],
        None,
    )
    .unwrap();

    let commits = crate::operations::info_archive(archive.to_str().unwrap(), false).unwrap();
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[1].id, original_tail_id);
    assert!(
        crate::operations::list_archive(archive.to_str().unwrap(), None, None)
            .unwrap()
            .is_empty()
    );
}

fn write_tar_with_raw_path(path: &std::path::Path, entry_name: &str, contents: &[u8]) {
    let mut archive = tar::Builder::new(std::fs::File::create(path).unwrap());
    let mut header = tar::Header::new_gnu();
    let path_bytes = entry_name.as_bytes();
    assert!(path_bytes.len() < 100);
    header.as_mut_bytes()[..100].fill(0);
    header.as_mut_bytes()[..path_bytes.len()].copy_from_slice(path_bytes);
    header.set_size(contents.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    archive.append(&header, Cursor::new(contents)).unwrap();
    archive.into_inner().unwrap();
}

fn write_tar_symlink(path: &std::path::Path, entry_name: &str, target: &str) {
    let mut archive = tar::Builder::new(std::fs::File::create(path).unwrap());
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Symlink);
    header.set_size(0);
    header.set_mode(0o777);
    header.set_link_name(target).unwrap();
    archive
        .append_data(&mut header, entry_name, Cursor::new([]))
        .unwrap();
    archive.into_inner().unwrap();
}

fn write_tar_with_hard_link(path: &std::path::Path, target: &str, link_name: &str) {
    let mut archive = tar::Builder::new(std::fs::File::create(path).unwrap());
    append_tar_entries(&mut archive, &[(target, b"hard-linked contents")]);
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Link);
    header.set_size(0);
    header.set_mode(0o644);
    header.set_link_name(target).unwrap();
    archive
        .append_data(&mut header, link_name, Cursor::new([]))
        .unwrap();
    archive.into_inner().unwrap();
}

fn write_zip(path: &std::path::Path, entries: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, contents) in entries {
        archive.start_file(name, options).unwrap();
        archive.write_all(contents).unwrap();
    }
    archive.finish().unwrap();
}

fn write_gzip(path: &std::path::Path, contents: &[u8]) {
    let mut encoder = flate2::write::GzEncoder::new(
        std::fs::File::create(path).unwrap(),
        flate2::Compression::default(),
    );
    encoder.write_all(contents).unwrap();
    encoder.finish().unwrap();
}

fn write_7z(path: &std::path::Path, entries: &[(&str, &[u8])]) {
    let mut archive =
        sevenz_rust2::ArchiveWriter::new(std::fs::File::create(path).unwrap()).unwrap();
    for (name, contents) in entries {
        archive
            .push_archive_entry(sevenz_rust2::ArchiveEntry::new_file(name), Some(*contents))
            .unwrap();
    }
    archive.finish().unwrap();
}

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
    let small2 = temp_dir.join("small2.bin");
    let small3 = temp_dir.join("small3.bin");
    let big = temp_dir.join("big.bin");

    std::fs::write(&empty, b"").unwrap();
    std::fs::write(&tiny1, b"1234").unwrap();
    std::fs::write(&tiny2, b"1234").unwrap();
    std::fs::write(&small, b"hello small world").unwrap(); // 17 bytes, block size 32 => SmallFile
    std::fs::write(&small2, b"second 99").unwrap();
    std::fs::write(&small3, b"another small one").unwrap();
    std::fs::write(&big, vec![b'B'; 40]).unwrap(); // block size 32 => 2 blocks

    let archive_path = temp_dir.join("groups.zar");
    crate::operations::create_archive_with_encoding(
        archive_path.to_str().unwrap(),
        &[empty, tiny1, tiny2, small, small2, small3, big],
        false,
        crate::operations::EncodingOptions {
            block_size: 32,
            compression_level: 1,
        },
    )
    .unwrap();

    let check_report =
        crate::operations::check_archive_with_recovery(archive_path.to_str().unwrap(), false, None)
            .unwrap();
    assert!(check_report.logical_state_readable);

    let detailed_info =
        crate::operations::info_archive_with_blocks(archive_path.to_str().unwrap(), true).unwrap();
    let blocks = &detailed_info[0].blocks;
    assert_eq!(blocks.len(), 5);
    for group in ["tiny files", "small files", "big files"] {
        assert!(
            blocks
                .iter()
                .any(|block| block.file_groups.contains(&group))
        );
    }
    assert!(
        blocks
            .iter()
            .all(|block| { block.compressed_size > 0 && block.ratio_percent.is_some() })
    );

    let toc =
        crate::operations::load_toc_through(archive_path.to_str().unwrap(), None, None).unwrap();
    let tiny_block_index = match toc.get(&crate::ArchivePath::new("tiny1.bin").unwrap()) {
        Some(crate::TocEntry::File(file)) => file.block_indices[0],
        _ => panic!("missing tiny1 entry"),
    };
    let small_block_index = match toc.get(&crate::ArchivePath::new("small.bin").unwrap()) {
        Some(crate::TocEntry::File(file)) => file.block_indices[0],
        _ => panic!("missing small entry"),
    };
    assert_eq!(
        blocks
            .iter()
            .find(|block| block.index == tiny_block_index)
            .unwrap()
            .file_count,
        2
    );
    assert_eq!(
        blocks
            .iter()
            .find(|block| block.index == small_block_index)
            .unwrap()
            .file_count,
        2
    );
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
    assert!(small_entry.offset_in_block.is_some());
    assert!(small_entry.checksum.is_some());

    let small2_entry = match toc.get(&crate::ArchivePath::new("small2.bin").unwrap()) {
        Some(crate::TocEntry::File(f)) => f.clone(),
        _ => panic!("missing second small entry"),
    };
    assert_eq!(small2_entry.size, 9);
    assert!(small2_entry.offset_in_block.is_some());

    let small3_entry = match toc.get(&crate::ArchivePath::new("small3.bin").unwrap()) {
        Some(crate::TocEntry::File(f)) => f.clone(),
        _ => panic!("missing third small entry"),
    };
    assert_eq!(small3_entry.size, 17);
    assert!(small3_entry.offset_in_block.is_some());
    let small_block_indices = [&small_entry, &small2_entry, &small3_entry]
        .into_iter()
        .map(|entry| entry.block_indices[0])
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(small_block_indices.len(), 2);

    let big_entry = match toc.get(&crate::ArchivePath::new("big.bin").unwrap()) {
        Some(crate::TocEntry::File(f)) => f.clone(),
        _ => panic!("missing big entry"),
    };
    assert_eq!(big_entry.size, 40);
    assert_eq!(big_entry.block_indices.len(), 2); // ceil(40 / 32) = 2 blocks
    assert!(big_entry.checksum.is_some());

    let extracted = crate::operations::extract_archive_to_memory(
        archive_path.to_str().unwrap(),
        None,
        None,
        None,
        0,
    )
    .unwrap();
    assert_eq!(extracted.get("small.bin").unwrap(), b"hello small world");
    assert_eq!(extracted.get("small2.bin").unwrap(), b"second 99");
    assert_eq!(extracted.get("small3.bin").unwrap(), b"another small one");
    let disk_output = temp_dir.join("extracted");
    crate::operations::extract_archive(archive_path.to_str().unwrap(), &disk_output, false)
        .unwrap();
    for (name, expected) in &extracted {
        assert_eq!(std::fs::read(disk_output.join(name)).unwrap(), *expected);
    }
    let partial = crate::operations::extract_archive_to_memory(
        archive_path.to_str().unwrap(),
        None,
        None,
        Some(4),
        3,
    )
    .unwrap();
    assert_eq!(partial.get("small.bin").unwrap(), b"lo s");
    assert_eq!(partial.get("small2.bin").unwrap(), b"ond ");
    assert_eq!(partial.get("small3.bin").unwrap(), b"ther");

    let single_output = temp_dir.join("single-small2.bin");
    crate::operations::extract_single_file(
        archive_path.to_str().unwrap(),
        "small2.bin",
        &single_output,
        &crate::operations::PasswordSet::default(),
    )
    .unwrap();
    assert_eq!(std::fs::read(single_output).unwrap(), b"second 99");

    crate::operations::update_custom_metadata(
        archive_path.to_str().unwrap(),
        &[(
            "packing-test".to_owned(),
            crate::MetadataType::String("preserve".to_owned()),
        )],
        &[],
    )
    .unwrap();
    let commit_ids = crate::operations::info_archive(archive_path.to_str().unwrap(), false)
        .unwrap()
        .iter()
        .map(|commit| format!("{:08x}", u32::from_le_bytes(commit.id)))
        .collect::<Vec<_>>();
    crate::operations::join_archive_with_encoding(
        archive_path.to_str().unwrap(),
        &commit_ids[0],
        commit_ids.last().unwrap(),
        crate::operations::EncodingOptions {
            block_size: 32,
            compression_level: 1,
        },
    )
    .unwrap();
    let joined = crate::operations::extract_archive_to_memory(
        archive_path.to_str().unwrap(),
        None,
        None,
        None,
        0,
    )
    .unwrap();
    assert_eq!(joined.get("small.bin").unwrap(), b"hello small world");
    assert_eq!(joined.get("small2.bin").unwrap(), b"second 99");
    assert_eq!(joined.get("small3.bin").unwrap(), b"another small one");

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
fn create_archive_imports_zip_entries_and_preserves_their_paths() {
    let directory = tempfile::tempdir().unwrap();
    let zip_path = directory.path().join("source.zip");
    let archive_path = directory.path().join("converted.zar");
    write_zip(
        &zip_path,
        &[("nested/hello.txt", b"hello from zip"), ("empty.txt", b"")],
    );

    crate::operations::create_archive(
        archive_path.to_str().unwrap(),
        std::slice::from_ref(&zip_path),
    )
    .unwrap();

    let contents = crate::operations::extract_archive_to_memory(
        archive_path.to_str().unwrap(),
        None,
        None,
        None,
        0,
    )
    .unwrap();
    assert_eq!(contents.get("nested/hello.txt").unwrap(), b"hello from zip");
    assert_eq!(contents.get("empty.txt").unwrap(), b"");
}

#[test]
fn add_archive_imports_zip_entries() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("original.txt");
    let archive_path = directory.path().join("archive.zar");
    let zip_path = directory.path().join("additional.zip");
    std::fs::write(&source_path, b"existing").unwrap();
    write_zip(&zip_path, &[("added/file.txt", b"new contents")]);

    crate::operations::create_archive(
        archive_path.to_str().unwrap(),
        std::slice::from_ref(&source_path),
    )
    .unwrap();
    crate::operations::add_archive(
        archive_path.to_str().unwrap(),
        std::slice::from_ref(&zip_path),
    )
    .unwrap();

    let contents = crate::operations::extract_archive_to_memory(
        archive_path.to_str().unwrap(),
        None,
        None,
        None,
        0,
    )
    .unwrap();
    assert_eq!(contents.get("original.txt").unwrap(), b"existing");
    assert_eq!(contents.get("added/file.txt").unwrap(), b"new contents");
}

#[test]
fn extract_zip_writes_files_in_parallel_and_preserves_paths() {
    let directory = tempfile::tempdir().unwrap();
    let zip_path = directory.path().join("source.zip");
    let output = directory.path().join("output");
    write_zip(
        &zip_path,
        &[("one/a.txt", b"first"), ("two/b.txt", b"second")],
    );

    crate::operations::extract_archive(zip_path.to_str().unwrap(), &output, false).unwrap();

    assert_eq!(std::fs::read(output.join("one/a.txt")).unwrap(), b"first");
    assert_eq!(std::fs::read(output.join("two/b.txt")).unwrap(), b"second");
}

#[test]
fn extract_7z_writes_files_in_parallel() {
    let directory = tempfile::tempdir().unwrap();
    let sevenz_path = directory.path().join("source.7z");
    let output = directory.path().join("output");
    write_7z(
        &sevenz_path,
        &[("one.txt", b"first"), ("two.txt", b"second")],
    );

    crate::operations::extract_archive(sevenz_path.to_str().unwrap(), &output, false).unwrap();

    assert_eq!(std::fs::read(output.join("one.txt")).unwrap(), b"first");
    assert_eq!(std::fs::read(output.join("two.txt")).unwrap(), b"second");
}

#[test]
fn extract_tar_and_tar_gz_write_entries() {
    let directory = tempfile::tempdir().unwrap();
    let tar_path = directory.path().join("source.tar");
    let tar_gz_path = directory.path().join("source.tar.gz");
    let tar_output = directory.path().join("tar-output");
    let tar_gz_output = directory.path().join("tar-gz-output");
    let entries = [
        ("one/a.txt", b"first".as_slice()),
        ("two/b.txt", b"second".as_slice()),
    ];
    write_tar(&tar_path, &entries, false);
    write_tar(&tar_gz_path, &entries, true);

    crate::operations::extract_archive(tar_path.to_str().unwrap(), &tar_output, false).unwrap();
    crate::operations::extract_archive(tar_gz_path.to_str().unwrap(), &tar_gz_output, false)
        .unwrap();

    assert_eq!(
        std::fs::read(tar_output.join("one/a.txt")).unwrap(),
        b"first"
    );
    assert_eq!(
        std::fs::read(tar_gz_output.join("two/b.txt")).unwrap(),
        b"second"
    );
}

#[test]
fn extract_standalone_gzip_strips_gz_extension() {
    let directory = tempfile::tempdir().unwrap();
    let gzip_path = directory.path().join("payload.txt.gz");
    let output = directory.path().join("output");
    write_gzip(&gzip_path, b"gzip payload");

    crate::operations::extract_archive(gzip_path.to_str().unwrap(), &output, false).unwrap();

    assert_eq!(
        std::fs::read(output.join("payload.txt")).unwrap(),
        b"gzip payload"
    );
}

#[test]
fn create_archive_rejects_zip_paths_that_escape_the_import_directory() {
    let directory = tempfile::tempdir().unwrap();
    let zip_path = directory.path().join("unsafe.zip");
    let archive_path = directory.path().join("unsafe.zar");
    write_zip(&zip_path, &[("../escaped.txt", b"outside")]);

    let error = crate::operations::create_archive(
        archive_path.to_str().unwrap(),
        std::slice::from_ref(&zip_path),
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid ZIP entry path"));
    assert!(!directory.path().join("escaped.txt").exists());
    assert!(!archive_path.exists());
}

#[test]
fn create_archive_imports_7z_entries_and_preserves_their_paths() {
    let directory = tempfile::tempdir().unwrap();
    let sevenz_path = directory.path().join("source.7Z");
    let archive_path = directory.path().join("converted.zar");
    write_7z(
        &sevenz_path,
        &[("nested/hello.txt", b"hello from 7z"), ("empty.txt", b"")],
    );

    crate::operations::create_archive(
        archive_path.to_str().unwrap(),
        std::slice::from_ref(&sevenz_path),
    )
    .unwrap();

    let contents = crate::operations::extract_archive_to_memory(
        archive_path.to_str().unwrap(),
        None,
        None,
        None,
        0,
    )
    .unwrap();
    assert_eq!(contents.get("nested/hello.txt").unwrap(), b"hello from 7z");
    assert_eq!(contents.get("empty.txt").unwrap(), b"");
}

#[test]
fn add_archive_imports_7z_entries() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("original.txt");
    let archive_path = directory.path().join("archive.zar");
    let sevenz_path = directory.path().join("additional.7z");
    std::fs::write(&source_path, b"existing").unwrap();
    write_7z(&sevenz_path, &[("added/file.txt", b"new contents")]);

    crate::operations::create_archive(
        archive_path.to_str().unwrap(),
        std::slice::from_ref(&source_path),
    )
    .unwrap();
    crate::operations::add_archive(
        archive_path.to_str().unwrap(),
        std::slice::from_ref(&sevenz_path),
    )
    .unwrap();

    let contents = crate::operations::extract_archive_to_memory(
        archive_path.to_str().unwrap(),
        None,
        None,
        None,
        0,
    )
    .unwrap();
    assert_eq!(contents.get("original.txt").unwrap(), b"existing");
    assert_eq!(contents.get("added/file.txt").unwrap(), b"new contents");
}

#[test]
fn create_archive_rejects_7z_paths_that_escape_the_import_directory() {
    let directory = tempfile::tempdir().unwrap();
    let sevenz_path = directory.path().join("unsafe.7z");
    let archive_path = directory.path().join("unsafe.zar");
    write_7z(&sevenz_path, &[("../escaped.txt", b"outside")]);

    let error = crate::operations::create_archive(
        archive_path.to_str().unwrap(),
        std::slice::from_ref(&sevenz_path),
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid 7z entry path"));
    assert!(!directory.path().join("escaped.txt").exists());
    assert!(!archive_path.exists());
}

#[test]
fn create_archive_imports_tar_entries_and_normalizes_dot_paths() {
    let directory = tempfile::tempdir().unwrap();
    let tar_path = directory.path().join("source.tar");
    let archive_path = directory.path().join("converted.zar");
    write_tar(
        &tar_path,
        &[
            ("./nested/hello.txt", b"hello from tar"),
            ("empty.txt", b""),
        ],
        false,
    );

    crate::operations::create_archive(
        archive_path.to_str().unwrap(),
        std::slice::from_ref(&tar_path),
    )
    .unwrap();

    let contents = crate::operations::extract_archive_to_memory(
        archive_path.to_str().unwrap(),
        None,
        None,
        None,
        0,
    )
    .unwrap();
    assert_eq!(contents.get("nested/hello.txt").unwrap(), b"hello from tar");
    assert_eq!(contents.get("empty.txt").unwrap(), b"");
}

#[test]
fn add_archive_imports_tar_gz_entries() {
    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("original.txt");
    let archive_path = directory.path().join("archive.zar");
    let tar_gz_path = directory.path().join("additional.TAR.GZ");
    std::fs::write(&source_path, b"existing").unwrap();
    write_tar(&tar_gz_path, &[("added/file.txt", b"new contents")], true);

    crate::operations::create_archive(
        archive_path.to_str().unwrap(),
        std::slice::from_ref(&source_path),
    )
    .unwrap();
    crate::operations::add_archive(
        archive_path.to_str().unwrap(),
        std::slice::from_ref(&tar_gz_path),
    )
    .unwrap();

    let contents = crate::operations::extract_archive_to_memory(
        archive_path.to_str().unwrap(),
        None,
        None,
        None,
        0,
    )
    .unwrap();
    assert_eq!(contents.get("original.txt").unwrap(), b"existing");
    assert_eq!(contents.get("added/file.txt").unwrap(), b"new contents");
}

#[test]
fn create_archive_imports_tgz_entries() {
    let directory = tempfile::tempdir().unwrap();
    let tar_gz_path = directory.path().join("source.TGZ");
    let archive_path = directory.path().join("converted.zar");
    write_tar(&tar_gz_path, &[("nested/file.txt", b"from tgz")], true);

    crate::operations::create_archive(
        archive_path.to_str().unwrap(),
        std::slice::from_ref(&tar_gz_path),
    )
    .unwrap();

    let contents = crate::operations::extract_archive_to_memory(
        archive_path.to_str().unwrap(),
        None,
        None,
        None,
        0,
    )
    .unwrap();
    assert_eq!(contents.get("nested/file.txt").unwrap(), b"from tgz");
}

#[test]
fn create_archive_imports_tar_gz_with_global_pax_header() {
    let directory = tempfile::tempdir().unwrap();
    let tar_gz_path = directory.path().join("source.tar.gz");
    let archive_path = directory.path().join("converted.zar");
    write_tar_gz_with_global_pax_header(&tar_gz_path, &[("nested/file.txt", b"from pax tar")]);

    crate::operations::create_archive(
        archive_path.to_str().unwrap(),
        std::slice::from_ref(&tar_gz_path),
    )
    .unwrap();

    let contents = crate::operations::extract_archive_to_memory(
        archive_path.to_str().unwrap(),
        None,
        None,
        None,
        0,
    )
    .unwrap();
    assert_eq!(contents.get("nested/file.txt").unwrap(), b"from pax tar");
}

#[test]
fn create_archive_rejects_tar_paths_that_escape_the_import_directory() {
    let directory = tempfile::tempdir().unwrap();
    let tar_path = directory.path().join("unsafe.tar");
    let archive_path = directory.path().join("unsafe.zar");
    write_tar_with_raw_path(&tar_path, "../escaped.txt", b"outside");

    let error = crate::operations::create_archive(
        archive_path.to_str().unwrap(),
        std::slice::from_ref(&tar_path),
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid TAR entry path"));
    assert!(!directory.path().join("escaped.txt").exists());
    assert!(!archive_path.exists());
}

#[test]
fn create_archive_imports_tar_symbolic_links_without_following_targets() {
    let directory = tempfile::tempdir().unwrap();
    let tar_path = directory.path().join("link.tar");
    let archive_path = directory.path().join("link.zar");
    write_tar_symlink(&tar_path, "linked.txt", "../outside.txt");

    crate::operations::create_archive(
        archive_path.to_str().unwrap(),
        std::slice::from_ref(&tar_path),
    )
    .unwrap();
    let toc = crate::operations::list_archive(archive_path.to_str().unwrap(), None, None).unwrap();
    assert!(matches!(
        toc.get(&ArchivePath::new("linked.txt").unwrap()),
        Some(TocEntry::Symlink { target }) if target == "../outside.txt"
    ));
    assert!(!directory.path().join("outside.txt").exists());
}

#[test]
fn create_archive_rejects_tar_entries_nested_under_symbolic_links() {
    let directory = tempfile::tempdir().unwrap();
    let tar_path = directory.path().join("link-parent.tar");
    let archive_path = directory.path().join("link-parent.zar");
    let mut archive = tar::Builder::new(std::fs::File::create(&tar_path).unwrap());
    let mut link_header = tar::Header::new_gnu();
    link_header.set_entry_type(tar::EntryType::Symlink);
    link_header.set_size(0);
    link_header.set_mode(0o777);
    link_header.set_link_name("../outside").unwrap();
    archive
        .append_data(&mut link_header, "redirect", Cursor::new([]))
        .unwrap();
    append_tar_entries(&mut archive, &[("redirect/escape.txt", b"outside")]);
    archive.into_inner().unwrap();

    assert!(
        crate::operations::create_archive(
            archive_path.to_str().unwrap(),
            std::slice::from_ref(&tar_path),
        )
        .is_err()
    );
    assert!(!directory.path().join("outside/escape.txt").exists());
    assert!(!archive_path.exists());
}

#[test]
fn create_archive_imports_tar_hard_links_as_files() {
    let directory = tempfile::tempdir().unwrap();
    let tar_path = directory.path().join("hard-link.tar");
    let archive_path = directory.path().join("hard-link.zar");
    write_tar_with_hard_link(&tar_path, "target.txt", "linked.txt");

    crate::operations::create_archive(
        archive_path.to_str().unwrap(),
        std::slice::from_ref(&tar_path),
    )
    .unwrap();

    let contents = crate::operations::extract_archive_to_memory(
        archive_path.to_str().unwrap(),
        None,
        None,
        None,
        0,
    )
    .unwrap();
    assert_eq!(contents.get("target.txt").unwrap(), b"hard-linked contents");
    assert_eq!(contents.get("linked.txt").unwrap(), b"hard-linked contents");
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
