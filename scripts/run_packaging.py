#!/usr/bin/env python3
"""Build packaging outputs for the current operating system and CPU architecture."""

from __future__ import annotations

import os
import platform
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LINUX_FORMATS = ("deb", "rpm", "appimage", "flatpak")
WINDOWS_TARGETS = {
    "amd64": ("x64", "x86_64-pc-windows-msvc"),
    "x64": ("x64", "x86_64-pc-windows-msvc"),
    "x86_64": ("x64", "x86_64-pc-windows-msvc"),
    "arm64": ("arm64", "aarch64-pc-windows-msvc"),
    "aarch64": ("arm64", "aarch64-pc-windows-msvc"),
}
FLATPAK_RUNTIME = (
    "org.freedesktop.Platform//26.08",
    "org.freedesktop.Sdk//26.08",
    "org.freedesktop.Sdk.Extension.rust-stable//26.08",
)
APPIMAGETOOL_URL = (
    "https://github.com/AppImage/appimagetool/releases/download/continuous/"
    "appimagetool-x86_64.AppImage"
)


class PackagingFailure(Exception):
    """A required packaging tool or host capability is unavailable."""


def command(args: list[str], env: dict[str, str] | None = None) -> None:
    print("+", subprocess.list2cmdline(args), flush=True)
    subprocess.run(args, cwd=ROOT, env=env, check=True)


def require(name: str) -> str:
    path = shutil.which(name)
    if path is None:
        raise PackagingFailure(f"required tool not found: {name}")
    return path


def linux_elevation() -> list[str]:
    if hasattr(os, "geteuid") and os.geteuid() == 0:
        return []
    return [require("sudo")]


def install_linux_tools() -> dict[str, str]:
    apt_get = require("apt-get")
    elevate = linux_elevation()
    command([*elevate, apt_get, "update"])
    command(
        [
            *elevate,
            apt_get,
            "install",
            "-y",
            "libxcb1-dev",
            "rpm",
            "curl",
            "flatpak",
            "flatpak-builder",
        ]
    )
    try:
        command([*elevate, apt_get, "install", "-y", "libfuse2t64"])
    except subprocess.CalledProcessError:
        command([*elevate, apt_get, "install", "-y", "libfuse2"])

    flatpak = require("flatpak")
    command(
        [
            flatpak,
            "remote-add",
            "--user",
            "--if-not-exists",
            "flathub",
            "https://flathub.org/repo/flathub.flatpakrepo",
        ]
    )
    command([flatpak, "install", "--user", "-y", "flathub", *FLATPAK_RUNTIME])

    appimagetool = Path(tempfile.gettempdir()) / "appimagetool"
    command([require("curl"), "-fsSL", "-o", str(appimagetool), APPIMAGETOOL_URL])
    appimagetool.chmod(0o755)
    env = os.environ.copy()
    env["APPIMAGETOOL"] = str(appimagetool)
    env["APPIMAGE_EXTRACT_AND_RUN"] = "1"
    return env


def build_linux_packages() -> None:
    env = install_linux_tools()
    script = ROOT / "packaging" / "linux" / "build-packages.sh"
    command(["bash", str(script), "all"], env)


def workspace_version() -> str:
    with (ROOT / "Cargo.toml").open("rb") as manifest:
        return tomllib.load(manifest)["workspace"]["package"]["version"]


def windows_target_for_machine(machine: str) -> tuple[str, str]:
    target = WINDOWS_TARGETS.get(machine.lower())
    if target is None:
        raise PackagingFailure(f"unsupported Windows CPU architecture: {machine}")
    return target


def inno_setup_compiler() -> str:
    compiler = shutil.which("ISCC.exe") or shutil.which("ISCC")
    if compiler:
        return compiler
    candidates = (
        Path(os.environ.get("ProgramFiles(x86)", "")) / "Inno Setup 6" / "ISCC.exe",
        Path(os.environ.get("ProgramFiles", "")) / "Inno Setup 6" / "ISCC.exe",
    )
    for candidate in candidates:
        if candidate.is_file():
            return str(candidate)

    choco = shutil.which("choco")
    if choco is None:
        raise PackagingFailure(
            "Inno Setup 6 is required; install it or install Chocolatey to let this script install it"
        )
    command([choco, "install", "innosetup", "--no-progress", "-y"])
    for candidate in candidates:
        if candidate.is_file():
            return str(candidate)
    compiler = shutil.which("ISCC.exe") or shutil.which("ISCC")
    if compiler is None:
        raise PackagingFailure("Inno Setup installation completed but ISCC.exe was not found")
    return compiler


def build_windows_installers() -> None:
    rustup = require("rustup")
    cargo = require("cargo")
    command([rustup, "toolchain", "install", "stable", "--profile", "minimal"])
    iscc = inno_setup_compiler()
    version = workspace_version()
    installer = ROOT / "packaging" / "windows" / "zar-rs.iss"

    architecture, target = windows_target_for_machine(platform.machine())
    command([rustup, "target", "add", target, "--toolchain", "stable"])
    command(
        [
            cargo,
            "+stable",
            "build",
            "--release",
            "--bins",
            "--locked",
            "--target",
            target,
        ]
    )
    binary_directory = ROOT / "target" / target / "release"
    command(
        [
            iscc,
            f"/DAppVersion={version}",
            f"/DArch={architecture}",
            f"/DSourceDir={binary_directory}",
            str(installer),
        ]
    )


def build_macos_packages() -> None:
    require("cargo")
    script = ROOT / "packaging" / "macos" / "build-packages.sh"
    command(["bash", str(script), "all"])


def build_for_system(system: str) -> None:
    if system == "Linux":
        build_linux_packages()
    elif system == "Windows":
        build_windows_installers()
    elif system == "Darwin":
        build_macos_packages()
    else:
        raise PackagingFailure(f"packaging is not supported on {system}")


def main() -> int:
    try:
        build_for_system(platform.system())
    except (PackagingFailure, subprocess.CalledProcessError) as error:
        print(f"packaging failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())