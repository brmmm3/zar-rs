use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchivePathError {
    Empty,
    Absolute,
    EmptyComponent,
    DotComponent,
    ParentComponent,
    NulByte,
    Backslash,
    DrivePrefix,
}

impl fmt::Display for ArchivePathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Empty => "archive path is empty",
            Self::Absolute => "archive path must be relative",
            Self::EmptyComponent => "archive path contains an empty component",
            Self::DotComponent => "archive path contains a '.' component",
            Self::ParentComponent => "archive path contains a '..' component",
            Self::NulByte => "archive path contains a NUL byte",
            Self::Backslash => "archive path must use '/' separators",
            Self::DrivePrefix => "archive path must not contain a platform drive prefix",
        };
        f.write_str(message)
    }
}

impl std::error::Error for ArchivePathError {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArchivePath(String);

impl ArchivePath {
    pub fn new(path: impl Into<String>) -> Result<Self, ArchivePathError> {
        let path = path.into();
        validate_archive_path(&path)?;
        Ok(Self(path))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for ArchivePath {
    type Error = ArchivePathError;

    fn try_from(path: &str) -> Result<Self, Self::Error> {
        Self::new(path)
    }
}

impl std::str::FromStr for ArchivePath {
    type Err = ArchivePathError;

    fn from_str(path: &str) -> Result<Self, Self::Err> {
        Self::new(path)
    }
}

impl fmt::Display for ArchivePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn validate_archive_path(path: &str) -> Result<(), ArchivePathError> {
    if path.is_empty() {
        return Err(ArchivePathError::Empty);
    }
    if path.starts_with('/') {
        return Err(ArchivePathError::Absolute);
    }
    if path.contains('\0') {
        return Err(ArchivePathError::NulByte);
    }
    if path.contains('\\') {
        return Err(ArchivePathError::Backslash);
    }

    let mut components = path.split('/');
    let first = components
        .next()
        .expect("non-empty path has first component");
    if first.len() >= 2 && first.as_bytes()[0].is_ascii_alphabetic() && first.as_bytes()[1] == b':'
    {
        return Err(ArchivePathError::DrivePrefix);
    }

    for component in std::iter::once(first).chain(components) {
        match component {
            "" => return Err(ArchivePathError::EmptyComponent),
            "." => return Err(ArchivePathError::DotComponent),
            ".." => return Err(ArchivePathError::ParentComponent),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_canonical_relative_utf8_paths() {
        let path = ArchivePath::try_from("docs/naïve file.txt").unwrap();
        assert_eq!(path.as_str(), "docs/naïve file.txt");
        assert_eq!(path.to_string(), "docs/naïve file.txt");
    }

    #[test]
    fn rejects_noncanonical_paths() {
        let invalid = [
            ("", ArchivePathError::Empty),
            ("/rooted", ArchivePathError::Absolute),
            ("a//b", ArchivePathError::EmptyComponent),
            ("a/", ArchivePathError::EmptyComponent),
            ("./a", ArchivePathError::DotComponent),
            ("a/../b", ArchivePathError::ParentComponent),
            ("a\0b", ArchivePathError::NulByte),
            ("a\\b", ArchivePathError::Backslash),
            ("C:/file", ArchivePathError::DrivePrefix),
            ("c:relative", ArchivePathError::DrivePrefix),
        ];

        for (value, expected_error) in invalid {
            assert_eq!(
                ArchivePath::try_from(value),
                Err(expected_error),
                "{value:?}"
            );
        }
    }

    #[test]
    fn compares_paths_exactly_without_case_folding() {
        let upper = ArchivePath::try_from("File.txt").unwrap();
        let lower = ArchivePath::try_from("file.txt").unwrap();
        assert_ne!(upper, lower);
    }
}
