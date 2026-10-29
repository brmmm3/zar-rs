# `zar` format plan

## Rust

- Use latest dependencies
- Use edition 2024

## Product scope and current implementation

This plan describes the format and the user-facing project built around it. The
implemented workspace contains the Rust library (`zarlib`), command-line tool
(`zarc`), desktop application (`zarg`), Python package (`pyzarlib`), C library
(`czarlib`), C++ CXX-bridge package (`cxxzarlib`), and Go cgo package
(`gozarlib`). Public APIs and
documentation must agree on format
semantics, password handling, extraction policies, and unsupported operations.

The current scope includes readable and password-encrypted commits; create,
append, remove, join, info, list, check, filesystem extraction, and in-memory
extraction; include/exclude scanning filters; and Rust, Python, C, C++, CLI,
and GUI entry points where listed below. Do not treat aspirational details as supported
features unless they are implemented in the library and exposed through the
relevant interface.

## 0. File grouping

Files are grouped into following types (Rust enum):

- Empty files (size 0), which have no data blocks or file checksum.
- Tiny files (sizes 1–8 bytes), stored as file data without a file-level checksum; computing a 64-bit checksum for data no larger than the checksum is not useful. Frame checksums still protect stored block payloads.
- Small files (sizes 9 bytes through one byte below block size), collected into blocks and carrying a file-level xxhash64 checksum.
- Big files (size at least block size), split into blocks and carrying a file-level xxhash64 checksum.

The current TOC model stores file entries uniformly; `FileGroupKind` derives these groups from file size and the commit's block size. File-level checksum presence is optional in the logical header, so the tiny-file rule does not change the wire format. Deduplication candidates are partitioned by uncompressed file size before checksum or content comparison; files of different sizes are never compared. Files of at least 9 bytes are matched by size plus xxhash64, while tiny files are matched by size plus exact content. During archive creation and append, files with extensions `gz`, `tgz`, `bz2`, `tbz2`, `lzo`, `lz4`, `lzma`, `z`, `snappy`, `zip`, `7z`, `rar`, `jpg`, `jpeg`, `png`, `gif`, `mpg`, `mpeg`, `avi`, `mp3`, or `zar` are stored uncompressed (extension comparison is case-insensitive). `zarc info -v` reports per-commit file counts and compressed/uncompressed sizes by file group, compression ratio, and deduplicated file count with saved uncompressed size. It reads only the logical commit header and block-frame metadata; it does not load payloads, verify their checksums, or decompress them. Its compressed size is stored block payload bytes; frame overhead is excluded. `zarc list` likewise reads and validates commit headers plus fixed-size block metadata needed for TOC references, without reading data payloads or decompressing data blocks.

- Small files (smaller than block size)
  - SmallFiles(Vec<DirEntrySmall>)
  - DirEntrySmall:
    - DirEntry
    - BlockNumer
    - OffsetInBlock
- Big files (files with the size at least block size)
  - BigFiles(Vec<DirEntry, Vec<(BlockOffset, BlockSize)>>)
- Empty files
  - EmptyFiles(Vec<DirEntry>)
- Symbolic links, named pipes, and devices are currently reported and skipped
  during create/add by default; `--strict` rejects the unsupported inputs.
  Their logical-header record tags are reserved/decoded, but create and
  extraction do not currently provide end-to-end support for these types.
- Hard links are not represented as distinct entries. Identical file contents
  can share block references through content deduplication.
- Removed files (files to remove from container)
  - Remove(Vec<String>)

## 1. Commit framing and integrity

A file begins with a container header. Commits follow sequentially; the commit chain is the physical order in the file. Each commit stores its data blocks first and its header last, so writers can stream data blocks directly to the final file and append the header once the TOC is finalized—no staging file is needed.

### Container header (12 bytes, all integers little-endian)

```text
ContainerHeader
  magic                 [4]  "ZAR0"
  first_header_offset   u64  absolute file offset to the first commit's header
```

The container header is written once when the archive is created. The `first_header_offset` is filled in after the first commit's header is written.

### Per-commit layout

Each commit is laid out on disk as:

```text
Commit:
  next_header_offset    u64   absolute offset to the next commit's header (0 if last commit)
  data blocks...              data-block frames in block-index order
  header frame                HeaderFrame (contains commit metadata, TOC, encryption descriptor)
  this_header_offset    u64   absolute offset to this commit's header start
```

The `next_header_offset` at the start of each commit forms a forward chain: the container header points to the first commit's header, and each commit's `next_header_offset` points to the next. The `this_header_offset` at the end of each commit enables reverse traversal: seeking to the end of the file and reading the last 8 bytes yields the most recent commit's header offset.

### Reading a commit

1. Read the container header (magic + `first_header_offset`).
2. Seek to `first_header_offset` and read the header frame. The header frame's cleartext metadata includes `commit_id`, `data_section_size`, `header_frame_size`, and the encryption descriptor (see section 2).
3. The 8-byte `next_header_offset` slot is at
   `[header_offset - data_section_size - 8, header_offset - data_section_size)`.
4. Data blocks occupy
   `[header_offset - data_section_size, header_offset)`.
5. To read the next commit, seek to `next_header_offset` and repeat from step 2. A zero `next_header_offset` indicates the last commit.

To find the last commit directly: seek to the end of the file, read the final 8 bytes (`this_header_offset`), and seek to that offset.

### Writing a commit (no staging file)

1. Record the current file position as `commit_start`.
2. Write 8 bytes of zeros as a placeholder for `next_header_offset`.
3. Scan input files, compress data blocks, and write block frames directly to the file. Track `data_section_size` as the total bytes of block frames written.
4. After all blocks are written, build the logical header (TOC, block references, metadata) and encode the header frame. Record `header_offset` as the current file position.
5. Write the header frame.
6. Write `this_header_offset` (the `header_offset` from step 5) as 8 bytes.
7. If this is the first commit, seek to the container header and write `first_header_offset`. If a previous commit exists, seek to that commit's `next_header_offset` placeholder and write `header_offset`.

### Integrity and validation

Reject arithmetic overflow, invalid sizes, and unsupported framing versions. The header frame checksum detects accidental corruption; it is **not** an authentication mechanism. A reader can skip a commit whose header it cannot decrypt by using `next_header_offset` to advance to the next commit. For resynchronization after damaged framing, an implementation may scan for header-frame magic, but must validate the frame and report that recovery was attempted.

The total commit size is computed as `8 + data_section_size + header_frame_size + 8` (next_header_offset + data + header + this_header_offset).

A commit’s header contains its own metadata and TOC. Commit-local encryption settings live in that header—not in a container-level header.

## 2. Header framing, compression, and encryption

The header frame has cleartext metadata followed by the payload:

```text
HeaderFrame
  magic                 [4]  "ZCMT"
  framing_version       u8
  flags                 u8
  commit_id             [4]  archive-scoped counter starting at 1 (little-endian u32)
  data_section_size     u64  total bytes of all data-block frames in this commit
  header_frame_size     u32  total bytes of this entire header frame (metadata + payload)
  compressed_header_len u32  length of the compressed/encrypted header payload
  header_checksum       u64  xxhash64 over the exact on-disk header payload bytes
  encryption_descriptor_len u32
  encryption_descriptor  [encryption_descriptor_len] bytes, bounded
  header_payload         [compressed_header_len] bytes
```

The header frame begins with the `ZCMT` magic and contains all commit-level metadata that was previously in the `CommitPrefix`. The `header_frame_size` field is the total size of this frame (all fields from `magic` through `header_payload`), enabling a reader to skip the entire header in one seek. The `data_section_size` field tells the reader how many bytes of data blocks precede the header. The `commit_id` is a 4-byte little-endian counter.

The descriptor has a stable wire encoding, not Rust enum discriminants. It identifies the mode, algorithms, nonce/KDF parameters, and recipient-key records required to decrypt the header. It is cleartext so a reader can select the decryption method. It is covered by the commit’s authenticated data when the header is encrypted.

The logical-header v0 canonical wire encoding is:

```text
LogicalHeader
  magic                 [4]  "ZHDR"
  version               u16  0
  record_count          u32
  records               [record_count] ordered records

Each record begins with:
  tag                   u8   1=put file, 2=put directory,
                             3=remove file, 4=remove directory
  path_len              u32
  path                  [path_len] UTF-8 canonical archive-path bytes

Put-file records then contain:
  file_size             u64
  block_reference_count u32
  block_indices         [block_reference_count] u32 values
```

All integers are little-endian. Record order is significant and is preserved exactly; block-index order is also preserved. Paths use the canonical archive-path rules in section 6. A zero-length file has no block references; a non-empty file has at least one. Duplicate block indices within one file are invalid. The encoding has no padding or trailing bytes. Readers reject unsupported versions, unknown record tags, invalid UTF-8 or paths, truncation, trailing bytes, and configured size/count limit violations.

Logical-header version 4 adds commit-scoped arbitrary metadata, version 5 adds per-entry platform metadata, and version 6 adds symbolic-link, named-pipe, and Unix device records while retaining version 0–3 compatibility. The v4 prefix is `magic`, `version`, `block_size` (`u32`, zero when absent), `metadata_count` (`u32`), metadata entries, then `record_count` and the existing records. Metadata entries are sorted by UTF-8 key bytes when encoding for deterministic output; each entry stores `key_len` (`u32`), key bytes, a one-byte value tag (`1=String`, `2=i64`, `3=f64`), then the value (`String` uses `value_len` (`u32`) plus UTF-8 bytes; numeric values use 8 little-endian bytes, with `f64` represented by its IEEE-754 bit pattern). Keys must be unique. Readers reject unknown value tags, duplicate keys, invalid UTF-8, and configured metadata limits. Metadata is an in-memory `HashMap<String, MetadataType>` and is scoped to the commit header. `f64` values preserve their exact bit pattern. Version 5 adds a sorted entry-metadata table after the v4 fields and before `record_count`: entry count (`u32`), followed by canonical path, platform tag string (`u16` byte length; currently `unix`, `windows`, or `other`), and three optional `u32` fields (owner, group, permissions), each encoded as a presence byte and a little-endian value when present. The table is keyed by archive path and duplicate or non-record paths are invalid. `create` and `add` accept `--metadata` to capture platform owner/group/permissions, and `extract --metadata` restores matching-platform values; platform mismatches are skipped with a warning. Unix extraction restores owner/group best-effort (warns if unavailable) and then permissions; Windows metadata restoration currently reports unsupported values.

Version 6 reuses the version 5 prefix and entry metadata table. File-type tags are `5=put symlink`, `6=put named pipe`, and `7=put device`. A symlink record stores a UTF-8 target as `target_len` (`u32`) followed by target bytes; an empty target or NUL is invalid. A device record stores kind (`1=character`, `2=block`), `major` (`u32`), and `minor` (`u32`). File-type entries participate in path replacement and removal like regular non-directory entries and carry no data blocks.

Although the logical-header format can represent symlinks, named pipes, and
devices, current `zarc create`/`add` do not write those entries and instead
skip them with `W_UNSUPPORTED_TYPE`, or fail when `--strict` is enabled.
Extraction therefore does not restore these entry types end to end. Preserve
the safety requirements above if these types are implemented later.

The processing order is:

1. Serialize the logical header using the canonical version selected for its contents (v0–v3 for headers without metadata; v4 for commit-scoped metadata; v5 for entry metadata).
2. Compress it with Zstandard.
3. If encryption is enabled, encrypt the compressed bytes with AEAD.
4. Store the resulting payload and its descriptor.

`compressed_header_len` is the exact number of payload bytes on disk (ciphertext plus authentication tag when encrypted). `header_checksum` is xxhash64 over those exact on-disk payload bytes. Checksum verification can detect accidental damage before decryption; it does not replace AEAD authentication.

For encrypted headers, AAD is the serialized fixed commit-header fields,
followed by the stored encrypted payload length and descriptor length and
bytes. For encrypted data blocks, AAD is `CommitHeader::identity_bytes()`
(magic, version, flags, and commit ID; 10 bytes), followed by the stored
payload length and the serialized block flags, reserved field, block index,
and nonce. Do not include the checksum or payload itself in AAD. Specify the
encoding unambiguously; do not use Rust's in-memory struct layout.

For unencrypted headers, the checksum is still calculated over the stored compressed header payload. The descriptor says `Unencrypted`; no AEAD operation is performed.

## 3. Encryption, passwords, and nonces

Each commit has its own encryption policy. The currently exposed encrypted
create/add APIs support password-based encryption with:

- **No encryption**
- **Password encryption** using a password-derived key to wrap the commit DEK,
  with AES-256-GCM or ChaCha20-Poly1305 for content encryption.

The descriptor wire model reserves passwordless symmetric and HPKE recipient
key-wrap variants (X25519/HKDF-SHA256/AES-128-GCM). Until recipient-key
creation, unlocking, and public APIs are implemented and tested, these
descriptor variants are not a supported user workflow.

Use a fresh cryptographically random DEK per commit and a fresh random nonce for each independently encrypted object. Never reuse a nonce with the same key. For HPKE, record the full suite and encapsulated key (`enc`) per recipient; HPKE wrapping uses its own context and nonce sequence, separate from content AEAD.

Passwords must not be used directly as keys. Derive a wrapping key using Argon2id version `0x13` and a per-commit random 16-byte salt. Accepted parameters are `memory_kib = 65,536–1,048,576`, `iterations = 2–10`, and `parallelism = 1–16`. Writers default to `memory_kib = 65,536`, `iterations = 3`, and `parallelism = 1`, subject to user configuration. Readers validate parameters before running Argon2id and reject out-of-range values; they must not silently clamp them. A deployment may impose stricter resource limits. Use the derived key to wrap the DEK with an AEAD; record the salt, parameters, nonce, and algorithm in the clear descriptor. A wrong password must fail authentication. Password wrapping and HPKE wrapping can coexist, allowing multiple ways to recover the same commit DEK.

All on-disk enums use explicit one-byte tags, not Rust enum discriminants. Integers are little-endian. Variable-length byte strings use a `u32` length prefix. The encryption descriptor is at most 64 KiB; recipient count at most 1,024; recipient IDs 1–256 bytes; HPKE `enc` at most 1 KiB; wrapped DEK at most 4 KiB; password salt exactly 16 bytes. Readers reject unknown mandatory tags, duplicate recipient IDs, truncated fields, trailing bytes, and length overflows. Unknown optional TLVs may be skipped only when their high-bit optional tag is set. The format version defines mandatory tags and exact field order.

Header encryption uses the commit DEK and a dedicated random 96-bit header nonce. Each block encrypted under that DEK uses a fresh random 96-bit nonce. Reject duplicate nonces within the commit; never reuse a nonce with the same key. Random nonces are stored in each encrypted header/block frame.

The supported scopes are `data`, `header`, and `header-and-data`; password
creation defaults to `header-and-data`. Data-only encryption leaves the
compressed logical header readable, so `info` and `list` can inspect it
without credentials while data extraction still requires the password.
Header-only encryption authenticates the logical header while leaving blocks
unencrypted. Store and report each commit's scope and content AEAD
independently; an archive may mix encrypted and unencrypted commits, passwords,
and AES/ChaCha algorithms.

For archive reads, a password map associates passwords with canonical
eight-hex-digit commit IDs. An explicitly mapped commit password takes
precedence over a single default/fallback password; the fallback is tried for
every encrypted commit without an override. Keep the existing single-password
Rust APIs as fallback-compatible wrappers. Never put a password value in CLI
arguments or logs: the CLI prompts without echo and accepts repeated
`--password-for COMMIT_ID` prompts, with bare `--password` supplying the
fallback. Python and C accept equivalent per-commit mappings.

When the header is encrypted, the reader must first parse the cleartext prefix and bounded header descriptor, obtain the DEK, authenticate/decrypt the header, then decompress and parse it. The descriptor must say whether header, data blocks, or both are encrypted.

## 4. Block frames and data integrity

Define each block frame as:

```text
BlockFrame
  compressed_len          u32  stored payload length, including AEAD tag if encrypted
  flags                   u16
  reserved                u16
  block_index             u32
  [nonce                   [12] bytes, present only if NONCE_PRESENT is set]
  checksum                u64
  payload                 [compressed_len] bytes
```

For version 0, `flags` is a bit field: bit 0 is `ENCRYPTED`, bit 1 is `NONCE_PRESENT`, bit 2 is `UNCOMPRESSED`, and bits 3–15 are reserved and MUST be zero. `UNCOMPRESSED` indicates that the stored payload is raw block data and is valid for both encrypted and unencrypted blocks. `ENCRYPTED = 0` requires `NONCE_PRESENT = 0`; `ENCRYPTED = 1` requires `NONCE_PRESENT = 1`. The nonce is exactly 12 bytes and appears immediately after `block_index`. `reserved` must be zero; readers reject nonzero reserved bits or fields. `block_index` is zero-based and unique within the commit. Each encrypted block uses a fresh nonce under that commit's DEK; duplicate nonces within a commit are rejected. The checksum is xxhash64 over the exact stored payload bytes (ciphertext and tag if encrypted). It is not a checksum of the header.

The processing order for data is:

1. Build a plaintext block from file data.
2. Compress it with Zstandard. If the compressed output is not strictly smaller than the original block, store the original bytes with the `UNCOMPRESSED` block flag (bit 2) set.
3. Encrypt the selected representation if enabled; the flag is included in AEAD associated data.
4. Store the frame and checksum its stored payload.

For encrypted blocks, AAD is the exact serialized header-frame fields from `magic` through `encryption_descriptor` (excluding `header_checksum` and `header_payload`), concatenated with the exact serialized block-frame fields through `nonce`, excluding `checksum` and `payload`. Ensure all lengths are finalized before calculating AEAD. For unencrypted blocks, checksums provide corruption detection but no authenticity.

The current logical header stores file sizes and ordered block references, not
an independent uncompressed-size field for each block. Enforce configured
payload and decompressed-block limits before allocation/decompression, validate
frame checksums, and verify reconstructed file sizes against the logical
header. Preserve block indices and ownership by commit to detect missing,
duplicate, or cross-commit references.

## 5. Block sizing and parallelism

Automatic block size is in the range **1–8 MiB**, subject to a memory budget. Let:

```text
budget = 50% of currently available RAM
workers = number of compression workers actually selected
per_worker_budget = budget / workers
```

Choose an automatic block size no larger than 8 MiB and no larger than the per-worker budget, rounded down to a supported size. If the per-worker budget is below 1 MiB, use 1 MiB as the minimum block size and reduce worker count until the estimated total working set fits the budget. If even one 1 MiB worker would exceed the budget, use one worker and issue a warning; optionally offer a low-memory mode with smaller blocks.

The memory estimate must include input buffers, compression workspace, encryption buffers, and queued output—not just the block buffer. Bound the queue so parallel workers cannot collectively exceed the budget. Manual block size is 64 KiB–32 MiB; it may exceed the automatic budget, but the CLI must warn and limit concurrency or fail if the configured memory limit would be exceeded. File creation uses a bounded reader/compressor pipeline: a dedicated reader fills block buffers from the input filesystem while the calling thread compresses blocks in parallel and writes frames in input order. The bounded queue applies backpressure when compression or output is slower than reading, and its depth is capped by the selected worker count to keep memory bounded. This allows slow storage reads to overlap compression without buffering an entire file.

### Create/add pipeline

The `create` and `add` commands use a multi-stage, multi-threaded pipeline that overlaps I/O, deduplication, compression, and encryption. The pipeline is assembled after the input file tree has been scanned and its TOC is known.

**File grouping.** After the file tree is scanned, every supported regular file
is classified into the groups defined in section 0: empty, tiny, small, and big
files. Symlinks and special files are currently skipped with a warning (or
rejected in strict mode). Hard links are not stored as separate entry types;
deduplication of equal file contents can reuse block references.

**Scanning and bounded worker pipeline.** The scanner walks inputs in
deterministic order, validates archive-relative paths, applies include/exclude
filters, classifies supported files, and builds TOC state. Compression workers
are selected using available memory and bounded worker-memory estimates rather
than a fixed per-core topology. Small/tiny and big-block work is sent through
bounded queues; backpressure prevents unbounded in-flight buffers.

Workers compress blocks using Zstandard and encrypt them inline when data
encryption is selected. Deduplication uses uncompressed size and xxhash64 as a
candidate key, then confirms content equality before reusing data.

**Queues.** Bounded queues feed the workers:

- A small/tiny queue holds small and tiny files. Because small files are smaller than the block size, several small files are packed into a single block. Each queued item carries its block number and file id.
- A big-file queue holds block-sized slices of big files. Each queued item carries its block number and file id.

**Small/tiny workers.** Workers pull from the small/tiny queue and accumulate
small files into blocks. When a block is full, it is compressed (and encrypted
when configured) and forwarded for writing.

**Big-file workers.** Workers pull block-sized slices from the big-file queue,
compress (and encrypt when configured) each block, and forward finished
frames for writing.

**Deduplication.** Deduplicate files by uncompressed size and content, retaining
the existing tiny-file exact-comparison rule. Do not deduplicate across
incompatible block boundaries or encryption contexts.

**Writer.** Write finished blocks in block-index order, track
`data_section_size`, and append the validated header frame after all data has
been written. Bound queued output and preserve the block-index ordering
invariant.

The queues are bounded so that total in-flight memory (input buffers, compression workspace, encryption buffers, and queued output) stays within the memory budget described above. Backpressure is applied when a queue is full, so readers and workers block rather than buffering unbounded data.

## 6. File paths, TOC, and deduplication

Define a canonical archive path format: UTF-8, `/` separators, relative paths only, no empty components, `.` or `..` components, NUL bytes, or platform drive prefixes. Preserve the original display name separately only if required. Path comparison and collision behavior must be explicitly documented; a portable default is exact Unicode code-point comparison, without implicit case folding or normalization. Extraction must still detect collisions on the destination filesystem (for example, case-insensitive filesystems).

TOC entries are commit-local changes:

- Add/replace entries provide the latest file or directory metadata and data references.
- Remove-file entries delete that path.
- Remove-directory entries delete the directory and all descendants from the accumulated TOC.
- A later entry at a removed path recreates it.
- Directory entries are explicit and create the directory even if it has no files. Parent directories need not have explicit entries; extraction creates missing parents as needed.
- Removing a nonexistent path is a no-op.

For deduplication, xxhash64 is only a candidate lookup key. Confirm a match by comparing the full content bytes (or a cryptographic digest plus byte comparison) before reusing stored data. Hash collisions must never cause incorrect content substitution.

Use portable, tagged metadata records. Store only metadata that can be meaningfully represented, such as permissions and timestamps, with platform-specific values in optional, namespaced extensions. Owners should be stored as platform-specific identifiers rather than assumed portable names. Symlink targets must be validated according to extraction policy. Devices and other special files must require explicit opt-in to create on extraction.

Unsupported file types are ignored with warnings by default; strict mode makes
them errors. Do not silently follow symlinks during scanning. Hard links are not
stored as explicit entries in the current implementation.

## 7. Joining commits and history

Treat commits as an ordered log. Normal extraction applies commits from oldest to newest, producing the final TOC state.

A join MUST select a contiguous range in physical commit order; reject non-contiguous selections. Replace the selected range at the position of its first commit, preserving the order of commits before and after it. The replacement must preserve the logical result of applying the selected range to the preceding history, including tombstones for removals and all referenced data needed after the selected commits are removed. Do not append the replacement after later commits, because that would change which version wins. If any selected commit cannot be read or decrypted, abort the stateful join. A separate copy-through operation may preserve unreadable commits unchanged, but must not claim to have compacted or merged their state. Commits outside the selected range remain byte-for-byte unchanged when possible.

Write to a temporary file, fsync as appropriate, and atomically replace the original only after the new container validates. “Encrypted commits without a password remain as is” is only safe for a copy-through operation that does not need to incorporate their contents.

## 8. Extraction and errors

For extraction, first read and validate commit framing and build the selected
commit sequence. Apply readable commits in order. If any encrypted portion
needed for the requested operation lacks credentials or authentication fails,
return an explicit error before writing extracted files; do not silently skip
encrypted commits. Credentials are supplied as a per-commit password mapping
with one default fallback, as described in section 3.

Warnings have stable categories, including `W_UNSUPPORTED_TYPE` (unsupported
input type), collision and destination-policy warnings, and `W_RECOVERY_SCAN`
(damaged framing required recovery scanning). Errors include malformed
framing/metadata, failed authentication, missing or incorrect credentials,
invalid paths, decompression limits exceeded, and failed writes. A checksum
mismatch is an error for that commit; recovery is an explicit `check --recover`
operation and does not modify the archive.

## 9. CLI interface

Use a consistent command structure, for example:

```text
zarc create <archive.zar> <paths...> [options]
zarc add <archive.zar> <paths...> [options]
zarc remove <archive.zar> <paths...> [options]
zarc extract <archive.zar> [--to DIR] [--through COMMIT_ID] [options]
zarc list <archive.zar> [--through COMMIT_ID] [--long]
zarc info <archive.zar> [--commits] [--json]
zarc check <archive.zar> [--recover] [--strict]
zarc join <archive.zar> --from ID --through ID [options]
```

Provide command aliases `c`, `a`, `r`, `j`, `k`, `i`, `l`, and `e` for
create, add, remove, join, check, info, list, and extract respectively. Create
and add support `--block-size SIZE|auto`, `--compression-level`, include/exclude
file and folder patterns, filter-list files, optional metadata, special-file
selection, progress, and strict input handling. Create refuses an existing
destination by default; `--overwrite` enables replacement while preserving
failure cleanup and atomic publication. The GUI exposes the same operation
settings where supported.

Encryption options include `--password`, `--aead aes-256-gcm|chacha20-poly1305`,
and `--encryption-scope data|header|header-and-data` on encrypted create/add.
Passwords are entered through a no-echo prompt, never a command-line value.
Read operations accept `--password` for a fallback and repeatable
`--password-for COMMIT_ID` for commit-specific prompts. Commit IDs are
displayed and supplied in canonical eight-hex-digit counter order; never use
the raw little-endian byte order as the display value.

For extraction, expose separate options for path collisions (`--collision error|latest|first|rename|ask`) and existing destination files (`--existing overwrite|skip|error|ask`). `ask` is interactive-only and should fail clearly in non-interactive mode. Add `--dry-run` to show the final TOC and planned actions without writing files.

`--through COMMIT_ID` selects the commit with that exact 8-hex-digit displayed
ID and applies commits from the beginning through and including it. The ID
must exist and be unique; there is no index interpretation, and absent or
malformed IDs are errors. Any `ask` collision or existing-file policy requires
an interactive terminal and fails early in non-interactive mode. `info`
reports per-commit header/data encryption status and AEAD; JSON output is
available for machine-readable framing and encryption information.

`add`, `remove`, and other in-place operations should use a temporary output, validate it, and atomically replace the original. Use these exit codes:

```text
0  Success; no warnings
1  Success with warnings
2  Usage or option error
3  I/O error
4  Invalid or corrupt container
5  Authentication/credential failure
6  Strict-mode warning or validation failure
```

For `check`, exit code 4 indicates detected corruption; valid containers with warnings return 1. For other commands, the same codes apply, with strict-mode warnings returning 6.

## 10. Rust model sketch

Use explicit wire tags and separate on-disk metadata from runtime secrets. Avoid serializing Rust enum layouts directly.

```rust
struct ContainerHeader {
    magic: [u8; 4],        // "ZAR0"
    first_header_offset: u64,
}

struct CommitHeader {
    magic: [u8; 4],              // "ZCMT"
    framing_version: u8,
    flags: u8,
    commit_id: [u8; 4],
    data_section_size: u64,
    header_frame_size: u32,
}

struct HeaderFrame {
    commit_header: CommitHeader, // serialized fixed size: 22 bytes
    payload_len: u32,
    checksum: u64,
    encryption_descriptor_len: u32,
    // followed by: encryption_descriptor, compressed/encrypted payload
}

enum EncryptionDescriptor {
    Unencrypted,
    Symmetric {
        scope: EncryptionScope,
        algorithm: ContentAead,
        nonce: [u8; 12],
    },
    Password {
        scope: EncryptionScope,
        algorithm: ContentAead,
        nonce: [u8; 12],
        key_wrap: PasswordKeyWrap,
    },
    Recipients {
        scope: EncryptionScope,
        algorithm: ContentAead,
        nonce: [u8; 12],
        recipients: Vec<RecipientKeyWrap>,
    },
    PasswordAndRecipients {
        scope: EncryptionScope,
        algorithm: ContentAead,
        nonce: [u8; 12],
        password: PasswordKeyWrap,
        recipients: Vec<RecipientKeyWrap>,
    },
}

enum EncryptionScope {
    Header,
    Data,
    HeaderAndData,
}

struct PasswordKeyWrap {
    algorithm: ContentAead,
    salt: [u8; 16],
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    wrap_nonce: [u8; 12],
    wrapped_dek: Vec<u8>,
}

struct RecipientKeyWrap {
    recipient_id: Vec<u8>,
    suite: HpkeSuite,
    enc: Vec<u8>,
    wrapped_dek: Vec<u8>,
}

struct PasswordSet {
    default: Option<Zeroizing<Vec<u8>>>,
    by_commit: BTreeMap<[u8; 4], Zeroizing<Vec<u8>>>,
}

// The descriptor reserves symmetric-key and recipient-key variants, but
// end-to-end public-key creation/unlocking is not exposed as a user workflow.

struct BlockFrame {
    compressed_len: u32,
    flags: u16,
    reserved: u16,
    block_index: u32,
    nonce: Option<[u8; 12]>,
    checksum: u64,
}

struct RuntimeSecrets {
    dek: [u8; 32],
}
```

## Rust workspace

The Cargo workspace uses Rust edition 2024 and contains:

- `zarlib`: format implementation, validation, compression/encryption, and
  archive operations.
- `zarc`: command-line application.
- `zarg`: native `egui` desktop archive manager using the same library APIs.
- `pyzarlib`: PyO3 module and Python package/CLI example.
- `czarlib`: C ABI library with a JSON request/response dispatcher.
- `cxxzarlib`: CXX bridge exposing the shared JSON operation dispatcher to C++.

The repository also contains `gozarlib`, a Go module outside the Cargo
workspace. It uses cgo to expose the C ABI operation dispatcher with Go
operation helpers, tests, and examples.

The repository also contains `jzarlib-ffm` and `jzarlib-jni`, separate Java
modules outside the Cargo workspace. They expose the full C ABI operation
surface with Java FFM (Java 22+) and JNI, respectively, and include operation
tests plus direct API and command-style examples.

## 11. Language bindings, desktop UI, documentation, and benchmark

Keep the Rust API as the behavior source of truth. The Python package exports
create, add, remove, join, check, info, list, filesystem extract, and
in-memory extract operations. In-memory extraction returns a mapping from
archive paths to bytes and accepts per-file size limits and a starting offset.
Python tests should cover the API and run native Rust library tests where
configured. Support Python 3.9 through 3.15 in metadata and CI. The CI matrix
also covers free-threaded CPython 3.14t/3.15t; preserve PyO3's no-GIL declaration
and verify concurrent calls and import without re-enabling the GIL.

The C package exports the same operation surface through a UTF-8 JSON
dispatcher, a public header, example program, and integration tests. Validate
request types and required fields explicitly, return allocated response/error
strings with the documented free function, and never retain caller-owned
request pointers.

The C++ package uses `cxx`/`cxx-build` to expose the same full JSON operation
dispatcher as the C wrapper through a CXX bridge. It returns Rust-owned strings
and maps request or archive errors to CXX errors rather than requiring manual
C string allocation. Port the C API integration coverage to native C++ tests
and include direct API and command-style examples. Document bridge-header and
static-library linking.

The Go package binds to `czarlib` through cgo and provides operation-specific
Go helpers over the full JSON dispatcher. Include complete operation coverage,
Go tests, direct API and command-style examples, and document the Rust library
build prerequisite and cgo runtime linking requirements.

The Java FFM module uses the stable Foreign Function & Memory API (Java 22+)
to call the C ABI directly. The separate JNI module provides a native C shim,
Java operation helpers, tests, and examples. Both must expose every operation
through the same JSON contract and document native-library build/loading.

The `zarg` GUI uses `zarlib` operations, runs long work outside the UI thread,
and reports progress, warnings, results, and confirmations. It supports
archive browsing with virtualized listings; create/add/remove/join/check/info/
list/extract; collision and existing-file policies; filters; metadata; and
supported password-aware listing/extraction. Embed the selected licensed
icons and preserve their attribution and license files.

Maintain a root project overview, CLI README, library developer guide, format
description, processing-pipeline diagrams, and package-specific Python/C/C++/Go/Java/GUI
documentation. Documentation must state real support boundaries, including
password fallback precedence and encrypted remove/join limitations.

Provide a Linux benchmark program that downloads `linux-4.0.1.tar.gz` into
`testdata/` only if absent, validates the tarball before extraction, and
refreshes an incomplete/mismatched source tree. Benchmark compression and
extraction with zarlib, ZIP, 7z, and tar.gz both without encryption and with
encryption. Because tar.gz has no native encryption, wrap its output with
OpenSSL CMS AES-256-GCM and document that wrapper's measured cost; document
that ZIP's legacy ZipCrypto is not comparable in strength to AES-256-GCM or
7z AES. Use ephemeral random credentials/keys and remove benchmark archives
and key material on exit. Before every extraction, empty `testresults/`;
after extraction, verify regular-file paths and sizes and clean the directory
again even when extraction fails. Account explicitly for symlinks skipped by
zarlib, case-fold collision renames, and 7z's unsafe-relative-link extraction
warnings. Generate a Markdown report with tool versions, method and caveats,
compression/extraction times, archive sizes, throughput, and SVG charts.

For CI and release validation, run formatting, strict Clippy, Rust tests,
Python wheel/pytest, C integration tests, and generated-document checks as
appropriate. State which runtimes were actually exercised locally versus
covered by CI; do not claim target-version support from metadata alone.
