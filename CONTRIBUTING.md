# Contributing

Thanks for considering a contribution to ZAR.

## Getting started

ZAR is a Cargo workspace containing the archive library, command-line and desktop
applications, language bindings, and network copy tool. Install a stable Rust
toolchain, then run these commands from the repository root:

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

Some packages have additional test or build requirements. See their package
documentation before changing those components.

## What CI checks

The GitHub Actions workflows run formatting, Clippy, and workspace tests. They
also build and test the C API, Python and C# bindings, Java bindings, and the
WASI/Node.js package, and build binaries for supported platforms.

The main workflow is [`ci.yml`](.github/workflows/ci.yml); packaging jobs are
defined in [`packaging.yml`](.github/workflows/packaging.yml).

## Pull requests

1. Open an issue first for larger changes so the design can be discussed.
2. Keep pull requests focused — one topic per pull request.
3. Add or update tests and documentation alongside behavior or API changes.
4. Add a suitable entry under `## Unreleased` in the root
   [`CHANGELOG.md`](CHANGELOG.md) for user-visible changes.
5. Run the relevant checks from “Getting started” before opening a pull request.

## Documentation map

| Path | Contents |
|---|---|
| [`ReadMe.md`](ReadMe.md) | Project overview, features, and workspace build instructions |
| [`doc/`](doc/) | Archive format, library developer guide, and benchmark reports |
| [`zarc/ReadMe.md`](zarc/ReadMe.md), [`zarg/ReadMe.md`](zarg/ReadMe.md) | CLI and desktop application |
| [`zarlib/`](zarlib/) | Core archive library |
| [`pyzarlib/README.md`](pyzarlib/README.md), [`czarlib/README.md`](czarlib/README.md), [`cxxzarlib/README.md`](cxxzarlib/README.md), [`gozarlib/README.md`](gozarlib/README.md) | Python, C, C++, and Go bindings |
| [`jzarlib-jni/README.md`](jzarlib-jni/README.md), [`jzarlib-ffm/README.md`](jzarlib-ffm/README.md), [`csharpzarlib/README.md`](csharpzarlib/README.md) | Java and C# bindings |
| [`wasizarlib/README.md`](wasizarlib/README.md), [`wasizarlib/nodejs/README.md`](wasizarlib/nodejs/README.md) | WASI component and Node.js package |
| [`zcopy/README.md`](zcopy/README.md) | Network copy tool |
| [`packaging/README.md`](packaging/README.md) | Platform packaging |

## Reporting bugs

Include your operating system and architecture, Rust or relevant runtime
version, the affected package or command, and a minimal reproduction. Include
the exact command and error output when possible. For performance reports,
include the input, environment, and steps needed to reproduce the measurement.

## License

By contributing, you agree that your contributions are licensed under
`MIT OR Apache-2.0`, matching the rest of the project. See
[`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE).
