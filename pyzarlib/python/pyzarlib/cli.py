"""Python command-line example built on the pyzarlib API."""

from __future__ import annotations

import argparse
import getpass
import json
import sys
import time
from pathlib import Path
from typing import Any, Sequence

from . import (
    add_archive,
    check_archive,
    create_archive,
    extract_archive,
    info_archive,
    join_archive,
    list_archive,
    remove_archive,
)


def _add_filters(parser: argparse.ArgumentParser, *, filelist: bool = True) -> None:
    parser.add_argument("--include-file", action="append", default=[], dest="include_files")
    parser.add_argument("--exclude-file", action="append", default=[], dest="exclude_files")
    parser.add_argument("--include-folder", action="append", default=[], dest="include_folders")
    parser.add_argument("--exclude-folder", action="append", default=[], dest="exclude_folders")
    if filelist:
        parser.add_argument("--filelist", action="append", type=Path, default=[])


def _add_encoding(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "--block-size",
        help="Block size in bytes, K/M suffix, or 'auto' (default: 8M)",
    )
    parser.add_argument(
        "--compression-level",
        type=int,
        help="Zstandard compression level (-7 through 22; default: 3)",
    )


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="zarc-py",
        description="Python example CLI for ZAR archives, powered by pyzarlib.",
    )
    commands = parser.add_subparsers(dest="command", required=True)

    create = commands.add_parser("create", aliases=["c"], help="create a new archive")
    create.add_argument("--strict", action="store_true")
    create.add_argument("--verbose", action="store_true")
    create.add_argument("--metadata", "--preserve-metadata", action="store_true")
    create.add_argument("--special-files", action="store_true")
    create.add_argument("--overwrite", action="store_true")
    create.add_argument("--password", action="store_true", help="prompt for encryption password")
    create.add_argument(
        "--aead",
        choices=["aes-256-gcm", "chacha20-poly1305"],
        default="aes-256-gcm",
        help="content encryption algorithm used with --password (default: aes-256-gcm)",
    )
    create.add_argument(
        "--encryption-scope",
        choices=["data", "header", "header-and-data"],
        default="header-and-data",
        help="what to encrypt (default: header-and-data)",
    )
    _add_filters(create)
    _add_encoding(create)
    create.add_argument("archive")
    create.add_argument("paths", nargs="+", type=Path)

    add = commands.add_parser("add", aliases=["a"], help="append files to an archive")
    add.add_argument("--strict", action="store_true")
    add.add_argument("--verbose", action="store_true")
    add.add_argument("--metadata", "--preserve-metadata", action="store_true")
    add.add_argument("--special-files", action="store_true")
    add.add_argument("--password", action="store_true", help="encrypt the new commit")
    add.add_argument("--aead", choices=["aes-256-gcm", "chacha20-poly1305"], default="aes-256-gcm")
    add.add_argument(
        "--encryption-scope",
        choices=["data", "header", "header-and-data"],
        default="header-and-data",
    )
    add.add_argument("--password-for", action="append", default=[], metavar="COMMIT_ID")
    _add_filters(add)
    _add_encoding(add)
    add.add_argument("archive")
    add.add_argument("paths", nargs="+", type=Path)

    remove = commands.add_parser("remove", aliases=["r"], help="append path tombstones")
    remove.add_argument("archive")
    remove.add_argument("paths", nargs="+", type=Path)

    join = commands.add_parser("join", aliases=["j"], help="compact an inclusive commit range")
    _add_encoding(join)
    join.add_argument("archive")
    join.add_argument("--from", dest="from_id", required=True)
    join.add_argument("--through", required=True)

    check = commands.add_parser("check", aliases=["k"], help="validate an archive")
    check.add_argument("--recover", action="store_true")
    check.add_argument("--password", action="store_true", help="prompt for archive password")
    check.add_argument("--password-for", action="append", default=[], metavar="COMMIT_ID")
    check.add_argument("archive")

    info = commands.add_parser("info", aliases=["i"], help="show commit metadata")
    info.add_argument("-v", "--verbose", action="store_true")
    info.add_argument("--json", action="store_true")
    info.add_argument("archive")

    listing = commands.add_parser("list", aliases=["l"], help="list archive entries")
    listing.add_argument("--through")
    listing.add_argument("--password", action="store_true", help="prompt for archive password")
    listing.add_argument("--password-for", action="append", default=[], metavar="COMMIT_ID")
    listing.add_argument("archive")

    extract = commands.add_parser("extract", aliases=["e"], help="extract archive contents")
    extract.add_argument("--to", default=".")
    extract.add_argument("--through")
    extract.add_argument("--collision", choices=["rename", "first", "latest", "error"], default="rename")
    extract.add_argument("--existing", choices=["error", "overwrite", "skip"], default="error")
    extract.add_argument("--dry-run", action="store_true")
    extract.add_argument("--metadata", "--preserve-metadata", action="store_true")
    extract.add_argument("--symlinks", action="store_true")
    extract.add_argument("--devices", action="store_true")
    extract.add_argument("--password", action="store_true", help="prompt for archive password")
    extract.add_argument("--password-for", action="append", default=[], metavar="COMMIT_ID")
    _add_filters(extract)
    extract.add_argument("archive")

    return parser


def _filters(args: argparse.Namespace) -> dict[str, list[str]]:
    include_files = list(args.include_files)
    exclude_files = list(args.exclude_files)
    include_folders = list(args.include_folders)
    exclude_folders = list(args.exclude_folders)

    for filelist in getattr(args, "filelist", []):
        try:
            lines = filelist.read_text(encoding="utf-8").splitlines()
        except OSError as error:
            raise ValueError(f"cannot read filter list {filelist}: {error}") from error
        for line_number, line in enumerate(lines, start=1):
            if not line.strip():
                continue
            if line.startswith("-"):
                pattern = line[1:]
                if not pattern:
                    raise ValueError(f"empty filter in {filelist} at line {line_number}")
                exclude_files.append(pattern)
                exclude_folders.append(pattern)
            else:
                include_files.append(line)
                include_folders.append(line)
    return {
        "include_files": include_files,
        "exclude_files": exclude_files,
        "include_folders": include_folders,
        "exclude_folders": exclude_folders,
    }


def _encoding(args: argparse.Namespace) -> dict[str, int]:
    block_size = getattr(args, "block_size", None)
    if block_size is None:
        parsed_block_size = 8 * 1024 * 1024
    elif block_size == "auto":
        parsed_block_size = 0
    else:
        multiplier = 1
        digits = block_size
        if block_size[-1:].lower() == "k":
            digits, multiplier = block_size[:-1], 1024
        elif block_size[-1:].lower() == "m":
            digits, multiplier = block_size[:-1], 1024 * 1024
        try:
            parsed_block_size = int(digits) * multiplier
        except ValueError as error:
            raise ValueError(f"invalid block size {block_size!r}") from error

    compression_level = getattr(args, "compression_level", None)
    return {
        "block_size": parsed_block_size,
        "compression_level": 3 if compression_level is None else compression_level,
    }


def _password(enabled: bool) -> bytes | None:
    if not enabled:
        return None
    value = getpass.getpass("Archive password: ")
    if not value:
        raise ValueError("password must not be empty")
    return value.encode()

def _passwords(
    enabled: bool, commit_ids: Sequence[str]
) -> tuple[bytes | None, dict[str, bytes]]:
    default = _password(enabled)
    per_commit: dict[str, bytes] = {}
    for commit_id in commit_ids:
        if commit_id in per_commit:
            raise ValueError(f"password for commit {commit_id} was specified more than once")
        value = getpass.getpass(f"Password for commit {commit_id}: ")
        if not value:
            raise ValueError("password must not be empty")
        per_commit[commit_id] = value.encode()
    return default, per_commit


def _display_size(size: int | None) -> str:
    if size is None:
        return "unavailable"
    units = ("B", "KiB", "MiB", "GiB", "TiB")
    value = float(size)
    unit = 0
    while value >= 1024 and unit < len(units) - 1:
        value /= 1024
        unit += 1
    return f"{size} B" if unit == 0 else f"{value:.2f} {units[unit]}"


def _format_commit(commit: dict[str, Any]) -> str:
    ratio = commit["ratio_percent"]
    ratio_text = "n/a" if ratio is None else f"{ratio:.2f}%"
    header_encrypted = commit["header_encrypted"]
    data_encrypted = commit["data_encrypted"]
    scope = (
        "header-and-data"
        if header_encrypted and data_encrypted
        else "header"
        if header_encrypted
        else "data"
        if data_encrypted
        else "none"
    )
    algorithm = commit["encryption_algorithm"] or "none"
    return (
        f"commit {commit['id']} size={_display_size(commit['size'])} "
        f"header={_display_size(commit['header_size'])} "
        f"data={_display_size(commit['data_size'])} "
        f"uncompressed={_display_size(commit['uncompressed_size'])} "
        f"ratio={ratio_text} encryption={scope} algorithm={algorithm}"
    )


def _run(args: argparse.Namespace) -> int:
    command = args.command

    if command in {"create", "c"}:
        result = create_archive(
            args.archive,
            args.paths,
            strict=args.strict,
            overwrite=args.overwrite,
            save_metadata=args.metadata,
            include_special_files=args.special_files,
            verbose=args.verbose,
            aead=args.aead,
            encryption_scope=args.encryption_scope,
            password=_password(args.password),
            **_encoding(args),
            **_filters(args),
        )
        for warning in result["warnings"]:
            print(warning, file=sys.stderr)
        if args.block_size == "auto":
            print(f"using automatic block size: {result['block_size']} bytes", file=sys.stderr)
        archive_size = Path(args.archive).stat().st_size
        print(f"Archive size: {_display_size(archive_size)}")
        return int(bool(result["warnings"]))

    if command in {"add", "a"}:
        result = add_archive(
            args.archive,
            args.paths,
            strict=args.strict,
            save_metadata=args.metadata,
            include_special_files=args.special_files,
            verbose=args.verbose,
            password=_password(args.password),
            aead=args.aead,
            encryption_scope=args.encryption_scope,
            passwords=_passwords(False, args.password_for)[1],
            **_encoding(args),
            **_filters(args),
        )
        for warning in result["warnings"]:
            print(warning, file=sys.stderr)
        print(f"Archive size: {_display_size(Path(args.archive).stat().st_size)}")
        return 0

    if command in {"remove", "r"}:
        remove_archive(args.archive, args.paths)
        return 0

    if command in {"join", "j"}:
        result = join_archive(
            args.archive,
            args.from_id,
            args.through,
            **_encoding(args),
        )
        for warning in result["warnings"]:
            print(warning, file=sys.stderr)
        return 0

    if command in {"check", "k"}:
        report = check_archive(
            args.archive,
            recover=args.recover,
            passwords=_passwords(False, args.password_for)[1],
            password=_password(args.password),
        )
        print(
            f"valid framing: {report['commits']} commit(s); "
            f"content-checked {report['checked_blocks']} unencrypted block(s)"
        )
        if report["recoveries"]:
            print(f"recovery scan found {report['recoveries']} damaged framing gap(s)")
        if report["encrypted_blocks"]:
            print(f"content not checked: {report['encrypted_blocks']} encrypted block(s)")
        if not report["logical_state_readable"]:
            print("logical TOC consistency not fully checked: encrypted header present")
        for message in report["recovery_messages"]:
            print(message, file=sys.stderr)
        return int(report["recoveries"] > 0)

    if command in {"info", "i"}:
        commits = info_archive(args.archive, verbose=args.verbose)
        if args.json:
            print(
                json.dumps(
                    {
                        "commit_count": len(commits),
                        "commits": [
                            {
                                "id": item["id"],
                                "size": item["size"],
                                "header_size": item["header_size"],
                                "data_size": item["data_size"],
                                "header_encrypted": item["header_encrypted"],
                                "data_encrypted": item["data_encrypted"],
                                "encryption_algorithm": item["encryption_algorithm"],
                            }
                            for item in commits
                        ],
                    }
                )
            )
        else:
            for commit in commits:
                print(_format_commit(commit))
                if commit["details"] is not None:
                    details = commit["details"]
                    for name, group in details["groups"].items():
                        print(
                            f"  {name}: {group['files']} file(s), compressed "
                            f"{_display_size(group['compressed_size'])}, "
                            f"uncompressed {_display_size(group['uncompressed_size'])}"
                        )
                    if details["deduplicated_files"]:
                        print(
                            f"  deduplicated files: {details['deduplicated_files']}, "
                            "saved uncompressed space: "
                            f"{_display_size(details['deduplicated_uncompressed_size'])}"
                        )
                    if details["symlinks"] or details["named_pipes"] or details["devices"]:
                        print(
                            f"  special entries: {details['symlinks']} symlinks, "
                            f"{details['named_pipes']} pipes, {details['devices']} devices"
                        )
            print(f"{len(commits)} commit(s)")
        return 0

    if command in {"list", "l"}:
        entries = list_archive(
            args.archive,
            through=args.through,
            passwords=_passwords(False, args.password_for)[1],
            password=_password(args.password),
        )
        for entry in entries:
            path = entry["path"]
            if entry["type"] == "directory":
                print(f"{path}/")
            elif entry["type"] == "file":
                print(f"{path}\t{entry['size']} bytes")
            elif entry["type"] == "symlink":
                print(f"{path} -> {entry['target']}")
            elif entry["type"] == "named_pipe":
                print(f"{path} [named pipe]")
            elif entry["type"] == "device":
                print(
                    f"{path} [{entry['device_kind']} device "
                    f"{entry['major']}:{entry['minor']}]"
                )
        return 0

    if command in {"extract", "e"}:
        plan = extract_archive(
            args.archive,
            args.to,
            dry_run=args.dry_run,
            through=args.through,
            passwords=_passwords(False, args.password_for)[1],
            password=_password(args.password),
            collision=args.collision,
            existing=args.existing,
            restore_metadata=args.metadata,
            restore_symlinks=args.symlinks,
            restore_devices=args.devices,
            **_filters(args),
        )
        if args.dry_run:
            for path in plan["directories"]:
                print(f"create directory {path}")
            for item in plan["files"]:
                kind = item["type"]
                if kind == "file":
                    print(f"write {item['path']} ({item['size']} bytes)")
                elif kind == "symlink":
                    print(f"create symlink {item['path']} -> {item['target']}")
                else:
                    print(f"create {kind} {item['path']}")
        return 0

    raise ValueError(f"unsupported command {command}")


def main(argv: Sequence[str] | None = None) -> int:
    parser = _parser()
    args = parser.parse_args(argv)
    started = time.monotonic()
    try:
        exit_code = _run(args)
    except OSError as error:
        print(f"zarc-py: {error}", file=sys.stderr)
        return 3
    except ValueError as error:
        print(f"zarc-py: {error}", file=sys.stderr)
        return 2
    except RuntimeError as error:
        print(f"zarc-py: {error}", file=sys.stderr)
        return 4
    print(f"Time: {time.monotonic() - started:.2f}s", file=sys.stderr)
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
