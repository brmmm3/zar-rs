use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use zarlib::operations::{
    AddInputOptions, CollisionPolicy, CommitRangeOptions, CreateInputOptions, EncodingOptions,
    ExistingPolicy, ExtractionOptions, FilterSet, PasswordSet,
    add_archive_with_encoding_progress_and_filters_and_encryption, change_encryption_for_commits,
    create_archive_with_password_progress_and_special_files, decrypt_archive_commits,
    encrypt_archive_commits, extract_archive_with_passwords_progress_and_filters, info_archive,
    join_archive_with_encoding_progress_and_passwords, list_archive, list_archive_with_passwords,
    remove_archive_commits, remove_archive_entries_with_progress_and_encryption, remove_commits,
};
use zarlib::{ArchivePath, ContentAead, EncryptionScope, TocEntry};

const PASSWORD_A: &[u8] = b"password A";
const PASSWORD_B: &[u8] = b"password B";
const PASSWORD_C: &[u8] = b"password C";
const PASSWORD_D: &[u8] = b"password D";
const PASSWORD_E: &[u8] = b"password E";

fn write_stage(parent: &Path, directory: &str, files: &[(&str, &[u8])]) -> PathBuf {
    let stage = parent.join(directory);
    std::fs::create_dir_all(&stage).unwrap();
    for (name, contents) in files {
        std::fs::write(stage.join(name), contents).unwrap();
    }
    stage
}

fn password_set(archive: &str, entries: &[(usize, &[u8])]) -> PasswordSet {
    let commits = info_archive(archive, false).unwrap();
    let mut passwords = PasswordSet::default();
    for (index, password) in entries {
        passwords.insert_for_commit(commits[*index].id, password.to_vec());
    }
    passwords
}

fn file_paths(toc: &zarlib::Toc) -> BTreeSet<String> {
    toc.iter()
        .filter(|(_, entry)| matches!(entry, TocEntry::File(_)))
        .map(|(path, _)| path.as_str().to_owned())
        .collect()
}

fn extract(archive: &str, destination: &Path, passwords: &PasswordSet) -> BTreeSet<String> {
    extract_filtered(archive, destination, passwords, &FilterSet::default())
}

fn extract_filtered(
    archive: &str,
    destination: &Path,
    passwords: &PasswordSet,
    filters: &FilterSet,
) -> BTreeSet<String> {
    let plan = extract_archive_with_passwords_progress_and_filters(
        archive,
        destination,
        ExtractionOptions {
            dry_run: false,
            to: None,
            collision_policy: CollisionPolicy::Error,
            existing_policy: ExistingPolicy::Error,
            restore_metadata: false,
            restore_symlinks: false,
            restore_devices: false,
            strip_top_level: false,
        },
        passwords,
        None,
        filters,
    )
    .unwrap();
    plan.files
        .iter()
        .map(|(path, _, _)| {
            path.strip_prefix(destination)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect()
}

fn add_unencrypted(archive: &str, paths: &[PathBuf], passwords: &PasswordSet) {
    add_archive_with_encoding_progress_and_filters_and_encryption(
        archive,
        paths,
        EncodingOptions::default(),
        None,
        AddInputOptions::default(),
        &FilterSet::default(),
        None,
        passwords,
    )
    .unwrap();
}

fn commit_id(id: [u8; 4]) -> String {
    format!("{:08x}", u32::from_le_bytes(id))
}

/// Test steps:
/// 1. Create an unencrypted archive, then add AES/password-A and
///    ChaCha/password-B encrypted commits.
/// 2. Add password-C encrypted tombstones, then unencrypted tombstones and a
///    final unencrypted commit.
/// 3. List and extract without passwords, verifying that locked commits are
///    skipped while the visible files remain correct.
/// 4. List and extract with all passwords, verifying the complete contents.
/// 5. Join the last two commits, then remove the last commit and verify the
///    restored contents.
/// 6. Join all readable commits with only password B, verifying that the
///    password-A and password-C commits remain separate and encrypted.
/// 7. Add unencrypted files in one commit.
/// 8. Add more unencrypted files in a second commit.
/// 9. Extract three selected files and verify their contents.
/// 10. Try extracting patterns that match no archived files.
/// 11. Try extracting to a path occupied by a regular file.
/// 12. Try extracting to a destination on a non-writable filesystem.
/// 13. Add readable and permission-restricted files, checking the readable
///     input is retained and inaccessible input is skipped when applicable.
/// 14. Remove commits 1 and 3 by ID.
/// 15. List the remaining files without passwords.
/// 16. Extract the remaining visible files without passwords.
/// 17. Encrypt the last commit.
/// 18. Re-encrypt the first encrypted commit with ChaCha and a new password.
/// 19. Decrypt all encrypted commits.
/// 20. Join the complete commit history and verify the resulting archive.
#[test]
fn encrypted_commits_and_removals_work_with_partial_and_complete_passwords() {
    let temp = tempfile::tempdir().unwrap();
    let root = write_stage(
        temp.path(),
        "root",
        &[("keep.txt", b"root keep"), ("remove.txt", b"root remove")],
    );
    let stage_a = write_stage(
        temp.path(),
        "stage-a",
        &[("keep.txt", b"A keep"), ("remove.txt", b"A remove")],
    );
    let stage_b = write_stage(
        temp.path(),
        "stage-b",
        &[("keep.txt", b"B keep"), ("remove.txt", b"B remove")],
    );
    let final_stage = write_stage(temp.path(), "final", &[("new.txt", b"final")]);
    let archive_path = temp.path().join("combinations.zar");
    let archive = archive_path.to_str().unwrap();

    create_archive_with_password_progress_and_special_files(
        archive,
        &[root],
        false,
        EncodingOptions::default(),
        None,
        None,
        CreateInputOptions::default(),
    )
    .unwrap();

    add_archive_with_encoding_progress_and_filters_and_encryption(
        archive,
        &[stage_a],
        EncodingOptions::default(),
        None,
        AddInputOptions::default(),
        &FilterSet::default(),
        Some((
            PASSWORD_A,
            ContentAead::Aes256Gcm,
            EncryptionScope::HeaderAndData,
        )),
        &PasswordSet::default(),
    )
    .unwrap();
    add_archive_with_encoding_progress_and_filters_and_encryption(
        archive,
        &[stage_b],
        EncodingOptions::default(),
        None,
        AddInputOptions::default(),
        &FilterSet::default(),
        Some((
            PASSWORD_B,
            ContentAead::ChaCha20Poly1305,
            EncryptionScope::HeaderAndData,
        )),
        &password_set(archive, &[(1, PASSWORD_A)]),
    )
    .unwrap();

    remove_archive_entries_with_progress_and_encryption(
        archive,
        &[ArchivePath::new("stage-a/remove.txt").unwrap()],
        None,
        Some((
            PASSWORD_C,
            ContentAead::Aes256Gcm,
            EncryptionScope::HeaderAndData,
        )),
        &password_set(archive, &[(1, PASSWORD_A), (2, PASSWORD_B)]),
    )
    .unwrap();
    remove_archive_entries_with_progress_and_encryption(
        archive,
        &[ArchivePath::new("stage-b/remove.txt").unwrap()],
        None,
        None,
        &password_set(
            archive,
            &[(1, PASSWORD_A), (2, PASSWORD_B), (3, PASSWORD_C)],
        ),
    )
    .unwrap();
    add_archive_with_encoding_progress_and_filters_and_encryption(
        archive,
        &[final_stage],
        EncodingOptions::default(),
        None,
        AddInputOptions::default(),
        &FilterSet::default(),
        None,
        &password_set(
            archive,
            &[(1, PASSWORD_A), (2, PASSWORD_B), (3, PASSWORD_C)],
        ),
    )
    .unwrap();

    let visible_without_password = BTreeSet::from([
        "root/keep.txt".to_owned(),
        "root/remove.txt".to_owned(),
        "final/new.txt".to_owned(),
    ]);
    assert_eq!(
        file_paths(&list_archive(archive, None, None).unwrap()),
        visible_without_password
    );

    let partial_destination = temp.path().join("partial");
    assert_eq!(
        extract(archive, &partial_destination, &PasswordSet::default()),
        visible_without_password
    );
    assert_eq!(
        std::fs::read(partial_destination.join("root/keep.txt")).unwrap(),
        b"root keep"
    );
    assert_eq!(
        std::fs::read(partial_destination.join("root/remove.txt")).unwrap(),
        b"root remove"
    );
    assert_eq!(
        std::fs::read(partial_destination.join("final/new.txt")).unwrap(),
        b"final"
    );

    let all_passwords = password_set(
        archive,
        &[(1, PASSWORD_A), (2, PASSWORD_B), (3, PASSWORD_C)],
    );
    let visible_with_passwords = BTreeSet::from([
        "root/keep.txt".to_owned(),
        "root/remove.txt".to_owned(),
        "stage-a/keep.txt".to_owned(),
        "stage-b/keep.txt".to_owned(),
        "final/new.txt".to_owned(),
    ]);
    assert_eq!(
        file_paths(&list_archive_with_passwords(archive, None, &all_passwords).unwrap()),
        visible_with_passwords
    );

    let complete_destination = temp.path().join("complete");
    assert_eq!(
        extract(archive, &complete_destination, &all_passwords),
        visible_with_passwords
    );
    for (path, expected) in [
        ("root/keep.txt", b"root keep".as_slice()),
        ("root/remove.txt", b"root remove".as_slice()),
        ("stage-a/keep.txt", b"A keep".as_slice()),
        ("stage-b/keep.txt", b"B keep".as_slice()),
        ("final/new.txt", b"final".as_slice()),
    ] {
        assert_eq!(
            std::fs::read(complete_destination.join(path)).unwrap(),
            expected,
            "{path}"
        );
    }

    let before_join = info_archive(archive, false).unwrap();
    join_archive_with_encoding_progress_and_passwords(
        archive,
        &format!("{:08x}", u32::from_le_bytes(before_join[4].id)),
        &format!("{:08x}", u32::from_le_bytes(before_join[5].id)),
        EncodingOptions::default(),
        None,
        &all_passwords,
    )
    .unwrap();
    let after_join = info_archive(archive, false).unwrap();
    assert_eq!(after_join.len(), 5);
    assert_eq!(
        file_paths(
            &list_archive_with_passwords(
                archive,
                None,
                &password_set(
                    archive,
                    &[(1, PASSWORD_A), (2, PASSWORD_B), (3, PASSWORD_C)]
                )
            )
            .unwrap()
        ),
        visible_with_passwords
    );

    remove_commits(archive, 1).unwrap();
    let after_undo = info_archive(archive, false).unwrap();
    assert_eq!(after_undo.len(), 4);
    let passwords_after_undo = password_set(
        archive,
        &[(1, PASSWORD_A), (2, PASSWORD_B), (3, PASSWORD_C)],
    );
    let visible_after_undo = BTreeSet::from([
        "root/keep.txt".to_owned(),
        "root/remove.txt".to_owned(),
        "stage-a/keep.txt".to_owned(),
        "stage-b/keep.txt".to_owned(),
        "stage-b/remove.txt".to_owned(),
    ]);
    assert_eq!(
        file_paths(&list_archive_with_passwords(archive, None, &passwords_after_undo).unwrap()),
        visible_after_undo
    );

    let root_commit_id = after_undo[0].id;
    let locked_a_commit_id = after_undo[1].id;
    let locked_c_commit_id = after_undo[3].id;
    let password_b_only = password_set(archive, &[(2, PASSWORD_B)]);
    join_archive_with_encoding_progress_and_passwords(
        archive,
        &format!("{:08x}", u32::from_le_bytes(root_commit_id)),
        &format!(
            "{:08x}",
            u32::from_le_bytes(after_undo[after_undo.len() - 1].id)
        ),
        EncodingOptions::default(),
        None,
        &password_b_only,
    )
    .unwrap();

    let joined_commits = info_archive(archive, false).unwrap();
    assert_eq!(joined_commits.len(), 3);
    assert!(!joined_commits[0].header_encrypted);
    assert_eq!(joined_commits[1].id, locked_a_commit_id);
    assert!(joined_commits[1].header_encrypted);
    assert_eq!(joined_commits[2].id, locked_c_commit_id);
    assert!(joined_commits[2].header_encrypted);
    let visible_after_partial_join = BTreeSet::from([
        "root/keep.txt".to_owned(),
        "root/remove.txt".to_owned(),
        "stage-b/keep.txt".to_owned(),
        "stage-b/remove.txt".to_owned(),
    ]);
    assert_eq!(
        file_paths(&list_archive(archive, None, None).unwrap()),
        visible_after_partial_join
    );

    let remaining_passwords = password_set(archive, &[(1, PASSWORD_A), (2, PASSWORD_C)]);
    assert_eq!(
        file_paths(&list_archive_with_passwords(archive, None, &remaining_passwords).unwrap()),
        visible_after_undo
    );

    let stage_seven = write_stage(
        temp.path(),
        "stage-seven",
        &[
            ("one.txt", b"step seven one"),
            ("two.txt", b"step seven two"),
            ("three.txt", b"step seven three"),
        ],
    );
    add_unencrypted(
        archive,
        &[stage_seven],
        &password_set(archive, &[(1, PASSWORD_A), (2, PASSWORD_C)]),
    );

    let stage_eight = write_stage(
        temp.path(),
        "stage-eight",
        &[
            ("four.txt", b"step eight four"),
            ("five.txt", b"step eight five"),
        ],
    );
    add_unencrypted(
        archive,
        &[stage_eight],
        &password_set(archive, &[(1, PASSWORD_A), (2, PASSWORD_C)]),
    );

    let three_files = FilterSet::from_patterns(
        &[
            "stage-seven/one.txt".to_owned(),
            "stage-seven/two.txt".to_owned(),
            "stage-seven/three.txt".to_owned(),
        ],
        &[],
        &[],
        &[],
        &[] as &[PathBuf],
    )
    .unwrap();
    let three_file_destination = temp.path().join("three-files");
    assert_eq!(
        extract_filtered(
            archive,
            &three_file_destination,
            &remaining_passwords,
            &three_files
        ),
        BTreeSet::from([
            "stage-seven/one.txt".to_owned(),
            "stage-seven/two.txt".to_owned(),
            "stage-seven/three.txt".to_owned(),
        ])
    );
    assert_eq!(
        std::fs::read(three_file_destination.join("stage-seven/one.txt")).unwrap(),
        b"step seven one"
    );
    assert_eq!(
        std::fs::read(three_file_destination.join("stage-seven/two.txt")).unwrap(),
        b"step seven two"
    );
    assert_eq!(
        std::fs::read(three_file_destination.join("stage-seven/three.txt")).unwrap(),
        b"step seven three"
    );

    let missing_files = FilterSet::from_patterns(
        &["missing-*.txt".to_owned()],
        &[],
        &[],
        &[],
        &[] as &[PathBuf],
    )
    .unwrap();
    assert!(
        extract_filtered(
            archive,
            &temp.path().join("missing-files"),
            &remaining_passwords,
            &missing_files,
        )
        .is_empty()
    );

    let invalid_destination = temp.path().join("not-a-directory");
    std::fs::write(&invalid_destination, b"regular file").unwrap();
    assert!(
        extract_archive_with_passwords_progress_and_filters(
            archive,
            &invalid_destination,
            ExtractionOptions {
                dry_run: false,
                to: None,
                collision_policy: CollisionPolicy::Error,
                existing_policy: ExistingPolicy::Error,
                restore_metadata: false,
                restore_symlinks: false,
                restore_devices: false,
                strip_top_level: false,
            },
            &remaining_passwords,
            None,
            &FilterSet::default(),
        )
        .is_err()
    );

    #[cfg(target_os = "linux")]
    {
        let unwritable_destination =
            PathBuf::from(format!("/proc/zarlib-unwritable-{}", std::process::id()));
        assert!(
            extract_archive_with_passwords_progress_and_filters(
                archive,
                &unwritable_destination,
                ExtractionOptions {
                    dry_run: false,
                    to: None,
                    collision_policy: CollisionPolicy::Error,
                    existing_policy: ExistingPolicy::Error,
                    restore_metadata: false,
                    restore_symlinks: false,
                    restore_devices: false,
                    strip_top_level: false,
                },
                &remaining_passwords,
                None,
                &FilterSet::default(),
            )
            .is_err()
        );
    }

    let stage_thirteen = write_stage(
        temp.path(),
        "stage-thirteen",
        &[("readable.txt", b"readable"), ("private.txt", b"private")],
    );
    let private_file = stage_thirteen.join("private.txt");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&private_file, std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    let can_read_private = std::fs::File::open(&private_file).is_ok();
    add_unencrypted(
        archive,
        &[stage_thirteen],
        &password_set(archive, &[(1, PASSWORD_A), (2, PASSWORD_C)]),
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&private_file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let after_unreadable_add = list_archive_with_passwords(
        archive,
        None,
        &password_set(archive, &[(1, PASSWORD_A), (2, PASSWORD_C)]),
    )
    .unwrap();
    assert!(
        after_unreadable_add
            .get(&ArchivePath::new("stage-thirteen/readable.txt").unwrap())
            .is_some()
    );
    assert_eq!(
        after_unreadable_add
            .get(&ArchivePath::new("stage-thirteen/private.txt").unwrap())
            .is_some(),
        can_read_private
    );

    let commits_before_removal = info_archive(archive, false).unwrap();
    assert!(commits_before_removal.len() >= 4);
    let remove_ids = vec![
        commit_id(commits_before_removal[0].id),
        commit_id(commits_before_removal[2].id),
    ];
    remove_archive_commits(archive, &remove_ids, None).unwrap();

    let expected_after_removal = BTreeSet::from([
        "stage-seven/one.txt".to_owned(),
        "stage-seven/two.txt".to_owned(),
        "stage-seven/three.txt".to_owned(),
        "stage-eight/four.txt".to_owned(),
        "stage-eight/five.txt".to_owned(),
        "stage-thirteen/readable.txt".to_owned(),
    ]);
    assert_eq!(
        file_paths(&list_archive(archive, None, None).unwrap()),
        expected_after_removal
    );
    let without_passwords_destination = temp.path().join("without-passwords");
    let mut expected_without_passwords = expected_after_removal.clone();
    if can_read_private {
        expected_without_passwords.insert("stage-thirteen/private.txt".to_owned());
    }
    assert_eq!(
        extract(
            archive,
            &without_passwords_destination,
            &PasswordSet::default()
        ),
        expected_without_passwords
    );

    let after_removal = info_archive(archive, false).unwrap();
    let last = after_removal.last().unwrap();
    let last_id = commit_id(last.id);
    encrypt_archive_commits(
        archive,
        CommitRangeOptions {
            from_id: &last_id,
            to_id: &last_id,
            output: None,
            passwords: &PasswordSet::default(),
            strict: true,
        },
        PASSWORD_E,
        ContentAead::Aes256Gcm,
        EncryptionScope::HeaderAndData,
    )
    .unwrap();

    let after_encrypt = info_archive(archive, false).unwrap();
    let first_encrypted = after_encrypt
        .iter()
        .find(|commit| commit.header_encrypted || commit.data_encrypted)
        .unwrap();
    let first_encrypted_id = commit_id(first_encrypted.id);
    let mut first_encrypted_password = PasswordSet::default();
    first_encrypted_password.insert_for_commit(first_encrypted.id, PASSWORD_A.to_vec());
    change_encryption_for_commits(
        archive,
        CommitRangeOptions {
            from_id: &first_encrypted_id,
            to_id: &first_encrypted_id,
            output: None,
            passwords: &first_encrypted_password,
            strict: true,
        },
        PASSWORD_D,
        ContentAead::ChaCha20Poly1305,
        EncryptionScope::HeaderAndData,
    )
    .unwrap();
    let after_reencrypt = info_archive(archive, false).unwrap();
    let reencrypted = after_reencrypt
        .iter()
        .find(|commit| commit.id == first_encrypted.id)
        .unwrap();
    assert_eq!(
        reencrypted.encryption_algorithm,
        Some(ContentAead::ChaCha20Poly1305)
    );

    let mut encrypted_passwords = PasswordSet::default();
    encrypted_passwords.insert_for_commit(first_encrypted.id, PASSWORD_D.to_vec());
    let last_encrypted = after_reencrypt.last().unwrap();
    encrypted_passwords.insert_for_commit(last_encrypted.id, PASSWORD_E.to_vec());
    decrypt_archive_commits(
        archive,
        CommitRangeOptions {
            from_id: &commit_id(after_reencrypt[0].id),
            to_id: &commit_id(last_encrypted.id),
            output: None,
            passwords: &encrypted_passwords,
            strict: true,
        },
    )
    .unwrap();
    let decrypted_commits = info_archive(archive, false).unwrap();
    assert!(
        decrypted_commits
            .iter()
            .all(|commit| { !commit.header_encrypted && !commit.data_encrypted })
    );
    let mut expected_after_decryption = expected_without_passwords.clone();
    expected_after_decryption.insert("stage-a/keep.txt".to_owned());
    expected_after_decryption.insert("stage-a/remove.txt".to_owned());
    assert_eq!(
        file_paths(&list_archive(archive, None, None).unwrap()),
        expected_after_decryption
    );

    let ids = info_archive(archive, false).unwrap();
    join_archive_with_encoding_progress_and_passwords(
        archive,
        &commit_id(ids[0].id),
        &commit_id(ids.last().unwrap().id),
        EncodingOptions::default(),
        None,
        &PasswordSet::default(),
    )
    .unwrap();
    let joined = info_archive(archive, false).unwrap();
    assert_eq!(joined.len(), 1);
    assert_eq!(
        file_paths(&list_archive(archive, None, None).unwrap()),
        expected_after_decryption
    );
}
