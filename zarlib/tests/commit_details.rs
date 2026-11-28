use zarlib::operations::{
    AddInputOptions, CommitDetails, CommitDetailsUpdate, CommitField, CreateFilterOptions,
    CreateInputOptions, EncodingOptions, FilterSet, PasswordSet, add_archive_with_commit_details,
    create_archive_with_commit_details, info_archive_with_passwords, update_commit_details,
};
use zarlib::{ContentAead, EncryptionScope};

#[test]
fn create_update_and_remove_commit_details_preserve_encrypted_history() {
    for (scope, algorithm) in [
        (EncryptionScope::Header, ContentAead::Aes256Gcm),
        (EncryptionScope::Data, ContentAead::Aes256Gcm),
        (
            EncryptionScope::HeaderAndData,
            ContentAead::ChaCha20Poly1305,
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("archive.zar");
        let file = dir.path().join("input");
        let retained = dir.path().join("retained");
        std::fs::write(&file, b"before").unwrap();
        std::fs::write(&retained, b"retained contents").unwrap();
        let path = archive.to_str().unwrap();
        let filters = FilterSet::default();
        let encryption = Some((b"secret".as_slice(), algorithm, scope));
        let passwords = PasswordSet::new(Some(b"secret".to_vec()));
        create_archive_with_commit_details(
            path,
            &[file.clone(), retained],
            false,
            EncodingOptions::default(),
            encryption,
            None,
            CreateFilterOptions {
                input: CreateInputOptions::default(),
                filters: &filters,
            },
            CommitDetails {
                name: Some("initial"),
                description: Some("first"),
            },
        )
        .unwrap();
        std::fs::write(&file, b"after").unwrap();
        add_archive_with_commit_details(
            path,
            &[file],
            EncodingOptions::default(),
            None,
            AddInputOptions::default(),
            &filters,
            encryption,
            &passwords,
            CommitDetails {
                name: Some("next"),
                description: None,
            },
        )
        .unwrap();
        let before = std::fs::read(path).unwrap();
        let update = CommitDetailsUpdate {
            name: CommitField::Set("changed_1"),
            description: CommitField::Set("new description"),
        };
        assert!(update_commit_details(path, "00000001", update, &PasswordSet::default()).is_err());
        assert!(
            update_commit_details(
                path,
                "00000001",
                update,
                &PasswordSet::new(Some(b"wrong".to_vec()))
            )
            .is_err()
        );
        assert_eq!(std::fs::read(path).unwrap(), before);
        update_commit_details(path, "00000001", update, &passwords).unwrap();
        let info = info_archive_with_passwords(path, false, &passwords).unwrap();
        assert_eq!(info.len(), 2);
        assert_eq!(info[0].name.as_deref(), Some("changed_1"));
        assert_eq!(info[0].description.as_deref(), Some("new description"));
        assert_eq!(info[1].name.as_deref(), Some("next"));
        let diff = zarlib::operations::diff_archive_with_passwords(
            path, "00000001", "00000002", &passwords,
        )
        .unwrap();
        assert_eq!(diff.changed.len(), 1);
        let prepared =
            zarlib::operations::PreparedExtraction::new_complete(path, None, &passwords).unwrap();
        assert_eq!(prepared.read_file_range("input", 0, 100).unwrap(), b"after");
        assert_eq!(
            prepared.read_file_range("retained", 0, 100).unwrap(),
            b"retained contents"
        );
        update_commit_details(
            path,
            "00000001",
            CommitDetailsUpdate {
                name: CommitField::Remove,
                description: CommitField::Remove,
            },
            &passwords,
        )
        .unwrap();
        let info = info_archive_with_passwords(path, false, &passwords).unwrap();
        assert_eq!(info[0].name, None);
        assert_eq!(info[0].description, None);
    }
}

#[test]
fn invalid_commit_details_do_not_change_archive() {
    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("archive.zar");
    let file = dir.path().join("input");
    std::fs::write(&file, b"data").unwrap();
    let path = archive.to_str().unwrap();
    zarlib::operations::create_archive(path, &[file]).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let before = std::fs::read(path).unwrap();
    for name in ["", "invalid-name", "x".repeat(65).as_str()] {
        assert!(
            update_commit_details(
                path,
                "00000001",
                CommitDetailsUpdate {
                    name: CommitField::Set(name),
                    ..Default::default()
                },
                &PasswordSet::default()
            )
            .is_err()
        );
    }
    let long = "x".repeat(1025);
    assert!(
        update_commit_details(
            path,
            "00000001",
            CommitDetailsUpdate {
                description: CommitField::Set(&long),
                ..Default::default()
            },
            &PasswordSet::default()
        )
        .is_err()
    );
    assert!(
        update_commit_details(
            path,
            "ffffffff",
            CommitDetailsUpdate {
                name: CommitField::Set("valid"),
                ..Default::default()
            },
            &PasswordSet::default()
        )
        .is_err()
    );
    assert_eq!(std::fs::read(path).unwrap(), before);
    let name = "a".repeat(64);
    let description = "é".repeat(1024);
    update_commit_details(
        path,
        "00000001",
        CommitDetailsUpdate {
            name: CommitField::Set(&name),
            description: CommitField::Set(&description),
        },
        &PasswordSet::default(),
    )
    .unwrap();
    let info = info_archive_with_passwords(path, false, &PasswordSet::default()).unwrap();
    assert_eq!(info[0].name.as_deref(), Some(name.as_str()));
    assert_eq!(info[0].description.as_deref(), Some(description.as_str()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
