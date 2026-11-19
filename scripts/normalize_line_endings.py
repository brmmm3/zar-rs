#!/usr/bin/env python3
"""Replace CRLF line endings with LF in Git-tracked and non-ignored files."""

from __future__ import annotations

import argparse
import os
import stat
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
_UNICODE_BOMS = (
    (b"\x00\x00\xfe\xff", "utf-32-be"),
    (b"\xff\xfe\x00\x00", "utf-32-le"),
    (b"\xfe\xff", "utf-16-be"),
    (b"\xff\xfe", "utf-16-le"),
)


def git_files(root: Path) -> list[Path]:
    result = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=root,
        stdout=subprocess.PIPE,
        check=True,
    )
    return [root / os.fsdecode(name) for name in result.stdout.split(b"\0") if name]


def normalize_file(path: Path, *, dry_run: bool = False) -> bool:
    try:
        metadata = path.lstat()
    except FileNotFoundError:
        return False
    if not stat.S_ISREG(metadata.st_mode):
        return False

    content = path.read_bytes()
    newline = b"\r\n"
    replacement = b"\n"
    for bom, encoding in _UNICODE_BOMS:
        if content.startswith(bom):
            if encoding.startswith("utf-32"):
                newline, replacement = (
                    (b"\r\x00\x00\x00\n\x00\x00\x00", b"\n\x00\x00\x00")
                    if encoding.endswith("le")
                    else (b"\x00\x00\x00\r\x00\x00\x00\n", b"\x00\x00\x00\n")
                )
            else:
                newline, replacement = (
                    (b"\r\x00\n\x00", b"\n\x00")
                    if encoding.endswith("le")
                    else (b"\x00\r\x00\n", b"\x00\n")
                )
            break
    else:
        if b"\x00" in content:
            return False

    normalized = content.replace(newline, replacement)
    if normalized == content:
        return False
    if not dry_run:
        path.write_bytes(normalized)
    return True


def normalize_repository(root: Path, *, dry_run: bool = False) -> list[Path]:
    changed = []
    for path in git_files(root):
        if normalize_file(path, dry_run=dry_run):
            changed.append(path)
    return changed


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="list files that would change without modifying them",
    )
    args = parser.parse_args(argv)
    changed = normalize_repository(ROOT, dry_run=args.dry_run)
    action = "Would normalize" if args.dry_run else "Normalized"
    for path in changed:
        print(f"{action}: {path.relative_to(ROOT).as_posix()}")
    print(f"{action} {len(changed)} file(s).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
