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
- [x] Validate current workspace: `cargo fmt --all -- --check`, `cargo test --workspace` (31 CLI tests and 37 library tests passed; doc-tests passed), and `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] Extend configurable block size and compression level to `zarc join`; `auto` selects a 1–8 MiB block size from 50% of currently available memory, using a bounded per-worker estimate. The current pipeline is sequential, so automatic selection uses one worker; tests cover sizing, worker reduction, and low-memory warnings.

## Open

- [ ] Implement encryption/decryption and key derivation. Until then, encrypted block contents cannot be validated and header encryption prevents complete TOC consistency checks.
- [ ] Verify or revise the provisional logical-header v0 wire encoding against an authoritative specification before claiming interoperability.
- [x] Rework extraction to retain only validated block locations and decoded lengths, then stream-decode selected blocks directly to destination files instead of retaining all extracted contents in memory; per-block decode remains limited to 32 MiB by default.
- [x] Make Unix extraction destination operations resilient to concurrent filesystem changes using directory-relative `openat`/`mkdirat`, `O_NOFOLLOW`, and exclusive creation for error/skip policies; non-Unix targets currently use a path-based fallback without equivalent race resistance.
- [ ] Expand archive-wide validation/recovery behavior, including the optional framing resynchronization described in `plan.md`.
