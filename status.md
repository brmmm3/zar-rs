# Project status

## Done

- [x] Set up a Rust 2024 workspace with the `zarlib` library crate and `zarc` CLI crate.
- [x] Implement container magic, commit prefixes, bounded frame parsing, Zstandard compression, xxhash64 checksums, and encryption-descriptor parsing/validation.
- [x] Implement canonical archive paths and ordered logical TOC operations, including recursive directory removal and commit-local block-reference checks.
- [x] Implement CLI commands for archive creation, validation, listing, inspection, extraction, append, removal, and commit-range join/compaction.
- [x] Add staging and validation safeguards for archive mutations, plus extraction dry-run, collision, and existing-file policies.
- [x] Extend `zarc check` to decompress and verify unencrypted blocks, apply readable TOC changes across commits, check block references and decoded file sizes, and report validation limits caused by encryption.
- [x] Add regression tests for size mismatches, cumulative TOC conflicts, per-commit block-index reuse, and encrypted-header reporting.
- [x] Add configurable `zarc create` and `zarc add` block sizes and Zstandard compression levels; manual sizes follow the documented 64 KiB–32 MiB range, with create/add round-trip coverage.
- [x] Validate current workspace: `cargo fmt --all`, `cargo test --workspace` (33 CLI tests and 37 library tests passed; doc-tests passed), and `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] Extend configurable block size and compression level to `zarc join`; `auto` selects a 1–8 MiB block size from 50% of currently available memory, using a bounded per-worker estimate. The current pipeline is sequential, so automatic selection uses one worker; tests cover sizing, worker reduction, and low-memory warnings.
- [x] Reject append, remove, and join on archives containing encrypted commits before any mutation; regression tests verify the archive bytes remain unchanged.
- [x] Validate current workspace: `cargo fmt --all`, `cargo test --workspace` (35 CLI tests and 44 library tests passed; doc-tests passed), and `cargo clippy --workspace --all-targets -- -D warnings`.

## Open

- [ ] Add password-aware `zarc check`, `list`, and `extract` flows; currently those commands cannot decrypt encrypted content.
- [ ] Verify or revise the provisional logical-header v0 wire encoding against an authoritative specification before claiming interoperability.
- [x] Rework extraction to retain only validated block locations and decoded lengths, then stream-decode selected blocks directly to destination files instead of retaining all extracted contents in memory; per-block decode remains limited to 32 MiB by default.
- [x] Make Unix extraction destination operations resilient to concurrent filesystem changes using directory-relative `openat`/`mkdirat`, `O_NOFOLLOW`, and exclusive creation for error/skip policies; non-Unix targets currently use a path-based fallback without equivalent race resistance.
- [x] Expand `zarc check` recovery behavior with opt-in `--recover` framing resynchronization. It scans for a prefix whose checksum and commit body validate, reports damaged framing gaps, resets derived TOC state across a gap, and exits with warning code 1; normal checks remain strict.
- [x] Add library content encryption/decryption (AES-256-GCM and ChaCha20-Poly1305) and Argon2id password key wrapping with bounded parameters.
- [x] Add `zarc create --password` using a no-echo terminal prompt and disk-backed staging; encrypted output is authenticated and validated with the generated DEK before publication.
- [x] Upgrade encryption dependencies to `aes-gcm 0.11.1`, `argon2 0.6.0`, `chacha20poly1305 0.11.0`, and `getrandom 0.4.3`; update nonce conversions for the current AEAD API.
- [x] Validate encryption dependency upgrade: `cargo fmt --all`, `cargo test --workspace` (35 CLI and 44 library tests; doc-tests passed), and `cargo clippy --workspace --all-targets -- -D warnings`.

