use super::*;
use std::{
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::symlink,
};
use zarlib::operations::{
    AddInputOptions, EncodingOptions, FilterSet, add_archive,
    add_archive_with_encoding_progress_and_filters_and_encryption, create_archive_with_encoding,
    create_archive_with_password,
};
use zarlib::{ContentAead, EncryptionScope};

fn fixture() -> (tempfile::TempDir, PathBuf, Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    fs::create_dir_all(input.join("nested")).unwrap();
    let content: Vec<u8> = (0..160_000).map(|i| (i % 251) as u8).collect();
    fs::write(input.join("nested/data.bin"), &content).unwrap();
    fs::write(input.join("small.txt"), b"small").unwrap();
    fs::write(input.join("empty.txt"), b"").unwrap();
    symlink("small.txt", input.join("link")).unwrap();
    let archive = dir.path().join("test.zar");
    create_archive_with_encoding(
        archive.to_str().unwrap(),
        &[input],
        false,
        EncodingOptions {
            block_size: 65536,
            compression_level: 3,
        },
    )
    .unwrap();
    (dir, archive, content)
}

fn prepared(path: &Path, to: Option<&str>) -> PreparedExtraction {
    PreparedExtraction::new_complete(path.to_str().unwrap(), to, &PasswordSet::default()).unwrap()
}

#[test]
fn maps_directories_symlinks_and_reads_file_ranges() {
    let (_dir, archive, content) = fixture();
    let fs = ArchiveFs::new(prepared(&archive, None)).unwrap();
    let root = fs.node(INodeNo(ROOT)).unwrap();
    assert_eq!(root.attr.kind, FileType::Directory);
    assert_eq!(root.attr.perm, 0o555);
    assert_eq!(root.attr.nlink, 3);
    let input = fs.child(INodeNo(ROOT), OsStr::new("input")).unwrap();
    let nested = fs.child(input.attr.ino, OsStr::new("nested")).unwrap();
    let data = fs.child(nested.attr.ino, OsStr::new("data.bin")).unwrap();
    assert_eq!(data.attr.size, content.len() as u64);
    assert_eq!(data.attr.perm, 0o444);
    let link = fs.child(input.attr.ino, OsStr::new("link")).unwrap();
    assert_eq!(link.target.as_deref(), Some("small.txt"));
    assert!(fs.child(input.attr.ino, OsStr::new("missing")).is_none());
    assert_eq!(
        fs.archive.read_file_range(&data.path, 65530, 30).unwrap(),
        content[65530..65560]
    );
    assert_eq!(
        fs.archive.read_file_range(&data.path, 159_990, 50).unwrap(),
        content[159990..]
    );
    assert!(
        fs.archive
            .read_file_range(&data.path, 200_000, 10)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        fs.archive.read_file_range("input/small.txt", 1, 3).unwrap(),
        b"mal"
    );
    assert!(
        fs.archive
            .read_file_range("input/empty.txt", 0, 100)
            .unwrap()
            .is_empty()
    );
    assert!(fs.archive.read_file_range("input/nested", 0, 1).is_err());
}

#[test]
fn selects_history_and_rejects_archive_replacement() {
    let (dir, archive, content) = fixture();
    fs::write(dir.path().join("input/nested/data.bin"), b"changed").unwrap();
    add_archive(archive.to_str().unwrap(), &[dir.path().join("input")]).unwrap();
    let old = prepared(&archive, Some("00000001"));
    let latest = prepared(&archive, None);
    assert_eq!(
        old.read_file_range("input/nested/data.bin", 0, 7).unwrap(),
        &content[..7]
    );
    assert_eq!(
        latest
            .read_file_range("input/nested/data.bin", 0, 100)
            .unwrap(),
        b"changed"
    );
    assert_eq!(
        latest.read_file_range("input/small.txt", 0, 10).unwrap(),
        b"small"
    );
    let replacement = dir.path().join("replacement.zar");
    let modified = fs::metadata(&archive).unwrap().modified().unwrap();
    fs::copy(&archive, &replacement).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&replacement)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    fs::rename(replacement, &archive).unwrap();
    assert!(
        latest
            .read_file_range("input/nested/data.bin", 0, 1)
            .unwrap_err()
            .to_string()
            .contains("changed")
    );
}

#[test]
fn locked_headers_and_wrong_password_are_not_partial_mounts() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("secret.txt");
    fs::write(&input, b"secret").unwrap();
    let archive = dir.path().join("secret.zar");
    create_archive_with_password(
        archive.to_str().unwrap(),
        &[input],
        false,
        EncodingOptions::default(),
        Some(b"secret"),
    )
    .unwrap();
    let path = archive.to_str().unwrap();
    assert!(PreparedExtraction::new_complete(path, None, &PasswordSet::default()).is_err());
    assert!(
        PreparedExtraction::new_complete(path, None, &PasswordSet::new(Some(b"wrong".to_vec())))
            .is_err()
    );
    let unlocked =
        PreparedExtraction::new_complete(path, None, &PasswordSet::new(Some(b"secret".to_vec())))
            .unwrap();
    assert_eq!(
        unlocked.read_file_range("secret.txt", 0, 99).unwrap(),
        b"secret"
    );
}

#[test]
fn commit_specific_passwords_unlock_the_complete_history() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("secret.txt");
    fs::write(&input, b"old").unwrap();
    let archive = dir.path().join("encrypted.zar");
    let path = archive.to_str().unwrap();
    create_archive_with_password(
        path,
        std::slice::from_ref(&input),
        false,
        EncodingOptions::default(),
        Some(b"first"),
    )
    .unwrap();
    fs::write(&input, b"new").unwrap();
    let default = PasswordSet::new(Some(b"first".to_vec()));
    add_archive_with_encoding_progress_and_filters_and_encryption(
        path,
        &[input],
        EncodingOptions::default(),
        None,
        AddInputOptions::default(),
        &FilterSet::default(),
        Some((
            b"second",
            ContentAead::Aes256Gcm,
            EncryptionScope::HeaderAndData,
        )),
        &default,
    )
    .unwrap();
    assert!(PreparedExtraction::new_complete(path, None, &default).is_err());
    let mut passwords = default;
    passwords.insert_for_commit(parse_commit_id("00000002").unwrap(), b"second".to_vec());
    let latest = PreparedExtraction::new_complete(path, None, &passwords).unwrap();
    assert_eq!(latest.read_file_range("secret.txt", 0, 10).unwrap(), b"new");
    let first = PreparedExtraction::new_complete(
        path,
        Some("00000001"),
        &PasswordSet::new(Some(b"first".to_vec())),
    )
    .unwrap();
    assert_eq!(first.read_file_range("secret.txt", 0, 10).unwrap(), b"old");
}

#[test]
fn password_files_and_invalid_specs() {
    let dir = tempfile::tempdir().unwrap();
    let password = dir.path().join("password");
    fs::write(&password, b"secret\r\n").unwrap();
    assert_eq!(password_file(&password).unwrap(), b"secret");
    let mut options = Options {
        archive: PathBuf::new(),
        mountpoint: PathBuf::new(),
        to: None,
        password_file: Some(password.clone()),
        password_for: vec![],
    };
    assert_eq!(
        passwords(&options).unwrap().password_for(&[0; 4]),
        Some(b"secret".as_slice())
    );
    options.password_for = vec!["bad".into()];
    assert!(passwords(&options).is_err());
    options.password_for = vec!["00000001=".into()];
    assert!(
        passwords(&options)
            .err()
            .unwrap()
            .to_string()
            .contains("requires a password file path")
    );
    options.password_for = vec![format!("00000001={}", password.display()); 2];
    assert!(passwords(&options).is_err());
    fs::write(&password, b"\n").unwrap();
    assert!(password_file(&password).is_err());
}

#[test]
fn rejects_mountpoints_that_are_not_directories_or_contain_the_archive() {
    let (dir, archive, _) = fixture();
    let options = |mountpoint| Options {
        archive: archive.clone(),
        mountpoint,
        to: None,
        password_file: None,
        password_for: vec![],
    };
    let file = dir.path().join("not-a-directory");
    fs::write(&file, b"file").unwrap();
    assert!(
        run(options(file))
            .unwrap_err()
            .to_string()
            .contains("mountpoint must be a directory")
    );
    assert!(
        run(options(dir.path().to_owned()))
            .unwrap_err()
            .to_string()
            .contains("archive must not be inside the mountpoint")
    );
}

#[test]
fn rejects_archive_paths_that_are_not_utf8() {
    use std::os::unix::ffi::OsStrExt;
    let (dir, archive, _) = fixture();
    let invalid = dir.path().join(OsStr::from_bytes(b"archive-\xff.zar"));
    fs::copy(archive, &invalid).unwrap();
    let mountpoint = dir.path().join("mount");
    fs::create_dir(&mountpoint).unwrap();
    let error = run(Options {
        archive: invalid,
        mountpoint,
        to: None,
        password_file: None,
        password_for: vec![],
    })
    .unwrap_err();
    assert!(error.to_string().contains("archive path must be UTF-8"));
}

#[test]
fn rejects_unsupported_special_files_before_mounting() {
    use std::os::unix::ffi::OsStrExt;
    use zarlib::operations::{
        CreateInputOptions, create_archive_with_password_progress_and_special_files,
    };

    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    fs::create_dir(&input).unwrap();
    let pipe = std::ffi::CString::new(input.join("pipe").as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(pipe.as_ptr(), 0o600) }, 0);
    let archive = dir.path().join("pipe.zar");
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
    let result = ArchiveFs::new(prepared(&archive, None));
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("unsupported special file in archive: input/pipe")
    );
}

#[test]
#[ignore = "requires a usable /dev/fuse and fusermount3"]
fn kernel_mount_is_read_only_and_serves_random_reads() {
    let (dir, archive, content) = fixture();
    let mountpoint = dir.path().join("mount");
    fs::create_dir(&mountpoint).unwrap();
    let fs = ArchiveFs::new(prepared(&archive, None)).unwrap();
    let mut config = Config::default();
    config.mount_options = vec![MountOption::RO, MountOption::NoDev, MountOption::NoSuid];
    let session = fuser::spawn_mount(fs, &mountpoint, &config).unwrap();
    let input = mountpoint.join("input");
    assert!(fs::read_dir(&input).unwrap().count() >= 4);
    assert_eq!(
        fs::read_link(input.join("link")).unwrap(),
        PathBuf::from("small.txt")
    );
    assert_eq!(fs::read(input.join("small.txt")).unwrap(), b"small");
    assert_eq!(fs::read(input.join("link")).unwrap(), b"small");
    let mut file = fs::File::open(input.join("nested/data.bin")).unwrap();
    file.seek(SeekFrom::Start(65530)).unwrap();
    let mut bytes = [0; 30];
    file.read_exact(&mut bytes).unwrap();
    assert_eq!(bytes.as_slice(), &content[65530..65560]);
    assert!(fs::write(input.join("small.txt"), b"no").is_err());
    assert!(fs::write(input.join("new"), b"no").is_err());
    fs::OpenOptions::new()
        .append(true)
        .open(&archive)
        .unwrap()
        .write_all(b"changed")
        .unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    assert!(file.read(&mut bytes).is_err());
    drop(file);
    drop(session);
}

#[test]
#[ignore = "requires a usable /dev/fuse and fusermount3"]
fn filesystem_rejects_writes_even_when_kernel_mount_is_writable() {
    let (dir, archive, _) = fixture();
    let mountpoint = dir.path().join("writable-mount");
    fs::create_dir(&mountpoint).unwrap();
    let filesystem = ArchiveFs::new(prepared(&archive, None)).unwrap();
    let session = fuser::spawn_mount(filesystem, &mountpoint, &Config::default()).unwrap();
    let input = mountpoint.join("input");
    let error = fs::metadata(input.join("missing")).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    let error = fs::OpenOptions::new()
        .write(true)
        .open(input.join("small.txt"))
        .unwrap_err();
    assert_eq!(error.raw_os_error(), Some(libc::EROFS));
    drop(session);
}
