# AGENTS.md (zarc - Command Line Interface)

`zarc` is the official command-line interface for the ZAR archiving system. It provides high-performance compression, deduplication, multi-commit history management, and AEAD encryption.

## Build & Test Instructions
- **Build**: `cargo build --bin zarc` (or `--release`)
- **Run tests**: `cargo test -p zarlib-cli`
- **Lint**: `cargo clippy -p zarlib-cli`
- **Format**: `cargo fmt`

## CLI Subcommands Reference

### Archive Creation & Modification
- **Create**: `cargo run --bin zarc -- create <archive.zar> <paths...>`
  - Short alias: `c`
  - Supports file/folder inputs as well as automated imports from `.zip`, `.7z`, `.rar`, `.tar`, `.iso`, `.tar.gz`, `.tgz`, and `.gz`.
  - Flags: `--password` (or `-p`), `--aead <aes-256-gcm|chacha20-poly1305>`, `--encryption-scope <data|header|header-and-data>`, `--block-size <size>`, `--compression-level <1-22>`, `--include-file`, `--exclude-folder`, `--overwrite`.
- **Add**: `cargo run --bin zarc -- add <archive.zar> <paths...>`
  - Short alias: `a`
  - Appends a new commit with added or modified entries.
- **Remove**: `cargo run --bin zarc -- remove <archive.zar> <archive_paths...>`
  - Short alias: `d`
  - Records the removal of paths in a new commit while preserving earlier history.
- **Remove Commits**: `cargo run --bin zarc -- remove-commits <archive.zar> [output.zar] --from <id> --to <id>`
  - Short alias: `dc`
  - Purges specific commit ranges from the history.
- **Undo**: `cargo run --bin zarc -- undo <archive.zar> [output.zar] [--steps <n>]`
  - Short alias: `u`
  - Rolls back recent commits.

### Archive Inspection & Extraction
- **Extract**: `cargo run --bin zarc -- extract <archive.zar> <dest_dir>`
  - Short alias: `x`
  - Flags: `--to <commit_id>`, `--password`, `--overwrite`, `--keep-old`, `--checksum <auto|always|never>`, `--dry-run`.
- **List**: `cargo run --bin zarc -- list <archive.zar>`
  - Short alias: `l`
  - Lists archive entries, compressed/uncompressed sizes, and modification dates.
- **Info**: `cargo run --bin zarc -- info <archive.zar>`
  - Short alias: `i`
  - Displays framing structure, commit IDs, encryption status, and metadata.
- **Check**: `cargo run --bin zarc -- check <archive.zar>`
  - Short alias: `k`
  - Verifies block checksums and cryptographic authentication tags.
- **Cat**: `cargo run --bin zarc -- cat <archive.zar> <entry_path>`
  - Short alias: `p`
  - Streams a single file's uncompressed content to stdout.
- **Grep**: `cargo run --bin zarc -- grep <archive.zar> <regex_pattern>`
  - Short alias: `g`
  - Searches for patterns inside archive contents without extracting to disk.
- **Join**: `cargo run --bin zarc -- join <dest.zar> <src1.zar> <src2.zar>`
  - Short alias: `j`
  - Combines commits from multiple archives into one.

## Architectural Notes & Edge Cases
- **Stack Size (Windows)**: Argument parsing on Windows runs in a dedicated thread with a 4MB stack to prevent stack overflow from deep clap command hierarchies.
- **Configuration Defaults**: `zarc.json` in the current directory or config directory is automatically read for defaults unless `--no-config` is supplied.
- **Path Normalization**: Internal archive paths always use forward slashes (`/`), trimmed of leading slashes and drive letters.
- **Pass-through to `zarlib`**: `zarc` acts strictly as an argument parser and terminal interface layer; core operations logic belongs in `zarlib::operations`.
