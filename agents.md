# ZAR implementation notes

## Workspace

- Rust 2024 workspace with `zarlib` (format/library) and `zarc` (CLI).
- Format plan: [plan.md](./plan.md).

## Implemented

- Container magic (`ZAR0`) and 56-byte little-endian commit prefixes (`ZCMT`) with xxhash64 integrity checks and checked size arithmetic.
- Header and data-block frame encoding/decoding with bounded payload reads, Zstandard compression/decompression, and xxhash64 payload checksums.
- Stable encryption-descriptor wire encoding, parameter validation, optional TLV handling, and scope metadata. Cryptographic encryption/decryption and key derivation are not implemented.
- Commit-body framing validation for descriptors, frame integrity, block-index uniqueness, encryption scope consistency, duplicate nonces, frame size limits, and block-count limits.
- Canonical archive-relative UTF-8 path validation using `ArchivePath`: slash-separated paths only, rejecting absolute/drive-prefixed paths, empty/dot/parent components, NUL, and backslashes. Comparison is exact (no case folding or Unicode normalization).
- CLI `check` validates commit bodies and frame integrity; `info` lists commit prefix sizes and IDs.

## Current limitations / next work

- Logical header/TOC encoding and commit operation model are not yet implemented.
- `zarc check` verifies framing and stored checksums, not decryption, decompression of all frames, or logical archive consistency.
- Archive creation, extraction, listing, joining, and file-system scanning are not implemented.
- Encryption descriptors are parsed and validated, but encryption and decryption are unsupported.
- Verify all provisional wire-format choices against the authoritative format specification before treating them as finalized.

## Validation

After the latest edits, run `cargo fmt --all`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings`.
