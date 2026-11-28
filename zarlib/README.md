# zarlib

`zarlib` is the Rust library for creating, reading, and updating ZAR archives.
It provides the archive engine used by `zarc`, `zarg`, and the project's
language bindings.

## Features

- Parallel block-based Zstandard compression and extraction, with content
  deduplication and random access to archived files.
- Commit-based archive history, including add, remove, and compaction operations.
- Password-based authenticated encryption with AES-256-GCM or
  ChaCha20-Poly1305, plus integrity checks and recovery options.
- Listing, filtering, and searching archive contents without extracting files.
- Import and extraction of ZIP, 7z, RAR, ISO 9660, TAR, and gzip archives,
  subject to the format limitations in the
  [project README](https://github.com/brmmm3/zar-rs#what-makes-zar-different).
- Streaming APIs for regular files and lower-level format and framing types.

## Installation

Add the crate to your Rust project:

```toml
[dependencies]
zarlib = "0.17"
```

For a local checkout, use `zarlib = { path = "../zarlib" }` instead, adjusting
the path as needed. The workspace uses the Rust 2024 edition.

## Create, inspect, and extract

The following example creates an unencrypted archive, checks it, lists the
latest state, and extracts it. Create a `documents` directory with files before
running it; `backup.zar` must not already exist.

```rust,no_run
use std::path::{Path, PathBuf};
use zarlib::operations::{
    check_archive_with_recovery, create_archive, extract_archive, list_archive,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    create_archive("backup.zar", &[PathBuf::from("documents")])?;

    let report = check_archive_with_recovery("backup.zar", false, None)?;
    println!("Checked {} commits", report.commits);

    let toc = list_archive("backup.zar", None, None)?;
    for (path, entry) in toc.iter() {
        println!("{}: {entry:?}", path.as_str());
    }

    extract_archive("backup.zar", Path::new("restored"), false)?;
    Ok(())
}
```

`list_archive` accepts an optional commit limit and password. Passing `true`
as the final argument to `extract_archive` produces an extraction plan without
writing files. The convenience extraction API defaults to reporting errors
for existing destination files. Use the options and password-aware variants
for other policies or encrypted histories.

## Comparing commits

`diff_archive("backup.zar", "00000001", "00000002", None)` compares
complete snapshots and returns `added`, `removed`, and `changed` maps keyed
by archive path. Each value contains `old_size` and `new_size` in bytes;
the absent side is `None`, and non-file entries have size zero. Changes include
content, type, and stored metadata. Reversing the IDs reverses the comparison.
Equal IDs compare that commit with its predecessor, or an empty snapshot for
the first commit.
Use `diff_archive_with_passwords` for histories with multiple passwords.

## Commit details

`create_archive_with_commit_details` and `add_archive_with_commit_details`
accept optional names and descriptions. `update_commit_details` changes or
removes either field of a selected commit without changing its ID. Use
`CommitField::Unchanged`, `Set`, or `Remove` to specify each update, and
provide the selected commit's password if it is encrypted. The archive is
rewritten through a temporary file. `info_archive_with_passwords` returns the
fields when the header is readable. Names contain 1-64 ASCII letters, digits,
underscores or periods; descriptions contain at most 1024 Unicode characters.

## Search

`grep_archive` searches regular files in an archive using a path glob and a
line-oriented regex, without extracting to disk:

```rust,no_run
use zarlib::operations::{grep_archive, GrepOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = grep_archive("backup.zar", "*.rs", "TODO|FIXME", GrepOptions {
        before: 1,
        after: 1,
        ..Default::default()
    })?;
    for file in matches {
        for line in file.lines {
            println!("{}:{}:{}", file.path, line.line_number, String::from_utf8_lossy(&line.bytes));
        }
    }
    Ok(())
}
```

The regex supports lookaround and backreferences. Results include matching and
context lines; `line.is_match` distinguishes them. The archive must remain
unchanged during a search. See the
[search documentation](https://github.com/brmmm3/zar-rs/blob/master/doc/zarlib-developer-guide.md#searching-files)
for details and limitations.

## Configuration and more documentation

`ZARLIB_MAX_CPU_COUNT` sets the requested compression and extraction worker
count: unset or zero uses the available cores, a positive value requests that
many workers, and a negative value reserves that many cores while retaining at
least one worker. Memory limits may reduce the actual worker count.
`ZARLIB_CPU_PRIORITY` can lower the priority of zarlib-owned workers on
supported platforms. `WorkerOverrides` can override either setting for new
worker pools without changing the environment.

- [API documentation](https://docs.rs/zarlib)
- [Developer guide](https://github.com/brmmm3/zar-rs/blob/master/doc/zarlib-developer-guide.md),
  including encryption, streaming, progress callbacks, and configuration
- [ZAR format specification](https://github.com/brmmm3/zar-rs/blob/master/doc/zar-file-format.md)
- [Project and language bindings](https://github.com/brmmm3/zar-rs)

From the workspace root, run `cargo test -p zarlib` to test the library or
`cargo doc -p zarlib --no-deps --open` to build its API documentation.

## License

Licensed under either the MIT License or Apache License, Version 2.0, at your
option.
