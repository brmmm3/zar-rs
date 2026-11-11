use super::common::{
    TEMP_FILE_ID, TemporaryFiles, read_commit_at_offset, read_first_header_offset,
    unique_temp_file_id,
};
use crate::{CommitValidationLimits, HEADER_OFFSET_SIZE, ValidatedCommit};
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Removes the requested number of commits from the end of an archive.
///
/// The first commit is the archive's root state and cannot be removed.
pub fn remove_commits(
    archive: &str,
    commits_to_remove: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if commits_to_remove == 0 {
        return Err("the number of commits to undo must be greater than zero".into());
    }

    let archive_path = Path::new(archive);
    let parent = archive_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        return Err(format!("archive parent is not a directory: {}", parent.display()).into());
    }

    let limits = CommitValidationLimits::default();
    let mut input = File::open(archive_path)?;
    let file_len = input.metadata()?.len();
    let mut header_offset = read_first_header_offset(&mut input)?;
    if header_offset == 0 {
        return Err("archive contains no commits".into());
    }

    let mut commits = Vec::new();
    let mut seen_offsets = HashSet::new();
    while header_offset != 0 {
        if !seen_offsets.insert(header_offset) {
            return Err("archive commit chain contains a cycle".into());
        }
        let (commit_header, data_start, next_header_offset) =
            read_commit_at_offset(&mut input, header_offset, limits)?;
        ValidatedCommit::read_body(&mut input, commit_header.clone(), limits)?;
        let commit_end = header_offset
            .checked_add(u64::from(commit_header.header_frame_size))
            .and_then(|offset| offset.checked_add(HEADER_OFFSET_SIZE))
            .ok_or("commit end overflow")?;
        if commit_end > file_len {
            return Err("commit extends beyond the end of the archive".into());
        }
        commits.push((header_offset, data_start, commit_end, next_header_offset));
        header_offset = next_header_offset;
    }

    if commits_to_remove >= commits.len() {
        return Err(format!(
            "cannot undo {commits_to_remove} commit(s): the first commit must be retained"
        )
        .into());
    }

    let retained_last = &commits[commits.len() - commits_to_remove - 1];
    let retained_end = retained_last.2;
    let next_offset_position = retained_last
        .1
        .checked_sub(HEADER_OFFSET_SIZE)
        .ok_or("next commit offset position underflow")?;

    let id = TEMP_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let unique_id = unique_temp_file_id()?;
    let stem = archive_path
        .file_name()
        .ok_or("archive path has no filename")?
        .to_string_lossy();
    let staged_archive: PathBuf = parent.join(format!(".{stem}.zarc-undo-{unique_id}-{id}.tmp"));
    let mut cleanup = TemporaryFiles(vec![staged_archive.clone()]);

    let mut staged = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged_archive)?;
    input.seek(SeekFrom::Start(0))?;
    io::copy(&mut (&mut input).take(retained_end), &mut staged)?;
    staged.seek(SeekFrom::Start(next_offset_position))?;
    staged.write_all(&0u64.to_le_bytes())?;
    staged.flush()?;
    drop(staged);

    let mut validation = File::open(&staged_archive)?;
    let mut header_offset = read_first_header_offset(&mut validation)?;
    let mut validated_commits = 0usize;
    while header_offset != 0 {
        let (commit_header, _, next_header_offset) =
            read_commit_at_offset(&mut validation, header_offset, limits)?;
        ValidatedCommit::read_body(&mut validation, commit_header, limits)?;
        validated_commits += 1;
        header_offset = next_header_offset;
    }
    if validated_commits != commits.len() - commits_to_remove {
        return Err("staged archive has an unexpected number of commits".into());
    }
    drop(validation);
    drop(input);

    std::fs::rename(&staged_archive, archive_path)?;
    cleanup.0.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::remove_commits;
    use crate::operations::{add_archive, create_archive};
    use std::fs;

    #[test]
    fn undo_removes_requested_tail_commits_and_preserves_root_commit() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("archive.zar");
        let first = directory.path().join("first.txt");
        let second = directory.path().join("second.txt");
        let third = directory.path().join("third.txt");
        fs::write(&first, "first").unwrap();
        fs::write(&second, "second").unwrap();
        fs::write(&third, "third").unwrap();

        create_archive(archive.to_str().unwrap(), std::slice::from_ref(&first)).unwrap();
        add_archive(archive.to_str().unwrap(), std::slice::from_ref(&second)).unwrap();
        add_archive(archive.to_str().unwrap(), std::slice::from_ref(&third)).unwrap();

        remove_commits(archive.to_str().unwrap(), 1).unwrap();
        let commits = crate::operations::info_archive(archive.to_str().unwrap(), false).unwrap();
        assert_eq!(commits.len(), 2);
        let contents =
            crate::operations::list_archive(archive.to_str().unwrap(), None, None).unwrap();
        assert!(
            contents
                .iter()
                .any(|(path, _)| path.as_str() == "first.txt")
        );
        assert!(
            contents
                .iter()
                .any(|(path, _)| path.as_str() == "second.txt")
        );
        assert!(
            !contents
                .iter()
                .any(|(path, _)| path.as_str() == "third.txt")
        );
    }

    #[test]
    fn undo_refuses_to_remove_the_first_commit_or_zero_commits() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("archive.zar");
        let first = directory.path().join("first.txt");
        let second = directory.path().join("second.txt");
        fs::write(&first, "first").unwrap();
        fs::write(&second, "second").unwrap();

        create_archive(archive.to_str().unwrap(), std::slice::from_ref(&first)).unwrap();
        add_archive(archive.to_str().unwrap(), std::slice::from_ref(&second)).unwrap();

        let error = remove_commits(archive.to_str().unwrap(), 2).unwrap_err();
        assert!(error.to_string().contains("first commit must be retained"));
        assert!(remove_commits(archive.to_str().unwrap(), 0).is_err());
        assert_eq!(
            crate::operations::info_archive(archive.to_str().unwrap(), false)
                .unwrap()
                .len(),
            2
        );
    }
}
