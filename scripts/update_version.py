#!/usr/bin/env python3
"""Update current project package versions without changing release history or hashes."""

from __future__ import annotations

import argparse
import json
import re
import sys
import tomllib
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
VERSION_PATTERN = r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"
PACKAGE_NAMES = ("zar-rs", "zar-rs-v3")
FIXED_FILES = (
    "Cargo.toml",
    "Cargo.lock",
    "pyzarlib/pyproject.toml",
    "jzarlib-ffm/pom.xml",
    "jzarlib-jni/pom.xml",
    "csharpzarlib/ZarLib/ZarLib.csproj",
    "wasizarlib/nodejs/package.json",
    "wasizarlib/nodejs/package-lock.json",
    "packaging/windows/zar-rs.iss",
    "packaging/README.md",
    *(f"packaging/windows/scoop/{name}.json" for name in PACKAGE_NAMES),
    *(
        f"packaging/windows/chocolatey/{name}/{file}"
        for name, nuspec in (("zar-rs", "zarg.nuspec"), ("zar-rs-v3", "zarc.nuspec"))
        for file in (nuspec, "tools/chocolateyInstall.ps1", "deploy-choco.sh")
    ),
)


def manifest_paths(root: Path) -> list[Path]:
    workspace = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    members = workspace["workspace"]["members"]
    manifests = {
        directory / "Cargo.toml"
        for member in members
        for directory in root.glob(member)
        if directory.is_dir()
    }
    return sorted({root / path for path in FIXED_FILES} | manifests)


def replace_field(text: str, pattern: str, value: str, expected: int = 1) -> str:
    updated, count = re.subn(
        pattern, lambda match: match[1] + value + match[2], text, flags=re.MULTILINE
    )
    if count != expected:
        raise ValueError(f"expected {expected} version field(s), found {count}: {pattern}")
    return updated


def toml_version(text: str, section: str, version: str) -> str:
    pattern = (
        rf"(?s)(^\[{re.escape(section)}\]\r?\n"
        rf"(?:(?!^\[).)*?^[ \t]*version[ \t]*=[ \t]*\")[^\"]+(\")"
    )
    return replace_field(text, pattern, version)


def release_urls(text: str, version: str) -> str:
    pattern = (
        r"((?:https://github\.com/brmmm3/zar-rs/releases/(?:download|tag)"
        r"|https://raw\.githubusercontent\.com/brmmm3/zar-rs)/)"
        + VERSION_PATTERN
        + r'([/<"\s])'
    )
    count = len(re.findall(pattern, text))
    if not count:
        raise ValueError("no versioned ZAR release URLs found")
    return replace_field(text, pattern, version, expected=count)


def json_text(text: str, data: dict[str, object]) -> str:
    indent_match = re.search(r'\n([ \t]+)"', text)
    if indent_match is None:
        raise ValueError("cannot determine JSON indentation")
    newline = "\r\n" if "\r\n" in text else "\n"
    updated = json.dumps(data, indent=indent_match[1], ensure_ascii=False)
    if text.endswith("\n"):
        updated += "\n"
    return updated.replace("\n", newline)


def plan_updates(root: Path, version: str) -> dict[Path, str]:
    if re.fullmatch(VERSION_PATTERN, version) is None:
        raise ValueError("version must be a stable MAJOR.MINOR.PATCH without leading zeros")
    original = {path: path.read_bytes().decode("utf-8") for path in manifest_paths(root)}
    updated = original.copy()
    workspace = tomllib.loads(original[root / "Cargo.toml"])
    old_version = workspace["workspace"]["package"]["version"]
    cargo_paths = [path for path in original if path.name == "Cargo.toml"]
    local_names: set[str] = set()
    local_directories = {path.parent.resolve() for path in cargo_paths}
    for path in cargo_paths:
        text = updated[path]
        data = tomllib.loads(text)
        if path == root / "Cargo.toml":
            text = toml_version(text, "workspace.package", version)
        if "package" in data:
            local_names.add(data["package"]["name"])
            if isinstance(data["package"].get("version"), str):
                text = toml_version(text, "package", version)

        def dependency(match: re.Match[str]) -> str:
            line = match[0]
            spec = next(iter(tomllib.loads(line.strip()).values()))
            if (
                "path" in spec
                and "version" in spec
                and (path.parent / spec["path"]).resolve() in local_directories
            ):
                return replace_field(line, r'(\bversion\s*=\s*")[^"]+(")', version)
            return line

        updated[path] = re.sub(
            r"^[ \t]*[\w-]+[ \t]*=[ \t]*\{[^{}\r\n]*\}",
            dependency,
            text,
            flags=re.MULTILINE,
        )

    lock_path = root / "Cargo.lock"
    lock_names: set[str] = set()

    def locked_package(match: re.Match[str]) -> str:
        block = match[0]
        package = tomllib.loads(block)["package"][0]
        if package["name"] in local_names and "source" not in package:
            lock_names.add(package["name"])
            return replace_field(block, r'(^version = ")[^"]+(")', version)
        return block

    updated[lock_path] = re.sub(
        r"^\[\[package\]\].*?(?=^\[\[package\]\]|\Z)",
        locked_package,
        original[lock_path],
        flags=re.MULTILINE | re.DOTALL,
    )
    if lock_names != local_names:
        raise ValueError("Cargo.lock does not contain every workspace package")
    for name in local_names:
        updated[lock_path] = updated[lock_path].replace(
            f'"{name} {old_version}"', f'"{name} {version}"'
        )

    path = root / "pyzarlib/pyproject.toml"
    updated[path] = toml_version(original[path], "project", version)
    for name in ("ffm", "jni"):
        path = root / f"jzarlib-{name}/pom.xml"
        updated[path] = replace_field(
            original[path],
            rf"(<artifactId>jzarlib-{name}</artifactId>\s*<version>)[^<]+(</version>)",
            version,
        )
    path = root / "csharpzarlib/ZarLib/ZarLib.csproj"
    updated[path] = replace_field(original[path], r"(<Version>)[^<]+(</Version>)", version)
    for filename in ("package.json", "package-lock.json"):
        path = root / "wasizarlib/nodejs" / filename
        data = json.loads(original[path])
        data["version"] = version
        if filename == "package-lock.json":
            data["packages"][""]["version"] = version
        updated[path] = json_text(original[path], data)

    for name, nuspec in (("zar-rs", "zarg.nuspec"), ("zar-rs-v3", "zarc.nuspec")):
        path = root / f"packaging/windows/scoop/{name}.json"
        data = json.loads(original[path])
        data["version"] = version
        for architecture in data["architecture"].values():
            architecture["url"] = release_urls(architecture["url"], version)
        updated[path] = json_text(original[path], data)
        package = root / "packaging/windows/chocolatey" / name
        path = package / nuspec
        updated[path] = release_urls(
            replace_field(original[path], r"(<version>)[^<]+(</version>)", version),
            version,
        )
        path = package / "tools/chocolateyInstall.ps1"
        updated[path] = release_urls(original[path], version)
        path = package / "deploy-choco.sh"
        updated[path] = replace_field(
            original[path], rf"({re.escape(name)}\.){VERSION_PATTERN}(\.nupkg)", version
        )

    path = root / "packaging/windows/zar-rs.iss"
    updated[path] = replace_field(original[path], r'(#define AppVersion ")[^"]+(")', version)
    updated[path] = replace_field(
        updated[path], rf"(/DAppVersion=){VERSION_PATTERN}( )", version
    )
    path = root / "packaging/README.md"
    updated[path] = replace_field(
        original[path], rf"(choco install zar-rs --version ){VERSION_PATTERN}( )", version
    )
    for path, text in updated.items():
        if path.suffix == ".toml" or path.name == "Cargo.lock":
            tomllib.loads(text)
        elif path.suffix in (".xml", ".csproj", ".nuspec"):
            ET.fromstring(text)
    return {path: text for path, text in updated.items() if text != original[path]}


def main(argv: list[str] | None = None, *, root: Path = ROOT) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version", help="stable release version, for example 0.15.0")
    parser.add_argument("--dry-run", action="store_true", help="preview without writing files")
    args = parser.parse_args(argv)
    try:
        updates = plan_updates(root, args.version)
        for path, text in updates.items():
            if not args.dry_run:
                path.write_bytes(text.encode("utf-8"))
            print(f"{'Would update' if args.dry_run else 'Updated'} {path.relative_to(root)}")
        if not updates:
            print(f"All package versions already match {args.version}.")
        print(
            "Warning: download checksums are unchanged. Replace Scoop and Chocolatey "
            "checksums with hashes of the new release artifacts before publishing.",
            file=sys.stderr,
        )
    except (OSError, ValueError, KeyError, ET.ParseError) as error:
        print(f"version update failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
