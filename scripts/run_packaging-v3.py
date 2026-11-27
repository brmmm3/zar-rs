#!/usr/bin/env python3
"""Build x86-64-v3 packages locally without overwriting baseline packages."""

import platform
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import run_packaging


def build() -> None:
    system = platform.system()
    machine = platform.machine().lower()
    if system not in ("Linux", "Windows", "Darwin") or machine not in (
        "x86_64", "amd64", "x64"
    ):
        raise run_packaging.PackagingFailure(
            f"x86-64-v3 packaging is not supported on {system} {machine}"
        )

    builders = {
        "Linux": (run_packaging.build_linux_packages, ("deb", "rpm", "AppImage", "flatpak")),
        "Windows": (run_packaging.build_windows_installers, ("exe",)),
        "Darwin": (run_packaging.build_macos_packages, ("dmg", "pkg")),
    }
    builder, extensions = builders[system]
    with tempfile.TemporaryDirectory(prefix="zar-packaging-v3-") as temp:
        stage = Path(temp)
        builder(v3=True, dist=stage)
        packages = []
        for extension in extensions:
            matches = list(stage.glob(f"*.{extension}"))
            if len(matches) != 1:
                raise run_packaging.PackagingFailure(
                    f"expected one .{extension} package, found {len(matches)}"
                )
            packages.append(matches[0])

        destination = run_packaging.ROOT / "dist"
        destination.mkdir(parents=True, exist_ok=True)
        targets = [destination / f"{package.stem}-v3{package.suffix}" for package in packages]
        for target in targets:
            if target.exists():
                raise run_packaging.PackagingFailure(f"output already exists: {target}")
        for package, target in zip(packages, targets):
            with package.open("rb") as source, target.open("xb") as output:
                shutil.copyfileobj(source, output)
            shutil.copystat(package, target)
            print(f"Created {target}")


def main() -> int:
    try:
        build()
    except (run_packaging.PackagingFailure, subprocess.CalledProcessError, OSError) as error:
        print(f"v3 packaging failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
