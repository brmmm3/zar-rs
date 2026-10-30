# ZAR

ZAR is a modern, append-friendly archive format with a fast, memory-safe Rust
implementation: a command-line tool, a desktop GUI, a reusable library, bindings
for many languages, and a network copy tool.

- **[`zarc`](./zarc/ReadMe.md)** is the command-line application.
- **[`zarg`](./zarg/ReadMe.md)** is the egui desktop archive manager.
- **[`zarlib`](./doc/zarlib-developer-guide.md)** is the reusable archive
  format and operations library.
- **[`zcopy`](./zcopy/README.md)** copies file trees between machines over the
  network, compressed and optionally encrypted, sending only missing or newer files.

## What makes ZAR different

| Feature | ZAR | zip | tar.gz | 7z |
|---|---|---|---|---|
| Fast parallel compression (zstd, block based) | ✅ | ❌ (deflate) | ❌ (single-threaded gzip) | ❌ (slow LZMA) |
| Random access to a single file | ✅ | ✅ | ❌ (stream must be read) | ⚠️ solid blocks |
| Append, remove, and join without rewriting data | ✅ (commits) | ⚠️ append only | ❌ | ❌ |
| History: older versions stay recoverable | ✅ | ❌ | ❌ | ❌ |
| Content deduplication (within one create/add) | ✅ | ❌ | ❌ | ⚠️ solid only |
| Modern authenticated encryption (AES-256-GCM / ChaCha20-Poly1305, Argon2id) | ✅ | ⚠️ weak ZipCrypto or AES | ❌ needs wrapper | ✅ AES-CBC |
| Encrypt data only, header only, or both; per-commit passwords | ✅ | ❌ | ❌ | ⚠️ |
| Per-block checksums and damage recovery (`check --recover`) | ✅ | ⚠️ CRC | ❌ | ⚠️ |
| Safe extraction (no path traversal, no-follow on Unix) | ✅ | varies | varies | varies |
| Library bindings: Python, C, C++, Go, Java, C# | ✅ | varies | ❌ | varies |

Highlights:

- **Speed.** Data is split into blocks compressed in parallel with Zstandard.
  On the Linux 4.0.1 source tree (525 MiB, 48,945 files, 8 cores, hot cache)
  zarlib compressed in 4.6 s versus 19.6 s for zip, 17.5 s for tar.gz and 93 s
  for 7z, and extracted in 4.9 s versus 10.9 s for zip. Encryption costs only
  about 30% extra time. 7z produces smaller archives, at 20 times the compression time.
  See the [full report](./doc/benchmark-report-linux.md) (single runs, not averages).
- **Commit-based history.** `add`, `remove`, and `join` append or compact
  commits instead of rewriting the archive; the archive is published atomically,
  and the original survives any failure.
- **Security by design.** Authenticated encryption with flexible scope,
  commit-specific passwords, bounded KDF parameters, strict canonical UTF-8
  archive paths, and extraction that cannot escape the destination.
- **Integrity.** Every frame is checksummed, `check` verifies the entire
  archive, and recovery scans past damaged regions.
- **One core, many front ends.** CLI, GUI, and seven language bindings share the
  same Rust implementation and behavior.
- **Network copy.** `zcopy` reuses the pipelines to send only new or changed
  files, compressed and encrypted, between machines.

Limitations to be aware of: ZAR does not restore symlinks or devices unless
asked, archive metadata has no modification times (`zcopy` transfers them
separately), and `remove`/`join` do not yet support encrypted headers. 7z still
wins on compression ratio.

## Language wrappers

`zarlib` is also available from other languages. All wrappers expose the same
operations (create, add, remove, join, check, info, list, extract, and
in-memory extract) and include tests and examples.

| Wrapper | Language | Mechanism |
|---------|----------|-----------|
| **[`pyzarlib`](./pyzarlib/README.md)** | Python | PyO3 extension module |
| **[`czarlib`](./czarlib/README.md)** | C | JSON C ABI (shared/static library and header) |
| **[`cxxzarlib`](./cxxzarlib/README.md)** | C++ | CXX bridge |
| **[`gozarlib`](./gozarlib/README.md)** | Go | cgo over `czarlib` |
| **[`jzarlib-ffm`](./jzarlib-ffm/README.md)** | Java 22+ | Foreign Function & Memory API over `czarlib` |
| **[`jzarlib-jni`](./jzarlib-jni/README.md)** | Java | JNI shim over `czarlib` |
| **[`csharpzarlib`](./csharpzarlib/README.md)** | C# (.NET 10) | P/Invoke over `czarlib` |

The Go, Java, and C# wrappers load the `czarlib` shared library, which is built
with `cargo build -p czarlib`.

The archive format stores data in commits. Commits support compressed and
deduplicated data blocks, an ordered logical table of contents (TOC), integrity
checks, and optional password-based encryption. See the
[format description](./doc/zar-file-format.md) for details.

## Build

Install a Rust toolchain that supports the 2024 edition, then build or test the
workspace from the repository root:

```sh
cargo build --workspace --release
cargo test --workspace
```

Run the CLI directly from the workspace:

```sh
cargo run --release -p zarc -- --help
cargo run --release -p zarg
```

Or install the executable into Cargo's binary directory:

```sh
cargo install --path zarc
```

## Quick start

```sh
# Create an archive from files and directories
zarc create backup.zar documents/ src/ notes.txt

# Replace backup.zar atomically with a newly created archive
zarc create --overwrite backup.zar documents/ src/ notes.txt

# Inspect the archive and list its current contents
zarc info -v backup.zar
zarc list backup.zar

# Verify the archive and extract its contents
zarc check backup.zar
zarc extract backup.zar --to ./restored/
```

Use `zarc <command> --help` for command-specific options. See the [CLI guide](./zarc/ReadMe.md)
for workflows, options, and limitations, and the [library developer guide](./doc/zarlib-developer-guide.md)
to call the Rust API directly.

## Format and safety notes

- The `.zar` format uses a `ZAR0` container header and ordered commits. Each
  commit stores data blocks followed by its header frame.
- Frame checksums detect accidental corruption; they are not authentication.
  Encrypted frames use authenticated encryption.
- `zarc check` verifies readable framing, checksums, and logical state. Without
  the password, encrypted payloads cannot be authenticated or decrypted and
  headers encrypted by password cannot be interpreted.
- `zarc extract` does not restore symlinks or device nodes unless their
  corresponding options are enabled. Symlink targets are restricted to the
  extraction root. Unix extraction uses no-follow directory-relative file
  operations; the non-Unix fallback does not provide equivalent resistance to
  path races.
- `zarc add`, `remove`, and `join` currently do not support archives whose
  logical state requires decrypting an encrypted header. `join` is for readable,
  unencrypted state.
- `zarc check --recover` scans past damaged framing without modifying the
  archive. It reports recovered gaps as warnings; logical state is restarted
  after each gap.

## License

This workspace is licensed under either of the licenses listed in the
workspace manifests: [MIT](./LICENSE-MIT) or [Apache-2.0](./LICENSE-APACHE).
