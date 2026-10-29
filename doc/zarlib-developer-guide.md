# `zarlib` developer guide

`zarlib` is the format and archive-operations crate behind the `zarc` CLI.
Applications can use its high-level operations without implementing framing,
compression, encryption, or TOC application themselves.

## Add the crate

For code in this workspace:

```toml
[dependencies]
zarlib = { path = "../zarlib" }
```

For a consumer outside this workspace, use the appropriate source or package
reference for your project. The current crate version is `0.1.0`. The workspace
uses Rust edition 2024.

## High-level API

High-level operations are exported from `zarlib::operations`. Operations report
failures as `Result<_, Box<dyn std::error::Error>>`; propagate errors rather
than treating a failed archive operation as success.

```rust,no_run
use std::path::{Path, PathBuf};

use zarlib::operations::{
    create_archive, extract_archive, list_archive,
};

fn archive_and_restore() -> Result<(), Box<dyn std::error::Error>> {
    let inputs = vec![PathBuf::from("src"), PathBuf::from("Cargo.toml")];

    // Creates a new, unencrypted archive. Existing output paths are rejected.
    create_archive("backup.zar", &inputs)?;

    // `None` means latest state; the password argument is absent for this
    // unencrypted archive.
    let toc = list_archive("backup.zar", None, None)?;
    for (path, entry) in toc.iter() {
        println!("{}: {entry:?}", path.as_str());
    }

    // This convenience function uses the default extraction policies.
    let _plan = extract_archive("backup.zar", Path::new("restored"), false)?;
    Ok(())
}
```

The `extract_archive` return value is an `ExtractionPlan`, containing the
effective TOC and planned directories and files. Pass `true` for the `dry_run`
argument to plan extraction without modifying the destination.

### Operation reference

The operation modules and their commonly used public entry points are:

| Module | Main APIs | Purpose |
|---|---|---|
| `operations::create` | `create_archive`, `create_archive_with_options`, `create_archive_with_encoding`, `create_archive_with_password` | Create a new archive; optional strict scanning, encoding options, or password encryption. |
| `operations::add` | `add_archive`, `add_archive_with_encoding` | Append a commit that adds or replaces paths. |
| `operations::remove` | `remove_archive` | Append tombstones for existing archive-relative paths. |
| `operations::join` | `join_archive_with_encoding` | Compact a contiguous commit range. |
| `operations::check` | `check_archive_with_recovery` | Validate framing and readable archive contents, optionally recovering past damaged framing. |
| `operations::info` | `info_archive` | Read commit summaries, optionally including verbose file-group statistics. |
| `operations::list` | `list_archive`, `load_toc_through` | Load the effective TOC, optionally as of a commit ID. |
| `operations::extract` | `extract_archive`, `extract_archive_with_options`, `extract_archive_with_progress` | Plan and perform extraction with explicit policies and password support. |

`operations` also re-exports common option, report, and callback types, including
`EncodingOptions`, `Progress`, `CheckReport`, `CommitInfo`, `ExtractionOptions`,
`CollisionPolicy`, and `ExistingPolicy`. Read the function signatures in the
module for the options appropriate to your use case.

For example, set encoding options when creating or appending:

```rust,no_run
use std::path::PathBuf;
use zarlib::operations::{EncodingOptions, create_archive_with_encoding};

fn create_with_custom_encoding() -> Result<(), Box<dyn std::error::Error>> {
    let inputs = vec![PathBuf::from("dataset")];
    let options = EncodingOptions {
        block_size: 4 * 1024 * 1024,
        compression_level: 5,
    };
    create_archive_with_encoding("dataset.zar", &inputs, false, options)?;
    Ok(())
}
```

The supported creation block size is 64 KiB through 32 MiB. The default is
8 MiB; the default compression level is 3, and valid Zstandard levels are -7
through 22.

## Progress callbacks

Long-running APIs expose progress-enabled variants that accept
`Option<&dyn Progress>`. Implement `Progress` when an application needs custom
progress UI or event reporting:

```rust
use zarlib::operations::{InputSummary, Progress};

struct AppProgress;

impl Progress for AppProgress {
    fn message(&self, message: &str) {
        println!("{message}");
    }

    fn advance(&self, amount: u64) {
        let _ = amount;
    }

    fn add_summary(&self, summary: &InputSummary) {
        println!("{} input bytes", summary.total_bytes);
    }
}
```

The callback is `Send + Sync`. Its default methods provide optional totals,
phases, warnings, interactive confirmation, and input/extraction summaries;
override only the behavior your application supports. For interactive
collision or existing-file `Ask` policies, provide an interactive callback that
implements confirmation. Otherwise, select a non-interactive policy.

## Data model and paths

- `ArchivePath::new` validates canonical archive-relative UTF-8 paths. Paths
  use `/` separators and reject absolute or drive-prefixed names, empty, `.` or
  `..` components, NUL, and backslashes.
- `LogicalHeader` represents ordered TOC changes and optional block-size and
  metadata information. Its `encode` and `decode` methods use the project's
  versioned logical-header encoding.
- `TocChange` describes puts and removals; `TocEntry` describes the effective
  file, directory, symlink, named-pipe, or device entry. `Toc::apply_commit`
  applies an ordered set of changes atomically and checks path-kind conflicts.
- `FileEntry` carries a logical size, commit-local block indices, and an
  optional xxhash64 file checksum. `Toc::apply_commit_with_available_blocks`
  additionally validates newly added files' references against a commit's
  available block indices.
- `FileGroupKind` derives the empty, tiny (1–8 bytes), small, and big grouping
  from the logical size and block size.

## Framing and validation APIs

The crate also exports lower-level framing and validation types, including
`ContainerHeader`, `CommitHeader`, `HeaderFrame`, `BlockFrame`,
`BlockFrameMetadata`, `ValidatedHeader`, and `ValidatedCommit`. They are useful
for format inspection, custom readers, and interoperability work. Prefer the
high-level `operations` APIs for ordinary archive usage, because they enforce
commit limits, block-reference checks, logical TOC consistency, and the
operation's integrity checks together.

The wire format uses a 12-byte `ZAR0` container header and `ZCMT` commit
headers. Data blocks precede their commit's header frame, allowing writers to
stream data before the complete logical header is known. See the repository's
[`plan.md`](../plan.md) for the current byte layouts and compatibility rules.
Changes to the logical-header wire layout require an explicit versioned
compatibility decision.

## Encryption and operation constraints

The library exports AES-256-GCM and ChaCha20-Poly1305 content primitives,
Argon2id password-based DEK wrapping, stable encryption descriptors, and
zeroizing key-handling helpers. `create_archive_with_password` and
`extract_archive_with_progress` (or `extract_archive_with_options`) accept
password bytes; do not log or persist those bytes unnecessarily.

Keep these current constraints in mind:

- Password-encrypted creation encrypts both headers and block payloads.
- Listing, checking, and extraction support password-aware access to encrypted
  content. When headers or blocks are encrypted, supply the correct password
  to authenticate and read them.
- Append, remove, and join reject encrypted archives when they cannot read the
  accumulated logical TOC. Encrypted stateful rewriting is not implemented.
- `check_archive_with_recovery` reports what was checked; inaccessible
  encrypted portions are not equivalent to verified content. Recovery
  resynchronizes after damaged framing and does not repair the archive.
- Unix extraction uses directory file descriptors and no-follow operations.
  The non-Unix path-based fallback does not offer the same resistance to
  filesystem path races.

## Validate changes

From the workspace root, run the focused crate checks during development and
the full workspace checks before release:

```sh
cargo fmt --all -- --check
cargo test -p zarlib
cargo clippy -p zarlib --all-targets -- -D warnings
cargo test --workspace
```
