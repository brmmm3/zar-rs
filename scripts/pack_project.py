#!/usr/bin/env python3
"""Pack the project into a zip archive, skipping everything .gitignore excludes.

Uses only the Python standard library. Nested .gitignore files, negation (`!`),
anchored and directory-only patterns, and `**` are supported. The `.git`
directory and the output archive itself are always skipped.

Usage: scripts/pack_project.py [-o OUTPUT.zip] [-l LEVEL] [-n] [-v]
"""

import argparse
import os
import re
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def translate(pattern: str) -> str:
    """Translate a gitignore glob (without `/` prefix/suffix handling) to a regex."""
    out = []
    i, n = 0, len(pattern)
    while i < n:
        c = pattern[i]
        if c == "*":
            if pattern.startswith("**", i):
                j = i + 2
                at_start = i == 0 or pattern[i - 1] == "/"
                if at_start and j < n and pattern[j] == "/":
                    out.append("(?:.*/)?")
                    i = j + 1
                    continue
                if at_start and j == n:
                    out.append(".*")
                    i = j
                    continue
                out.append("[^/]*")
                i = j
                continue
            out.append("[^/]*")
        elif c == "?":
            out.append("[^/]")
        elif c == "[":
            j = i + 1
            if j < n and pattern[j] in "!^":
                j += 1
            if j < n and pattern[j] == "]":
                j += 1
            while j < n and pattern[j] != "]":
                j += 1
            if j >= n:
                out.append(re.escape(c))
            else:
                body = pattern[i + 1 : j]
                if body[:1] in "!^":
                    body = "^" + body[1:]
                out.append("[" + body.replace("\\", "\\\\") + "]")
                i = j
        elif c == "\\" and i + 1 < n:
            i += 1
            out.append(re.escape(pattern[i]))
        else:
            out.append(re.escape(c))
        i += 1
    return "".join(out)


class Rule:
    def __init__(self, line: str):
        self.negated = line.startswith("!")
        if self.negated:
            line = line[1:]
        self.dir_only = line.endswith("/")
        line = line.rstrip("/")
        anchored = "/" in line
        line = line.lstrip("/")
        body = translate(line)
        self.regex = re.compile(("" if anchored else "(?:.*/)?") + body + r"\Z")

    def matches(self, rel: str, is_dir: bool) -> bool:
        return (is_dir or not self.dir_only) and self.regex.match(rel) is not None


def load_rules(path: Path) -> list[Rule]:
    rules = []
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return rules
    for raw in text.splitlines():
        line = raw.rstrip("\r\n")
        # Trailing spaces are ignored unless escaped.
        while line.endswith(" ") and not line.endswith("\\ "):
            line = line[:-1]
        if not line or line.startswith("#"):
            continue
        rules.append(Rule(line))
    return rules


class Ignorer:
    """Evaluates .gitignore files from the root down to the entry's directory."""

    def __init__(self, root: Path):
        self.root = root
        self.cache: dict[Path, list[Rule]] = {}

    def rules_for(self, directory: Path) -> list[Rule]:
        if directory not in self.cache:
            self.cache[directory] = load_rules(directory / ".gitignore")
        return self.cache[directory]

    def is_ignored(self, path: Path, is_dir: bool) -> bool:
        rel_parts = path.relative_to(self.root).parts
        ignored = False
        # Later (deeper, lower) rules override earlier ones.
        for depth in range(len(rel_parts)):
            directory = self.root.joinpath(*rel_parts[:depth])
            rel = "/".join(rel_parts[depth:])
            for rule in self.rules_for(directory):
                if rule.matches(rel, is_dir):
                    ignored = not rule.negated
        return ignored


def collect(root: Path, skip: Path | None):
    ignorer = Ignorer(root)
    for current, dirs, files in os.walk(root):
        here = Path(current)
        kept = []
        for name in sorted(dirs):
            path = here / name
            if name == ".git" or ignorer.is_ignored(path, True):
                continue
            if path.is_symlink():
                files.append(name)  # store the link target as a file entry
                continue
            kept.append(name)
        dirs[:] = kept
        for name in sorted(files):
            path = here / name
            if skip is not None and path.resolve() == skip:
                continue
            if not ignorer.is_ignored(path, False):
                yield path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument(
        "-o", "--output", type=Path, default=ROOT / f"{ROOT.name}.zip",
        help="output zip file (default: <project>.zip in the project root)",
    )
    parser.add_argument("-l", "--level", type=int, default=6, help="deflate level 0-9")
    parser.add_argument("-n", "--dry-run", action="store_true", help="list files only")
    parser.add_argument("-v", "--verbose", action="store_true", help="print each file")
    args = parser.parse_args()
    output = args.output.resolve()

    files = list(collect(ROOT, output))
    if args.dry_run:
        for path in files:
            print(path.relative_to(ROOT).as_posix())
        print(f"{len(files)} file(s) would be packed", file=sys.stderr)
        return 0

    temp = output.with_name(output.name + ".part")
    total = 0
    try:
        with zipfile.ZipFile(
            temp, "w", zipfile.ZIP_DEFLATED, compresslevel=args.level
        ) as archive:
            for path in files:
                name = path.relative_to(ROOT).as_posix()
                if args.verbose:
                    print(name)
                try:
                    archive.write(path, f"{ROOT.name}/{name}")
                    total += path.stat().st_size
                except OSError as error:
                    print(f"warning: skipped {name}: {error}", file=sys.stderr)
        os.replace(temp, output)
    finally:
        if temp.exists():
            temp.unlink()
    print(
        f"Packed {len(files)} file(s), {total / 1048576:.1f} MiB -> "
        f"{output} ({output.stat().st_size / 1048576:.1f} MiB)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
