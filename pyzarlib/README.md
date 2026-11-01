# pyzarlib

`pyzarlib` provides Python bindings to the Rust `zarlib` archive library.
The package exposes high-level create, add, remove, join, check, info, list,
and extract operations.

## Python compatibility

The package requires Python 3.9 or newer. The CI matrix covers standard CPython
3.9 through 3.15, plus free-threaded CPython 3.14t and 3.15t. The PyO3
extension declares that it does not require the GIL; concurrent-call tests
exercise the binding from multiple Python threads. A free-threaded test verifies
that importing the extension does not turn the GIL back on.

Compatibility review: the official [Python 3.14](https://docs.python.org/3.14/whatsnew/3.14.html)
and [Python 3.15](https://docs.python.org/3.15/whatsnew/3.15.html) release
notes and porting sections, and the [PyO3 free-threading guide](https://pyo3.rs/v0.30.1/free-threading.html).

## Build and install

Install [Maturin](https://maturin.rs) and build/install the package from this
directory:

```sh
python -m pip install maturin
maturin develop --release
```

To build a wheel:

```sh
maturin build --release
```

For development, install the package with pytest support and run the Python API
tests from this directory:

```sh
python -m pip install -e ".[test]"
maturin develop
python -m pytest
```

## Python zarc example

The package includes a Python CLI port of the main `zarc` archive commands.
Install the package to get the `zarc-py` command, or run the example entry point
from this directory:

```sh
zarc-py --help
python examples/zarc.py create backup.zar src/ README.md
zarc-py list backup.zar
zarc-py check backup.zar
zarc-py extract backup.zar --to restored --dry-run
```

It supports create (`c`), add (`a`), remove (`r`), join (`j`), check (`k`),
info (`i`), list (`l`), and extract (`e`), including path filters and file
list rules. `--password` prompts without echo where encryption is supported.
Progress bars and interactive `ask` extraction policies are omitted because
the current Python binding does not provide an interactive progress callback.
The source is in [`python/pyzarlib/cli.py`](./python/pyzarlib/cli.py); it is an
example consumer of the same public Python API documented below, not a second
archive implementation.

The Python API tests exercise archive operations through `pyzarlib`; the
pytest suite also runs the complete native `zarlib` unit-test suite so
Rust-only framing, crypto, and data-model tests are included.

## Example

```python
from pathlib import Path

import pyzarlib

created = pyzarlib.create_archive(
    "backup.zar",
    [Path("src"), Path("Cargo.toml")],
)
print(f"Created with {len(created['warnings'])} warning(s)")

for entry in pyzarlib.list_archive("backup.zar"):
    print(entry["path"], entry["type"], entry.get("size", ""))

report = pyzarlib.check_archive("backup.zar")
print(f"Checked {report['commits']} commit(s)")

plan = pyzarlib.extract_archive(
    "backup.zar",
    "restored",
    dry_run=True,
)
print(f"Would extract {len(plan['files'])} file(s)")

contents = pyzarlib.extract_archive_to_memory(
    "backup.zar",
    size_limit=1024 * 1024,
    offset=0,
)
for path, data in contents.items():
    print(path, len(data))
```

## Password encryption: AES-256-GCM and ChaCha20-Poly1305

Password-encrypted creation supports independently encrypted headers and data.
The `encryption_scope` argument accepts `"data"`, `"header"`, or
`"header-and-data"` (the default). With `"data"`, `info_archive` and
`list_archive` can read the logical header without a password, but extracting
data still requires one. `info_archive` reports each commit's header/data
encryption status and AEAD algorithm.

The `aead` argument accepts `"aes-256-gcm"` or `"chacha20-poly1305"` and
defaults to AES-256-GCM. Each commit has its own encryption descriptor. For
operations over multiple commits, pass `passwords={commit_id: password_bytes}`.
A single `password=b"..."` is a fallback tried for every encrypted commit not
listed in `passwords`; a specific entry takes precedence. The algorithm is
read from each commit's descriptor.

```python
from getpass import getpass
from pathlib import Path

import pyzarlib

password = getpass("Archive password: ").encode("utf-8")
inputs = [Path("secrets")]

for algorithm in ("aes-256-gcm", "chacha20-poly1305"):
    archive = f"secrets-{algorithm}.zar"
    pyzarlib.create_archive(
        archive,
        inputs,
        password=password,
        aead=algorithm,
    )

    check = pyzarlib.check_archive(archive, password=password)
    entries = pyzarlib.list_archive(archive, password=password)
    plan = pyzarlib.extract_archive(
        archive,
        f"restored-{algorithm}",
        password=password,
        dry_run=True,
    )
    print(algorithm, check["commits"], len(entries), len(plan["files"]))
```

In a real script, don't put a password in source code or command-line
arguments. The Python `zarc-py` CLI prompts without echo:

```sh
zarc-py create --password secrets-aes.zar secrets/
zarc-py create --password --aead chacha20-poly1305 secrets-chacha.zar secrets/
zarc-py check --password secrets-chacha.zar
zarc-py list --password secrets-chacha.zar
zarc-py extract --password --to restored/ secrets-chacha.zar
```

The archive stores the selected AEAD in its encryption descriptor. The
implementation creates fresh key/nonce material and uses Argon2id for
password-based key wrapping. Don't attempt to reuse nonces or store the
password in the archive. `add_archive` can append a new encrypted commit;
provide credentials for encrypted earlier commits when those commits need
validation. `remove_archive` and `join_archive` still reject encrypted archives
because encrypted stateful rewriting is not implemented.

### Different passwords per commit

The global `password` is used as a fallback; explicit commit IDs override it.
Commit IDs are the eight-hex-digit values returned by `info_archive`.

```python
commit_passwords = {
    "00000001": b"older-commit-password",
    "00000002": b"newer-commit-password",
}

entries = pyzarlib.list_archive("mixed.zar", passwords=commit_passwords)
contents = pyzarlib.extract_archive_to_memory(
    "mixed.zar",
    passwords=commit_passwords,
)
```

### Data-only encryption

```python
pyzarlib.create_archive(
    "visible-header.zar",
    ["public-index.txt"],
    password=b"data-password",
    encryption_scope="data",
)

print(pyzarlib.info_archive("visible-header.zar")[0]["data_encrypted"])
```

## Examples matching the `zarc` commands

These Python examples use the same operations as the `zarc` create, add,
remove, join, check, info, list, and extract commands. Run them from a working
directory containing `src/`, `README.md`, and any other referenced input
files. Inputs may be strings or `pathlib.Path` objects.
These examples use an unencrypted archive. Removing and joining encrypted
archives are not supported by the current high-level operations.

### Create

Equivalent CLI: `zarc create backup.zar src/ README.md`

```python
import pyzarlib

result = pyzarlib.create_archive("backup.zar", ["src", "README.md"])
print(f"Created using {result['block_size']} byte blocks")
```

### Add or replace files

Equivalent CLI: `zarc add backup.zar config.toml`

```python
import pyzarlib

pyzarlib.add_archive("backup.zar", ["config.toml"])
```

### List and inspect commits

Equivalent CLI: `zarc list backup.zar` and `zarc info backup.zar`

```python
import pyzarlib

for entry in pyzarlib.list_archive("backup.zar"):
    print(entry["path"], entry["type"], entry.get("size", ""))

for commit in pyzarlib.info_archive("backup.zar"):
    print(f"commit {commit['id']}: {commit['size']} bytes")
```

### Check integrity

Equivalent CLI: `zarc check backup.zar`

```python
import pyzarlib

report = pyzarlib.check_archive("backup.zar")
print(
    f"{report['commits']} commits checked; "
    f"{report['checked_blocks']} data blocks verified"
)
```

### Extract files

Equivalent CLI: `zarc extract backup.zar --to restored`

```python
import pyzarlib

plan = pyzarlib.extract_archive("backup.zar", "restored")
print(f"Extracted {len(plan['files'])} files")
```

Pass `dry_run=True` to preview the extraction without writing files.

### Remove a path

Equivalent CLI: `zarc remove backup.zar obsolete.txt`

```python
import pyzarlib

pyzarlib.remove_archive("backup.zar", ["obsolete.txt"])
```

Removal paths are archive-relative names, not filesystem paths to delete.
Removing a directory also removes its descendants from the effective table of
contents; earlier commits remain unchanged.

### Join a commit range

Equivalent CLI: `zarc join backup.zar --from <first-id> --through <last-id>`

The IDs are the eight-hex-digit commit IDs returned by `info_archive`. This
example compacts the first two commits in physical order:

```python
import pyzarlib

commits = pyzarlib.info_archive("backup.zar")
if len(commits) >= 2:
    result = pyzarlib.join_archive(
        "backup.zar",
        commits[0]["id"],
        commits[1]["id"],
    )
    print(f"Join completed with {len(result['warnings'])} warning(s)")
```

The range endpoints are inclusive and must identify a contiguous range in
physical commit order.

## API

- `create_archive(archive, paths, *, strict=False, overwrite=False,
  save_metadata=False, save_dates=False, include_special_files=False,
  verbose=False, block_size=8388608, compression_level=3, password=None, include_files=None,
  exclude_files=None, include_folders=None, exclude_folders=None)` creates an
  archive and returns its resolved block size and warnings.
- `add_archive(archive, paths, *, strict=False, save_metadata=False,
  save_dates=False, include_special_files=False, verbose=False, block_size=8388608,
  compression_level=3, include_files=None, exclude_files=None,
  include_folders=None, exclude_folders=None)` appends a commit and returns
  warnings.
- `remove_archive(archive, paths)` appends tombstones.
- `join_archive(archive, from_id, through_id, *, block_size=8388608,
  compression_level=3)` compacts a contiguous commit range and returns
  warnings.
- `check_archive(archive, *, recover=False, password=None)` validates the
  archive and returns commit/block/recovery counts.
- `info_archive(archive, *, verbose=False)` returns commit size, compression,
  and optional detailed statistics.
- `list_archive(archive, *, through=None, password=None)` returns the
  effective TOC as dictionaries.
- `extract_archive(archive, destination, *, dry_run=False, through=None,
  password=None, collision="rename", existing="error", restore_metadata=False,
  restore_symlinks=False, restore_devices=False, include_files=None,
  exclude_files=None, include_folders=None, exclude_folders=None)` extracts
  selected entries and returns the resulting plan as dictionaries. Set
  `dry_run=True` to plan without writing files.
- `extract_archive_to_memory(archive, *, through=None, password=None,
  size_limit=None, offset=0, include_files=None, exclude_files=None,
  include_folders=None, exclude_folders=None)` returns a dictionary mapping
  archive-relative regular-file paths to `bytes`. The same `offset` is applied
  to each file; `size_limit` caps the number of bytes returned after that
  offset. Files are returned even when the offset is past end-of-file, in
  which case their value is `b""`. Directories, symlinks, pipes, and devices
  are omitted.

Password arguments accept `bytes` (or `None`). Input paths and destinations
accept strings and `pathlib.Path` values. Include/exclude arguments are
sequences of fnmatch patterns. Compression block size is 64 KiB through 32
MiB; `block_size=0` requests automatic sizing. Zstandard compression levels
range from -7 through 22.

`create_archive` also accepts `aead="aes-256-gcm"` or
`aead="chacha20-poly1305"` when creating an encrypted archive; the default is
AES-256-GCM. This option selects the password wrapping and content AEAD
algorithm. Read operations use the algorithm recorded in the archive and need
only the matching password.

Long-running Rust operations release the Python GIL. The initial binding does
not expose per-operation progress callbacks.
