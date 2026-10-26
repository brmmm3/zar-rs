# `zar` format plan

## Rust

- Use latest dependencies
- Use edition 2024

## 0. File grouping

Files are grouped into following types (Rust enum):

- Small files (smaller than block size)
  - SmallFiles(Vec<DirEntrySmall>)
  - DirEntrySmall:
    - DirEntry
    - BlockNumer
    - OffsetInBlock
- Big files (files with the size at least block size)
  - BigFiles(Vec<DirEntry, Vec<BlockNumbers>>)
- Empty files
  - EmptyFiles(Vec<DirEntry>)
- Symbolic links
  - Symlink(Vec<DirEntry>)
- Hard links
  - Hardlinks(Vec<DirEntry>)
- Devices
  - Devices(Vec<DirEntry>)
- Removed files (files to remove from container)
  - Remove(Vec<String>)

## 1. Commit framing and integrity

A file begins with the four-byte magic `ZAR0`. Commits follow sequentially; the commit chain is the physical order in the file.

Use a fixed-size, cleartext commit prefix so a reader can find and frame a commit without decrypting its header:

```text
CommitPrefix (38 bytes, all integers little-endian)
  magic                 [4]  "ZCMT"
  framing_version       u8
  flags                 u8
  total_commit_size     u64  includes this entire prefix
  commit_id             u32  archive-scoped counter starting at 1
  header_frame_size     u32  includes header-frame metadata and payload (maximum <4 GiB)
  data_section_size     u64  total bytes of all data-block frames
  prefix_checksum       u64  xxhash64 of preceding 30 prefix bytes
```

Require:

```text
total_commit_size = 38 + header_frame_size + data_section_size
```

Reject arithmetic overflow, invalid sizes, and unsupported framing versions. The prefix checksum detects accidental corruption; it is **not** an authentication mechanism. A reader can skip a framed commit it cannot decrypt. For resynchronization after damaged framing, an implementation may scan for `ZCMT`, but must validate the prefix and report that recovery was attempted.

A commit’s header contains its own metadata and TOC. Commit-local encryption settings live in that header—not in a container-level header.

## 2. Header framing, compression, and encryption

The header frame has cleartext metadata followed by the payload:

```text
HeaderFrame
  compressed_header_len  u32
  header_checksum        u64
  encryption_descriptor_len u32
  encryption_descriptor  [encryption_descriptor_len] bytes, bounded
  header_payload         [compressed_header_len] bytes
```

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

The processing order is:

1. Serialize the logical header using the canonical v0 encoding above.
2. Compress it with Zstandard.
3. If encryption is enabled, encrypt the compressed bytes with AEAD.
4. Store the resulting payload and its descriptor.

`compressed_header_len` is the exact number of payload bytes on disk (ciphertext plus authentication tag when encrypted). `header_checksum` is xxhash64 over those exact on-disk payload bytes. Checksum verification can detect accidental damage before decryption; it does not replace AEAD authentication.

For encrypted headers, define AAD as the exact serialized `CommitPrefix` **excluding** `prefix_checksum`, concatenated with the exact serialized header-frame metadata and encryption descriptor, excluding both `header_checksum` and `header_payload`. The header checksum is computed from the stored payload, so including it in AAD would create a circular dependency. The prefix’s final `total_commit_size` and frame lengths must be finalized before computing header AEAD. Specify the encoding unambiguously; do not use Rust’s in-memory struct layout.

For unencrypted headers, the checksum is still calculated over the stored compressed header payload. The descriptor says `Unencrypted`; no AEAD operation is performed.

## 3. Encryption, passwords, and nonces

Each commit has its own encryption policy. Support:

- **No encryption**
- **Symmetric encryption** with AES-256-GCM or ChaCha20-Poly1305
- **Recipient encryption** using HPKE to wrap a fresh random 32-byte DEK for each recipient
- Optionally, **password encryption**, using a password-derived key to wrap the DEK

Use a fresh cryptographically random DEK per commit and a fresh random nonce for each independently encrypted object. Never reuse a nonce with the same key. For HPKE, record the full suite and encapsulated key (`enc`) per recipient; HPKE wrapping uses its own context and nonce sequence, separate from content AEAD.

Passwords must not be used directly as keys. Derive a wrapping key using Argon2id version `0x13` and a per-commit random 16-byte salt. Accepted parameters are `memory_kib = 65,536–1,048,576`, `iterations = 2–10`, and `parallelism = 1–16`. Writers default to `memory_kib = 65,536`, `iterations = 3`, and `parallelism = 1`, subject to user configuration. Readers validate parameters before running Argon2id and reject out-of-range values; they must not silently clamp them. A deployment may impose stricter resource limits. Use the derived key to wrap the DEK with an AEAD; record the salt, parameters, nonce, and algorithm in the clear descriptor. A wrong password must fail authentication. Password wrapping and HPKE wrapping can coexist, allowing multiple ways to recover the same commit DEK.

All on-disk enums use explicit one-byte tags, not Rust enum discriminants. Integers are little-endian. Variable-length byte strings use a `u32` length prefix. The encryption descriptor is at most 64 KiB; recipient count at most 1,024; recipient IDs 1–256 bytes; HPKE `enc` at most 1 KiB; wrapped DEK at most 4 KiB; password salt exactly 16 bytes. Readers reject unknown mandatory tags, duplicate recipient IDs, truncated fields, trailing bytes, and length overflows. Unknown optional TLVs may be skipped only when their high-bit optional tag is set. The format version defines mandatory tags and exact field order.

Header encryption uses the commit DEK and a dedicated random 96-bit header nonce. Each block encrypted under that DEK uses a fresh random 96-bit nonce. Reject duplicate nonces within the commit; never reuse a nonce with the same key. Random nonces are stored in each encrypted header/block frame.

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

For version 0, `flags` is a bit field: bit 0 is `ENCRYPTED`, bit 1 is `NONCE_PRESENT`, and bits 2–15 are reserved and MUST be zero. `ENCRYPTED = 0` requires `NONCE_PRESENT = 0`; `ENCRYPTED = 1` requires `NONCE_PRESENT = 1`. The nonce is exactly 12 bytes and appears immediately after `block_index`. `reserved` must be zero; readers reject nonzero reserved bits or fields. `block_index` is zero-based and unique within the commit. Each encrypted block uses a fresh nonce under that commit's DEK; duplicate nonces within a commit are rejected. The checksum is xxhash64 over the exact stored payload bytes (ciphertext and tag if encrypted). It is not a checksum of the header.

The processing order for data is:

1. Build a plaintext block from file data.
2. Compress it with Zstandard.
3. Encrypt the compressed bytes if enabled.
4. Store the frame and checksum its stored payload.

For encrypted blocks, AAD is the exact serialized commit-prefix bytes excluding `prefix_checksum`, concatenated with the exact serialized block-frame fields through `nonce`, excluding `checksum` and `payload`. Ensure all lengths are finalized before calculating AEAD. For unencrypted blocks, checksums provide corruption detection but no authenticity.

Put both compressed and uncompressed lengths, and the block’s index, in authenticated header metadata so decompression can enforce size limits and detect truncation or reordering. Enforce configured maximum sizes before allocation/decompression.

## 5. Block sizing and parallelism

Automatic block size is in the range **1–8 MiB**, subject to a memory budget. Let:

```text
budget = 50% of currently available RAM
workers = number of compression workers actually selected
per_worker_budget = budget / workers
```

Choose an automatic block size no larger than 8 MiB and no larger than the per-worker budget, rounded down to a supported size. If the per-worker budget is below 1 MiB, use 1 MiB as the minimum block size and reduce worker count until the estimated total working set fits the budget. If even one 1 MiB worker would exceed the budget, use one worker and issue a warning; optionally offer a low-memory mode with smaller blocks.

The memory estimate must include input buffers, compression workspace, encryption buffers, and queued output—not just the block buffer. Bound the queue so parallel workers cannot collectively exceed the budget. Manual block size is 64 KiB–32 MiB; it may exceed the automatic budget, but the CLI must warn and limit concurrency or fail if the configured memory limit would be exceeded.

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

Unsupported file types are ignored with warnings by default; strict mode makes them errors. Do not silently follow symlinks during scanning unless the user explicitly requests it. A hard-link entry contains its archive-relative target path. The target must resolve to a regular file in the effective TOC when the commit is applied. It may target a file added earlier in the same commit or an earlier commit, but not a later entry. Invalid or unavailable targets are skipped with a warning in non-strict mode and fail in strict mode. If the destination filesystem cannot create hard links, extraction fails for that entry by default; `--hardlinks copy` may instead write a copy and issue a warning.

## 7. Joining commits and history

Treat commits as an ordered log. Normal extraction applies commits from oldest to newest, producing the final TOC state.

A join MUST select a contiguous range in physical commit order; reject non-contiguous selections. Replace the selected range at the position of its first commit, preserving the order of commits before and after it. The replacement must preserve the logical result of applying the selected range to the preceding history, including tombstones for removals and all referenced data needed after the selected commits are removed. Do not append the replacement after later commits, because that would change which version wins. If any selected commit cannot be read or decrypted, abort the stateful join. A separate copy-through operation may preserve unreadable commits unchanged, but must not claim to have compacted or merged their state. Commits outside the selected range remain byte-for-byte unchanged when possible.

Write to a temporary file, fsync as appropriate, and atomically replace the original only after the new container validates. “Encrypted commits without a password remain as is” is only safe for a copy-through operation that does not need to incorporate their contents.

## 8. Extraction and errors

For extraction, first read and validate commit framing and build the selected commit sequence. Apply readable commits in order. In interactive mode, an encrypted commit without credentials is skipped with a warning by default. In non-interactive mode, missing credentials are an error unless `--skip-encrypted` is explicitly given; with that option the commit is skipped and `W_ENCRYPTED_SKIPPED` is reported. Credentials must be associated with a commit ID or recipient identity, not ambiguous password ordering. In strict mode, every warning is fatal; abort before writing extracted files, or stage output and publish it only after success.

Warnings have stable categories: `W_ENCRYPTED_SKIPPED` (encrypted commit lacks credentials), `W_UNSUPPORTED_TYPE` (unsupported file type), `W_COLLISION_RESOLVED` (archive paths collide), `W_DESTINATION_SKIPPED` (destination file skipped), `W_METADATA_NOT_APPLIED` (metadata could not be restored), `W_HARDLINK_COPIED` (hard link extracted as a copy), and `W_RECOVERY_SCAN` (damaged framing required recovery scanning). Errors include malformed framing or metadata, failed authentication, invalid paths, decompression limits exceeded, and failed writes. A checksum mismatch is an error for that commit; non-strict extraction may skip it and continue only if framing safely locates the next commit. Otherwise stop or enter an explicitly requested recovery scan.

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

Common options should include `--compression-level`, `--workers N`, `--leave-cores N`, `--performance-cores`, `--block-size SIZE|auto`, `--memory-limit SIZE`, and `--strict`. Encryption options should be explicit, for example `--encrypt aes-256-gcm`, `--encrypt chacha20-poly1305`, `--password-for COMMIT_ID` (prompt for the password), or recipient public-key inputs. Avoid passwords in command-line arguments because they may be exposed in process listings; support interactive prompts and protected input sources.

For extraction, expose separate options for path collisions (`--collision error|latest|first|rename|ask`) and existing destination files (`--existing overwrite|skip|error|ask`). `ask` is interactive-only and should fail clearly in non-interactive mode. Add `--dry-run` to show the final TOC and planned actions without writing files.

`--through COMMIT_ID` selects the commit with that exact 8-hex-digit ID (the little-endian encoding of its nonzero `u32` counter) and applies commits from the beginning through and including it. The ID must exist and be unique; there is no index interpretation, and absent or malformed IDs are errors. Use `--password-for COMMIT_ID` to prompt for a password; never accept passwords directly as command-line values. Recipient private keys are supplied through protected key-file options and matched against recipient IDs. Any `ask` collision, overwrite, or rename policy requires an interactive terminal and is an error in non-interactive mode.

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
struct CommitPrefix {
    magic: [u8; 4],
    framing_version: u8,
    flags: u8,
    total_commit_size: u64,
    commit_id: u32,
    header_frame_size: u32,
    data_section_size: u64,
    prefix_checksum: u64,
}

enum HeaderEncryption {
    None,
    Aes256Gcm {
        nonce: [u8; 12],
        key_source: KeySource,
    },
    ChaCha20Poly1305 {
        nonce: [u8; 12],
        key_source: KeySource,
    },
}

enum KeySource {
    Password {
        kdf: PasswordKdf,
        salt: Vec<u8>,
        memory_kib: u32,
        iterations: u32,
        parallelism: u32,
        wrapped_dek: Vec<u8>,
        wrap_nonce: [u8; 12],
    },
    Recipients(Vec<RecipientEntry>),
    PasswordAndRecipients {
        password: PasswordKeyWrap,
        recipients: Vec<RecipientEntry>,
    },
}

struct RecipientEntry {
    recipient_id: Vec<u8>,
    suite: HpkeSuite,
    enc: Vec<u8>,
    wrapped_dek: Vec<u8>,
}

struct HpkeSuite {
    kem: HpkeKem,
    kdf: HpkeKdf,
    aead: HpkeAead,
}

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

- For library use folder and crate name zarlib
- For cli use folder and crate name zarc
