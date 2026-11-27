#!/usr/bin/env python3
"""Check per-crate line coverage from a cargo-llvm-cov JSON summary."""

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CRATES = ("zarlib", "zarlib-cli", "zarlib-gui")


def summarize(report: dict, root: Path = ROOT) -> dict[str, tuple[int, int]]:
    data = report["data"]
    if len(data) != 1:
        raise ValueError("expected exactly one coverage data set")
    totals = {crate: (0, 0) for crate in CRATES}
    seen = set()
    for entry in data[0]["files"]:
        path = Path(entry["filename"])
        if not path.is_relative_to(root):
            if any(f"/{crate}/src/" in path.as_posix() for crate in CRATES):
                raise ValueError(f"source path is outside the workspace: {path}")
            continue
        parts = path.relative_to(root).parts
        if len(parts) < 3 or parts[0] not in totals or parts[1] != "src":
            continue
        if path in seen:
            raise ValueError(f"duplicate coverage entry: {path}")
        seen.add(path)
        lines = entry["summary"]["lines"]
        count, covered = lines["count"], lines["covered"]
        if type(count) is not int or type(covered) is not int or not 0 <= covered <= count:
            raise ValueError(f"invalid line counts for {path}")
        previous_count, previous_covered = totals[parts[0]]
        totals[parts[0]] = (previous_count + count, previous_covered + covered)
    for crate, (count, _) in totals.items():
        if count == 0:
            raise ValueError(f"no instrumented source lines found for {crate}")
    return totals


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path, help="cargo-llvm-cov JSON summary")
    args = parser.parse_args()
    try:
        report = json.loads(args.report.read_text(encoding="utf-8"))
        totals = summarize(report)
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"coverage report error: {error}", file=sys.stderr)
        return 2
    for crate, (count, covered) in totals.items():
        print(f"{crate}: {covered}/{count} lines ({covered / count * 100:.2f}%)")
    return int(any(covered * 100 <= count * 95 for count, covered in totals.values()))


if __name__ == "__main__":
    sys.exit(main())
