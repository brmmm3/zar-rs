use super::common::{
    PasswordSet, TEMP_FILE_ID, TemporaryFiles, parse_commit_id, read_commit_at_offset,
    read_commit_with_passwords, read_first_header_offset, read_header_with_passwords,
    unique_temp_file_id,
};
use super::transform::{
    copy_file_region, paths_refer_to_same_file, replace_archive, resolve_output_path,
    validate_staged_archive,
};
use crate::{
    ArchivePath, CommitValidationLimits, ContainerHeader, HEADER_OFFSET_SIZE, Toc, TocChange,
};
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

/// Validation performed on the retained history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoveCommitsResult {
    /// False when an encrypted header prevented replay validation.
    pub replay_validated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossCommitReference {
    pub referencing_commit: [u8; 4],
    pub source_commit: [u8; 4],
    pub path: ArchivePath,
}

pub fn cross_commit_references(
    archive: &str,
) -> Result<Vec<CrossCommitReference>, Box<dyn std::error::Error>> {
    cross_commit_references_with_passwords(archive, &PasswordSet::default())
}

pub fn cross_commit_references_with_passwords(
    archive: &str,
    passwords: &PasswordSet,
) -> Result<Vec<CrossCommitReference>, Box<dyn std::error::Error>> {
    let mut file = File::open(archive)?;
    let limits = CommitValidationLimits::default();
    let mut offset = read_first_header_offset(&mut file)?;
    let mut offsets = HashSet::new();
    let mut toc = Toc::default();
    let mut references = Vec::new();
    while offset != 0 {
        if !offsets.insert(offset) {
            return Err("archive commit chain contains a cycle".into());
        }
        let (commit, _, next) = read_commit_at_offset(&mut file, offset, limits)?;
        file.seek(SeekFrom::Start(offset))?;
        let (validated, _) = read_header_with_passwords(&mut file, limits, passwords)?;
        let header = validated.logical_header.ok_or_else(|| {
            format!(
                "password required to inspect commit {:08x}",
                u32::from_le_bytes(commit.commit_id)
            )
        })?;
        toc.apply_archive_commit(commit.commit_id, &header.changes)?;
        for change in header.changes {
            if let TocChange::PutFile { path, entry } = change
                && let Some(source_commit) = entry.source_commit
            {
                references.push(CrossCommitReference {
                    referencing_commit: commit.commit_id,
                    source_commit,
                    path,
                });
            }
        }
        offset = next;
    }
    Ok(references)
}

/// Removes the specified commits, preserving all other commits in their original order.
///
/// IDs are eight hexadecimal digits. At least one commit must remain. Retained
/// commits are copied without decrypting them. A retained encrypted header after
/// a removed commit must be unlocked to rule out dangling cross-commit references;
/// use [`remove_archive_commits_with_passwords`] to supply its password.
/// Remaining changes are replayed in order; removing a commit can change their
/// meaning. The returned `replay_validated` flag indicates whether all retained
/// headers could be replayed.
/// If `output` is `None`, the input is replaced atomically; otherwise the result
/// is written to a different archive path.
pub fn remove_archive_commits(
    archive: &str,
    commit_ids: &[String],
    output: Option<&str>,
) -> Result<RemoveCommitsResult, Box<dyn std::error::Error>> {
    remove_archive_commits_with_passwords(archive, commit_ids, output, &PasswordSet::default())
}

pub fn remove_archive_commits_with_passwords(
    archive: &str,
    commit_ids: &[String],
    output: Option<&str>,
    passwords: &PasswordSet,
) -> Result<RemoveCommitsResult, Box<dyn std::error::Error>> {
    if commit_ids.is_empty() {
        return Err("at least one commit ID is required".into());
    }
    let mut selected = HashSet::new();
    for id in commit_ids {
        if !selected.insert(parse_commit_id(id)?) {
            return Err(format!("duplicate commit ID: {id}").into());
        }
    }
    let archive_path = std::fs::canonicalize(archive)?;
    let destination = match output {
        Some(path) => resolve_output_path(Path::new(path))?,
        None => archive_path.clone(),
    };
    if output.is_some() && paths_refer_to_same_file(&archive_path, &destination)? {
        return Err("output path must not be the same as the input archive".into());
    }
    let parent = destination
        .parent()
        .ok_or("output path has no parent directory")?;
    let stem = destination
        .file_name()
        .ok_or("output path has no filename")?
        .to_string_lossy();
    let limits = CommitValidationLimits::default();
    let mut input = File::open(&archive_path)?;
    let file_len = input.metadata()?.len();
    let mut offset = read_first_header_offset(&mut input)?;
    let mut offsets = HashSet::new();
    let mut counts = HashMap::new();
    let mut commits = Vec::new();
    while offset != 0 {
        if !offsets.insert(offset) {
            return Err("archive commit chain contains a cycle".into());
        }
        let (header, data_start, next) = read_commit_at_offset(&mut input, offset, limits)?;
        if data_start < crate::CONTAINER_HEADER_SIZE + HEADER_OFFSET_SIZE {
            return Err("commit data begins before the archive data region".into());
        }
        let end = offset
            .checked_add(u64::from(header.header_frame_size))
            .and_then(|value| value.checked_add(HEADER_OFFSET_SIZE))
            .ok_or("commit end overflow")?;
        if end > file_len {
            return Err("commit extends beyond the end of the archive".into());
        }
        let (validated, _) =
            read_commit_with_passwords(&mut input, header.clone(), limits, passwords)?;
        *counts.entry(header.commit_id).or_insert(0usize) += 1;
        commits.push((header.commit_id, data_start, offset, end, validated));
        offset = next;
    }
    for id in &selected {
        match counts.get(id) {
            None => {
                return Err(format!("commit {:08x} was not found", u32::from_le_bytes(*id)).into());
            }
            Some(1) => {}
            Some(_) => {
                return Err(format!("commit {:08x} is ambiguous", u32::from_le_bytes(*id)).into());
            }
        }
    }
    if selected.len() == commits.len() {
        return Err("at least one commit must remain".into());
    }
    let mut removed_before = false;
    let mut dependencies = Vec::new();
    for (id, _, _, _, commit) in &commits {
        if selected.contains(id) {
            removed_before = true;
            continue;
        }
        if let Some(header) = &commit.logical_header {
            for change in &header.changes {
                if let TocChange::PutFile { path, entry } = change
                    && let Some(source) = entry.source_commit
                    && selected.contains(&source)
                {
                    dependencies.push(format!(
                        "commit {:08x}, path {path}, references commit {:08x}",
                        u32::from_le_bytes(*id),
                        u32::from_le_bytes(source)
                    ));
                }
            }
        } else if removed_before {
            return Err(format!("cannot check cross-commit references in encrypted commit {:08x}; provide its password", u32::from_le_bytes(*id)).into());
        }
    }
    if !dependencies.is_empty() {
        return Err(format!(
            "cannot remove referenced commits: {}",
            dependencies.join("; ")
        )
        .into());
    }
    let replay_validated = commits
        .iter()
        .filter(|(id, ..)| !selected.contains(id))
        .all(|(_, _, _, _, commit)| commit.logical_header.is_some());
    if replay_validated {
        let mut toc = Toc::default();
        for (id, _, _, _, commit) in commits.iter().filter(|(id, ..)| !selected.contains(id)) {
            let header = commit.logical_header.as_ref().expect("checked above");
            Toc::check_available_blocks(&header.changes, &commit.block_indices)
                .and_then(|()| toc.apply_archive_commit(*id, &header.changes))
                .map_err(|error| format!("remaining commits cannot be replayed: {error}"))?;
        }
    }

    let id = TEMP_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let unique = unique_temp_file_id()?;
    let staged_path = parent.join(format!(".{stem}.zarc-remove-commits-{unique}-{id}.tmp"));
    let mut cleanup = TemporaryFiles(vec![staged_path.clone()]);
    let mut staged = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&staged_path)?;
    ContainerHeader::new(0).write_to(&mut staged)?;
    let mut previous_next = None;
    for (commit_id, data_start, header_offset, end, _) in commits {
        if selected.contains(&commit_id) {
            continue;
        }
        let next_position = staged.stream_position()?;
        staged.write_all(&0u64.to_le_bytes())?;
        let new_data_start = staged.stream_position()?;
        let new_header = new_data_start
            .checked_add(header_offset - data_start)
            .ok_or("output header offset overflow")?;
        copy_file_region(&mut input, &mut staged, data_start, end - data_start)?;
        let end_position = staged.stream_position()?;
        staged.seek(SeekFrom::Start(previous_next.unwrap_or(4)))?;
        staged.write_all(&new_header.to_le_bytes())?;
        staged.seek(SeekFrom::Start(end_position))?;
        staged.seek(SeekFrom::Start(end_position - HEADER_OFFSET_SIZE))?;
        staged.write_all(&new_header.to_le_bytes())?;
        staged.seek(SeekFrom::Start(end_position))?;
        previous_next = Some(next_position);
    }
    staged.flush()?;
    drop(staged);
    validate_staged_archive(&staged_path, limits)
        .map_err(|error| format!("staged archive failed validation: {error}"))?;
    drop(input);
    replace_archive(&staged_path, &destination)?;
    cleanup.0.clear();
    Ok(RemoveCommitsResult { replay_validated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::{
        CommitRangeOptions, PasswordSet, add_archive, create_archive, encrypt_archive_commits,
        extract_archive_to_memory_with_passwords, info_archive, list_archive_with_passwords,
        remove_archive,
    };
    use crate::{ContentAead, EncryptionScope};

    fn id(id: [u8; 4]) -> String {
        format!("{:08x}", u32::from_le_bytes(id))
    }

    #[test]
    fn removes_arbitrary_commits_and_preserves_encrypted_commits_in_output() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("archive.zar");
        let output = directory.path().join("output.zar");
        let archive = archive.to_str().unwrap();
        for index in 0..4 {
            let file = directory.path().join(format!("{index}.txt"));
            std::fs::write(&file, format!("contents {index}")).unwrap();
            if index == 0 {
                create_archive(archive, &[file]).unwrap();
            } else {
                add_archive(archive, &[file]).unwrap();
            }
        }
        let commits = info_archive(archive, false).unwrap();
        let encrypted_id = id(commits[2].id);
        encrypt_archive_commits(
            archive,
            CommitRangeOptions {
                from_id: &encrypted_id,
                to_id: &encrypted_id,
                output: None,
                passwords: &PasswordSet::default(),
                strict: false,
            },
            b"secret",
            ContentAead::Aes256Gcm,
            EncryptionScope::HeaderAndData,
        )
        .unwrap();
        let before = std::fs::read(archive).unwrap();
        let removed = vec![id(commits[0].id), id(commits[3].id)];
        let mut passwords = PasswordSet::default();
        passwords.insert_for_commit(commits[2].id, b"secret".to_vec());
        let result =
            remove_archive_commits_with_passwords(archive, &removed, output.to_str(), &passwords)
                .unwrap();
        assert!(result.replay_validated);
        assert_eq!(std::fs::read(archive).unwrap(), before);
        let output = output.to_str().unwrap();
        let retained = info_archive(output, false).unwrap();
        assert_eq!(
            retained.iter().map(|commit| commit.id).collect::<Vec<_>>(),
            vec![commits[1].id, commits[2].id]
        );
        let toc = list_archive_with_passwords(output, None, &passwords).unwrap();
        let names = toc
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["1.txt", "2.txt"]);
        let extracted =
            extract_archive_to_memory_with_passwords(output, None, &passwords, None, 0).unwrap();
        assert_eq!(extracted["1.txt"], b"contents 1");
        assert_eq!(extracted["2.txt"], b"contents 2");
        remove_archive_commits_with_passwords(output, &[id(commits[1].id)], None, &passwords)
            .unwrap();
        assert_eq!(info_archive(output, false).unwrap().len(), 1);
        assert_eq!(
            list_archive_with_passwords(output, None, &passwords)
                .unwrap()
                .len(),
            1
        );
        let result = remove_archive_commits(archive, &[encrypted_id], None).unwrap();
        assert!(result.replay_validated);
        assert_eq!(info_archive(archive, false).unwrap().len(), 3);
    }

    #[test]
    fn rejects_invalid_requests_without_changing_archive_or_output() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("archive.zar");
        let output = directory.path().join("output.zar");
        let file = directory.path().join("file.txt");
        std::fs::write(&file, b"contents").unwrap();
        let archive = archive.to_str().unwrap();
        create_archive(archive, &[file]).unwrap();
        let id = id(info_archive(archive, false).unwrap()[0].id);
        let original = std::fs::read(archive).unwrap();
        std::fs::write(&output, b"unchanged").unwrap();
        for ids in [
            vec![],
            vec!["invalid".into()],
            vec!["ffffffff".into()],
            vec![id.clone()],
            vec![id.clone(), id.clone()],
        ] {
            assert!(remove_archive_commits(archive, &ids, output.to_str()).is_err());
            assert_eq!(std::fs::read(archive).unwrap(), original);
            assert_eq!(std::fs::read(&output).unwrap(), b"unchanged");
        }
        assert!(remove_archive_commits(archive, &[id], Some(archive)).is_err());
        assert_eq!(std::fs::read(archive).unwrap(), original);
    }

    #[test]
    fn rejects_removing_a_commit_needed_to_replay_the_remaining_history() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("archive.zar");
        let item = directory.path().join("item");
        std::fs::create_dir(&item).unwrap();
        std::fs::write(item.join("child"), b"child").unwrap();
        let archive = archive.to_str().unwrap();
        create_archive(archive, std::slice::from_ref(&item)).unwrap();
        remove_archive(archive, &[Path::new("item").to_path_buf()]).unwrap();
        std::fs::remove_file(item.join("child")).unwrap();
        std::fs::remove_dir(&item).unwrap();
        std::fs::write(&item, b"file").unwrap();
        add_archive(archive, &[item]).unwrap();
        let commits = info_archive(archive, false).unwrap();
        let original = std::fs::read(archive).unwrap();
        let error = remove_archive_commits(archive, &[id(commits[1].id)], None).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("remaining commits cannot be replayed")
        );
        assert_eq!(std::fs::read(archive).unwrap(), original);
    }
}
