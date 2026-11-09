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
PYTHON_VERSIONS = ("3.11", "3.12", "3.13", "3.14", "3.15", "3.14t", "3.15t")
WHEEL_VERSIONS = PYTHON_VERSIONS[:5]
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


def command(args: list[str], cwd: Path = ROOT, env: dict[str, str] | None = None) -> None:
    print("+", subprocess.list2cmdline(args), flush=True)
    subprocess.run(args, cwd=cwd, env=env, check=True)


def require(name: str) -> str:
    path = shutil.which(name)
    if path is None:
        raise JobFailure(f"required tool not found: {name}")
    return path


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
        [executable, "-c", "import sys,sysconfig; print(f'{sys.version_info.major}.{sys.version_info.minor}' + ('t' if sysconfig.get_config_var('Py_GIL_DISABLED') else ''))"],
        text=True,
        capture_output=True,
        check=True,
    )
    return result.stdout.strip()


def find_python(version: str, overrides: dict[str, str]) -> str:
    candidates = []
    if version in overrides:
        candidates.append(overrides[version])
    else:
        path = shutil.which(f"python{version}")
        if path:
            candidates.append(path)
        pyenv = Path(os.environ.get("PYENV_ROOT", Path.home() / ".pyenv")) / "versions"
        if pyenv.is_dir():
            suffix = "t" if version.endswith("t") else ""
            prefix = version.removesuffix("t") + "."
            candidates.extend(
                str(path / "bin" / "python")
                for path in sorted(pyenv.iterdir(), reverse=True)
                if path.name.startswith(prefix) and path.name.endswith(suffix)
                and (suffix or not path.name.endswith("t"))
                and (path / "bin" / "python").is_file()
            )
    for candidate in candidates:
        try:
            if python_identity(candidate) == version:
                return candidate
        except (OSError, subprocess.CalledProcessError):
            continue
    raise JobFailure(f"Python {version} not found; supply --python {version}=/path/to/python")


def c_tests() -> None:
    require("cc")
    command(["cargo", "build", "--locked", "-p", "czarlib"])
    command(["cargo", "test", "--locked", "-p", "zarlib"])
    with tempfile.TemporaryDirectory(prefix="zar-ci-c-") as temp:
        tmp = Path(temp)
        library = ROOT / "target/debug"
        common = ["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-I", "czarlib/include"]
        link = ["-L", str(library), "-lczarlib", f"-Wl,-rpath,{library}"]
        api = tmp / "czarlib-test"
        example = tmp / "zarc-c"
        command(common + ["czarlib/tests/test_c_api.c"] + link + ["-pthread", "-o", str(api)])
        command([str(api)])
        command(common + ["czarlib/examples/zarc.c"] + link + ["-o", str(example)])
        command([str(example), "--help"])
        (tmp / "input.txt").write_text("initial", encoding="utf-8")
        (tmp / "added.txt").write_text("added", encoding="utf-8")
        for args in (["c", "sample.zar", "input.txt"], ["a", "sample.zar", "added.txt"], ["k", "sample.zar"]):
            command([str(example)] + args, cwd=tmp)
        info = subprocess.run([str(example), "i", "sample.zar"], cwd=tmp, text=True, capture_output=True, check=True)
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


def wasi() -> None:
    require("cargo")
    require("npm")
    if not (ROOT / "wasizarlib/nodejs/node_modules").is_dir():
        raise JobFailure("wasizarlib/nodejs/node_modules is missing; install with npm ci --include=optional")
    env = os.environ.copy()
    env["CFLAGS_wasm32_wasip2"] = "-isystem /usr/include/wasm32-wasi"
    command(["cargo", "build", "--release", "--locked", "-p", "wasizarlib", "--target", "wasm32-wasip2"], env=env)
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
    command([interpreter, "-c", "import pytest"])
    with tempfile.TemporaryDirectory(prefix="zar-ci-python-") as temp:
        venv = Path(temp) / "venv"
        command([interpreter, "-m", "venv", "--system-site-packages", str(venv)])
        env = os.environ.copy()
        env["VIRTUAL_ENV"] = str(venv)
        env["PATH"] = str(venv / ("Scripts" if os.name == "nt" else "bin")) + os.pathsep + env["PATH"]
        if version.endswith("t"):
            env["PYTHON_GIL"] = "0"
        vpython = venv / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        command([maturin, "develop", "--release"], cwd=ROOT / "pyzarlib", env=env)
        command([str(vpython), "-Wd", "-m", "pytest"], cwd=ROOT / "pyzarlib", env=env)


def java_bindings(module: str, system: str) -> None:
    require("mvn")
    require("javac")
    env = os.environ.copy()
    command(["cargo", "build", "--manifest-path", "Cargo.toml", "-p", "czarlib"])
    if module == "jzarlib-jni":
        if system == "Windows":
            require("cl")
            home = os.environ.get("JAVA_HOME")
            if not home:
                raise JobFailure("JAVA_HOME is required for the Windows JNI build")
            native = ROOT / "jzarlib-jni/target/native"
            native.mkdir(parents=True, exist_ok=True)
            command(["cl", "/nologo", "/O2", "/W4", "/WX", "/LD", f"/I{home}/include", f"/I{home}/include/win32", f"/I{ROOT}/czarlib/include", str(ROOT / "jzarlib-jni/src/main/c/jzarlib_jni.c"), str(ROOT / "target/debug/czarlib.dll.lib"), "/link", f"/OUT:{native / 'zarlib_jni.dll'}"])
            copy_required(ROOT / "target/debug/czarlib.dll", native)
            env["PATH"] = str(native) + os.pathsep + env["PATH"]
        else:
            command(["sh", "./jzarlib-jni/build-native.sh"])
    command(["mvn", "--batch-mode", "--no-transfer-progress", "verify"], cwd=ROOT / module, env=env)
    with tempfile.TemporaryDirectory(prefix="zar-ci-java-") as temp:
        runner = {"Linux": "ubuntu-latest", "Darwin": "macos-latest", "Windows": "windows-latest"}[system]
        stage = Path(temp) / f"{module}-{runner}"
        copy_required(one_file(ROOT / module / "target", f"{module}-*.jar"), stage)
        lib = {"Linux": "libczarlib.so", "Darwin": "libczarlib.dylib", "Windows": "czarlib.dll"}[system]
        copy_required(ROOT / "target/debug" / lib, stage)
        if module == "jzarlib-jni":
            jni = {"Linux": "libzarlib_jni.so", "Darwin": "libzarlib_jni.dylib", "Windows": "zarlib_jni.dll"}[system]
            copy_required(ROOT / module / "target/native" / jni, stage)
        DIST.mkdir(parents=True, exist_ok=True)
        copy_required(one_file(stage, "*.jar"), DIST / stage.name)
        for library in stage.iterdir():
            if library.suffix != ".jar":
                copy_required(library, DIST / stage.name)
        shutil.copy2(one_file(stage, "*.jar"), DIST / f"{stage.name}.jar")
        archive_directory(stage, DIST / f"{stage.name}.tar.gz")


def binaries(name: str, target: str, static: bool, windows: bool) -> None:
    env = os.environ.copy()
    if static:
        require("musl-gcc")
        env["RUSTFLAGS"] = "-C target-feature=+crt-static"
        env[f"CARGO_TARGET_{target.upper().replace('-', '_')}_LINKER"] = "musl-gcc"
    command(["cargo", "build", "--release", "--bins", "--locked", "--target", target], env=env)
    with tempfile.TemporaryDirectory(prefix="zar-ci-binaries-") as temp:
        stage = Path(temp) / f"zar-rs-{name}"
        for binary in ("zarc", "zarg"):
            copy_required(ROOT / "target" / target / "release" / (binary + (".exe" if windows else "")), stage)
        archive_directory(stage, DIST / f"{stage.name}.{'zip' if windows else 'tar.gz'}", windows)


def windows_nuget(arch: str, target: str) -> None:
    require("dotnet")
    command(["cargo", "build", "--locked", "--release", "-p", "czarlib", "--target", target])
    native = ROOT / "target" / target / "release/czarlib.dll"
    if not native.is_file():
        raise JobFailure(f"expected native library not found: {native}")
    with tempfile.TemporaryDirectory(prefix="zar-ci-nuget-") as temp:
        command(["dotnet", "pack", "csharpzarlib/ZarLib/ZarLib.csproj", "--configuration", "Release", "--output", temp, f"-p:PackageId=ZarLib.{arch}", f"-p:RuntimeIdentifier=win-{arch}", f"-p:NativeLibraryPath={native}"])
        copy_required(one_file(Path(temp), "*.nupkg"), DIST)


def wheels(overrides: dict[str, str]) -> None:
    maturin = require("maturin")
    interpreters = [find_python(version, overrides) for version in WHEEL_VERSIONS]
    with tempfile.TemporaryDirectory(prefix="zar-ci-wheels-") as temp:
        command([maturin, "build", "--release", "--out", temp, "--interpreter"] + interpreters, cwd=ROOT / "pyzarlib")
        files = list(Path(temp).glob("*.whl"))
        if len(files) < len(WHEEL_VERSIONS):
            raise JobFailure(f"expected at least {len(WHEEL_VERSIONS)} wheels, found {len(files)}")
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
        failed = [need for need in needs if self.results.get(need, ("MISSING", ""))[0] != "PASS"]
        if failed:
            self.skip(name, "prerequisite job did not pass: " + ", ".join(failed))
            return
        print(f"\n=== {name} ===", flush=True)
        try:
            action()
        except (JobFailure, OSError, subprocess.CalledProcessError, ValueError) as error:
            self.results[name] = ("FAIL", str(error))
            print(f"FAIL {name}: {error}", file=sys.stderr, flush=True)
        else:
            self.results[name] = ("PASS", "")
            print(f"PASS {name}", flush=True)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--python", action="append", default=[], metavar="VERSION=PATH", help="select an interpreter for a Python matrix entry; repeat as needed")
    args = parser.parse_args(argv)
    overrides = {}
    for value in args.python:
        version, separator, path = value.partition("=")
        if not separator or version not in PYTHON_VERSIONS or not path or version in overrides:
            parser.error(f"invalid or duplicate --python: {value}")
        overrides[version] = str(Path(path).expanduser().resolve())

    system, arch = platform.system(), host_arch()
    runner = Runner()
    linux = system == "Linux"
    core = ("cargo-fmt", "cargo-clippy", "cargo-test")
    if linux:
        runner.run("cargo-fmt", lambda: command(["cargo", "fmt", "--all", "--check"]))
        runner.run("cargo-clippy", lambda: command(["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]))
        runner.run("cargo-test", lambda: command(["cargo", "test", "--workspace"]))
        runner.run("czarlib", c_tests)
        runner.run("wasizarlib", wasi, core)
        for version in PYTHON_VERSIONS:
            runner.run(f"pyzarlib-{version}", lambda version=version: python_tests(version, overrides))
        runner.run("csharp-test", lambda: (command(["cargo", "build", "--locked", "-p", "czarlib"]), command(["dotnet", "test", "csharpzarlib/ZarLib.Tests/ZarLib.Tests.csproj", "--configuration", "Release"])))
    else:
        for name in (*core, "czarlib", "wasizarlib", *(f"pyzarlib-{v}" for v in PYTHON_VERSIONS), "csharp-test"):
            runner.skip(name, "CI job runs on Linux")

    for cs_arch, cs_target in (("x64", "x86_64-pc-windows-msvc"), ("arm64", "aarch64-pc-windows-msvc")):
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
        if (system, arch) != (os_name, machine):
            runner.skip(f"build-{name}", f"requires {os_name} {machine}")
        else:
            runner.run(f"build-{name}", lambda n=name, t=target, s=static: binaries(n, t, s, system == "Windows"), core if linux else ())

    wheel_platforms = (("Linux", "x86_64"), ("Linux", "aarch64"), ("Darwin", "x86_64"), ("Darwin", "aarch64"), ("Windows", "x86_64"), ("Windows", "aarch64"))
    for os_name, machine in wheel_platforms:
        name = f"release-wheels-{os_name.lower()}-{machine}"
        if (system, arch) != (os_name, machine):
            runner.skip(name, f"requires {os_name} {machine}")
        else:
            # Maturin's local Linux build does not provide the action's manylinux container.
            runner.run(name, lambda: wheels(overrides), core + tuple(f"pyzarlib-{v}" for v in PYTHON_VERSIONS) if linux else ())
    if linux:
        runner.run("release-sdist", sdist, core)
    else:
        runner.skip("release-sdist", "CI job runs on Linux")

    print("\n=== Summary ===")
    for name, (status, reason) in runner.results.items():
        print(f"{status:4} {name}" + (f": {reason}" if reason else ""))
    print(f"Artifacts: {DIST}")
    print("GitHub artifact reuse, uploads, and release publication are not run locally.")
    if linux:
        print("Linux wheels are built on the host, not in the CI manylinux container.")
    return int(any(status == "FAIL" for status, _ in runner.results.values()))


if __name__ == "__main__":
    raise SystemExit(main())
