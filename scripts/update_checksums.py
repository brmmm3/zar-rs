#!/usr/bin/env python3
"""Update Scoop and Chocolatey checksums from a published GitHub release."""

from __future__ import annotations

import argparse
from html.parser import HTMLParser
import json
from pathlib import Path
import re
import sys
from urllib.request import Request, urlopen

from update_version import ROOT, VERSION_PATTERN, json_text

ASSETS = {
    "x86_64": "zar-rs-windows-x86_64.zip",
    "aarch64": "zar-rs-windows-aarch64.zip",
    "x86_64-v3": "zar-rs-windows-x86_64-v3.zip",
}
RELEASE = "https://github.com/brmmm3/zar-rs/releases"


class ReleaseAssets(HTMLParser):
    def __init__(self, version: str) -> None:
        super().__init__()
        self.version = version
        self.links: set[str] = set()
        self.digests: dict[str, str] = {}

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        attributes = dict(attrs)
        if tag == "a":
            self.links.add(attributes.get("href") or "")
        elif tag == "clipboard-copy":
            label = attributes.get("aria-label") or ""
            prefix = "Copy to clipboard digest for "
            if label.startswith(prefix):
                name = label.removeprefix(prefix)
                value = attributes.get("value") or ""
                if name in ASSETS.values():
                    if name in self.digests or re.fullmatch(r"sha256:[0-9a-fA-F]{64}", value) is None:
                        raise ValueError(f"invalid or duplicate SHA-256 digest for {name}")
                    self.digests[name] = value[7:].lower()

    def checksums(self) -> dict[str, str]:
        for name in ASSETS.values():
            link = f"/brmmm3/zar-rs/releases/download/{self.version}/{name}"
            if link not in self.links or name not in self.digests:
                raise ValueError(f"release {self.version} is missing the asset or SHA-256 digest for {name}")
        return {architecture: self.digests[name] for architecture, name in ASSETS.items()}


def fetch_checksums(version: str) -> dict[str, str]:
    request = Request(
        f"{RELEASE}/expanded_assets/{version}",
        headers={"User-Agent": "zar-rs-checksum-updater"},
    )
    with urlopen(request, timeout=30) as response:
        page = response.read().decode("utf-8")
    assets = ReleaseAssets(version)
    assets.feed(page)
    return assets.checksums()


def plan_updates(root: Path, version: str, checksums: dict[str, str]) -> dict[Path, str]:
    if re.fullmatch(VERSION_PATTERN, version) is None:
        raise ValueError("version must be a stable MAJOR.MINOR.PATCH without leading zeros")
    if set(checksums) != set(ASSETS) or any(
        re.fullmatch(r"[0-9a-f]{64}", digest) is None for digest in checksums.values()
    ):
        raise ValueError("expected a SHA-256 digest for each Windows release asset")

    updated: dict[Path, str] = {}
    for package, architectures in (
        ("zar-rs", {"64bit": "x86_64", "arm64": "aarch64"}),
        ("zar-rs-v3", {"64bit": "x86_64-v3"}),
    ):
        scoop = root / f"packaging/windows/scoop/{package}.json"
        original = scoop.read_bytes().decode("utf-8")
        data = json.loads(original)
        if data["version"] != version or set(data["architecture"]) != set(architectures):
            raise ValueError(f"{scoop}: version or architectures do not match {version}")
        for architecture, asset in architectures.items():
            entry = data["architecture"][architecture]
            if entry["url"] != f"{RELEASE}/download/{version}/{ASSETS[asset]}":
                raise ValueError(f"{scoop}: unexpected release URL for {architecture}")
            entry["hash"] = checksums[asset]
        updated[scoop] = json_text(original, data)

        installer = root / f"packaging/windows/chocolatey/{package}/tools/chocolateyInstall.ps1"
        original = installer.read_bytes().decode("utf-8")
        suffix = "-v3" if package.endswith("-v3") else ""
        if f'{RELEASE}/download/{version}/zar-rs-windows-$arch{suffix}.zip' not in original:
            raise ValueError(f"{installer}: unexpected release URL")
        text = original
        for architecture, asset in architectures.items():
            name = "AMD64" if architecture == "64bit" else "ARM64"
            arch = "x86_64" if architecture == "64bit" else "aarch64"
            pattern = (
                rf"('{name}'\s*\{{\s*\$arch\s*=\s*'{arch}'\s*\$checksum\s*=\s*')"
                r"[0-9a-fA-F]{64}(')"
            )
            text, count = re.subn(pattern, lambda match: match[1] + checksums[asset] + match[2], text)
            if count != 1:
                raise ValueError(f"{installer}: expected one checksum for {name}, found {count}")
        updated[installer] = text

    return {path: text for path, text in updated.items() if text.encode("utf-8") != path.read_bytes()}


def main(argv: list[str] | None = None, *, root: Path = ROOT) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version", nargs="?", help="release version (defaults to the Scoop version)")
    parser.add_argument("--dry-run", action="store_true", help="preview without writing files")
    args = parser.parse_args(argv)
    try:
        version = args.version or json.loads(
            (root / "packaging/windows/scoop/zar-rs.json").read_bytes()
        )["version"]
        if re.fullmatch(VERSION_PATTERN, version) is None:
            raise ValueError("version must be a stable MAJOR.MINOR.PATCH without leading zeros")
        updates = plan_updates(root, version, fetch_checksums(version))
        for path, text in updates.items():
            if not args.dry_run:
                path.write_bytes(text.encode("utf-8"))
            print(f"{'Would update' if args.dry_run else 'Updated'} {path.relative_to(root)}")
        if not updates:
            print(f"All packaging checksums already match release {version}.")
    except (OSError, ValueError, KeyError) as error:
        print(f"checksum update failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())