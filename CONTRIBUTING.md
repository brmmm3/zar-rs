# Contributing

Thanks for considering a contribution to ZAR.

## Getting started

ZAR is a Cargo workspace containing the archive library, command-line and
desktop applications, language bindings, and network copy tool. Install a stable
Rust toolchain, then run these commands from the repository root:

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

### Running the CI workflow locally

Run `python3 scripts/run_ci.py` from any directory to execute the applicable
`ci.yml` jobs and matrix entries sequentially. The script runs the release
builds even when the checkout is not tagged and puts produced artifacts in
`dist/`; it never publishes a release. Existing files in `dist/` are not
cleaned, so remove or move old artifacts separately when a clean output is
needed.

The script runs jobs on the current operating system and CPU architecture,
including the C++, Go, and C# bindings where their native toolchains are
supported, release-mode binding builds, the Ruff, rumdl, and zizmor checks, and
the matching Linux musl build. Other matrix entries are explicitly reported as
skipped; run the script on each CI host to cover the full matrix.
GitHub checkout, artifact reuse, uploads, and publication are not performed.
Linux wheels are built on the host rather than in the CI manylinux container,
so their portability may differ. Cross-host job dependencies cannot be enforced
locally; run the Linux checks before packaging on other hosts.

Install the stable Rust toolchain beforehand. The runner installs missing
`rustfmt`/`clippy` components and Rust targets as needed. On Linux it installs
supported build tools through the system package manager; on Windows it prefers
Chocolatey and falls back to `winget` for supported tools. It installs
`maturin`, `rumdl`, and
`zizmor` with Cargo and Ruff with pip. Windows binary builds use `cargo-xwin`;
the runner installs it along with Clang/LLVM when needed. These installs may
require administrator privileges or `sudo`. Pass `--no-install` to disable
automatic setup.

Python wheels are built in release mode for CPython 3.9 through 3.15. Linux
Python test matrix versions are provisioned with pyenv, including
free-threaded builds, and each temporary test environment installs `pytest>=8`.
Windows wheel builds use the Python launcher and install missing versions with
Chocolatey, falling back to `winget`. Select a specific interpreter with, for
example, `--python 3.14t=/path/to/python`. Other versioned runtimes, including
Node.js 22 must be installed separately. The WASI Node package must
already have its dependencies installed with `npm ci --include=optional`.
Missing prerequisites fail the affected job and are listed in the summary; jobs
blocked by failed dependencies are skipped. Any failed job gives the script a
nonzero exit status.

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
| [`README.md`](README.md) | Project overview, features, and workspace build instructions |
| [`doc/`](doc/) | Archive format, library developer guide, and benchmark reports |
| [`zarc/README.md`](zarc/README.md), [`zarg/README.md`](zarg/README.md) | CLI and desktop application |
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
