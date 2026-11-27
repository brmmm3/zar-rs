# AGENTS.md (zarlib - Core Archive Library)

`zarlib` is the foundational engine powering the entire ZAR ecosystem. It implements the binary format, framing primitives, content-defined deduplication, block compression, commit-based historical snapshots, and AEAD encryption.

## Build & Test Instructions
- **Check**: `cargo check -p zarlib`
- **Run tests**: `cargo test -p zarlib`
- **Lint**: `cargo clippy -p zarlib`
- **Format**: `cargo fmt`

## Architecture & Core Modules

### 1. Format & Framing
- `ContainerHeader` (`CONTAINER_MAGIC = *b"ZAR0"`): 12-byte header with a pointer (`first_header_offset`) to the first commit.
- `CommitHeader` (`COMMIT_MAGIC = *b"ZCMT"`): Carries commit-level metadata (ID, flags, data size, header size). Written at the *end* of each commit so all sizes are deterministic.
- `frames`: Defines `BlockFrame` (data chunks) and `HeaderFrame` (commit TOC and index).
- `header`: Encodes `LogicalHeader`, entry metadata, and index entries.
- `toc`: `Toc` (Table of Contents), storing `TocEntry` items (files, directories, symlinks) with metadata and chunk lists.

### 2. Encryption & Cryptography (`crypto`, `encryption`)
- **Key Derivation**: Argon2id for password-based key derivation.
- **Payload & Header Encryption**: AEAD with AES-256-GCM or ChaCha20-Poly1305.
- **Key Wrapping**: Randomly generated Data Encryption Keys (DEKs) wrapped with password-derived keys or HPKE recipient public keys.
- **Memory Safety**: Sensitive keys and credentials utilize `zeroize::Zeroizing` to ensure wipe on drop.

### 3. High-Level Operations (`operations`)
The public application API sits in `zarlib::operations`:
- `create`: New single-commit archive creation with multi-threaded deduplication and block compression.
- `add`: Incremental commit additions/updates to an existing archive.
- `extract`: Parallel extraction with directory creation, timestamp restoration, and validation.
- `list` / `info`: In-memory metadata and commit inspection.
- `check`: Cryptographic authentication and integrity verification.
- `grep`: Pattern searching inside archives without disk extraction.
- `join`: History and archive concatenation.
- `remove` / `remove_commits`: Path-level deletions or commit history purging.
- `transform`: Re-compression and re-encryption of existing commits.
- `undo`: Commit history rollback.
- `stream` / `pipeline`: Low-overhead streaming pipelines for network and IPC transfers.

### 4. Input Format Importers
`zarlib` automatically inspects and imports external formats as native ZAR archives:
- ZIP (`zip`), 7z (`sevenz-rust2`), RAR (`rars`), ISO 9660 (`libmkiso`), and TAR / TAR.GZ / TGZ (`tar`, `flate2`).

## Design Guidelines & Conventions
- **Microsoft Pragmatic Rust Guidelines**: All code must conform to Microsoft Rust guidelines (`M-*` rules for safety, error handling, naming, and performance).
- **Error Handling**: Library APIs return `Result<T, Box<dyn std::error::Error>>` or specific typed errors (`FramingError`, `TocError`, `LogicalHeaderError`). Never panic on invalid archive input.
- **Cross-Platform Filesystem**:
  - Windows: Uses `jwalk-meta` and `windows-sys` for metadata and hard-link deduplication.
  - Unix: Uses `scandir` and `libc`.
  - Always normalize internal archive paths using `ArchivePath` with forward slashes (`/`).
