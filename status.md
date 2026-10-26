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

- [x] Add password-aware `zarc check`, `list`, and `extract` flows with no-echo prompts, authenticated encrypted block validation, and streamed extraction; wrong or missing passwords fail before extraction writes files. Regression tests verify wrong and correct passwords across check/list/extract.
- [x] Resolve strict Clippy findings in CLI password handling and add a named validated-commit result type.
- [x] Adopt the existing logical-header v0 byte layout as the canonical project encoding; document its exact fields in `plan.md` and add a fixed golden-byte encode/decode compatibility vector.
- [x] Rework extraction to retain only validated block locations and decoded lengths, then stream-decode selected blocks directly to destination files instead of retaining all extracted contents in memory; per-block decode remains limited to 32 MiB by default.
- [x] Make Unix extraction destination operations resilient to concurrent filesystem changes using directory-relative `openat`/`mkdirat`, `O_NOFOLLOW`, and exclusive creation for error/skip policies; non-Unix targets currently use a path-based fallback without equivalent race resistance.
- [x] Expand `zarc check` recovery behavior with opt-in `--recover` framing resynchronization. It scans for a prefix whose checksum and commit body validate, reports damaged framing gaps, resets derived TOC state across a gap, and exits with warning code 1; normal checks remain strict.
- [x] Add library content encryption/decryption (AES-256-GCM and ChaCha20-Poly1305) and Argon2id password key wrapping with bounded parameters.
- [x] Add `zarc create --password` using a no-echo terminal prompt and disk-backed staging; encrypted output is authenticated and validated with the generated DEK before publication.
- [x] Upgrade encryption dependencies to `aes-gcm 0.11.1`, `argon2 0.6.0`, `chacha20poly1305 0.11.0`, and `getrandom 0.4.3`; update nonce conversions for the current AEAD API.
- [x] Validate encryption dependency upgrade: `cargo fmt --all`, `cargo test --workspace` (35 CLI and 44 library tests; doc-tests passed), and `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] Add zeroization for transient password-derived wrapping keys and recovered DEK plaintext buffers.
- [x] Track zeroization of long-lived archive DEKs across CLI operations using drop-zeroized key buffers, including per-block extraction key copies; password-unwrapped DEKs use `Zeroizing` ownership.
- [x] Enable the AES-GCM and ChaCha20-Poly1305 `zeroize` backend features; their AES/ChaCha key schedules and AEAD-held key state are zeroized when cipher instances drop, as implemented by the selected RustCrypto crates.
- [x] Keep `unwrap_dek_with_password` returning zeroize-on-drop `Zeroizing<[u8; 32]>` and expose `unwrap_dek_with_password_zeroizing` as an explicit alias for callers that want the security property to be obvious in the function name.
- [x] Validate long-lived DEK zeroization changes: `cargo fmt --all -- --check`, `cargo test --workspace` (36 CLI and 44 library tests; doc-tests passed), and `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] Reconcile stale encryption and password-aware read-flow notes in `agents.md` with the implemented behavior; rerun `cargo test --workspace` successfully (36 CLI tests, 44 library tests, doc-tests passed).
- [x] Add `zarc info --json` output containing commit IDs and framing sizes; document the option in CLI help and test the emitted JSON structure.
- [x] Validate the latest `info --json` changes with `cargo fmt --all -- --check`, `cargo test --workspace` (37 CLI tests and 44 library tests passed; doc-tests passed), and `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] Move the `zarc` CLI tests from `main.rs` into `zarc/src/tests.rs`; validate with `cargo fmt --all -- --check`, `cargo test --workspace` (37 CLI tests and 44 library tests passed; doc-tests passed), `cargo clippy --workspace --all-targets -- -D warnings`, and `git diff --check`.
- [x] Move the `zarlib` library tests from `lib.rs` into `zarlib/src/tests.rs`; `cargo fmt --all -- --check`, `cargo test --workspace` (37 CLI tests, 44 library tests, doc-tests passed), strict workspace Clippy, and `git diff --check` all passed.
- [x] Adopt the existing logical-header v0 layout as the canonical project encoding; specify field order, tags, endianness, paths, and record ordering in `plan.md`, and add a complete golden-byte encode/decode compatibility vector. Validation: `cargo fmt --all -- --check`, workspace tests (37 CLI, 45 library, doc-tests), strict workspace Clippy, targeted golden-vector test, and `git diff --check` passed.

