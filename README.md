# ZAR

ZAR is an archive format and toolkit with compression, deduplication, and
commit-based history. It is written in Rust and includes a command-line tool,
a desktop GUI, a reusable library, language bindings, and a network copy tool.

See the [changelog](./CHANGELOG.md) for recent project updates. See
[CONTRIBUTING.md](./CONTRIBUTING.md) for development setup, checks, and pull
request guidance.

- **[`zarc`](./zarc/README.md)** is the command-line application.
- **[`zarg`](./zarg/README.md)** is the egui desktop archive manager.
- **[`zarlib`](./doc/zarlib-developer-guide.md)** is the reusable archive
  format and operations library.
- **[`zcopy`](./zcopy/README.md)** copies file trees between machines over the
  network using compressed, optionally encrypted ZAR batches. It can update
  missing or newer files on the destination or save a complete packed archive.

## What makes ZAR different

| Feature | ZAR | zip | tar.gz | 7z |
|---|---|---|---|---|
| Fast parallel compression (zstd, block based) | ✅ | ❌ (deflate) | ❌ (single-threaded gzip) | ❌ (slow LZMA) |
| Random access to a single file | ✅ | ✅ | ❌ (stream must be read) | ⚠️ solid blocks |
| Commit-based updates and compaction | ✅ | ⚠️ append only | ❌ | ❌ |
| History retained until explicitly compacted or removed | ✅ | ❌ | ❌ | ❌ |
| Content deduplication (within one create/add) | ✅ | ❌ | ❌ | ⚠️ solid only |
| Modern authenticated encryption (AES-256-GCM / ChaCha20-Poly1305, Argon2id) | ✅ | ⚠️ weak ZipCrypto or AES | ❌ needs wrapper | ✅ AES-CBC |
| Encrypt data only, header only, or both; per-commit passwords | ✅ | ❌ | ❌ | ⚠️ |
| Per-block checksums and damage recovery (`check --recover`) | ✅ | ⚠️ CRC | ❌ | ⚠️ |
| Safe extraction (no path traversal, no-follow on Unix) | ✅ | varies | varies | varies |
| Library integrations: Python, C, C++, Go, Java, C#, WASI | ✅ | varies | ❌ | varies |

Highlights:

- **Speed.** Data is split into blocks compressed in parallel with Zstandard.
  In one [Linux Core i7 benchmark](./doc/benchmarks/benchmark-report-linux-x86_64-core-i7.md)
  of the 525 MiB Linux 4.0.1 source tree, hot-cache unencrypted ZAR creation
  took 0.86 s and full extraction took 0.88–1.10 s. The same run produced a
  smaller 7z archive, but 7z creation took 24.91–24.94 s. Results depend on
  hardware, data, settings, and cache state. See
  [how to run benchmarks](./doc/benchmarks/README.md#running-benchmarks) and
  the [benchmark overview](./doc/benchmarks/README.md) for reports and charts.
  The [archive benchmark workflow](./.github/workflows/benchmark-archives.yml)
  runs on Windows, Linux, and macOS; each run uploads its generated report and
  chart from the `doc/benchmarks/` folder as workflow artifacts.
- **ZIP, 7z, TAR, and gzip extraction/import.** Pass a `.zip`, `.7z`, `.tar`,
  `.tar.gz`, `.tgz`, or standalone `.gz` file to `zarc extract` to write its
  contents directly; regular files are written in parallel. Standalone gzip
  extraction removes the `.gz` suffix from the output filename. These formats
  can also be passed to create or add, where their contents are imported under
  their paths in the new ZAR commit. This works through `zarlib`, `zarc`,
  `zarg`, and the language bindings. Encrypted ZIP/7z archives and ZIP/7z links
  are not supported. TAR symbolic links are preserved where the host supports
  them; hard links are imported as regular files. TAR paths with harmless `.`
  components are normalized; parent traversal and special entries are rejected.
  Source archive timestamps are not retained; `--dates` records import-time
  filesystem timestamps instead. 7z entries using COPY, LZMA/LZMA2, BZip2, PPMD,
  DEFLATE, Brotli, LZ4, or Zstandard are supported.
- **Batch archive conversion.** `zarc convert FOLDER` converts supported
  archive files directly inside a folder into separate `.zar` files, with an
  optional fnmatch filename filter and a separate output folder.
- **Commit-based history.** `add` records new versions, while `remove` records
  tombstones. Historical states remain available until commits are removed or
  compacted with `join`. Mutations stage a replacement archive; joining can
  rewrite compressed data.
- **Security by design.** Authenticated encryption with flexible scope,
  commit-specific passwords, bounded KDF parameters, strict canonical UTF-8
  archive paths, and extraction that cannot escape the destination.
- **Integrity.** Every frame is checksummed, `check` verifies the entire
  archive, and recovery scans past damaged regions.
- **One core, many front ends.** CLI, GUI, seven language bindings, and a
  server-side WASI component share the same Rust implementation and behavior.
- **Network copy.** `zcopy` transfers files missing from the destination or
  newer by modification time, or sends a complete packed archive. Encryption
  is optional; without a password the connection is not authenticated.

## Limitations

- `zarc extract` restores symlinks and devices only when explicitly requested.
  Device creation may require privileges, and symlink targets must remain
  within the extraction root.
- `create --dates` and `add --dates` can record entry modification times.
  Current archive extraction does not restore them. `zcopy` transfers and
  restores file modification times separately in its unpack mode.
- Only logical-header version 8 is supported. Archives created by unreleased
  versions that used logical-header versions 0–7 cannot be interpreted by
  current readers; all new commits use version 8.
- `zarc add` can use passwords for existing encrypted commits. The CLI
  `remove` and `join` commands do not accept those passwords: `remove` cannot
  operate when encrypted commits need passwords, and `join` skips locked
  commits rather than including them. Password-aware Rust library operations
  support removal and joining with credentials, subject to their own validation
  rules.
- Listing and extraction without passwords can omit commits with locked
  headers, producing a partial view. A visible data-encrypted commit still
  requires its password for extraction. Use passwords for all relevant
  commits when a complete result is required.
- `zcopy` is a one-way copy, not a mirror: it does not propagate deletions
  and compares whole-second modification times rather than file contents.
  See its [README](./zcopy/README.md) for cache and concurrency details.
- Compression ratio depends on data and settings. Some 7z configurations
  produce smaller archives in the provided benchmarks.

## Language bindings and runtime packages

`zarlib` is available through language bindings and a WASI component. They
expose archive operations through their respective language or runtime
interfaces; see each package's documentation for its API and limitations.

| Package | Language/runtime | Mechanism |
|---------|----------|-----------|
| **[`pyzarlib`](./pyzarlib/README.md)** | Python | PyO3 extension module |
| **[`czarlib`](./czarlib/README.md)** | C | JSON C ABI (shared/static library and header) |
| **[`cxxzarlib`](./cxxzarlib/README.md)** | C++ | CXX bridge |
| **[`gozarlib`](./gozarlib/README.md)** | Go | cgo over `czarlib` |
| **[`jzarlib-ffm`](./jzarlib-ffm/README.md)** | Java 25+ | Foreign Function & Memory API over `czarlib` |
| **[`jzarlib-jni`](./jzarlib-jni/README.md)** | Java | JNI shim over `czarlib` |
| **[`csharpzarlib`](./csharpzarlib/README.md)** | C# (.NET 10) | P/Invoke over `czarlib` |
| **[`wasizarlib`](./wasizarlib/README.md)** | WebAssembly / Node.js | WASI Preview 2 component with a WIT interface; Node support via Jco and the Preview 2 shim |

The Go, Java, and C# wrappers load the `czarlib` shared library, which is built
with `cargo build -p czarlib`.

The WASI component is built for `wasm32-wasip2`; its filesystem access is
limited to directories preopened by the server-side WASI runtime. Node.js
applications can run it through Jco and the Preview 2 shim; Node's built-in
`node:wasi` API is Preview 1 and does not directly run this component.

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
cargo run --release -p zcopy -- --help
```

Build the server-side WASI component separately:

```sh
rustup target add wasm32-wasip2
cargo build --release -p wasizarlib --target wasm32-wasip2
```

Or install the executable into Cargo's binary directory:

```sh
cargo install --path zarc
```

## Installers and packages

The [packaging](./packaging) directory contains installer definitions for the
`zarc` CLI and the `zarg` GUI:

- **Windows:** an [Inno Setup](https://jrsoftware.org/isinfo.php) script
  (`packaging/windows/zar-rs.iss`) that can also add `zarc` to the `PATH`.
- **Linux:** `packaging/linux/build-packages.sh [deb|rpm|appimage|flatpak|all]`
  builds `.deb`, `.rpm`, AppImage and Flatpak packages into `dist/`.
- **macOS:** `packaging/macos/build-packages.sh [dmg|pkg|all]` builds a `.dmg`
  with `Zarg.app` and a `.pkg` that also installs `zarc`.

```sh
iscc /DAppVersion=0.9.0 packaging\windows\zar-rs.iss   # Windows
packaging/linux/build-packages.sh deb                  # Linux
```

See [packaging/README.md](./packaging/README.md) for prerequisites and options.

## Quick start

```sh
# Create an archive from files and directories
zarc create backup.zar documents/ src/ notes.txt

# Convert a supported archive's contents into a ZAR archive
zarc create backup.zar downloaded.zip
zarc create backup-from-7z.zar downloaded.7z
zarc create backup-from-tar.zar downloaded.tar
zarc create backup-from-tar-gz.zar downloaded.tar.gz
zarc create backup-from-tgz.zar downloaded.tgz

# Replace backup.zar atomically with a newly created archive
zarc create --overwrite backup.zar documents/ src/ notes.txt

# Inspect the archive and list its current contents
zarc info -v backup.zar
zarc list backup.zar

# Verify the archive and extract its contents
zarc check backup.zar
zarc extract backup.zar --output ./restored/
```

Use `zarc <command> --help` for command-specific options. See the
[CLI guide](./zarc/README.md) for workflows, options, and limitations, and the
[library developer guide](./doc/zarlib-developer-guide.md) to call the Rust API
directly.

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
- `zarc add` accepts passwords for existing encrypted commits; `remove` and
  `join` do not. `join` operates on readable commits and leaves locked commits
  unchanged; some histories cannot be joined without unlocking a preceding
  header. The password-aware Rust library API provides more options.
- `zarc check --recover` scans past damaged framing without modifying the
  archive. It reports recovered gaps as warnings; logical state is restarted
  after each gap.

## License

This workspace is licensed under either of the licenses listed in the
workspace manifests: [MIT](./LICENSE-MIT) or [Apache-2.0](./LICENSE-APACHE).

## Notes to AI use

This project is a refactoring of another, fully hand written project. The whole
refactoring with several improvments in code and documentation were done with
AI.
