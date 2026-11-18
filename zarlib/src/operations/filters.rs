use crate::{ArchivePath, Toc, TocEntry};
use globset::{GlobBuilder, GlobMatcher};
use std::collections::BTreeSet;
use std::io::BufRead;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FilterTarget {
    File,
    Folder,
    Any,
}

struct FilterRule {
    include: bool,
    target: FilterTarget,
    matcher: GlobMatcher,
}

#[derive(Default)]
pub struct FilterSet {
    rules: Vec<FilterRule>,
    selected_paths: Option<BTreeSet<ArchivePath>>,
}

impl FilterSet {
    pub fn from_patterns(
        include_files: &[String],
        exclude_files: &[String],
        include_folders: &[String],
        exclude_folders: &[String],
        filelists: &[impl AsRef<Path>],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut filters = Self::default();
        for pattern in include_files {
            filters.add(pattern, true, FilterTarget::File)?;
        }
        for pattern in exclude_files {
            filters.add(pattern, false, FilterTarget::File)?;
        }
        for pattern in include_folders {
            filters.add(pattern, true, FilterTarget::Folder)?;
        }
        for pattern in exclude_folders {
            filters.add(pattern, false, FilterTarget::Folder)?;
        }
        for filelist in filelists {
            let file = std::fs::File::open(filelist).map_err(|error| {
                format!(
                    "cannot open filter list {}: {error}",
                    filelist.as_ref().display()
                )
            })?;
            for (line_number, line) in std::io::BufReader::new(file).lines().enumerate() {
                let line = line.map_err(|error| {
                    format!(
                        "cannot read filter list {} at line {}: {error}",
                        filelist.as_ref().display(),
                        line_number + 1
                    )
                })?;
                if line.trim().is_empty() {
                    continue;
                }
                let (include, pattern) = match line.strip_prefix('-') {
                    Some(pattern) => (false, pattern),
                    None => (true, line.as_str()),
                };
                if pattern.is_empty() {
                    return Err(format!(
                        "empty filter in {} at line {}",
                        filelist.as_ref().display(),
                        line_number + 1
                    )
                    .into());
                }
                filters
                    .add(pattern, include, FilterTarget::Any)
                    .map_err(|error| {
                        format!(
                            "invalid filter list {} at line {}: {error}",
                            filelist.as_ref().display(),
                            line_number + 1
                        )
                    })?;
            }
        }
        Ok(filters)
    }

    fn add(
        &mut self,
        pattern: &str,
        include: bool,
        target: FilterTarget,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if pattern.is_empty() {
            return Err("filter pattern must not be empty".into());
        }
        let matcher = GlobBuilder::new(pattern)
            .backslash_escape(false)
            .literal_separator(false)
            .build()
            .map_err(|error| format!("invalid filter pattern `{pattern}`: {error}"))?
            .compile_matcher();
        self.rules.push(FilterRule {
            include,
            target,
            matcher,
        });
        Ok(())
    }

    /// Restrict matches to these exact archive paths and their descendants.
    ///
    /// This restriction intersects with the existing include and exclude rules.
    pub fn restrict_to_paths(&mut self, paths: impl IntoIterator<Item = ArchivePath>) {
        self.selected_paths = Some(paths.into_iter().collect());
    }

    pub fn includes(&self, path: &str, is_directory: bool) -> bool {
        if self.selected_paths.as_ref().is_some_and(|selected| {
            !selected.contains(path)
                && !path_ancestors(path).any(|ancestor| selected.contains(ancestor))
        }) {
            return false;
        }
        if self.rules.is_empty() {
            return true;
        }
        if self.rules.iter().any(|rule| {
            if rule.include {
                return false;
            }
            match rule.target {
                FilterTarget::File => !is_directory && rule.matcher.is_match(path),
                FilterTarget::Folder => {
                    if is_directory && rule.matcher.is_match(path) {
                        true
                    } else {
                        path_ancestors(path).any(|ancestor| rule.matcher.is_match(ancestor))
                    }
                }
                FilterTarget::Any => {
                    rule.matcher.is_match(path)
                        || (!is_directory
                            && path_ancestors(path).any(|ancestor| rule.matcher.is_match(ancestor)))
                }
            }
        }) {
            return false;
        }

        let has_includes = self.rules.iter().any(|rule| rule.include);
        if !has_includes {
            return true;
        }
        self.rules.iter().any(|rule| {
            if !rule.include {
                return false;
            }
            match rule.target {
                FilterTarget::File => !is_directory && rule.matcher.is_match(path),
                FilterTarget::Folder => {
                    if is_directory && rule.matcher.is_match(path) {
                        true
                    } else {
                        path_ancestors(path).any(|ancestor| rule.matcher.is_match(ancestor))
                    }
                }
                FilterTarget::Any => {
                    rule.matcher.is_match(path)
                        || (!is_directory
                            && path_ancestors(path).any(|ancestor| rule.matcher.is_match(ancestor)))
                }
            }
        })
    }

    pub fn filter_toc(&self, toc: &Toc) -> Result<Toc, Box<dyn std::error::Error>> {
        let included_entries: Vec<_> = toc
            .iter()
            .filter(|(path, entry)| {
                !matches!(entry, TocEntry::Directory) && self.includes(path.as_str(), false)
            })
            .map(|(path, _)| path.as_str().to_owned())
            .collect();
        let mut required_directories = std::collections::BTreeSet::new();
        for path in included_entries {
            for (index, _) in path.match_indices('/') {
                required_directories.insert(path[..index].to_owned());
            }
        }

        Ok(toc.filter_entries(|path, entry| match entry {
            TocEntry::Directory => {
                self.includes(path.as_str(), true) || required_directories.contains(path.as_str())
            }
            _ => self.includes(path.as_str(), false),
        }))
    }
}

fn path_ancestors(path: &str) -> impl Iterator<Item = &str> {
    path.match_indices('/').map(|(index, _)| &path[..index])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TocChange;

    #[test]
    fn filter_lists_report_invalid_patterns_and_input_errors() {
        let directory = tempfile::tempdir().unwrap();
        let list = directory.path().join("filters.txt");
        let load = || FilterSet::from_patterns(&[], &[], &[], &[], &[&list]);
        assert!(
            load()
                .err()
                .unwrap()
                .to_string()
                .contains("cannot open filter list")
        );
        std::fs::write(&list, b"\n-\n").unwrap();
        assert!(load().err().unwrap().to_string().contains("empty filter"));
        std::fs::write(&list, b"\n[\n").unwrap();
        assert!(
            load()
                .err()
                .unwrap()
                .to_string()
                .contains("invalid filter list")
        );
        std::fs::write(&list, b"\xff\n").unwrap();
        assert!(
            load()
                .err()
                .unwrap()
                .to_string()
                .contains("cannot read filter list")
        );
        assert!(
            FilterSet::from_patterns(&[String::new()], &[], &[], &[], &[] as &[&Path]).is_err()
        );
    }

    #[test]
    fn filtered_toc_preserves_parent_directories_and_special_entries() {
        use crate::DeviceKind;
        let path = |value| ArchivePath::new(value).unwrap();
        let mut toc = Toc::default();
        toc.apply_commit(&[
            TocChange::PutDirectory { path: path("docs") },
            TocChange::PutSymlink {
                path: path("docs/link"),
                target: "file".into(),
            },
            TocChange::PutNamedPipe {
                path: path("docs/pipe"),
            },
            TocChange::PutDevice {
                path: path("docs/device"),
                kind: DeviceKind::Character,
                major: 2,
                minor: 3,
            },
            TocChange::PutDirectory {
                path: path("other"),
            },
        ])
        .unwrap();
        let filters =
            FilterSet::from_patterns(&["docs/*".to_owned()], &[], &[], &[], &[] as &[&Path])
                .unwrap();
        let filtered = filters.filter_toc(&toc).unwrap();
        assert_eq!(filtered.get(&path("docs")), Some(&TocEntry::Directory));
        assert_eq!(
            filtered.get(&path("docs/link")),
            Some(&TocEntry::Symlink {
                target: "file".into()
            })
        );
        assert_eq!(filtered.get(&path("docs/pipe")), Some(&TocEntry::NamedPipe));
        assert_eq!(
            filtered.get(&path("docs/device")),
            Some(&TocEntry::Device {
                kind: DeviceKind::Character,
                major: 2,
                minor: 3
            })
        );
        assert!(filtered.get(&path("other")).is_none());
    }

    #[test]
    fn include_exclude_and_folder_patterns_match_paths() {
        let filters = FilterSet::from_patterns(
            &["*.txt".to_owned()],
            &["secret*".to_owned()],
            &["docs".to_owned()],
            &["docs/private".to_owned()],
            &[] as &[&Path],
        )
        .unwrap();

        assert!(filters.includes("notes.txt", false));
        assert!(filters.includes("docs/readme.md", false));
        assert!(!filters.includes("secret.txt", false));
        assert!(!filters.includes("docs/private/key.txt", false));
        assert!(!filters.includes("image.png", false));
        assert!(filters.includes("docs", true));
    }

    #[test]
    fn selected_paths_intersect_filters_and_match_whole_components() {
        let mut filters = FilterSet::from_patterns(
            &["*.txt".to_owned()],
            &["*/skip.txt".to_owned()],
            &[],
            &[],
            &[] as &[&Path],
        )
        .unwrap();
        filters.restrict_to_paths(
            ["docs/[selected]", "exact?.txt"]
                .into_iter()
                .map(|path| ArchivePath::new(path).unwrap()),
        );

        assert!(filters.includes("docs/[selected]/child/keep.txt", false));
        assert!(filters.includes("exact?.txt", false));
        assert!(!filters.includes("exact1.txt", false));
        assert!(!filters.includes("docs/[selected]2/keep.txt", false));
        assert!(!filters.includes("docs/[selected]/skip.txt", false));
        assert!(!filters.includes("docs/[selected]/other.bin", false));

        filters.restrict_to_paths([]);
        assert!(!filters.includes("exact?.txt", false));
    }

    #[test]
    fn filelist_lines_are_includes_unless_prefixed_with_minus() {
        let filelist = std::env::temp_dir().join(format!(
            "zarlib-filters-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&filelist, "*.txt\n-private/**\n").unwrap();
        let filters = FilterSet::from_patterns(&[], &[], &[], &[], &[filelist.as_path()]).unwrap();
        std::fs::remove_file(filelist).unwrap();

        assert!(filters.includes("docs/readme.txt", false));
        assert!(!filters.includes("private/key.txt", false));
        assert!(!filters.includes("image.png", false));
    }
}
