#!/usr/bin/env python3
"""Run the locally applicable ci.yml jobs sequentially; write artifacts to dist."""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DIST = ROOT / "dist"
AUTO_INSTALL_ENABLED = True
PYTHON_VERSIONS = ("3.11", "3.12", "3.13", "3.14", "3.15", "3.14t", "3.15t")
WHEEL_VERSIONS = ("3.9", "3.10", "3.11", "3.12", "3.13", "3.14", "3.15")
PLATFORMS = (
    ("linux-x86_64", "Linux", "x86_64", "x86_64-unknown-linux-gnu", False),
    ("linux-aarch64", "Linux", "aarch64", "aarch64-unknown-linux-gnu", False),
    ("linux-x86_64-static", "Linux", "x86_64", "x86_64-unknown-linux-musl", True),
    ("linux-aarch64-static", "Linux", "aarch64", "aarch64-unknown-linux-musl", True),
    ("macos-aarch64", "Darwin", "aarch64", "aarch64-apple-darwin", False),
    ("macos-x86_64", "Darwin", "x86_64", "x86_64-apple-darwin", False),
    ("windows-x86_64", "Windows", "x86_64", "x86_64-pc-windows-msvc", False),
    ("windows-aarch64", "Windows", "aarch64", "aarch64-pc-windows-msvc", False),
)


class JobFailure(Exception):
    pass


def command(
    args: list[str], cwd: Path = ROOT, env: dict[str, str] | None = None
) -> None:
    print("+", subprocess.list2cmdline(args), flush=True)
    subprocess.run(args, cwd=cwd, env=env, check=True)


def require(name: str) -> str:
    path = shutil.which(name)
    if path is None and AUTO_INSTALL_ENABLED:
        install_missing_tool(name)
        path = shutil.which(name)
    if path is None:
        raise JobFailure(f"required tool not found: {name}")
    return path


def install_linux_packages(packages: list[str]) -> None:
    managers = {
        "apt-get": ["apt-get", "install", "-y"],
        "dnf": ["dnf", "install", "-y"],
        "yum": ["yum", "install", "-y"],
        "pacman": ["pacman", "-S", "--noconfirm", "--needed"],
        "zypper": ["zypper", "--non-interactive", "install"],
        "apk": ["apk", "add"],
    }
    if not AUTO_INSTALL_ENABLED:
        raise JobFailure("automatic installation is disabled")
    manager = next((name for name in managers if shutil.which(name)), None)
    if manager is None:
        raise JobFailure(
            "cannot install missing tools: no supported Linux package manager found"
        )

    elevate: list[str] = []
    if hasattr(os, "geteuid") and os.geteuid() != 0:
        sudo = shutil.which("sudo")
        if sudo is None:
            raise JobFailure("installing Linux tools requires root or sudo")
        elevate = [sudo]

    env = os.environ.copy()
    if manager == "apt-get":
        env["DEBIAN_FRONTEND"] = "noninteractive"
        subprocess.run([*elevate, "apt-get", "update"], env=env, check=False)
    print(f"Installing Linux tools with {manager}: {', '.join(packages)}")
    command([*elevate, *managers[manager], *packages], env=env)


def refresh_windows_path() -> None:
    if platform.system() != "Windows":
        return
    try:
        import winreg
    except ImportError:
        return

    entries = []
    for hive, key_path in (
        (winreg.HKEY_CURRENT_USER, "Environment"),
        (winreg.HKEY_LOCAL_MACHINE, r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment"),
    ):
        try:
            with winreg.OpenKey(hive, key_path) as key:
                value = winreg.QueryValueEx(key, "Path")[0]
        except OSError:
            continue
        entries.append(winreg.ExpandEnvironmentStrings(value))

    entries.append(os.environ.get("PATH", ""))
    os.environ["PATH"] = os.pathsep.join(dict.fromkeys(entries))


_LINUX_TOOL_PACKAGES = {
    "apt-get": {
        "cc": ["build-essential"],
        "c++": ["g++"],
        "g++": ["g++"],
        "go": ["golang-go"],
        "clang": ["clang"],
        "clang-cl": ["clang"],
        "lld-link": ["lld"],
        "mvn": ["maven"],
        "javac": ["openjdk-25-jdk"],
        "musl-gcc": ["musl-tools"],
    },
    "dnf": {
        "cc": ["gcc"],
        "c++": ["gcc-c++"],
        "g++": ["gcc-c++"],
        "go": ["golang"],
        "clang": ["clang"],
        "clang-cl": ["clang"],
        "lld-link": ["lld"],
        "mvn": ["maven"],
        "javac": ["java-25-openjdk-devel"],
        "musl-gcc": ["musl-gcc"],
    },
    "yum": {
        "cc": ["gcc"],
        "c++": ["gcc-c++"],
        "g++": ["gcc-c++"],
        "go": ["golang"],
        "clang": ["clang"],
        "clang-cl": ["clang"],
        "lld-link": ["lld"],
        "mvn": ["maven"],
        "javac": ["java-25-openjdk-devel"],
        "musl-gcc": ["musl-gcc"],
    },
    "pacman": {
        "cc": ["base-devel"],
        "c++": ["gcc"],
        "g++": ["gcc"],
        "go": ["go"],
        "clang": ["clang"],
        "clang-cl": ["clang"],
        "lld-link": ["lld"],
        "mvn": ["maven"],
        "javac": ["jdk-openjdk"],
        "musl-gcc": ["musl"],
    },
    "zypper": {
        "cc": ["gcc"],
        "c++": ["gcc-c++"],
        "g++": ["gcc-c++"],
        "go": ["go"],
        "clang": ["clang"],
        "clang-cl": ["clang"],
        "lld-link": ["lld"],
        "mvn": ["maven"],
        "javac": ["java-25-openjdk-devel"],
        "musl-gcc": ["musl-gcc"],
    },
    "apk": {
        "cc": ["build-base"],
        "c++": ["g++"],
        "g++": ["g++"],
        "go": ["go"],
        "clang": ["clang"],
        "clang-cl": ["clang"],
        "lld-link": ["lld"],
        "mvn": ["maven"],
        "javac": ["openjdk25-jdk"],
        "musl-gcc": ["musl-dev"],
    },
}

_PYENV_BUILD_PACKAGES = {
    "apt-get": [
        "build-essential",
        "libssl-dev",
        "zlib1g-dev",
        "libbz2-dev",
        "libreadline-dev",
        "libsqlite3-dev",
        "curl",
        "llvm",
        "libncursesw5-dev",
        "xz-utils",
        "tk-dev",
        "libffi-dev",
        "liblzma-dev",
    ],
    "dnf": [
        "gcc",
        "make",
        "patch",
        "zlib-devel",
        "bzip2-devel",
        "readline-devel",
        "sqlite-devel",
        "openssl-devel",
        "tk-devel",
        "libffi-devel",
        "xz-devel",
    ],
    "yum": [
        "gcc",
        "make",
        "patch",
        "zlib-devel",
        "bzip2-devel",
        "readline-devel",
        "sqlite-devel",
        "openssl-devel",
        "tk-devel",
        "libffi-devel",
        "xz-devel",
    ],
    "pacman": ["base-devel", "openssl", "zlib", "xz", "tk", "readline", "sqlite", "libffi"],
    "zypper": [
        "gcc",
        "make",
        "patch",
        "zlib-devel",
        "libopenssl-devel",
        "readline-devel",
        "sqlite3-devel",
        "xz-devel",
        "tk-devel",
        "libffi-devel",
    ],
    "apk": [
        "build-base",
        "zlib-dev",
        "bzip2-dev",
        "readline-dev",
        "sqlite-dev",
        "openssl-dev",
        "tk-dev",
        "libffi-dev",
        "xz-dev",
    ],
}


def install_missing_tool(name: str) -> None:
    if name == "maturin":
        cargo = shutil.which("cargo")
        if cargo:
            command([cargo, "install", "--locked", "maturin"])
        return
    if name in ("zizmor", "rumdl"):
        cargo = require("cargo")
        command([cargo, "install", "--locked", name])
        return
    if name == "ruff":
        command([sys.executable, "-m", "pip", "install", "--user", "ruff"])
        scripts = Path(sys.executable).parent / ("Scripts" if os.name == "nt" else "bin")
        user_base = subprocess.run(
            [sys.executable, "-m", "site", "--user-base"],
            text=True,
            capture_output=True,
            check=True,
        )
        user_scripts = Path(user_base.stdout.strip()) / (
            "Scripts" if os.name == "nt" else "bin"
        )
        for directory in (scripts, user_scripts):
            if directory.is_dir():
                os.environ["PATH"] = str(directory) + os.pathsep + os.environ["PATH"]
        return
    if platform.system() == "Windows":
        choco = shutil.which("choco")
        winget = shutil.which("winget")
        packages = {
            "dotnet": ("Microsoft.DotNet.SDK.10", "dotnet-sdk"),
            "go": ("GoLang.Go", "golang"),
            "mvn": ("Apache.Maven", "maven"),
            "javac": ("EclipseAdoptium.Temurin.25.JDK", "temurin"),
            "cc": ("LLVM.LLVM", "llvm"),
            "c++": ("LLVM.LLVM", "llvm"),
            "g++": ("LLVM.LLVM", "llvm"),
            "clang": ("LLVM.LLVM", "llvm"),
        }
        package = packages.get(name)
        if name.startswith("python-"):
            version = name.removeprefix("python-")
            package = (f"Python.Python.{version}", f"python{version.replace('.', '')}")
        if package and choco:
            command([choco, "install", package[1], "--yes", "--no-progress"])
        elif package and winget:
            command(
                [
                    winget,
                    "install",
                    "--id",
                    package[0],
                    "--exact",
                    "--silent",
                    "--accept-source-agreements",
                    "--accept-package-agreements",
                ]
            )
        else:
            return
        refresh_windows_path()
        llvm_bin = Path(os.environ.get("ProgramFiles", "C:\\Program Files")) / "LLVM/bin"
        if name in ("cc", "c++", "g++", "clang") and llvm_bin.is_dir():
            os.environ["PATH"] = str(llvm_bin) + os.pathsep + os.environ["PATH"]
        return
    if platform.system() != "Linux":
        return
    manager = next((name for name in _LINUX_TOOL_PACKAGES if shutil.which(name)), None)
    packages = _LINUX_TOOL_PACKAGES.get(manager, {}).get(name, [])
    if packages:
        install_linux_packages(packages)


def ensure_rustup_item(kind: str, item: str) -> None:
    rustup = require("rustup")
    result = subprocess.run(
        [rustup, kind, "list", "--installed"],
        text=True,
        capture_output=True,
        check=True,
    )
    installed = result.stdout.splitlines()
    present = (
        item in installed
        if kind == "target"
        else any(line.startswith(f"{item}-") for line in installed)
    )
    if present:
        return
    if not AUTO_INSTALL_ENABLED:
        raise JobFailure(f"Rust {kind} missing: {item}; rerun without --no-install")
    command([rustup, kind, "add", item])


def ensure_cargo_component(component: str) -> None:
    result = subprocess.run(
        ["cargo", component, "--version"],
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode == 0:
        return
    rustup_component = {"fmt": "rustfmt"}.get(component, component)
    ensure_rustup_item("component", rustup_component)


def ensure_windows_cross_toolchain(target: str) -> None:
    ensure_rustup_item("target", target)
    ensure_rustup_item("component", "llvm-tools")

    if any(not shutil.which(name) for name in ("clang", "clang-cl", "lld-link")):
        install_linux_packages(["clang", "lld", "llvm"])
    for name in ("clang", "clang-cl", "lld-link"):
        require(name)

    if shutil.which("cargo-xwin") is None:
        if not AUTO_INSTALL_ENABLED:
            raise JobFailure("cargo-xwin missing; rerun without --no-install")
        command(["cargo", "install", "--locked", "cargo-xwin"])


def cargo_fmt() -> None:
    ensure_cargo_component("fmt")
    check = ["cargo", "fmt", "--all", "--check"]
    try:
        command(check)
    except subprocess.CalledProcessError:
        print("Formatting check failed; applying cargo fmt and checking again.", flush=True)
        command(["cargo", "fmt", "--all"])
        command(check)


def cargo_clippy() -> None:
    ensure_cargo_component("clippy")
    check = ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]
    try:
        command(check)
    except subprocess.CalledProcessError:
        print("Clippy check failed; applying suggested fixes and checking again.", flush=True)
        command(
            [
                "cargo",
                "clippy",
                "--workspace",
                "--all-targets",
                "--fix",
                "--allow-dirty",
                "--allow-staged",
                "--",
                "-D",
                "warnings",
            ]
        )
        command(check)


def copy_required(source: Path, destination: Path) -> None:
    if not source.is_file():
        raise JobFailure(f"expected artifact not found: {source}")
    destination.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination / source.name)


def one_file(directory: Path, pattern: str) -> Path:
    files = sorted(directory.glob(pattern))
    if len(files) != 1:
        raise JobFailure(f"expected one {pattern} in {directory}, found {len(files)}")
    return files[0]


def archive_directory(directory: Path, output: Path, windows: bool = False) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    if windows:
        with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as archive:
            for path in sorted(directory.rglob("*")):
                if path.is_file():
                    archive.write(path, path.relative_to(directory.parent))
    else:
        with tarfile.open(output, "w:gz") as archive:
            archive.add(directory, arcname=directory.name)


def host_arch() -> str:
    machine = platform.machine().lower()
    return {"amd64": "x86_64", "arm64": "aarch64"}.get(machine, machine)


def python_identity(executable: str) -> str:
    result = subprocess.run(
        [
            executable,
            "-c",
            "import sys,sysconfig; print(f'{sys.version_info.major}.{sys.version_info.minor}' + ('t' if sysconfig.get_config_var('Py_GIL_DISABLED') else ''))",
        ],
        text=True,
        capture_output=True,
        check=True,
    )
    return result.stdout.strip()


def pyenv_python(version: str) -> str:
    pyenv = require("pyenv")
    known = subprocess.run(
        [pyenv, "latest", "--known", version],
        text=True,
        capture_output=True,
        check=True,
    ).stdout.strip()
    if not known:
        raise JobFailure(f"pyenv has no known build for Python {version}")

    prefix = subprocess.run(
        [pyenv, "prefix", known], text=True, capture_output=True, check=False
    )
    if prefix.returncode != 0:
        if not AUTO_INSTALL_ENABLED:
            raise JobFailure(f"Python {version} is not installed by pyenv")
        manager = next(
            (name for name in _PYENV_BUILD_PACKAGES if shutil.which(name)), None
        )
        if manager:
            install_linux_packages(_PYENV_BUILD_PACKAGES[manager])
        command([pyenv, "install", "--skip-existing", known])
        prefix = subprocess.run(
            [pyenv, "prefix", known], text=True, capture_output=True, check=True
        )

    install_prefix = Path(prefix.stdout.strip())
    executable = (
        install_prefix / "python.exe"
        if os.name == "nt"
        else install_prefix / "bin" / "python"
    )
    return str(executable)


def windows_python(version: str) -> str:
    def find_with_launcher() -> str | None:
        launcher = shutil.which("py")
        if launcher is None:
            return None
        result = subprocess.run(
            [launcher, f"-{version}", "-c", "import sys; print(sys.executable)"],
            text=True,
            capture_output=True,
            check=False,
        )
        executable = result.stdout.strip()
        return executable if result.returncode == 0 and executable else None

    executable = find_with_launcher()
    if executable:
        return executable
    if not AUTO_INSTALL_ENABLED:
        raise JobFailure(f"Python {version} is not installed; rerun without --no-install")
    install_missing_tool(f"python-{version}")
    refresh_windows_path()
    executable = find_with_launcher()
    if executable is None:
        raise JobFailure(f"Python {version} remains unavailable after installation")
    return executable


def find_python(version: str, overrides: dict[str, str]) -> str:
    candidate = overrides.get(version)
    if candidate is None:
        candidate = (
            windows_python(version)
            if platform.system() == "Windows"
            else pyenv_python(version)
        )
    try:
        if python_identity(candidate) != version:
            raise JobFailure(f"Python at {candidate} does not match {version}")
    except (OSError, subprocess.CalledProcessError) as error:
        raise JobFailure(f"Python {version} is unavailable at {candidate}: {error}") from error
    return candidate


def c_tests(system: str) -> None:
    windows = system == "Windows"
    command(["cargo", "build", "--locked", "--release", "-p", "czarlib"])
    if windows:
        print("SKIP C API integration tests: harness requires POSIX APIs", flush=True)
        return
    compiler = require("cc")
    command(["cargo", "test", "--locked", "-p", "zarlib"])
    with tempfile.TemporaryDirectory(prefix="zar-ci-c-") as temp:
        tmp = Path(temp)
        library = ROOT / "target/release"

        def compile_args(source: str, output: Path, pthread: bool = False) -> list[str]:
            return [
                compiler,
                "-std=c11",
                "-O2",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-I",
                "czarlib/include",
                source,
                "-L",
                str(library),
                "-lczarlib",
                f"-Wl,-rpath,{library}",
                *(["-pthread"] if pthread else []),
                "-o",
                str(output),
            ]

        api = tmp / "czarlib-test"
        example = tmp / "zarc-c"
        command(compile_args("czarlib/tests/test_c_api.c", api, pthread=True))
        command([str(api)])
        command(compile_args("czarlib/examples/zarc.c", example))
        command([str(example), "--help"])
        (tmp / "input.txt").write_text("initial", encoding="utf-8")
        (tmp / "added.txt").write_text("added", encoding="utf-8")
        for args in (
            ["c", "sample.zar", "input.txt"],
            ["a", "sample.zar", "added.txt"],
            ["k", "sample.zar"],
        ):
            command([str(example)] + args, cwd=tmp)
        info = subprocess.run(
            [str(example), "i", "sample.zar"],
            cwd=tmp,
            text=True,
            capture_output=True,
            check=True,
        )
        print(info.stdout, end="")
        commits = json.loads(info.stdout)
        if len(commits) < 2 or not commits[0].get("id") or not commits[1].get("id"):
            raise JobFailure("C example did not return two commit IDs")
        for args in (
            ["l", "sample.zar"],
            ["e", "sample.zar", "restored", "--dry-run"],
            ["r", "sample.zar", "added.txt"],
            ["j", "sample.zar", "--from", commits[0]["id"], "--to", commits[1]["id"]],
        ):
            command([str(example)] + args, cwd=tmp)


def cxx_api_tests() -> None:
    compiler = require("c++")
    bridge_headers = sorted(
        (ROOT / "target/release/build").glob("cxxzarlib-*/out/cxxbridge/include")
    )
    cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    cxx_headers = sorted(
        (cargo_home / "registry/src").glob("*/cxx-*/include")
    )
    if len(bridge_headers) != 1 or not cxx_headers:
        raise JobFailure("CXX bridge or crate headers were not generated")
    with tempfile.TemporaryDirectory(prefix="zar-ci-cxx-") as temp:
        executable = Path(temp) / "cxxzarlib-test"
        command(
            [
                compiler,
                "-std=c++17",
                "-O2",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-I",
                str(bridge_headers[0]),
                "-I",
                str(cxx_headers[0]),
                "cxxzarlib/tests/test_cpp_api.cpp",
                "target/release/libcxxzarlib.a",
                "-ldl",
                "-lpthread",
                "-lm",
                "-o",
                str(executable),
            ]
        )
        command([str(executable)])


def go_tests(system: str) -> None:
    require("go")
    require("cc")
    command(["cargo", "build", "--locked", "--release", "-p", "czarlib"])
    env = os.environ.copy()
    library_path = str(ROOT / "target/release")
    variable = "DYLD_LIBRARY_PATH" if system == "Darwin" else "LD_LIBRARY_PATH"
    env[variable] = library_path + os.pathsep + env.get(variable, "")
    command(["go", "test", "./..."], cwd=ROOT / "gozarlib", env=env)


def csharp_tests() -> None:
    require("dotnet")
    command(["cargo", "build", "--locked", "--release", "-p", "czarlib"])
    project = "csharpzarlib/ZarLib.Tests/ZarLib.Tests.csproj"
    native_library = {
        "Windows": "czarlib.dll",
        "Darwin": "libczarlib.dylib",
        "Linux": "libczarlib.so",
    }[platform.system()]
    env = os.environ.copy()
    env["ZAR_LIBRARY"] = str(ROOT / "target/release" / native_library)
    command(
        [
            "dotnet",
            "restore",
            project,
            "--source",
            "https://api.nuget.org/v3/index.json",
        ]
    )
    command(
        [
            "dotnet",
            "test",
            project,
            "--configuration",
            "Release",
            "--no-restore",
        ],
        env=env,
    )


def ensure_java25() -> None:
    javac = require("javac")

    def configure_java(executable: str) -> None:
        java_bin = Path(executable).resolve().parent
        os.environ["JAVA_HOME"] = str(java_bin.parent)
        os.environ["PATH"] = str(java_bin) + os.pathsep + os.environ.get("PATH", "")

    def major_version(executable: str) -> int | None:
        result = subprocess.run(
            [executable, "-version"],
            text=True,
            capture_output=True,
            check=False,
        )
        output = result.stdout + result.stderr
        version = next(
            (token for token in output.split() if token[:1].isdigit()), None
        )
        if result.returncode != 0 or version is None:
            return None
        try:
            return int(version.split(".", maxsplit=1)[0])
        except ValueError:
            return None

    version = major_version(javac)
    if version is not None and version >= 25:
        configure_java(javac)
        return
    if not AUTO_INSTALL_ENABLED:
        raise JobFailure("Java 25 or newer is required; rerun without --no-install")

    install_missing_tool("javac")
    refresh_windows_path()
    javac = shutil.which("javac")
    version = major_version(javac) if javac else None
    if version is None or version < 25:
        raise JobFailure("Java 25 or newer is required for the Java binding builds")
    configure_java(javac)


def windows_msvc_environment() -> tuple[str, dict[str, str]]:
    program_files = os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)")
    vswhere = Path(program_files) / "Microsoft Visual Studio/Installer/vswhere.exe"
    if not vswhere.is_file():
        raise JobFailure("Visual Studio Build Tools with the C++ workload are required")

    installation = subprocess.run(
        [
            str(vswhere),
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationPath",
        ],
        text=True,
        capture_output=True,
        check=True,
    ).stdout.strip()
    if not installation:
        raise JobFailure("Visual Studio C++ Build Tools workload is not installed")

    vcvars = Path(installation) / "VC/Auxiliary/Build/vcvarsall.bat"
    with tempfile.TemporaryDirectory(prefix="zar-ci-vcvars-") as temp:
        activation = Path(temp) / "activate.cmd"
        activation.write_text(
            f'@echo off\r\ncall "{vcvars}" x64\r\n'
            "if errorlevel 1 exit /b %errorlevel%\r\nset\r\n",
            encoding="utf-8",
        )
        result = subprocess.run(
            ["cmd.exe", "/d", "/c", activation.name],
            cwd=temp,
            text=True,
            capture_output=True,
            check=True,
        )
    env = os.environ.copy()
    for line in result.stdout.splitlines():
        key, separator, value = line.partition("=")
        if not separator or not key:
            continue
        for existing in tuple(env):
            if existing.casefold() == key.casefold():
                del env[existing]
        env[key] = value

    compiler = shutil.which("cl", path=env.get("Path", env.get("PATH", "")))
    if compiler is None:
        raise JobFailure("cl.exe was not found in the activated Visual Studio environment")
    return compiler, env


def zizmor_check() -> None:
    require("zizmor")
    command(["zizmor", "--offline", "--min-severity=low", "."])


def ruff_check() -> None:
    require("ruff")
    command(["ruff", "check", "."])


def rumdl_check() -> None:
    require("rumdl")
    command(["rumdl", "check", "."])


def wasi() -> None:
    require("cargo")
    require("npm")
    require("clang")
    if not Path("/usr/include/wasm32-wasi").is_dir():
        raise JobFailure(
            "WASI sysroot /usr/include/wasm32-wasi is missing; install wasi-libc"
        )
    if not (ROOT / "wasizarlib/nodejs/node_modules").is_dir():
        raise JobFailure(
            "wasizarlib/nodejs/node_modules is missing; install with npm ci --include=optional"
        )
    env = os.environ.copy()
    env["CFLAGS_wasm32_wasip2"] = "-isystem /usr/include/wasm32-wasi"
    command(
        [
            "cargo",
            "build",
            "--release",
            "--locked",
            "-p",
            "wasizarlib",
            "--target",
            "wasm32-wasip2",
        ],
        env=env,
    )
    node = ROOT / "wasizarlib/nodejs"
    command(["npm", "run", "build:js"], cwd=node)
    command(["npm", "test"], cwd=node)
    with tempfile.TemporaryDirectory(prefix="zar-ci-wasi-") as temp:
        tmp = Path(temp)
        command(["npm", "pack", "--pack-destination", str(tmp)], cwd=node)
        stage = tmp / "wasizarlib"
        copy_required(ROOT / "target/wasm32-wasip2/release/wasizarlib.wasm", stage)
        copy_required(ROOT / "wasizarlib/README.md", stage)
        copy_required(ROOT / "wasizarlib/wit/world.wit", stage / "wit")
        copy_required(one_file(tmp, "*.tgz"), stage)
        archive_directory(stage, DIST / "wasizarlib-wasip2.tar.gz")


def python_tests(version: str, overrides: dict[str, str]) -> None:
    interpreter = find_python(version, overrides)
    maturin = require("maturin")
    with tempfile.TemporaryDirectory(prefix="zar-ci-python-") as temp:
        venv = Path(temp) / "venv"
        command([interpreter, "-m", "venv", str(venv)])
        env = os.environ.copy()
        env["VIRTUAL_ENV"] = str(venv)
        env["PATH"] = (
            str(venv / ("Scripts" if os.name == "nt" else "bin"))
            + os.pathsep
            + env["PATH"]
        )
        if version.endswith("t"):
            env["PYTHON_GIL"] = "0"
        vpython = venv / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        command([str(vpython), "-m", "pip", "install", "pytest>=8"], env=env)
        command([maturin, "develop", "--release"], cwd=ROOT / "pyzarlib", env=env)
        command([str(vpython), "-Wd", "-m", "pytest"], cwd=ROOT / "pyzarlib", env=env)


def java_bindings(module: str, system: str) -> None:
    require("mvn")
    ensure_java25()
    env = os.environ.copy()
    command(
        ["cargo", "build", "--manifest-path", "Cargo.toml", "--release", "-p", "czarlib"]
    )
    if module == "jzarlib-jni":
        if system == "Windows":
            compiler, env = windows_msvc_environment()
            home = os.environ.get("JAVA_HOME")
            if not home:
                raise JobFailure("JAVA_HOME is required for the Windows JNI build")
            native = ROOT / "jzarlib-jni/target/native"
            native.mkdir(parents=True, exist_ok=True)
            command(
                [
                    compiler,
                    "/nologo",
                    "/O2",
                    "/W4",
                    "/WX",
                    "/LD",
                    f"/I{home}/include",
                    f"/I{home}/include/win32",
                    f"/I{ROOT}/czarlib/include",
                    str(ROOT / "jzarlib-jni/src/main/c/jzarlib_jni.c"),
                    str(ROOT / "target/release/czarlib.dll.lib"),
                    "/link",
                    f"/OUT:{native / 'zarlib_jni.dll'}",
                ],
                env=env,
            )
            copy_required(ROOT / "target/release/czarlib.dll", native)
            env["PATH"] = str(native) + os.pathsep + env["PATH"]
        else:
            command(["sh", "./jzarlib-jni/build-native.sh"])
    command(
        ["mvn", "--batch-mode", "--no-transfer-progress", "verify"],
        cwd=ROOT / module,
        env=env,
    )
    with tempfile.TemporaryDirectory(prefix="zar-ci-java-") as temp:
        runner = {
            "Linux": "ubuntu-latest",
            "Darwin": "macos-latest",
            "Windows": "windows-latest",
        }[system]
        stage = Path(temp) / f"{module}-{runner}"
        copy_required(one_file(ROOT / module / "target", f"{module}-*.jar"), stage)
        lib = {
            "Linux": "libczarlib.so",
            "Darwin": "libczarlib.dylib",
            "Windows": "czarlib.dll",
        }[system]
        copy_required(ROOT / "target/release" / lib, stage)
        if module == "jzarlib-jni":
            jni = {
                "Linux": "libzarlib_jni.so",
                "Darwin": "libzarlib_jni.dylib",
                "Windows": "zarlib_jni.dll",
            }[system]
            copy_required(ROOT / module / "target/native" / jni, stage)
        DIST.mkdir(parents=True, exist_ok=True)
        copy_required(one_file(stage, "*.jar"), DIST / stage.name)
        for library in stage.iterdir():
            if library.suffix != ".jar":
                copy_required(library, DIST / stage.name)
        shutil.copy2(one_file(stage, "*.jar"), DIST / f"{stage.name}.jar")
        archive_directory(stage, DIST / f"{stage.name}.tar.gz")


def binaries(
    name: str,
    target: str,
    static: bool,
    windows: bool,
    cross_windows: bool = False,
) -> None:
    env = os.environ.copy()
    if static:
        require("musl-gcc")
        env["RUSTFLAGS"] = "-C target-feature=+crt-static"
        env[f"CARGO_TARGET_{target.upper().replace('-', '_')}_LINKER"] = "musl-gcc"
    if cross_windows:
        ensure_windows_cross_toolchain(target)
        env["XWIN_CROSS_COMPILER"] = "clang-cl"
        build = ["cargo", "xwin", "build"]
    else:
        if windows:
            ensure_rustup_item("target", target)
        build = ["cargo", "build"]
    command(
        build
        + ["--workspace", "--release", "--bins", "--locked", "--target", target],
        env=env,
    )
    windows = windows or cross_windows
    with tempfile.TemporaryDirectory(prefix="zar-ci-binaries-") as temp:
        stage = Path(temp) / f"zar-rs-{name}"
        for binary in ("zarc", "zarg", "zcopy"):
            copy_required(
                ROOT
                / "target"
                / target
                / "release"
                / (binary + (".exe" if windows else "")),
                stage,
            )
        archive_directory(
            stage, DIST / f"{stage.name}.{'zip' if windows else 'tar.gz'}", windows
        )


def windows_nuget(arch: str, target: str) -> None:
    require("dotnet")
    command(
        ["cargo", "build", "--locked", "--release", "-p", "czarlib", "--target", target]
    )
    native = ROOT / "target" / target / "release/czarlib.dll"
    if not native.is_file():
        raise JobFailure(f"expected native library not found: {native}")
    with tempfile.TemporaryDirectory(prefix="zar-ci-nuget-") as temp:
        command(
            [
                "dotnet",
                "pack",
                "csharpzarlib/ZarLib/ZarLib.csproj",
                "--configuration",
                "Release",
                "--output",
                temp,
                f"-p:PackageId=ZarLib.{arch}",
                f"-p:RuntimeIdentifier=win-{arch}",
                f"-p:NativeLibraryPath={native}",
            ]
        )
        copy_required(one_file(Path(temp), "*.nupkg"), DIST)


def wheels(overrides: dict[str, str]) -> None:
    maturin = require("maturin")
    interpreters = [find_python(version, overrides) for version in WHEEL_VERSIONS]
    with tempfile.TemporaryDirectory(prefix="zar-ci-wheels-") as temp:
        command(
            [maturin, "build", "--release", "--out", temp, "--interpreter"]
            + interpreters,
            cwd=ROOT / "pyzarlib",
        )
        files = list(Path(temp).glob("*.whl"))
        if len(files) < len(WHEEL_VERSIONS):
            raise JobFailure(
                f"expected at least {len(WHEEL_VERSIONS)} wheels, found {len(files)}"
            )
        for wheel in files:
            copy_required(wheel, DIST)


def sdist() -> None:
    maturin = require("maturin")
    with tempfile.TemporaryDirectory(prefix="zar-ci-sdist-") as temp:
        command([maturin, "sdist", "--out", temp], cwd=ROOT / "pyzarlib")
        copy_required(one_file(Path(temp), "*.tar.gz"), DIST)


class Runner:
    def __init__(self) -> None:
        self.results: dict[str, tuple[str, str]] = {}

    def skip(self, name: str, reason: str) -> None:
        self.results[name] = ("SKIP", reason)
        print(f"\nSKIP {name}: {reason}", flush=True)

    def run(self, name: str, action, needs: tuple[str, ...] = ()) -> None:
        failed = [
            need
            for need in needs
            if self.results.get(need, ("MISSING", ""))[0] != "PASS"
        ]
        if failed:
            self.skip(name, "prerequisite job did not pass: " + ", ".join(failed))
            return
        print(f"\n=== {name} ===", flush=True)
        try:
            action()
        except (
            JobFailure,
            OSError,
            subprocess.CalledProcessError,
            ValueError,
        ) as error:
            self.results[name] = ("FAIL", str(error))
            print(f"FAIL {name}: {error}", file=sys.stderr, flush=True)
        else:
            self.results[name] = ("PASS", "")
            print(f"PASS {name}", flush=True)


def main(argv: list[str] | None = None) -> int:
    global AUTO_INSTALL_ENABLED

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--no-install",
        action="store_true",
        help="do not install missing build tools, linters, or Rust components",
    )
    parser.add_argument(
        "--python",
        action="append",
        default=[],
        metavar="VERSION=PATH",
        help="select an interpreter for a Python matrix entry; repeat as needed",
    )
    args = parser.parse_args(argv)
    overrides = {}
    for value in args.python:
        version, separator, path = value.partition("=")
        if (
            not separator
            or version not in PYTHON_VERSIONS
            or not path
            or version in overrides
        ):
            parser.error(f"invalid or duplicate --python: {value}")
        overrides[version] = str(Path(path).expanduser().resolve())

    AUTO_INSTALL_ENABLED = not args.no_install
    system, arch = platform.system(), host_arch()
    runner = Runner()
    linux = system == "Linux"
    core_platform = system in ("Linux", "Windows")
    core = ("cargo-fmt", "cargo-clippy", "cargo-test")
    if core_platform:
        runner.run("cargo-fmt", cargo_fmt)
        runner.run("cargo-clippy", cargo_clippy)
        runner.run("cargo-test", lambda: command(["cargo", "test", "--workspace"]))
        runner.run(
            "zcopy-test",
            lambda: command(["cargo", "test", "--locked", "-p", "zcopy"]),
        )
        runner.run("czarlib", lambda: c_tests(system))
    else:
        for name in (*core, "zcopy-test", "czarlib"):
            runner.skip(name, "CI job runs on Linux and Windows")

    runner.run("ruff", ruff_check)
    runner.run("rumdl", rumdl_check)
    runner.run("zizmor", zizmor_check)

    runner.run(
        "cxxzarlib-build",
        lambda: command(
            ["cargo", "build", "--locked", "--release", "-p", "cxxzarlib"]
        ),
    )
    if system == "Linux":
        runner.run("cxxzarlib-native-test", cxx_api_tests, ("cxxzarlib-build",))
    else:
        runner.skip("cxxzarlib-native-test", "native harness uses Linux linker flags")

    if system in ("Linux", "Darwin"):
        runner.run("gozarlib-test", lambda: go_tests(system))
    else:
        runner.skip("gozarlib-test", "cgo library flags support Linux and macOS")

    if linux:
        runner.run("wasizarlib", wasi, core)
        for version in PYTHON_VERSIONS:
            runner.run(
                f"pyzarlib-{version}",
                lambda version=version: python_tests(version, overrides),
            )
        runner.run("csharp-test", csharp_tests)
    else:
        for name in (
            "wasizarlib",
            *(f"pyzarlib-{v}" for v in PYTHON_VERSIONS),
        ):
            runner.skip(name, "CI job runs on Linux")
        if system == "Windows":
            runner.run("csharp-test", csharp_tests)
        else:
            runner.skip("csharp-test", "CI job runs on Linux and Windows")

    for cs_arch, cs_target in (
        ("x64", "x86_64-pc-windows-msvc"),
        ("arm64", "aarch64-pc-windows-msvc"),
    ):
        name = f"csharp-pack-windows-{cs_arch}"
        if system != "Windows" or arch != ("x86_64" if cs_arch == "x64" else "aarch64"):
            runner.skip(name, "requires matching Windows host")
        else:
            runner.run(name, lambda a=cs_arch, t=cs_target: windows_nuget(a, t))

    for module in ("jzarlib-ffm", "jzarlib-jni"):
        for os_name in ("Linux", "Darwin", "Windows"):
            name = f"{module}-{os_name.lower()}"
            if system != os_name:
                runner.skip(name, f"requires {os_name}")
            else:
                runner.run(name, lambda module=module: java_bindings(module, system))

    for name, os_name, machine, target, static in PLATFORMS:
        cross_windows = linux and os_name == "Windows"
        if cross_windows:
            runner.run(
                f"build-{name}",
                lambda n=name, t=target, s=static: binaries(
                    n, t, s, False, cross_windows=True
                ),
                core,
            )
        elif (system, arch) != (os_name, machine):
            runner.skip(f"build-{name}", f"requires {os_name} {machine}")
        else:
            runner.run(
                f"build-{name}",
                lambda n=name, t=target, s=static: binaries(
                    n, t, s, system == "Windows"
                ),
                core if linux else (),
            )

    wheel_platforms = (
        ("Linux", "x86_64"),
        ("Linux", "aarch64"),
        ("Darwin", "x86_64"),
        ("Darwin", "aarch64"),
        ("Windows", "x86_64"),
        ("Windows", "aarch64"),
    )
    for os_name, machine in wheel_platforms:
        name = f"release-wheels-{os_name.lower()}-{machine}"
        if (system, arch) != (os_name, machine):
            runner.skip(name, f"requires {os_name} {machine}")
        else:
            # Maturin's local Linux build does not provide the action's manylinux container.
            runner.run(
                name,
                lambda: wheels(overrides),
                core + tuple(f"pyzarlib-{v}" for v in PYTHON_VERSIONS) if linux else (),
            )
    if linux:
        runner.run("release-sdist", sdist, core)
    else:
        runner.skip("release-sdist", "CI job runs on Linux")

    print("\n=== Summary ===")
    for name, (status, reason) in runner.results.items():
        print(f"{status:4} {name}" + (f": {reason}" if reason else ""))
    print(f"Artifacts: {DIST}")
    print(
        "GitHub artifact reuse, uploads, and release publication are not run locally."
    )
    if linux:
        print("Linux wheels are built on the host, not in the CI manylinux container.")
    return int(any(status == "FAIL" for status, _ in runner.results.values()))


if __name__ == "__main__":
    raise SystemExit(main())
