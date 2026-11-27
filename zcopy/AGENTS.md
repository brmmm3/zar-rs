# AGENTS.md (zcopy - Fast Directory Transfer Tool)

`zcopy` (`zarlib-zcopy`) is a high-speed directory copy and synchronization tool that streams directory trees over TCP using ZAR pipelines and zstd compression. Transfers can be authenticated and encrypted using a shared password.

## Build & Test Instructions
- **Package Name**: `zarlib-zcopy`
- **Binary Name**: `zcopy`
- **Build**: `cargo build --release -p zarlib-zcopy`
- **Run tests**: `cargo test -p zarlib-zcopy`
- **Lint**: `cargo clippy -p zarlib-zcopy`
- **Format**: `cargo fmt`

## Operational Modes

### 1. Server Mode
Listens for incoming client transfers:
```sh
cargo run --bin zcopy -- --server <destination_dir> [--allow <ips...>] [--port <port>]
# Short alias
cargo run --bin zcopy -- -s <destination_dir>
```
- Listens by default on `0.0.0.0:7878`.
- Scans and caches destination directory metadata at startup.
- Sends a compressed manifest to connecting clients and receives ZAR streams.

### 2. Client Mode
Transfers a source directory tree to the server:
```sh
cargo run --bin zcopy -- --client <host:port> [source_dir]
# Short alias
cargo run --bin zcopy -- -c <host:port> [source_dir]
```
- `source_dir` defaults to `.` (current directory).
- Compares source against server manifest (based on whole-second modification timestamps).
- Transmits only missing or newer files, compressed with Zstandard.

### 3. Packed Mode
Sends the full source tree as a `.zar` archive file rather than extracting on the server:
```sh
cargo run --bin zcopy -- --client <host:port> --pack [source_dir]
```

## Security & Encryption
- Shared secret password: set via `--password <pass>` or the `ZCOPY_PASSWORD` environment variable.
- When configured, connections are encrypted with AEAD and authenticated. Both server and client must supply matching passwords.

## Architecture & Edge Cases
- **One-way Copy**: `zcopy` is a one-way transfer tool, not a two-way synchronization engine. Deletions on the source are not propagated to the destination.
- **Timestamp Resolution**: Comparisons use whole-second modification times. Ensure system clocks are reasonably synchronized.
- **Streaming Pipeline**: Employs `zarlib` streaming primitives and multi-threaded worker pools for parallel chunk compression and extraction.
