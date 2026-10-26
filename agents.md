# ZAR implementation notes

## Workspace

- Rust 2024 workspace with `zarlib` (format/library) and `zarc` (CLI).
- Format plan: [plan.md](./plan.md).

## Implemented

- Container magic (`ZAR0`) and 56-byte little-endian commit prefixes (`ZCMT`) with xxhash64 integrity checks and checked size arithmetic.
- Header and data-block frame encoding/decoding with bounded payload reads, Zstandard compression/decompression, and xxhash64 payload checksums.
- Stable encryption-descriptor wire encoding, parameter validation, optional TLV handling, and scope metadata. Cryptographic encryption/decryption and key derivation are not implemented.
- Commit-body framing validation for descriptors, frame integrity, block-index uniqueness, encryption scope consistency, duplicate nonces, frame size limits, and block-count limits. Unencrypted logical headers are decompressed and decoded during validation, then each new file operation is checked against blocks in that commit; encrypted headers remain opaque because decryption is unsupported. Data-only encryption descriptors still permit header compression/decompression, while header-encrypted descriptors are rejected by plaintext header operations.
- Canonical archive-relative UTF-8 path validation using `ArchivePath`: slash-separated paths only, rejecting absolute/drive-prefixed paths, empty/dot/parent components, NUL, and backslashes. Comparison is exact (no case folding or Unicode normalization).
- In-memory logical TOC model with file and directory entries, ordered put/remove operations, recursive directory removal, path-kind conflict detection, and atomic per-commit application. The `apply_commit_with_available_blocks` API validates each newly added file's references against the current commit's validated block indices while preserving existing references to blocks in prior commits. `ValidatedCommit` exposes the block-index set collected during frame validation.
- Provisional logical-header v0 encoding/decoding for ordered TOC operations, using explicit record tags, little-endian integer fields, bounded allocation/size limits, and canonical `ArchivePath` validation. It rejects unknown versions/tags, invalid UTF-8/path data, truncated records (including implausible declared block-reference counts before allocation), and trailing bytes. This layout is provisional and must be reconciled with the authoritative format specification before interoperability is claimed.
- CLI `check` validates commit bodies and frame integrity; `info` lists commit prefix sizes and IDs. `list` validates and applies readable logical headers in commit order, checks each commit's new file references against that commit's block indices, and prints the final TOC.
- CLI `extract` materializes readable, unencrypted file contents by processing commits in order and associating block indices only with their owning commit. It supports `--to DIR` and `--dry-run`, verifies reconstructed file sizes, and rejects header- or data-encrypted commits. Before writing, it checks destination targets for symlinks and incompatible existing file/directory types.
- CLI extraction tests cover nested paths, dry-run non-mutation, size mismatch rejection, encrypted-data rejection, and block-index reuse across commits.

## Current limitations / next work

- Logical-header v0 serialization currently encodes TOC operations and file block indices, but format-wide canonical interoperability is not established until the provisional wire layout is verified against the authoritative specification.
- `zarc check` verifies framing and stored checksums, not decryption, decompression of all data frames, or logical archive consistency.
- Archive creation, joining, and file-system scanning are not implemented.
- Encryption descriptors are parsed and validated, but encryption and decryption are unsupported.
- Extraction is memory-backed and bounded to 32 MiB per decoded block by default. Destination symlink checks are not atomic with file creation, so concurrent filesystem changes could create a time-of-check/time-of-use race.
- Verify all provisional wire-format choices against the authoritative format specification before treating them as finalized.

## Validation

Latest validation passed: `cargo fmt --all`, `cargo test --workspace` (5 CLI tests and 37 library tests), and `cargo clippy --workspace --all-targets -- -D warnings`. Commit validation checks new file references against blocks in the current commit; TOC application preserves entries from earlier commits. CLI extraction decodes each commit's unencrypted blocks and materializes file data before writing the final TOC.
