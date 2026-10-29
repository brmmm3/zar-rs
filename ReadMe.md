# ZAR

ZAR is a Rust workspace for creating, inspecting, validating, and extracting
`.zar` archives. It contains three crates:

- **[`zarc`](./zarc/ReadMe.md)** is the command-line application.
- **[`zarg`](./zarg/ReadMe.md)** is the egui desktop archive manager.
- **[`zarlib`](./doc/zarlib-developer-guide.md)** is the reusable archive
  format and operations library.

The archive format stores data in commits. Commits support compressed and
deduplicated data blocks, an ordered logical table of contents (TOC), integrity
checks, and optional password-based encryption.

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
workspace manifests: MIT or Apache-2.0.
