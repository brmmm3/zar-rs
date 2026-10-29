# pyzarlib

`pyzarlib` provides Python bindings to the Rust `zarlib` archive library.
The package exposes high-level create, add, remove, join, check, info, list,
and extract operations.

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

## API

- `create_archive(archive, paths, *, strict=False, overwrite=False,
  save_metadata=False, include_special_files=False, verbose=False,
  block_size=8388608, compression_level=3, password=None, include_files=None,
  exclude_files=None, include_folders=None, exclude_folders=None)` creates an
  archive and returns its resolved block size and warnings.
- `add_archive(archive, paths, *, strict=False, save_metadata=False,
  include_special_files=False, verbose=False, block_size=8388608,
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

Long-running Rust operations release the Python GIL. The initial binding does
not expose per-operation progress callbacks.
