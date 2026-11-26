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

For applications that need file data in memory, use
`operations::extract_archive_to_memory`. It returns a `HashMap` from canonical
archive-relative file paths to byte vectors. The optional size limit caps the
bytes returned per file after the specified offset. Directories and special
entries are omitted. The filtered variant accepts the same `FilterSet` used by
file extraction.

For repeated random reads from an unchanged archive,
`PreparedExtraction::new_complete` validates the selected history and rejects
locked headers. `read_file_range` reads one regular file by offset and length
without rebuilding that state. It validates decoded blocks, caches one decoded
block, and verifies the file checksum only when reading the whole file.
If the archive changes or is replaced, subsequent reads fail. The
[`zarmount` FUSE filesystem](../zarmount/README.md) uses this API.

## Searching files

`grep_archive` searches regular files in parallel on native targets (sequentially
on WASI). Its file filter is a case-sensitive archive-relative glob using the
same rules as `FilterSet`, and its pattern is a `fancy_regex` byte regex matched
against each line, with support for lookaround and backreferences. For example:

```rust,no_run
use zarlib::operations::{grep_archive, GrepOptions};

let files = grep_archive("backup.zar", "*.rs", "TODO|FIXME", GrepOptions {
    before: 1,
    after: 2,
    ..Default::default()
})?;
for file in files {
    for line in file.lines {
        println!("{}:{}:{}", file.path, line.line_number, String::from_utf8_lossy(&line.bytes));
    }
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

Results contain only files with matches, ordered by archive path and line
number, with overlapping context returned once and each line marked as a
match or context. Line bytes exclude LF or CRLF terminators; non-UTF-8 bytes
are preserved. Matching is Unicode-aware even on non-UTF-8 input; invalid
bytes cannot be matched individually. Unlike the previous regex engine,
`fancy_regex` does not support inline Unicode mode changes such as `(?-u:...)`.
`GrepOptions` also supports a commit limit and per-commit
passwords. The archive must remain unchanged while searching. Selected files
are fully validated, even when they have no matches. Regex matching errors,
including exceeded backtracking limits, fail the search. No temporary files
are created: file contents are searched in memory as blocks are decoded, and
lines spanning blocks are reassembled before matching. Memory use includes
returned lines, before-context, the longest line and decoded blocks.

For repeated or incremental searches, `PreparedGrep::new` reads and unlocks
the headers once. Its `search` method filters paths before scheduling file
workers and invokes a thread-safe callback as lines arrive. Use
`search_with_filters` to intersect a glob with an exact-path restriction.
Lines from different files can interleave; cancellation, callback errors, or
integrity failures can occur after some lines have been delivered. Selected
blocks and file checksums are checked while searching; unselected block payloads
are not checked. A prepared header rejects an archive that has changed.
The zarg Find dialog and the zarc grep shell reuse prepared headers across
searches, rebuilding them when their archive view changes.

## Streaming regular files

`create_archive_from_readers` consumes named `Read` inputs in blocks and writes
a standard single-commit ZAR archive to an empty `Write + Seek` destination
that permits writes at seeked positions (not append mode).
It handles regular files only: it does not deduplicate inputs, collect file
metadata, or use the parallel filesystem creation pipeline. Pass
`StreamEncryption` to enable password-based encryption. Block size must be
between 64 KiB and 32 MiB; unlike the path-based API, automatic sizing is not
supported. An error can leave a partial archive, so callers should stage the
destination if they need atomic publication and flush it when needed.

`extract_single_file_to_writer` accepts a `Read + Seek` archive and streams a
selected regular file to a `Write` destination. Archive bytes must start at
offset zero. It accepts a commit limit and per-commit passwords, rejects
locked headers in the selected history, and validates selected data and the
file checksum. Unselected data blocks are not necessarily checked; use
`check_archive_with_recovery` to verify the whole archive. The writer may
contain partial data after an error; flush it in the caller when needed. The
archive must not change during extraction.

```rust,no_run
use std::io::Cursor;
use zarlib::operations::{
    create_archive_from_readers, extract_single_file_to_writer, EncodingOptions,
    PasswordSet, StreamInput,
};

let mut source = Cursor::new(b"hello from a stream");
let mut archive = Cursor::new(Vec::new());
create_archive_from_readers(
    &mut archive,
    &mut [StreamInput { path: "hello.txt", reader: &mut source }],
    EncodingOptions::default(),
    None,
)?;
let mut output = Vec::new();
extract_single_file_to_writer(
    &mut archive, "hello.txt", &mut output, &PasswordSet::default(), None,
)?;
assert_eq!(output, b"hello from a stream");
# Ok::<(), Box<dyn std::error::Error>>(())
```

These APIs buffer compressed and decoded blocks, as well as archive metadata;
they do not require buffering an entire file. They do not provide non-seekable
archive input or output. Existing path-based APIs retain their existing
parallel processing and filesystem behavior.

### TCP streaming example

The [server](../zarlib/examples/streaming_server.rs) sends one regular file,
and the [client](../zarlib/examples/streaming_client.rs) reads it directly from
the socket into a new seekable archive, then extracts it to stdout. From the
workspace root, run the server and client in separate terminals:

```sh
cargo run -p zarlib --example streaming_server -- 127.0.0.1:7878 README.md
```

```sh
cargo run -p zarlib --example streaming_client -- 127.0.0.1:7878 received.zar README.md > received.txt
```

The server accepts one connection and the input file must not change during
transfer. The client refuses to overwrite an existing archive. The protocol
uses an eight-byte big-endian length and rejects incomplete transfers. On error
the archive or redirected output may be partial; discard it. This is a
minimal local demonstration, without authentication, TLS, or transfer size
limits, and is not suitable for untrusted networks.

The server logs client requests to stderr by default. Pass `-q` or `--quiet`
to suppress request logs; its listening message and errors remain visible.

To serve selected files from ZAR archives in a directory, start the server with
the directory instead of a file:

```sh
cargo run -p zarlib --example streaming_server -- 127.0.0.1:7878 /path/to/archives
cargo run -p zarlib --example streaming_client -- 127.0.0.1:7878 --list
cargo run -p zarlib --example streaming_client -- 127.0.0.1:7878 \
  --zar example.zar --output output --select 'docs/*.md' --select '*.txt'
```

The directory server handles requests sequentially until stopped. `-l` or
`--list` lists regular `.zar` files directly inside the configured directory.
For a transfer, `-z` or `--zar` names one archive and repeated `-s` or
`--select` options provide quoted fnmatch patterns. Archive names and patterns
are case-sensitive. Without patterns, all regular files are selected. Patterns
match archive-relative paths using zarlib's filters (`*` can match `/`);
matching files retain their paths under
`-o` or `--output` (the current directory by default). Symlinks and other
special entries are not transferred. A missing archive, invalid pattern, or no
matching files is an error. Encrypted archives requiring passwords are not
supported. The server prepares and sends one file at a time, keeping files up
to 8 MiB in memory and staging larger files under unique temporary names.
Case-distinct archive paths are sent unchanged. On a case-insensitive client filesystem, distinct paths
may still collide: the client refuses existing files by default, but `-f` or
`--force` can overwrite an earlier received file. An interactive progress bar with the same byte,
throughput, and ETA display as `zarc` is shown on stderr by default;
`--no-progress` disables it. A failed transfer may leave partial output.

Directory mode uses a separate request/response protocol from the one-file
example: a `ZARREQ03` magic value, length-prefixed UTF-8 archive name and
patterns, followed by either an error or the file count, total uncompressed
bytes, and a per-file status followed by length-framed paths and file contents.
An empty archive name and zero patterns request the archive list. The server
also accepts older `ZARREQ01` and `ZARREQ02` requests, which stage all selected
files before responding; `ZARREQ01` responses omit the total byte count. The
current client requires a server that supports `ZARREQ03`. Requests are limited
to 64 patterns and 64 KiB per text field;
there is no file-size or total-transfer limit. Like the original example, this
mode has no authentication or TLS. Run it only on a trusted network, with a
trusted archive directory and output directory that other users cannot modify
during a transfer.

On Linux, `python scripts/test_streaming_bandwidth.py` builds the release
examples and sends `testdata/linux-4.0.1.zar` over loopback limited to
100 Mbit/s twice. The first client writes the received ZAR; the second stages
the ZAR and uses zarlib's existing tree extraction to restore the files. The script verifies
the archive hash and compares file contents, directories, and symlink targets
against a reference extraction. It samples server and client CPU and resident
memory every 0.5 seconds. The summary reports effective throughput for both
compressed archive bytes and uncompressed regular-file contents; elapsed time
includes client output and extraction, not just network transfer. CPU is
expressed as 100% per core, and RSS peaks are sampled estimates.

The test needs `cargo`, `unshare`, `ip`, and `tc`, and permission to create
unprivileged user and network namespaces. The limiter is confined to the
temporary namespace, not the host loopback. Several GiB of temporary disk
space and several minutes may be needed. Use `--archive PATH` to select
another ZAR archive, or `--timeout SECONDS` to change the one-hour timeout
per test.

See the [streaming guide](./streaming.md) for a detailed interpretation of a
supplied run and its rate-label discrepancy.

Because ZAR headers follow the data, tree extraction starts after the client
has staged the complete archive. Both the reference and the client use the
existing `rename` collision policy: names that collide when case-folded may
be renamed, even on a case-sensitive filesystem.

## Custom archive metadata

The Rust API can read and update arbitrary archive-level key/value metadata
through `operations::read_custom_metadata` and
`operations::update_custom_metadata`. Updates append a metadata-only commit;
they do not rewrite existing commits. Values are `MetadataType` variants
(`String`, `I64`, or `F64`), and keys beginning with `zar.` are reserved.
Metadata updates currently require an unencrypted archive.

```rust,no_run
use zarlib::{MetadataType, operations::{read_custom_metadata, update_custom_metadata}};

update_custom_metadata(
    "backup.zar",
    &[("project".to_owned(), MetadataType::String("example".to_owned()))],
    &[],
)?;

let metadata = read_custom_metadata("backup.zar")?;
println!("{metadata:?}");
# Ok::<(), Box<dyn std::error::Error>>(())
```

Later commits override earlier values; removing a key hides its earlier value.
The CLI offers the same feature with `zarc meta backup.zar --set
project=example`, `zarc meta backup.zar`, and
`zarc meta backup.zar --remove project`. The GUI exposes it through its Custom
metadata dialog.

## Examples matching the `zarc` commands

These snippets use the same operations as the `zarc` create, add, remove,
join, check, info, list, and extract commands. Run them from a working
directory containing `src/`, `README.md`, and any other referenced input
files. `PathBuf` inputs are mapped to archive-relative paths.
These examples use an unencrypted archive. Encrypted commits can be appended.
The library's `remove_archive_entries_with_progress_and_encryption` operation
can remove entries from encrypted archives when supplied the passwords for all
encrypted commits, and can encrypt the removal commit. Commits whose encrypted
headers cannot be unlocked are omitted from passwordless listing and
extraction; removals of entries in those hidden commits are treated as no-ops
in the visible state. The password-aware join operation consolidates readable
commits in the selected range at the first readable commit's position, while
preserving locked commits and their relative order. Supply passwords by commit
ID for encrypted commits to include them in the join.

### Create

Equivalent CLI: `zarc create backup.zar src/ README.md`

```rust,no_run
use std::path::PathBuf;
use zarlib::operations::create_archive;

create_archive(
    "backup.zar",
    &[PathBuf::from("src"), PathBuf::from("README.md")],
)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`create_archive` and `add_archive` also accept `.zip`, `.7z`, `.rar`, `.iso`, `.tar`, `.tar.gz`,
`.tgz`, and `.gz` paths in their input lists. Entries are imported using their internal
paths, then processed like other inputs. Encrypted ZIP/7z/RAR sources and ZIP/7z/RAR
links are rejected. Multi-volume RAR archives and RAR special entries are
rejected. ISO import reads ISO 9660, preferring Rock Ridge over Joliet and the
primary namespace. ISO links, special entries, and UDF-only images are not
supported. ISO imports are limited to 100,000 entries, 16 MiB of directory
metadata, and 64 directory levels. TAR symbolic links are preserved where the host supports
them, hard links are imported as regular files, and TAR special entries are
rejected. Safe TAR paths with leading `./` components are normalized, but
parent traversal is rejected. Source archive timestamps are not retained;
when `save_modification_time`, `save_creation_time`, or `save_access_time` is enabled, the
import-time filesystem timestamps are
recorded instead. 7z entries using COPY, LZMA/LZMA2,
BZip2, PPMD, DEFLATE, Brotli, LZ4, or Zstandard are supported.

### Add or replace files

Equivalent CLI: `zarc add backup.zar config.toml`

```rust,no_run
use std::path::PathBuf;
use zarlib::operations::add_archive;

add_archive("backup.zar", &[PathBuf::from("config.toml")])?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

### List and inspect commits

Equivalent CLI: `zarc list backup.zar` and `zarc info backup.zar`

```rust,no_run
use zarlib::operations::{info_archive, list_archive};

let toc = list_archive("backup.zar", None, None)?;
for (path, entry) in toc.iter() {
    println!("{}: {entry:?}", path.as_str());
}

for commit in info_archive("backup.zar", false)? {
    let id = commit
        .id
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    println!("commit {id}: {} bytes", commit.size);
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Check integrity

Equivalent CLI: `zarc check backup.zar`

```rust,no_run
use zarlib::operations::check_archive_with_recovery;

let report = check_archive_with_recovery("backup.zar", false, None)?;
println!(
    "{} commits checked; {} data blocks verified",
    report.commits, report.checked_blocks
);
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Extract files

Equivalent CLI: `zarc extract backup.zar --output restored`

```rust,no_run
use std::path::Path;
use zarlib::operations::extract_archive;

let plan = extract_archive("backup.zar", Path::new("restored"), false)?;
println!("Extracted {} files", plan.files.len());
# Ok::<(), Box<dyn std::error::Error>>(())
```

Pass `true` for the final argument to preview the extraction without writing
files.

### Remove a path

Equivalent CLI: `zarc remove backup.zar obsolete.txt`

```rust,no_run
use std::path::PathBuf;
use zarlib::operations::remove_archive;

remove_archive("backup.zar", &[PathBuf::from("obsolete.txt")])?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Removal paths are archive-relative names, not filesystem paths to delete.
Removing a directory also removes its descendants from the effective table of
contents; earlier commits remain unchanged.

### Remove commits

Equivalent CLI: `zarc remove-commits backup.zar 00000002 00000004 --output reduced.zar`

```rust,no_run
use zarlib::operations::remove_archive_commits;

let ids = vec!["00000002".to_owned(), "00000004".to_owned()];
let result = remove_archive_commits("backup.zar", &ids, Some("reduced.zar"))?;
if !result.replay_validated {
    eprintln!("Retained encrypted headers prevented replay validation");
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

Pass `None` for the output to replace the input archive. At least one commit
must remain. Remaining commits are replayed in order, and incompatible
histories are rejected when all retained headers are readable. Encrypted
headers are copied without passwords, so their effects cannot be validated;
`replay_validated` reports whether that validation was performed.

### Join a commit range

Equivalent CLI: `zarc join backup.zar --from <first-id> --to <last-id>`

The IDs are the eight-hex-digit commit IDs shown by `info_archive`. This example
compacts the first two commits in physical order:

```rust,no_run
use zarlib::operations::{EncodingOptions, info_archive, join_archive_with_encoding};

let commits = info_archive("backup.zar", false)?;
if commits.len() >= 2 {
    let id = |bytes: &[u8; 4]| {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
    };
    join_archive_with_encoding(
        "backup.zar",
        &id(&commits[0].id),
        &id(&commits[1].id),
        EncodingOptions::default(),
    )?;
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

The range endpoints are inclusive and must identify a contiguous range in
physical commit order.

### Operation reference

The operation modules and their commonly used public entry points are:

| Module | Main APIs | Purpose |
| --- | --- | --- |
| `operations::create` | `create_archive`, `create_archive_with_options`, `create_archive_with_encoding`, `create_archive_with_password_progress_and_filters`, `create_archive_with_password_progress_and_filters_and_aead` | Create a new archive; optional strict scanning, encoding options, password encryption, AEAD selection, and path filters. |
| `operations::add` | `add_archive`, `add_archive_with_encoding`, `add_archive_with_encoding_progress_and_options_and_filters` | Append a commit that adds or replaces paths, optionally filtered. |
| `operations::remove` | `remove_archive` | Append tombstones for existing archive-relative paths. |
| `operations::remove_commits` | `remove_archive_commits` | Remove selected commits in place or to a separate archive. |
| `operations::join` | `join_archive_with_encoding` | Compact a contiguous commit range. |
| `operations::check` | `check_archive_with_recovery` | Validate framing and readable archive contents, optionally recovering past damaged framing. |
| `operations::info` | `info_archive` | Read commit summaries, optionally including verbose file-group statistics. |
| `operations::list` | `list_archive`, `load_toc_through` | Load the effective TOC, optionally as of a commit ID. |
| `operations::extract` | `extract_archive`, `extract_archive_with_options`, `extract_archive_with_progress_and_filters` | Plan and perform extraction with explicit policies, filters, and password support. |

`FilterSet::from_patterns` builds reusable fnmatch filters for create, add, and
extract operations. Include rules limit matching entries; exclude rules take
precedence. Folder rules apply to the matching folder and its descendants.
Patterns match archive-relative paths, and `*` can match `/`. Its final
argument accepts filter-list paths whose non-empty lines use the CLI
`--filelist` syntax. The filtered create API packages its input settings and
filter reference in `CreateFilterOptions`; the add and extract variants accept
the filter set separately.

`operations` also re-exports common option, report, and callback types,
including `EncodingOptions`, `Progress`, `CheckReport`, `CommitInfo`,
`ExtractionOptions`, `CollisionPolicy`, and `ExistingPolicy`. Read the function
signatures in the module for the options appropriate to your use case.

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

`ZARLIB_MAX_CPU_COUNT` controls the requested worker count for parallel
compression and extraction. If unset or `0`, zarlib uses the available cores.
A positive integer requests that many workers; a negative integer reserves
that many available cores, retaining at least one worker. For example, `-2`
uses the available core count minus two, or one if fewer than three are
available. Invalid or out-of-range values produce a warning and use the default.
Memory limits can reduce the actual worker count further. This is a per-operation
worker setting, not a process-wide limit on threads or CPU usage.

`ZARLIB_CPU_PRIORITY` optionally reduces the priority of zarlib-owned
compression, file-reading, and parallel extraction worker threads. Unset or
`0` leaves priority unchanged. Positive values increase each worker's inherited
Linux nice value by that amount, up to 19. Windows lowers the worker's thread
priority by that many levels, down to `THREAD_PRIORITY_LOWEST`; on macOS any
positive value places the worker in background scheduling mode. Other
platforms, including WASI, do not support the setting. Invalid values,
unsupported platforms, and failures to change priority produce warnings.
The calling thread and unrelated application threads are not adjusted; work
performed directly on the caller, including serial operations, is unaffected.

Applications can set `WorkerOverrides` with `set_worker_overrides` to override
either environment setting safely at runtime. `None` uses the corresponding
environment variable; `Some(0)` explicitly selects its zero behavior. New
worker pools use the updated values, while existing pools are unchanged. The
overrides are process-wide and do not modify environment variables.

For path-based create and add operations, `ZARLIB_SKIP_COMPRESS_EXTENSIONS`
replaces the default list of extensions whose data is stored without
compression when new blocks are written. Set it to a
comma-separated list, for example `ZARLIB_SKIP_COMPRESS_EXTENSIONS=jpg,png,zip`.
Matching is ASCII case-insensitive; surrounding ASCII whitespace and an
optional leading `.` are ignored. An empty value disables the extension-based
skip list. Unsetting the variable restores `SKIP_COMPRESS_EXTENSIONS`. The
setting is process-wide and is consulted as files are processed; it does not
apply to `create_archive_from_readers`. Blocks that do not compress effectively
may still be stored uncompressed regardless of their extensions; deduplicated
files can reuse existing blocks regardless of the extension policy.

## Python bindings

The workspace also includes [pyzarlib](../pyzarlib/README.md), a PyO3 package
that exposes high-level create, add, remove, join, check, info, list, and
extract operations to Python. See its README for build instructions, API
options, and examples.

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
  optional xxhash3 file checksum. `Toc::apply_commit_with_available_blocks`
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
stream data before the complete logical header is known. See the
[ZAR file architecture diagram](./zar-file-format.md) for the current
container, commit, header-frame, and block-frame layouts. Changes to the
logical-header wire layout require an explicit versioned compatibility
decision.

For the execution flow of the CLI `create` and `add` operations, see the
[PlantUML processing pipelines](./processing-pipelines.md).

## Encryption and operation constraints

The library exports AES-256-GCM and ChaCha20-Poly1305 content primitives,
Argon2id password-based DEK wrapping, stable encryption descriptors, and
zeroizing key-handling helpers. The high-level creator defaults to
AES-256-GCM. To select the content algorithm, use
`create_archive_with_password_progress_and_filters_and_aead`; the selected AEAD
is used both for the content and the password-wrapped data-encryption key.
Pass the password as bytes and never log or persist those bytes unnecessarily.

`create_archive_with_password_progress_and_filters_and_encryption` additionally
selects `EncryptionScope::Data`, `EncryptionScope::Header`,
`EncryptionScope::HeaderAndData`. Data-only encryption leaves the logical TOC
readable: `info_archive` and `list_archive` can read it without credentials,
while extraction still requires the password for the data blocks. `info`
reports each commit's header/data encryption state and content algorithm.

Each commit may use a different password and AEAD. Operations that read
multiple encrypted commits accept `PasswordSet`: insert a password for a
specific `[u8; 4]` commit ID with `insert_for_commit`. A password supplied as
the set's default is used for every encrypted commit without an explicit
entry; explicit entries take precedence. Existing single-password API
functions remain available and use that password as the default for all
encrypted commits.

```rust,no_run
use zarlib::operations::{
    PasswordSet, extract_archive_to_memory_with_passwords, info_archive,
};

fn read_mixed_archive() -> Result<(), Box<dyn std::error::Error>> {
    let archive = "mixed.zar";
    let mut passwords = PasswordSet::default();
    passwords.insert_for_commit([1, 0, 0, 0], b"older-password".to_vec());
    passwords.insert_for_commit([2, 0, 0, 0], b"newer-password".to_vec());
    let details = info_archive(archive, false)?;
    println!("commit 1 header encrypted: {}", details[0].header_encrypted);
    let files =
        extract_archive_to_memory_with_passwords(archive, None, &passwords, None, 0)?;
    println!("extracted {} file(s)", files.len());
    Ok(())
}
```

### Create AES-256-GCM or ChaCha20-Poly1305 archives

The following example creates two separate encrypted archives with the same
input, choosing one of the supported algorithms for each. The password prompt
is intentionally outside this example; obtain it securely rather than
hard-coding it in a real application.

```rust,no_run
use std::path::PathBuf;
use zarlib::ContentAead;
use zarlib::operations::{
    CreateFilterOptions, CreateInputOptions, EncodingOptions, FilterSet,
    create_archive_with_password_progress_and_filters_and_aead,
};

fn create_encrypted_examples(password: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let paths = vec![PathBuf::from("secrets")];
    let filters = FilterSet::default();
    for (archive, aead) in [
        ("secrets-aes.zar", ContentAead::Aes256Gcm),
        ("secrets-chacha.zar", ContentAead::ChaCha20Poly1305),
    ] {
        create_archive_with_password_progress_and_filters_and_aead(
            archive,
            &paths,
            false,
            EncodingOptions::default(),
            Some((password, aead)),
            None,
            CreateFilterOptions {
                input: CreateInputOptions::default(),
                filters: &filters,
            },
        )?;
    }
    Ok(())
}
```

`ContentAead::Aes256Gcm` selects AES-256-GCM; `ContentAead::ChaCha20Poly1305`
selects ChaCha20-Poly1305. The library generates a fresh data-encryption key
and nonces, and uses Argon2id to derive the password key that wraps the data
key. Do not create or reuse your own nonce/key material for these high-level
operations.

### Unlock, check, list, and extract

The algorithm is recorded in each commit's encryption descriptor. Readers
select the matching AEAD from the descriptor, so the same password-aware
operations work for both algorithms:

```rust,no_run
use std::path::Path;
use zarlib::operations::{
    CollisionPolicy, ExistingPolicy, ExtractionOptions, check_archive_with_recovery,
    extract_archive_with_progress, list_archive,
};

fn read_encrypted_example(password: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let archive = "secrets-chacha.zar";
    let check = check_archive_with_recovery(archive, false, Some(password))?;
    println!("checked {} commit(s)", check.commits);

    let toc = list_archive(archive, None, Some(password))?;
    for (path, entry) in toc.iter() {
        println!("{}: {entry:?}", path.as_str());
    }

    let plan = extract_archive_with_progress(
        archive,
        Path::new("restored"),
        ExtractionOptions {
            dry_run: false,
            to: None,
            collision_policy: CollisionPolicy::Rename,
            existing_policy: ExistingPolicy::Error,
            restore_metadata: false,
            restore_symlinks: false,
            restore_devices: false,
        },
        Some(password),
        None,
    )?;
    println!("extracted {} file(s)", plan.files.len());
    Ok(())
}
```

For a dry run, set `dry_run: true`. A missing or incorrect password fails
authentication. `check_archive_with_recovery` can report encrypted payloads as
unverified when no password is supplied; use the password when you need
authenticated content validation.

The `zarc` CLI also provides password prompting without echo and defaults to
AES-256-GCM:

```sh
zarc create --password secrets-aes.zar secrets/
zarc create --password --aead chacha20-poly1305 secrets-chacha.zar secrets/
zarc create --password --encryption-scope data visible-header.zar public/
zarc add --password --aead chacha20-poly1305 archive.zar updated/
zarc check --password secrets-chacha.zar
zarc list --password secrets-chacha.zar
zarc extract --password secrets-chacha.zar --output restored/
zarc extract --password-for 00000001 --password-for 00000002 mixed.zar
```

Do not pass passwords as command-line arguments; the CLI prompts interactively.
The CLI accepts `--password-for COMMIT_ID` repeatedly for commit-specific
prompts; a plain `--password` supplies a fallback used for all encrypted
commits. `add` supports appending an encrypted commit when credentials are
available for existing encrypted commits it validates. `remove` and `join`
still reject encrypted archives because they cannot rewrite encrypted state.

Keep these current constraints in mind:

- Password-encrypted commits can encrypt data, headers, or both.
- Listing, checking, and extraction support password-aware access to encrypted
  content. When headers or blocks are encrypted, supply the correct password
  to authenticate and read them.
- `add` can append encrypted commits. `remove` and `join` do not yet rewrite
  encrypted stateful TOCs.
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

## Archive format benchmark

See the [benchmark instructions](./benchmarks/README.md#running-benchmarks)
for GUI and cross-format script usage, prerequisites, cache modes, output
interpretation, and important cleanup warnings.
