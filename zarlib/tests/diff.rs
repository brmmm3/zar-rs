use std::path::PathBuf;

use zarlib::ArchivePath;
use zarlib::operations::{
    EncodingOptions, add_archive, create_archive, create_archive_with_password, diff_archive,
    remove_archive,
};

#[test]
fn compares_snapshots_in_both_directions() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input");
    std::fs::create_dir(&input).unwrap();
    std::fs::write(input.join("change"), b"old").unwrap();
    std::fs::write(input.join("remove"), b"remove").unwrap();
    std::fs::write(input.join("same"), b"same").unwrap();
    let archive = temp.path().join("history.zar");
    let name = archive.to_str().unwrap();
    create_archive(name, std::slice::from_ref(&input)).unwrap();
    std::fs::write(input.join("change"), b"newer").unwrap();
    std::fs::write(input.join("add"), b"add").unwrap();
    add_archive(name, std::slice::from_ref(&input)).unwrap();
    remove_archive(name, &[PathBuf::from("input/remove")]).unwrap();

    let path = |value| ArchivePath::new(value).unwrap();
    let forward = diff_archive(name, "00000001", "00000003", None).unwrap();
    assert_eq!(forward.added[&path("input/add")].new_size, Some(3));
    assert_eq!(forward.removed[&path("input/remove")].old_size, Some(6));
    assert_eq!(forward.changed[&path("input/change")].old_size, Some(3));
    assert_eq!(forward.changed[&path("input/change")].new_size, Some(5));
    assert!(!forward.changed.contains_key(&path("input/same")));
    let reverse = diff_archive(name, "00000003", "00000001", None).unwrap();
    assert_eq!(reverse.added[&path("input/remove")].new_size, Some(6));
    assert_eq!(reverse.removed[&path("input/add")].old_size, Some(3));
    assert_eq!(reverse.changed[&path("input/change")].old_size, Some(5));
    assert_eq!(reverse.changed[&path("input/change")].new_size, Some(3));
    let first = diff_archive(name, "00000001", "00000001", None).unwrap();
    assert_eq!(first.added[&path("input/change")].new_size, Some(3));
    let third = diff_archive(name, "00000003", "00000003", None).unwrap();
    assert_eq!(third.removed[&path("input/remove")].old_size, Some(6));
    assert!(third.added.is_empty() && third.changed.is_empty());
    assert!(diff_archive(name, "00000001", "ffffffff", None).is_err());
    assert!(diff_archive(name, "invalid", "00000001", None).is_err());
}

#[test]
fn detects_same_size_content_changes_and_non_file_entries() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input");
    std::fs::create_dir(&input).unwrap();
    std::fs::write(input.join("file"), b"abc").unwrap();
    let archive = temp.path().join("history.zar");
    let name = archive.to_str().unwrap();
    create_archive(name, std::slice::from_ref(&input)).unwrap();
    std::fs::write(input.join("file"), b"xyz").unwrap();
    std::fs::create_dir(input.join("newdir")).unwrap();
    add_archive(name, std::slice::from_ref(&input)).unwrap();
    let diff = diff_archive(name, "00000001", "00000002", None).unwrap();
    assert_eq!(
        diff.changed[&ArchivePath::new("input/file").unwrap()].new_size,
        Some(3)
    );
    assert_eq!(
        diff.added[&ArchivePath::new("input/newdir").unwrap()].new_size,
        Some(0)
    );
}

#[test]
fn encrypted_snapshots_require_passwords() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input");
    let archive = temp.path().join("encrypted.zar");
    std::fs::write(&input, b"secret").unwrap();
    let name = archive.to_str().unwrap();
    create_archive_with_password(
        name,
        &[input],
        false,
        EncodingOptions::default(),
        Some(b"secret"),
    )
    .unwrap();
    assert!(diff_archive(name, "00000001", "00000001", None).is_err());
    assert!(diff_archive(name, "00000001", "00000001", Some(b"wrong")).is_err());
    let diff = diff_archive(name, "00000001", "00000001", Some(b"secret")).unwrap();
    assert_eq!(
        diff.added[&ArchivePath::new("input").unwrap()].new_size,
        Some(6)
    );
}

#[cfg(unix)]
#[test]
fn detects_metadata_only_changes() {
    use std::os::unix::fs::PermissionsExt;
    use zarlib::operations::{
        AddInputOptions, CreateFilterOptions, CreateInputOptions, FilterSet,
        add_archive_with_encoding_progress_and_options,
        create_archive_with_password_progress_and_filters_and_encryption,
    };

    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input");
    let archive = temp.path().join("metadata.zar");
    std::fs::write(&input, b"unchanged").unwrap();
    std::fs::set_permissions(&input, std::fs::Permissions::from_mode(0o600)).unwrap();
    let name = archive.to_str().unwrap();
    create_archive_with_password_progress_and_filters_and_encryption(
        name,
        std::slice::from_ref(&input),
        false,
        EncodingOptions::default(),
        None,
        None,
        CreateFilterOptions {
            input: CreateInputOptions {
                save_metadata: true,
                ..Default::default()
            },
            filters: &FilterSet::default(),
        },
    )
    .unwrap();
    std::fs::set_permissions(&input, std::fs::Permissions::from_mode(0o644)).unwrap();
    add_archive_with_encoding_progress_and_options(
        name,
        &[input],
        EncodingOptions::default(),
        None,
        AddInputOptions {
            save_metadata: true,
            ..Default::default()
        },
    )
    .unwrap();
    let diff = diff_archive(name, "00000001", "00000002", None).unwrap();
    let entry = &diff.changed[&ArchivePath::new("input").unwrap()];
    assert_eq!((entry.old_size, entry.new_size), (Some(9), Some(9)));
}

#[cfg(unix)]
#[test]
fn detects_link_target_and_entry_type_changes() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("input");
    let link = input.join("link");
    let archive = temp.path().join("links.zar");
    std::fs::create_dir(&input).unwrap();
    symlink("first", &link).unwrap();
    let name = archive.to_str().unwrap();
    create_archive(name, std::slice::from_ref(&input)).unwrap();
    std::fs::remove_file(&link).unwrap();
    symlink("second", &link).unwrap();
    add_archive(name, std::slice::from_ref(&input)).unwrap();
    std::fs::remove_file(&link).unwrap();
    std::fs::write(&link, b"file").unwrap();
    add_archive(name, &[input]).unwrap();
    let path = ArchivePath::new("input/link").unwrap();
    let changed = diff_archive(name, "00000001", "00000002", None).unwrap();
    assert_eq!(
        (
            changed.changed[&path].old_size,
            changed.changed[&path].new_size
        ),
        (Some(0), Some(0))
    );
    let changed = diff_archive(name, "00000002", "00000003", None).unwrap();
    assert_eq!(
        (
            changed.changed[&path].old_size,
            changed.changed[&path].new_size
        ),
        (Some(0), Some(4))
    );
}
