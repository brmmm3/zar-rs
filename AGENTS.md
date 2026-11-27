# AGENTS.md (Root)

This repository contains the complete ZAR archiving and data transfer ecosystem implemented in Rust.

## Workspace Architecture & Sub-Agents Guide
- **`zarlib`** (`zarlib/AGENTS.md`): Foundational archive engine. Framing, TOC, content deduplication, zstd compression, AEAD encryption (AES-256-GCM / ChaCha20-Poly1305), and public operations API.
- **`zarc`** (`zarlib-cli/AGENTS.md`): High-performance command-line interface for creating, modifying, extracting, querying, and verifying archives.
- **`zarg`** (`zarlib-gui/AGENTS.md`): Cross-platform GUI archive manager built on `eframe` (`egui`) with visual staging, history views, and standalone shell context actions (`--context extract-here`, `--context extract-to`, etc.).
- **`zcopy`** (`zcopy/AGENTS.md`): Rapid TCP directory transfer and sync tool streaming ZAR pipelines (`zarlib-zcopy`).
- **`zarmcp`** (`zarmcp/AGENTS.md`): Model Context Protocol (MCP) stdio server exposing archive operations to AI agents.
- **`zarmount`** (`zarmount/AGENTS.md`): Virtual filesystem driver mounting ZAR archives read-only (FUSE on Linux, ProjFS on Windows).

## Setup & Compilation
- **Check entire workspace**: `cargo check --workspace`
- **Build all release binaries**: `cargo build --release --bins`
- **Run workspace tests**: `cargo test -p zarlib -p zarlib-cli -p zarlib-gui -p zarlib-zcopy -p zarmcp -p zarmount`
- **Format check**: `cargo fmt --check`
- **Linter check**: `cargo clippy --workspace`

## Engineering Standards & Guidelines
- **Microsoft Pragmatic Rust Guidelines**: All code must conform to Microsoft Rust guidelines (`M-*` rules for safety, error handling, naming, and performance).
- **Zeroize Secrets**: Cryptographic keys, DEKs, and passwords must use `zeroize::Zeroizing`.
- **Path Normalization**: Internal archive paths always use forward slashes (`/`), stripped of leading slashes and drive letters.
- **Layer Separation**: The CLI and GUI binaries are presentation layers. Core archival, framing, and transformation algorithms belong in `zarlib::operations`.
