#!/usr/bin/env python3
"""Benchmark zarlib, zip, 7z, tar, and tar.gz using the Linux 4.0.1 source tree.

Every format is measured with a hot file cache (the source tree is read once
beforehand) and, when running with root/administrator rights, with a cold cache
(dropped before each measured operation).
"""

from __future__ import annotations

import argparse
import concurrent.futures
import datetime as dt
from collections import deque
from dataclasses import dataclass
import html
import json
import os
import platform
import plistlib
import re
import scandir_rs
import secrets
import shutil
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath
from typing import Callable

if os.name == "posix":
    import fcntl
    import pty
    import select
    import termios

import psutil


ROOT = Path(__file__).resolve().parents[1]
TESTDATA = ROOT / "testdata"
TESTRESULTS = ROOT / "testresults"
SOURCE_NAME = "linux-4.0.1"
SOURCE_ARCHIVE = TESTDATA / f"{SOURCE_NAME}.tar.gz"
SOURCE_DIR = TESTDATA / SOURCE_NAME
ARCHIVE_DIR = TESTDATA / "benchmark-archives"
OS_TAG = {"linux": "linux", "win32": "windows", "darwin": "macos"}.get(
    sys.platform, re.sub(r"\W+", "-", sys.platform)
)
REPORT_PATH = ROOT / "doc" / f"benchmark-report-{OS_TAG}.md"
CHART_PATH = ROOT / "doc" / f"benchmark-charts-{OS_TAG}.svg"
SOURCE_URL = "https://cdn.kernel.org/pub/linux/kernel/v4.x/linux-4.0.1.tar.gz"
ENCRYPTED_PASSWORD = secrets.token_urlsafe(24)
PROMPT_RE = re.compile(rb"(?im)^[^\r\n]*(?:password|passphrase)[^\r\n]*:\s*$")
RAMMAP_URL = "https://download.sysinternals.com/files/RAMMap.zip"
TOOLS_DIR = TESTDATA / "tools"
COLLISION_SUFFIX_RE = re.compile(r" \(\d+\)(?=\.[^.]+$|$)")


@dataclass(frozen=True)
class StorageInfo:
    location: str
    media_type: str
    model: str
    device: str


@dataclass(frozen=True)
class BenchmarkHardware:
    operating_system: str
    cpu_model: str
    storage: tuple[StorageInfo, ...]


def _hardware_command_output(
    command: list[str], timeout: float = 10.0
) -> bytes | None:
    try:
        result = subprocess.run(
            command,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            check=False,
            timeout=timeout,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    if result.returncode != 0 or not result.stdout.strip():
        return None
    return result.stdout


def _cpu_model() -> str:
    if sys.platform.startswith("linux"):
        try:
            lines = Path("/proc/cpuinfo").read_text(
                encoding="utf-8", errors="replace"
            ).splitlines()
        except OSError:
            lines = []
        for key in ("model name", "hardware", "processor"):
            for line in lines:
                field, separator, value = line.partition(":")
                if separator and field.strip().casefold() == key:
                    value = value.strip()
                    if value and not value.isdecimal():
                        return value

    elif sys.platform == "win32":
        powershell = shutil.which("powershell.exe") or shutil.which("pwsh")
        if powershell:
            output = _hardware_command_output(
                [
                    powershell,
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    "(Get-CimInstance Win32_Processor | "
                    "Select-Object -First 1 -ExpandProperty Name)",
                ]
            )
            if output:
                return output.decode("utf-8", errors="replace").strip()

    elif sys.platform == "darwin":
        sysctl = shutil.which("sysctl")
        if sysctl:
            output = _hardware_command_output(
                [sysctl, "-n", "machdep.cpu.brand_string"]
            )
            if output:
                return output.decode("utf-8", errors="replace").strip()
        system_profiler = shutil.which("system_profiler")
        if system_profiler:
            output = _hardware_command_output(
                [system_profiler, "SPHardwareDataType", "-json"], timeout=20.0
            )
            if output:
                try:
                    hardware = json.loads(output)
                except json.JSONDecodeError:
                    hardware = {}
                for item in hardware.get("SPHardwareDataType", []):
                    for key in ("chip_type", "cpu_type", "processor_name"):
                        value = item.get(key)
                        if isinstance(value, str) and value.strip():
                            return value.strip()

    processor = platform.processor().strip()
    if processor and processor.casefold() not in {
        "unknown",
        platform.machine().casefold(),
    }:
        return processor
    return "Not reported by operating system"


def _linux_storage_info(location: str, path: Path) -> StorageInfo:
    try:
        device_number = path.stat().st_dev
    except OSError as error:
        return StorageInfo(
            location,
            "Unknown (could not inspect benchmark path)",
            "Not reported",
            error.strerror or "device unavailable",
        )

    major = os.major(device_number)
    minor = os.minor(device_number)
    block_path = Path("/sys/dev/block") / f"{major}:{minor}"
    try:
        block = block_path.resolve(strict=True)
    except OSError:
        return StorageInfo(
            location,
            "Unknown (backing device not exposed)",
            "Not reported",
            f"filesystem device {major}:{minor}",
        )

    if (block / "partition").is_file():
        block = block.parent
    device = block.name
    try:
        rotational = (block / "queue" / "rotational").read_text(
            encoding="ascii"
        ).strip()
    except OSError:
        rotational = ""
    media_type = {
        "0": "SSD / non-rotational (may be virtual)",
        "1": "HDD / rotational",
    }.get(rotational, "Unknown (rotation status not exposed)")

    model = "Not reported"
    for candidate in (
        block / "device" / "model",
        block / "device" / "name",
        block / "model",
    ):
        try:
            value = candidate.read_text(encoding="utf-8", errors="replace").strip()
        except OSError:
            continue
        if value:
            model = " ".join(value.split())
            break
    return StorageInfo(location, media_type, model, device)


def _windows_storage_info(location: str, path: Path) -> StorageInfo:
    drive = path.resolve().drive.rstrip("\\/")
    drive_letter = drive.rstrip(":")
    if len(drive_letter) != 1:
        return StorageInfo(
            location,
            "Unknown (drive mapping not exposed)",
            "Not reported",
            drive or "device unavailable",
        )
    powershell = shutil.which("powershell.exe") or shutil.which("pwsh")
    if not powershell:
        return StorageInfo(
            location,
            "Unknown (PowerShell unavailable)",
            "Not reported",
            f"{drive_letter}:",
        )
    script = (
        "$ErrorActionPreference = 'Stop'; "
        f"$partition = Get-Partition -DriveLetter '{drive_letter}' | "
        "Select-Object -First 1; "
        "if ($null -eq $partition) { exit 1 }; "
        "$disk = Get-Disk -Number $partition.DiskNumber; "
        "[pscustomobject]@{ Number = $disk.Number; "
        "MediaType = [string]$disk.MediaType; "
        "BusType = [string]$disk.BusType; "
        "Model = [string]$disk.FriendlyName } | ConvertTo-Json -Compress"
    )
    output = _hardware_command_output(
        [powershell, "-NoProfile", "-NonInteractive", "-Command", script]
    )
    if not output:
        return StorageInfo(
            location,
            "Unknown (Windows did not expose disk details)",
            "Not reported",
            f"{drive_letter}:",
        )
    try:
        disk = json.loads(output)
    except json.JSONDecodeError:
        disk = {}
    media = str(disk.get("MediaType", "")).strip()
    bus = str(disk.get("BusType", "")).strip()
    if media.casefold() == "ssd" or bus.casefold() == "nvme":
        media_type = "SSD" if media.casefold() == "ssd" else "SSD (NVMe)"
    elif media.casefold() == "hdd":
        media_type = "HDD"
    else:
        media_type = f"Unknown (Windows reported {media or 'no media type'})"
    model = str(disk.get("Model", "")).strip() or "Not reported"
    number = disk.get("Number")
    device = f"disk {number}" if number is not None else f"{drive_letter}:"
    if bus:
        device += f" ({bus})"
    return StorageInfo(location, media_type, model, device)


def _macos_storage_info(location: str, path: Path) -> StorageInfo:
    diskutil = shutil.which("diskutil")
    if not diskutil:
        return StorageInfo(
            location,
            "Unknown (diskutil unavailable)",
            "Not reported",
            "device unavailable",
        )
    output = _hardware_command_output(
        [diskutil, "info", "-plist", str(path.resolve())]
    )
    if not output:
        return StorageInfo(
            location,
            "Unknown (diskutil did not expose disk details)",
            "Not reported",
            "device unavailable",
        )
    try:
        disk = plistlib.loads(output)
    except plistlib.InvalidFileException:
        disk = {}
    solid_state = disk.get("SolidState")
    if solid_state is True or str(solid_state).casefold() in {"yes", "true", "1"}:
        media_type = "SSD"
    elif solid_state is False or str(solid_state).casefold() in {"no", "false", "0"}:
        media_type = "HDD"
    elif str(disk.get("BusProtocol", "")).casefold() == "nvme":
        media_type = "SSD (NVMe)"
    else:
        media_type = "Unknown (diskutil did not report media type)"
    model = next(
        (
            str(disk[key]).strip()
            for key in ("MediaName", "DeviceModel", "IOKitMediaName")
            if disk.get(key)
        ),
        "Not reported",
    )
    device = str(disk.get("DeviceIdentifier", "device unavailable"))
    return StorageInfo(location, media_type, model, device)


def _storage_info(location: str, path: Path) -> StorageInfo:
    if sys.platform.startswith("linux"):
        return _linux_storage_info(location, path)
    if sys.platform == "win32":
        return _windows_storage_info(location, path)
    if sys.platform == "darwin":
        return _macos_storage_info(location, path)
    return StorageInfo(
        location,
        "Unknown (unsupported operating system)",
        "Not reported",
        "device unavailable",
    )


def collect_benchmark_hardware() -> BenchmarkHardware:
    return BenchmarkHardware(
        operating_system=platform.platform(),
        cpu_model=_cpu_model(),
        storage=(
            _storage_info("Source and archives", TESTDATA),
            _storage_info("Extraction output", TESTRESULTS),
        ),
    )


class ResourceMonitor:
    """Records CPU and RAM usage at 1-second intervals in a background thread."""

    def __init__(self) -> None:
        self._samples: list[tuple[float, float, float]] = []  # (timestamp_offset, cpu_percent, ram_gib)
        self._stop_event = threading.Event()
        self._thread: threading.Thread | None = None
        self._start_time = 0.0

    def start(self) -> None:
        self._samples.clear()
        self._stop_event.clear()
        self._start_time = time.monotonic()
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()

    def stop(self) -> None:
        if self._thread is not None:
            self._stop_event.set()
            self._thread.join(timeout=5)
            self._thread = None

    @property
    def samples(self) -> list[tuple[float, float, float]]:
        return list(self._samples)

    def _run(self) -> None:
        while not self._stop_event.wait(1.0):
            elapsed = time.monotonic() - self._start_time
            cpu = psutil.cpu_percent(interval=0)
            ram = psutil.virtual_memory().used / (1024 ** 3)
            self._samples.append((elapsed, cpu, ram))


def require_tools() -> dict[str, str]:
    print("Checking required tools...")
    tools: dict[str, str] = {}
    missing: list[str] = []
    for name in ("cargo", "tar"):
        found = shutil.which(name)
        if found:
            tools[name] = found
        else:
            missing.append(name)

    openssl = shutil.which("openssl")
    if openssl:
        tools["openssl"] = openssl
    else:
        print(
            "warning: openssl not found; skipping encrypted tar/tar.gz benchmarks",
            file=sys.stderr,
        )

    seven_zip = next(
        (found for name in ("7z", "7zz", "7za") if (found := shutil.which(name))), None
    )
    if seven_zip is None and os.name == "nt":
        for base in (os.environ.get("ProgramFiles"), os.environ.get("ProgramFiles(x86)")):
            candidate = Path(base) / "7-Zip" / "7z.exe" if base else None
            if candidate and candidate.is_file():
                seven_zip = str(candidate)
                break
    if seven_zip:
        tools["7z"] = seven_zip
    else:
        missing.append("7z")

    # Info-ZIP is rarely installed on Windows; the zip format is skipped there.
    for name in ("zip", "unzip"):
        found = shutil.which(name)
        if found:
            tools[name] = found
        elif os.name != "nt":
            missing.append(name)
        else:
            print(f"warning: {name} not found; skipping the zip benchmark", file=sys.stderr)
    if missing:
        raise RuntimeError(f"Required benchmark tools are unavailable: {', '.join(missing)}")
    return tools


def is_gnu_tar(tar_executable: str) -> bool:
    result = subprocess.run(
        [tar_executable, "--version"],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    return b"GNU tar" in result.stdout


def is_privileged() -> bool:
    if os.name == "nt":
        import ctypes

        try:
            return bool(ctypes.windll.shell32.IsUserAnAdmin())  # type: ignore[attr-defined]
        except (AttributeError, OSError):
            return False
    return os.geteuid() == 0


def rammap_eula_accepted() -> bool:
    import winreg

    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Sysinternals\RAMMap") as key:
            return winreg.QueryValueEx(key, "EulaAccepted")[0] == 1
    except OSError:
        return False


def download_rammap() -> Path:
    executable = TOOLS_DIR / "RAMMap.exe"
    if executable.is_file():
        return executable
    TOOLS_DIR.mkdir(parents=True, exist_ok=True)
    archive = TOOLS_DIR / "RAMMap.zip"
    print(f"Downloading {RAMMAP_URL}")
    try:
        urllib.request.urlretrieve(RAMMAP_URL, archive)
        with zipfile.ZipFile(archive) as bundle:
            for name in ("RAMMap.exe", "Eula.txt"):
                with bundle.open(name) as source, (TOOLS_DIR / name).open("wb") as target:
                    shutil.copyfileobj(source, target)
    finally:
        archive.unlink(missing_ok=True)
    return executable


def resolve_cache_modes(mode: str, accept_eula: bool) -> tuple[list[str], Path | None]:
    """Returns the cache states to benchmark and the RAMMap path on Windows."""
    print("Resolving cache modes...")
    modes: list[str] = []
    rammap: Path | None = None
    if mode in ("hot", "both"):
        modes.append("hot")
    if mode in ("cold", "both"):
        reason = ""
        if not is_privileged():
            reason = (
                "not running as root" if os.name != "nt" else "not running elevated"
            )
        elif os.name == "nt":
            if not accept_eula and not rammap_eula_accepted():
                reason = (
                    "RAMMap's license has not been accepted; rerun with "
                    "--accept-sysinternals-eula to accept the Sysinternals EULA"
                )
            else:
                rammap = download_rammap()
        elif not sys.platform.startswith("linux"):
            reason = "cold cache dropping is only implemented for Linux and Windows"
        if reason:
            print(f"warning: skipping cold benchmarks: {reason}", file=sys.stderr)
        else:
            modes.append("cold")
    if not modes:
        raise RuntimeError("No benchmark cache mode is available")
    return modes, rammap


def drop_caches(rammap: Path | None, accept_eula: bool) -> None:
    if os.name == "nt":
        assert rammap is not None
        command = [str(rammap)]
        if accept_eula:
            command.append("-accepteula")
        run_command([*command, "-Et"])
    else:
        run_command(["sync"])
        Path("/proc/sys/vm/drop_caches").write_text("3\n", encoding="ascii")


def read_files(root: Path) -> None:
    """Reads every regular file below root (or the file itself) once to warm the cache.

    Uses a thread pool to parallelise I/O and warm the cache faster.
    """
    if root.is_file():
        paths = [root]
    else:
        walk = scandir_rs.Walk(
            str(root),
            return_type=scandir_rs.ReturnType.Ext,
            follow_links=False,
        )
        paths = [
            root / entry[0] / name
            for entry in walk
            for name in entry[2]
        ]

    def _read_one(path: Path) -> None:
        if path.is_symlink():
            return
        try:
            with path.open("rb") as handle:
                while handle.read(1024 * 1024):
                    pass
        except OSError:
            pass

    workers = min(32, (os.cpu_count() or 1) * 2)
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
        list(pool.map(_read_one, paths))


def run_command(
    command: list[str], *, cwd: Path | None = None, input_data: bytes | None = None
) -> None:
    result = subprocess.run(
        command,
        cwd=cwd,
        input=input_data,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if result.returncode:
        output = result.stdout.decode("utf-8", errors="replace")
        raise RuntimeError(
            f"Command failed with exit code {result.returncode}: "
            f"{' '.join(command)}\n{output[-6000:]}"
        )


def run_7z_extract(command: list[str], cwd: Path, expected_link_warnings: int) -> None:
    result = subprocess.run(
        command,
        cwd=cwd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    output = result.stdout.decode("utf-8", errors="replace")
    link_errors = re.findall(
        r"(?m)^ERROR: Dangerous link path was ignored :", output
    )
    all_errors = re.findall(r"(?m)^ERROR: (.*)$", output)
    if result.returncode == 0:
        return
    if (
        result.returncode == 2
        and expected_link_warnings > 0
        and len(link_errors) == expected_link_warnings
        and len(all_errors) == expected_link_warnings
        and f"Sub items Errors: {expected_link_warnings}" in output
    ):
        return
    raise RuntimeError(
        f"7z extraction failed with exit code {result.returncode}:\n{output[-6000:]}"
    )


def run_with_password(command: list[str], cwd: Path, password: str) -> None:
    if os.name != "posix":
        raise RuntimeError("Interactive password entry is only supported on POSIX")
    master_fd, slave_fd = pty.openpty()

    def attach_controlling_terminal() -> None:
        fcntl.ioctl(slave_fd, termios.TIOCSCTTY, 0)

    process = subprocess.Popen(
        command,
        cwd=cwd,
        stdin=slave_fd,
        stdout=slave_fd,
        stderr=slave_fd,
        start_new_session=True,
        preexec_fn=attach_controlling_terminal,
        close_fds=True,
    )
    os.close(slave_fd)
    output = bytearray()
    handled_prompts = 0
    deadline = time.monotonic() + 2 * 60 * 60

    try:
        while True:
            if time.monotonic() > deadline:
                process.kill()
                raise TimeoutError(f"Command exceeded two hours: {' '.join(command)}")
            ready, _, _ = select.select([master_fd], [], [], 0.2)
            if ready:
                try:
                    chunk = os.read(master_fd, 65536)
                except OSError:
                    chunk = b""
                output.extend(chunk)
                prompts = list(PROMPT_RE.finditer(output))
                while handled_prompts < len(prompts):
                    os.write(master_fd, (password + "\n").encode("utf-8"))
                    handled_prompts += 1
            if process.poll() is not None:
                try:
                    while True:
                        chunk = os.read(master_fd, 65536)
                        if not chunk:
                            break
                        output.extend(chunk)
                except OSError:
                    pass
                break

        return_code = process.wait()
        if return_code:
            text = output.decode("utf-8", errors="replace")
            raise RuntimeError(
                f"Command failed with exit code {return_code}: "
                f"{' '.join(command)}\n{text[-6000:]}"
            )
        if not handled_prompts:
            raise RuntimeError(
                f"Command did not request its encryption password: {' '.join(command)}"
            )
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        os.close(master_fd)


def validate_tarball(path: Path) -> None:
    expected_root = SOURCE_NAME
    with tarfile.open(path, "r:gz") as archive:
        for member in archive:
            name = PurePosixPath(member.name)
            if name.is_absolute() or ".." in name.parts or not name.parts:
                raise RuntimeError(f"Unsafe path in source tarball: {member.name!r}")
            if name.parts[0] != expected_root:
                raise RuntimeError(
                    f"Unexpected top-level path in source tarball: {member.name!r}"
                )
            if member.issym() or member.islnk():
                link = PurePosixPath(member.linkname)
                if link.is_absolute():
                    raise RuntimeError(f"Absolute link in source tarball: {member.name!r}")
                target = (
                    PurePosixPath(*name.parts[:-1], *link.parts)
                    if member.issym()
                    else link
                )
                resolved: list[str] = []
                for part in target.parts:
                    if part in ("", "."):
                        continue
                    if part == "..":
                        if not resolved:
                            raise RuntimeError(
                                f"Link escapes source tree in source tarball: {member.name!r}"
                            )
                        resolved.pop()
                    else:
                        resolved.append(part)
                if not resolved or resolved[0] != expected_root:
                    raise RuntimeError(
                        f"Link escapes source tree in source tarball: {member.name!r}"
                    )
            elif not (member.isfile() or member.isdir()):
                raise RuntimeError(
                    f"Unsupported special file in source tarball: {member.name!r}"
                )


def source_matches_tarball(tarball: Path) -> bool:
    """Check that the extracted source tree matches the tarball.

    On Windows, tolerate known platform limitations:
    - Missing symlinks and hard links, which the Windows extractor skips.
    - Case-colliding files (Windows is case-insensitive; the uppercase variant
      from the tarball overwrites the lowercase one, so sizes may differ).
    """
    max_tolerated = 50 if os.name == "nt" else 0

    members: list[tarfile.TarInfo] = []
    with tarfile.open(tarball, "r:gz") as archive:
        members = list(archive)

    mismatches = 0
    mismatch_lock = threading.Lock()
    early_exit = threading.Event()

    def _check_member(member: tarfile.TarInfo) -> None:
        nonlocal mismatches
        if early_exit.is_set():
            return
        path = TESTDATA.joinpath(*PurePosixPath(member.name).parts)
        ok = True
        if member.isdir():
            ok = not path.is_symlink() and path.is_dir()
        elif member.issym():
            matches_link = path.is_symlink() and os.readlink(path) == member.linkname
            ok = matches_link or (
                os.name == "nt" and not path.exists() and not path.is_symlink()
            )
        elif member.islnk():
            matches_file = not path.is_symlink() and path.is_file()
            ok = matches_file or (
                os.name == "nt" and not path.exists() and not path.is_symlink()
            )
        else:
            ok = path.is_file() and not path.is_symlink() and path.stat().st_size == member.size
        if not ok:
            with mismatch_lock:
                mismatches += 1
                if mismatches > max_tolerated:
                    early_exit.set()

    workers = min(32, (os.cpu_count() or 1) * 2)
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
        list(pool.map(_check_member, members))
    return mismatches <= max_tolerated


def extract_source_tarball(tar_executable: str) -> None:
    if os.name == "nt":
        with tarfile.open(SOURCE_ARCHIVE, "r:gz") as archive:
            members = [
                member
                for member in archive
                if member.isfile() or member.isdir()
            ]
            skipped_links = len(archive.getmembers()) - len(members)
            archive.extractall(path=TESTDATA, members=members)
        print(
            f"Extracted {len(members):,} source entries; skipped "
            f"{skipped_links:,} symbolic/hard links unsupported on Windows."
        )
        return

    run_command(
        [
            tar_executable,
            "--extract",
            "--gzip",
            "--file",
            str(SOURCE_ARCHIVE),
            "--directory",
            str(TESTDATA),
            "--no-same-owner",
            *(["--overwrite"] if is_gnu_tar(tar_executable) else []),
        ]
    )


def ensure_source(tar_executable: str) -> None:
    print("Ensuring source archive is available and extracted...")
    TESTDATA.mkdir(parents=True, exist_ok=True)
    if not SOURCE_ARCHIVE.is_file():
        temporary = SOURCE_ARCHIVE.with_suffix(".tar.gz.download")
        try:
            print(f"Downloading {SOURCE_URL}")
            urllib.request.urlretrieve(SOURCE_URL, temporary)
            temporary.replace(SOURCE_ARCHIVE)
        finally:
            temporary.unlink(missing_ok=True)

    validate_tarball(SOURCE_ARCHIVE)
    if not SOURCE_DIR.is_dir() or not source_matches_tarball(SOURCE_ARCHIVE):
        action = "Extracting" if not SOURCE_DIR.exists() else "Refreshing"
        print(f"{action} {SOURCE_ARCHIVE.name} in {TESTDATA}")
        extract_source_tarball(tar_executable)
    if not SOURCE_DIR.is_dir():
        raise RuntimeError(f"Source extraction did not create {SOURCE_DIR}")
    if not source_matches_tarball(SOURCE_ARCHIVE):
        raise RuntimeError(f"Source folder does not match {SOURCE_ARCHIVE.name}")


def _make_writable_and_retry(function: Callable[..., object], path: str, _error: object) -> None:
    os.chmod(path, 0o700)
    function(path)


def clean_testresults() -> None:
    if TESTRESULTS.is_symlink():
        raise RuntimeError(f"Refusing to clean symlinked results directory: {TESTRESULTS}")
    TESTRESULTS.mkdir(parents=True, exist_ok=True)
    for entry in list(TESTRESULTS.iterdir()):
        if entry.is_symlink() or entry.is_file():
            _remove_with_retry(entry)
        elif entry.is_dir():
            _rmtree_with_retry(entry)
        else:
            _remove_with_retry(entry)


def _rmtree_with_retry(path: Path, max_attempts: int = 5) -> None:
    for attempt in range(max_attempts):
        try:
            shutil.rmtree(path, onexc=_make_writable_and_retry)
            return
        except OSError:
            if attempt == max_attempts - 1:
                raise
            time.sleep(0.5)


def _remove_with_retry(path: Path, max_attempts: int = 5) -> None:
    for attempt in range(max_attempts):
        try:
            path.unlink()
            return
        except OSError:
            if attempt == max_attempts - 1:
                raise
            time.sleep(0.5)


def source_manifest() -> tuple[list[tuple[str, int]], list[str], int]:
    print("Generating source manifest...")
    files: list[tuple[str, int]] = []
    symlinks: list[str] = []
    total_size = 0
    scandir = scandir_rs.Scandir(
        str(SOURCE_DIR),
        return_type=scandir_rs.ReturnType.Ext,
        follow_links=False,
    )
    collected = scandir.collect()
    # scandir-rs 3.x returns a ScandirResults object, 2.x a (results, errors) tuple.
    results = collected.results if hasattr(collected, "results") else collected[0]
    for entry in results:
        relative = entry.path.replace("\\", "/")
        if entry.is_symlink:
            symlinks.append(relative)
        elif entry.is_file:
            size = entry.st_size
            files.append((relative, size))
            total_size += size
    return files, symlinks, total_size


def verify_extraction(
    expected: list[tuple[str, int]], symlinks: list[str], allow_collision_renames: bool
) -> int:
    extracted_root = TESTRESULTS / SOURCE_NAME
    if not extracted_root.is_dir():
        raise RuntimeError(f"Extraction did not produce the expected folder {SOURCE_NAME}")

    expected_files = dict(expected)
    actual_files: dict[str, int] = {}
    actual_dirs: set[str] = set()
    walk = scandir_rs.Walk(
        str(extracted_root),
        return_type=scandir_rs.ReturnType.Ext,
        follow_links=False,
    )
    for entry in walk:
        dir_path = entry[0]
        for name in entry[1]:
            path = extracted_root / dir_path / name
            if path.is_symlink():
                continue
            actual_dirs.add((Path(dir_path) / name).as_posix())
        for name in entry[2]:
            relative = (Path(dir_path) / name).as_posix()
            if relative in symlinks:
                continue
            path = extracted_root / dir_path / name
            if path.is_symlink():
                continue
            actual_files[relative] = path.stat().st_size

    # Windows reserved DOS device names (CON, PRN, AUX, NUL, COM1-9, LPT1-9)
    # cause some archivers to rename files like aux.c -> _aux.c on extraction.
    _WINDOWS_RESERVED = re.compile(
        r"(?:^|/)"
        r"(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])"
        r"(?=\.[^/]*$|$)",
        re.IGNORECASE,
    )

    matched_files: dict[str, int] = {}
    renamed_paths = 0
    unmapped_paths: list[str] = []
    for relative, size in actual_files.items():
        if relative in expected_files:
            matched_files[relative] = size
    for relative, size in actual_files.items():
        if relative in expected_files:
            continue
        target = ""
        if allow_collision_renames:
            parts = relative.split("/")
            pending = deque([parts])
            seen = {tuple(parts)}
            while pending:
                candidate_parts = pending.popleft()
                candidate = "/".join(candidate_parts)
                if candidate in expected_files:
                    if candidate not in matched_files:
                        target = candidate
                    else:
                        available = sorted(
                            path
                            for path in expected_files
                            if path.casefold() == candidate.casefold()
                            and path not in matched_files
                        )
                        if available:
                            target = available[0]
                    if target:
                        renamed_paths += 1
                        break
                for index, component in enumerate(candidate_parts):
                    restored = COLLISION_SUFFIX_RE.sub("", component)
                    if restored == component:
                        continue
                    variant = candidate_parts.copy()
                    variant[index] = restored
                    key = tuple(variant)
                    if key not in seen:
                        seen.add(key)
                        pending.append(variant)
        # Windows reserved-name renaming: if the extracted file has a leading
        # underscore where the expected file has a reserved DOS name, map it.
        if not target and os.name == "nt":
            for expected_path in expected_files:
                if expected_path not in matched_files:
                    expected_parts = expected_path.split("/")
                    actual_parts = relative.split("/")
                    if len(expected_parts) == len(actual_parts):
                        diffs = 0
                        match = True
                        for ep, ap in zip(expected_parts, actual_parts):
                            if ep == ap:
                                continue
                            reserved_match = _WINDOWS_RESERVED.match(ep)
                            if reserved_match and ap == "_" + ep:
                                diffs += 1
                            else:
                                match = False
                                break
                        if match and diffs == 1:
                            target = expected_path
                            renamed_paths += 1
                            break
        if target:
            if target in matched_files:
                raise RuntimeError(f"Multiple extracted files map to {target!r}")
            matched_files[target] = size
        else:
            unmapped_paths.append(relative)

    missing = sorted(expected_files.keys() - matched_files.keys())
    extra = sorted(unmapped_paths)
    changed = sorted(
        path
        for path in expected_files.keys() & matched_files.keys()
        if expected_files[path] != matched_files[path]
    )
    if missing or extra or changed:
        msg = ("Extracted files do not match the source tree: "
               f"{len(missing)} missing, {len(extra)} unexpected, {len(changed)} size mismatches; "
               f"examples: missing={missing[:3]}, extra={extra[:3]}, changed={changed[:3]}")
        if len(extra) > 1000:
            raise RuntimeError(msg)
        print(msg)
    return renamed_paths


def measure(action: Callable[[], None]) -> float:
    start = time.perf_counter()
    action()
    return time.perf_counter() - start


def archive_path(method: str, encrypted: bool) -> Path:
    suffixes = {
        "zarlib": ".zar",
        "zip": ".zip",
        "7z": ".7z",
        "tar": ".tar",
        "tar.gz": ".tar.gz",
    }
    name = f"{SOURCE_NAME}-{method}{'-encrypted' if encrypted else ''}{suffixes[method]}"
    if method in ("tar", "tar.gz") and encrypted:
        name += ".cms"
    return ARCHIVE_DIR / name


def build_zarc(cargo: str) -> Path:
    print("Building zarc CLI...")
    binary = ROOT / "target" / "release" / ("zarc.exe" if os.name == "nt" else "zarc")
    run_command([cargo, "build", "--release", "--bin", "zarc"], cwd=ROOT)
    if not binary.is_file():
        raise RuntimeError(f"Release CLI was not built at {binary}")
    return binary


def compress_zarlib(binary: Path, path: Path, encrypted: bool) -> None:
    command = [str(binary), "create"]
    if encrypted:
        command.extend(["--password", "--aead", "aes-256-gcm"])
    command.extend([str(path), SOURCE_NAME])
    if encrypted:
        run_with_password(command, TESTDATA, ENCRYPTED_PASSWORD)
    else:
        run_command(command, cwd=TESTDATA)


def compress_zip(zip_executable: str, path: Path, encrypted: bool) -> None:
    command = [zip_executable, "-q", "-r", "-6"]
    interactive = encrypted and os.name == "posix"
    if interactive:
        command.append("-e")
    elif encrypted:
        command.extend(["-P", ENCRYPTED_PASSWORD])
    command.extend([str(path), SOURCE_NAME])
    if interactive:
        run_with_password(command, TESTDATA, ENCRYPTED_PASSWORD)
    else:
        run_command(command, cwd=TESTDATA)


def compress_7z(seven_zip: str, path: Path, encrypted: bool, password: str) -> None:
    command = [seven_zip, "a", "-t7z", "-mx=5", "-bd", "-snl"]
    if encrypted:
        command.extend(["-mhe=on", f"-p{password}"])
    command.extend([str(path), SOURCE_NAME])
    run_command(command, cwd=TESTDATA)


def compress_tar(
    method: str,
    tar_executable: str,
    openssl: str,
    path: Path,
    encrypted: bool,
    certificate: Path | None,
    private_key: Path | None,
    temporary_directory: Path,
) -> None:
    compression = ["--gzip"] if method == "tar.gz" else []
    if not encrypted:
        run_command(
            [
                tar_executable,
                "--create",
                *compression,
                "--file",
                str(path),
                "--directory",
                str(TESTDATA),
                SOURCE_NAME,
            ]
        )
        return

    if certificate is None or private_key is None:
        raise RuntimeError("The temporary OpenSSL certificate was not initialized")
    compressed = temporary_directory / f"encrypted-source.{method}"
    run_command(
        [
            tar_executable,
            "--create",
            *compression,
            "--file",
            str(compressed),
            "--directory",
            str(TESTDATA),
            SOURCE_NAME,
        ]
    )
    run_command(
        [
            openssl,
            "cms",
            "-encrypt",
            "-binary",
            "-aes-256-gcm",
            "-in",
            str(compressed),
            "-out",
            str(path),
            "-outform",
            "DER",
            str(certificate),
        ]
    )


def extract_archive(
    method: str,
    encrypted: bool,
    path: Path,
    tools: dict[str, str],
    binary: Path,
    certificate: Path | None,
    private_key: Path | None,
    temporary_directory: Path,
    password: str,
    expected_link_warnings: int,
) -> None:
    if method == "zarlib":
        command = [str(binary), "extract"]
        if encrypted:
            command.append("--password")
        command.extend(["--collision", "rename"])
        command.extend([str(path), "--to", str(TESTRESULTS)])
        if encrypted:
            run_with_password(command, TESTDATA, ENCRYPTED_PASSWORD)
        else:
            run_command(command, cwd=TESTDATA)
    elif method == "zip":
        command = [tools["unzip"], "-q", "-o"]
        if encrypted and os.name != "posix":
            command.extend(["-P", ENCRYPTED_PASSWORD])
        command.extend([str(path), "-d", str(TESTRESULTS)])
        if encrypted and os.name == "posix":
            run_with_password(command, TESTDATA, ENCRYPTED_PASSWORD)
        else:
            run_command(command, cwd=TESTDATA)
    elif method == "7z":
        command = [tools["7z"], "x", "-bd", "-y", f"-o{TESTRESULTS}"]
        if encrypted:
            command.append(f"-p{password}")
        command.append(str(path))
        run_7z_extract(command, TESTDATA, expected_link_warnings)
    elif method in ("tar", "tar.gz"):
        compressed = path
        if encrypted:
            if certificate is None or private_key is None:
                raise RuntimeError("The temporary OpenSSL certificate was not initialized")
            compressed = temporary_directory / f"decrypted-source.{method}"
            run_command(
                [
                    tools["openssl"],
                    "cms",
                    "-decrypt",
                    "-binary",
                    "-in",
                    str(path),
                    "-inform",
                    "DER",
                    "-recip",
                    str(certificate),
                    "-inkey",
                    str(private_key),
                    "-out",
                    str(compressed),
                ]
            )
        run_command(
            [
                tools["tar"],
                "--extract",
                *(["--gzip"] if method == "tar.gz" else []),
                "--file",
                str(compressed),
                "--directory",
                str(TESTRESULTS),
                "--no-same-owner",
            ]
        )
    else:
        raise RuntimeError(f"Unknown archive format: {method}")


def svg_chart(
    results: list[dict[str, object]],
    source_bytes: int,
    hardware: BenchmarkHardware,
) -> str:
    width = 1120
    label_width = 250
    plot_width = 700
    row_height = 30
    bar_panel_height = 80 + row_height * len(results)
    top = 120
    # Time-series panels for CPU and RAM are taller: 200 px each
    ts_panel_height = 220
    height = top + bar_panel_height * 3 + ts_panel_height * 2 + 40
    multiple_caches = len({item["cache"] for item in results}) > 1
    colors = {
        "zarlib": "#2563eb",
        "zip": "#16a34a",
        "7z": "#9333ea",
        "tar": "#0891b2",
        "tar.gz": "#ea580c",
    }
    logical_cores = os.cpu_count() or "unknown"
    cpu_summary = (
        f"CPU: {hardware.cpu_model} ({platform.machine()}, "
        f"{logical_cores} logical cores)"
    )
    storage_summaries = [
        f"Storage ({item.location}): {item.media_type}; model: {item.model}; "
        f"device: {item.device}"
        for item in hardware.storage
    ]

    def chart_panel(title: str, key: str, unit: str, y: int, divisor: float = 1.0) -> str:
        values = [float(item[key]) / divisor for item in results]
        maximum = max(values) if values else 1.0
        maximum = max(maximum, 1.0)
        parts = [
            f'<text x="24" y="{y}" class="heading">{html.escape(title)}</text>',
        ]
        plot_x = label_width
        grid_top = y + 34
        grid_height = row_height * len(results)
        for tick in range(5):
            x = plot_x + int(plot_width * tick / 4)
            parts.append(
                f'<line x1="{x}" y1="{grid_top}" x2="{x}" y2="{grid_top + grid_height}" class="grid"/>'
            )
            parts.append(
                f'<text x="{x}" y="{grid_top - 8}" text-anchor="middle" class="axis">'
                f"{maximum * tick / 4:.1f}</text>"
            )
        for index, item in enumerate(results):
            row_y = grid_top + index * row_height
            method = str(item["method"])
            encrypted = bool(item["encrypted"])
            value = float(item[key]) / divisor
            bar_width = max(1, int(plot_width * value / maximum))
            color = colors[method]
            opacity = "0.60" if encrypted else "1.0"
            encryption = "encrypted" if encrypted else "plain"
            label = f"{method} - {encryption}"
            if multiple_caches:
                label += f" - {item['cache']}"
            parts.extend(
                [
                    f'<text x="{label_width - 12}" y="{row_y + 17}" '
                    f'text-anchor="end" class="label">{html.escape(label)}</text>',
                    f'<rect x="{plot_x}" y="{row_y + 3}" width="{bar_width}" '
                    f'height="18" rx="4" fill="{color}" opacity="{opacity}"/>',
                    f'<text x="{plot_x + bar_width + 8}" y="{row_y + 17}" '
                    f'class="value">{value:.2f} {unit}</text>',
                ]
            )
        return "\n".join(parts)

    def _resource_panel(
        title: str,
        resource_key: str,  # "compression_resources" or "extraction_resources"
        y: int,
    ) -> str:
        """Draw a time-series line chart for CPU % and RAM GiB."""
        parts = [
            f'<text x="24" y="{y}" class="heading">{html.escape(title)}</text>',
        ]
        plot_x = label_width
        chart_top = y + 34
        chart_height = 160
        chart_bottom = chart_top + chart_height

        # Collect all time-series data
        all_cpu: list[float] = []
        all_ram: list[float] = []
        max_time = 0.0
        for item in results:
            samples = item.get(resource_key, [])
            if samples:
                for t, cpu, ram in samples:
                    all_cpu.append(cpu)
                    all_ram.append(ram)
                    if t > max_time:
                        max_time = t
        if not all_cpu:
            parts.append(
                f'<text x="{plot_x}" y="{chart_top + chart_height // 2}" '
                f'class="axis">No resource data recorded</text>'
            )
            return "\n".join(parts)

        max_time = max(max_time, 1.0)
        cpu_max = max(max(all_cpu), 1.0)
        ram_max = max(max(all_ram), 0.1)

        # Y-axis labels (CPU left, RAM right)
        for tick in range(5):
            y_pos = chart_bottom - int(chart_height * tick / 4)
            parts.append(
                f'<line x1="{plot_x}" y1="{y_pos}" x2="{plot_x + plot_width}" '
                f'y2="{y_pos}" class="grid"/>'
            )
            # CPU label (left)
            parts.append(
                f'<text x="{plot_x - 8}" y="{y_pos + 4}" text-anchor="end" '
                f'class="axis">{cpu_max * tick / 4:.0f}</text>'
            )
            # RAM label (right)
            parts.append(
                f'<text x="{plot_x + plot_width + 8}" y="{y_pos + 4}" '
                f'text-anchor="start" class="axis">{ram_max * tick / 4:.1f}</text>'
            )

        # Axis labels
        parts.append(
            f'<text x="{plot_x - 8}" y="{chart_top - 14}" text-anchor="end" '
            f'class="axis" fill="#2563eb">CPU %</text>'
        )
        parts.append(
            f'<text x="{plot_x + plot_width + 8}" y="{chart_top - 14}" '
            f'text-anchor="start" class="axis" fill="#16a34a">RAM GiB</text>'
        )

        # X-axis time labels
        for tick in range(5):
            x = plot_x + int(plot_width * tick / 4)
            parts.append(
                f'<text x="{x}" y="{chart_bottom + 16}" text-anchor="middle" '
                f'class="axis">{max_time * tick / 4:.1f}s</text>'
            )

        # Draw one line per result
        cpu_opacity = "0.55"
        ram_opacity = "0.55"
        for item in results:
            samples = item.get(resource_key, [])
            if not samples:
                continue
            method = str(item["method"])
            color = colors.get(method, "#666")
            encrypted = bool(item["encrypted"])
            dash = "6,3" if encrypted else ""

            def _to_px(t: float, val: float, vmax: float) -> tuple[int, int]:
                x = plot_x + int(plot_width * t / max_time) if max_time > 0 else plot_x
                y = chart_bottom - int(chart_height * val / vmax)
                return x, y

            # CPU line
            cpu_pts = " ".join(
                f"{_to_px(t, cpu, cpu_max)[0]},{_to_px(t, cpu, cpu_max)[1]}"
                for t, cpu, _ram in samples
            )
            parts.append(
                f'<polyline points="{cpu_pts}" fill="none" stroke="{color}" '
                f'stroke-width="1.5" opacity="{cpu_opacity}" stroke-dasharray="{dash}"/>'
            )

            # RAM line
            ram_pts = " ".join(
                f"{_to_px(t, ram, ram_max)[0]},{_to_px(t, ram, ram_max)[1]}"
                for t, _cpu, ram in samples
            )
            parts.append(
                f'<polyline points="{ram_pts}" fill="none" stroke="{color}" '
                f'stroke-width="1.5" opacity="{ram_opacity}" '
                f'stroke-dasharray="{dash}" stroke-dashoffset="4"/>'
            )

        # Legend
        legend_x = plot_x
        legend_y = chart_bottom + 36
        parts.append(
            f'<text x="{legend_x}" y="{legend_y}" class="axis" font-weight="600">'
            f"Line colour = archive format; solid = plain, dashed = encrypted; "
            f"thick line = CPU %, thin dashed = RAM GiB</text>"
        )

        return "\n".join(parts)

    archive_panel = chart_panel("Archive size", "archive_bytes", "MiB", top, 1024 * 1024)
    compression_panel = chart_panel(
        "Compression time", "compression_seconds", "s", top + bar_panel_height
    )
    extraction_panel = chart_panel(
        "Extraction time", "extraction_seconds", "s", top + bar_panel_height * 2
    )
    cpu_ram_compress_y = top + bar_panel_height * 3 + 10
    cpu_ram_extract_y = cpu_ram_compress_y + ts_panel_height
    cpu_ram_compress = _resource_panel(
        "CPU & RAM during compression", "compression_resources", cpu_ram_compress_y
    )
    cpu_ram_extract = _resource_panel(
        "CPU & RAM during extraction", "extraction_resources", cpu_ram_extract_y
    )
    metadata = [
        '<text x="24" y="24" class="heading">Benchmark system</text>',
        f'<text x="24" y="46" class="metadata">'
        f'Operating system: {html.escape(hardware.operating_system)}</text>',
        f'<text x="24" y="64" class="metadata">{html.escape(cpu_summary)}</text>',
    ]
    metadata.extend(
        f'<text x="24" y="{82 + index * 18}" class="metadata">'
        f"{html.escape(summary)}</text>"
        for index, summary in enumerate(storage_summaries)
    )
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        f'viewBox="0 0 {width} {height}" role="img" aria-labelledby="title description">\n'
        f'<title id="title">{html.escape(OS_TAG)} archive benchmark charts</title>\n'
        f'<desc id="description">Archive size, compression time, extraction time, '
        f'CPU utilisation, and RAM usage '
        f'for archive formats with and without encryption, with hot and cold file caches where available. Source size: '
        f'{source_bytes / (1024 * 1024):.1f} MiB. Operating system: '
        f'{html.escape(hardware.operating_system)}. {html.escape(cpu_summary)}. '
        f'{html.escape("; ".join(storage_summaries))}</desc>\n'
        """<style>
  text { font-family: system-ui, sans-serif; fill: #172033; }
  .heading { font-size: 19px; font-weight: 700; }
  .label { font-size: 13px; }
  .metadata { font-size: 12px; fill: #475569; }
  .axis { font-size: 11px; fill: #64748b; }
  .value { font-size: 12px; font-weight: 600; }
  .grid { stroke: #dbe3ee; stroke-width: 1; }
</style>
<rect width="100%" height="100%" fill="#ffffff"/>
"""
        + "\n".join(metadata)
        + "\n"
        + archive_panel
        + "\n"
        + compression_panel
        + "\n"
        + extraction_panel
        + "\n"
        + cpu_ram_compress
        + "\n"
        + cpu_ram_extract
        + "\n</svg>\n"
    )


def write_report(
    results: list[dict[str, object]],
    source_bytes: int,
    source_files: int,
    symlink_count: int,
    tools: dict[str, str],
    notes: list[str],
) -> None:
    hardware = collect_benchmark_hardware()
    CHART_PATH.write_text(
        svg_chart(results, source_bytes, hardware), encoding="utf-8"
    )
    timestamp = dt.datetime.now(dt.timezone.utc).astimezone().isoformat(timespec="seconds")
    logical_cores = os.cpu_count() or "unknown"
    storage_lines = "\n".join(
        f"- Benchmark storage ({storage.location}): {storage.media_type}; "
        f"model: {storage.model}; device: {storage.device}"
        for storage in hardware.storage
    )

    def _avg_resource(key: str, item: dict[str, object]) -> tuple[float, float]:
        samples = item.get(key, [])
        if not samples:
            return (0.0, 0.0)
        cpu_vals = [s[1] for s in samples]
        ram_vals = [s[2] for s in samples]
        return (sum(cpu_vals) / len(cpu_vals), sum(ram_vals) / len(ram_vals))

    table_rows = []
    for item in results:
        encryption = "Yes" if item["encrypted"] else "No"
        size_mib = int(item["archive_bytes"]) / (1024 * 1024)
        compression_speed = source_bytes / 1024 / 1024 / float(item["compression_seconds"])
        extraction_speed = source_bytes / 1024 / 1024 / float(item["extraction_seconds"])
        avg_cpu_c, avg_ram_c = _avg_resource("compression_resources", item)
        avg_cpu_e, avg_ram_e = _avg_resource("extraction_resources", item)
        table_rows.append(
            "| {method} | {encrypted} | {cache} | {size:.1f} MiB | {compress:.2f} s "
            "({compress_speed:.1f} MiB/s) | {extract:.2f} s ({extract_speed:.1f} MiB/s) |"
            " {cpu_c:.0f} % / {ram_c:.1f} GiB | {cpu_e:.0f} % / {ram_e:.1f} GiB |".format(
                method=item["method"],
                cache=item["cache"],
                encrypted=encryption,
                size=size_mib,
                compress=float(item["compression_seconds"]),
                compress_speed=compression_speed,
                extract=float(item["extraction_seconds"]),
                extract_speed=extraction_speed,
                cpu_c=avg_cpu_c,
                ram_c=avg_ram_c,
                cpu_e=avg_cpu_e,
                ram_e=avg_ram_e,
            )
        )
    tool_lines = "\n".join(f"- `{name}`: `{version}`" for name, version in tools.items())
    zarlib_renames = [
        int(item.get("renamed_paths", 0))
        for item in results
        if item["method"] == "zarlib"
    ]
    collision_note = (
        f"During its {len(zarlib_renames)} extraction run(s), `zarlib` renamed "
        f"{sum(zarlib_renames)} path(s) total"
    )
    if zarlib_renames and len(set(zarlib_renames)) == 1:
        collision_note += f" ({zarlib_renames[0]} per run)"
    collision_note += " due to case-folded collisions; generated suffixes are normalized only for comparison."
    REPORT_PATH.write_text(
        f"""# Linux 4.0.1 Archive Benchmark

Run date: {timestamp}  
- Operating system: {hardware.operating_system}
- CPU: {hardware.cpu_model} ({platform.machine()}, {logical_cores} logical cores)
{storage_lines}

![Host OS, CPU and storage information, archive size, compression time, extraction time, CPU, and RAM charts](./{CHART_PATH.name})

## Results

Source: [`linux-4.0.1.tar.gz`](https://cdn.kernel.org/pub/linux/kernel/v4.x/linux-4.0.1.tar.gz)  
Uncompressed source size: {source_bytes / (1024 * 1024):.1f} MiB  
Regular files verified: {source_files:,}  
Symbolic links excluded from path/size verification: {symlink_count}

| Format | Encryption | Cache | Archive size | Compression | Extraction | CPU / RAM (compress) | CPU / RAM (extract) |
|---|---:|---:|---:|---:|---:|---:|---:|
{chr(10).join(table_rows)}

Throughput uses the uncompressed regular-file size. Each extraction was run into
a freshly emptied `testresults/` directory, then checked against the source tree
for matching regular-file paths and sizes. {collision_note} The results
directory was emptied again after each check. Symlink entries are excluded
because `zarlib` currently reports and skips unsupported input symlinks. ZIP
and tar preserve links; 7z preserves them in its archive but may skip unsafe
relative links during extraction, as noted above.

CPU utilisation and RAM consumption were sampled at 1-second intervals during
each compression and extraction operation using `psutil`. The table shows the
average CPU % and RAM GiB across all samples for each phase. The time-series
charts below the bar charts plot every sample, with one line per archive format
(solid = plain, dashed = encrypted; thick line = CPU %, thin dashed line = RAM GiB).
The CPU model and operating system are collected from the host. Storage details
describe the volumes holding the source/archive files and extraction output.
The operating system may not expose physical media behind virtual or layered
storage; such cases are identified as unknown or non-rotational/possibly virtual.

## Method

- **Cache state**: `hot` runs read every file of the source tree (and, before
  extraction, the archive) once before the measured operation. `cold` runs drop
  the operating system file cache before every measured operation: `sync` plus
  writing `3` to `/proc/sys/vm/drop_caches` on Linux (root required), or
  `RAMMap.exe -Et` on Windows (elevation required). Cold runs are skipped with
  a warning when the privileges are missing.
{chr(10).join(notes)}
- `zarlib`: `zarc` release build, default Zstandard compression level 3;
  encrypted runs use AES-256-GCM and the default header-and-data scope.
- `zip`: Info-ZIP recursive DEFLATE at level 6, storing symlinks as links.
  Encrypted runs use the implementation's standard ZipCrypto mode
  (legacy/weak encryption).
- `7z`: 7z format with LZMA2 at level 5. Encrypted runs use AES-256 and
  encrypted headers; symlinks are stored as links. The 7-Zip CLI accepts
  passwords only as a command-line switch; this runner generates a temporary
  random password and never saves it. During extraction, 7-Zip rejects
  relative symlink targets containing parent components; the runner
  accepts only the expected warnings for those source links and still
  verifies every regular file.
- `tar`: GNU tar without compression (a pure archiving baseline). The
  encrypted run times tar plus OpenSSL CMS AES-256-GCM encryption and is
  decrypted before extraction, like `tar.gz`.
- `tar.gz`: GNU tar with gzip's default compression level. Since tar.gz has no
  built-in encryption, the encrypted run times tar/gzip plus OpenSSL CMS
  AES-256-GCM encryption. It is decrypted before extraction. A temporary
  self-signed RSA certificate is used to wrap the CMS content key; the
  temporary private key is deleted when the benchmark exits.
- Compression and extraction are each measured once per format/configuration.
  Build time, source download/extraction, and post-extraction verification are
  excluded from the measured operation times.

## Tool versions

{tool_lines}

The generated archives and temporary key material are removed after the run.
The source archive and extracted source tree remain in `testdata/`.
""",
        encoding="utf-8",
    )


def run_benchmarks(
    tools: dict[str, str], cargo: str, mode: str = "both", accept_eula: bool = False
) -> None:
    caches, rammap = resolve_cache_modes(mode, accept_eula)
    TESTRESULTS.mkdir(parents=True, exist_ok=True)
    ensure_source(tools["tar"])
    binary = build_zarc(cargo)
    expected, symlinks, source_bytes = source_manifest()
    if not expected or not source_bytes:
        raise RuntimeError(f"No regular source files were found in {SOURCE_DIR}")
    print(
        f"Benchmarking {len(expected):,} files ({source_bytes / (1024 * 1024):.1f} MiB)"
    )

    methods = tuple(
        method
        for method in ("zarlib", "zip", "7z", "tar", "tar.gz")
        if method != "zip" or ("zip" in tools and "unzip" in tools)
    )
    notes: list[str] = []
    # zarc only reads passwords from a POSIX terminal.
    skipped = {("zarlib", True)} if os.name != "posix" else set()
    if skipped:
        notes.append(
            "- `zarlib` encrypted runs are skipped on this platform because `zarc` "
            "can only prompt for passwords on POSIX terminals."
        )
    if os.name != "posix":
        notes.append(
            "- On this platform `zip`/`unzip` receive the password with `-P` "
            "instead of an interactive prompt."
        )
    has_openssl = "openssl" in tools
    if not has_openssl:
        notes.append(
            "- `openssl` was not found on PATH; encrypted tar/tar.gz benchmarks "
            "are skipped."
        )
        # Also skip encrypted zarlib on non-POSIX (already handled above).
        # Skip encrypted tar/tar.gz too.
        for method in ("tar", "tar.gz"):
            skipped.add((method, True))
    results: list[dict[str, object]] = []
    versions = tool_versions(tools)
    if ARCHIVE_DIR.exists():
        print(f"Remove existing benchmark archive directory: {ARCHIVE_DIR}")
        shutil.rmtree(ARCHIVE_DIR)
    if ARCHIVE_DIR.exists():
        raise RuntimeError(f"Refusing to overwrite an existing benchmark archive directory: {ARCHIVE_DIR}")
    ARCHIVE_DIR.mkdir()
    try:
        with tempfile.TemporaryDirectory(prefix="zar-benchmark-", dir=TESTDATA) as temporary:
            temporary_directory = Path(temporary)
            certificate: Path | None = None
            private_key: Path | None = None
            if has_openssl:
                certificate = temporary_directory / "benchmark-cert.pem"
                private_key = temporary_directory / "benchmark-key.pem"
                run_command(
                    [
                        tools["openssl"],
                        "req",
                        "-x509",
                        "-newkey",
                        "rsa:2048",
                        "-keyout",
                        str(private_key),
                        "-out",
                        str(certificate),
                        "-days",
                        "1",
                        "-nodes",
                        "-subj",
                        "/CN=zar-benchmark-temporary",
                    ]
                )

            for cache, method in ((c, m) for c in caches for m in methods):
                runs_per_test = 2 if cache == "hot" else 1
                for encrypted in (False, True):
                    if (method, encrypted) in skipped:
                        continue
                    path = archive_path(method, encrypted)
                    path.unlink(missing_ok=True)
                    print(f"Compressing {method} (encrypted={encrypted}, {cache} cache)...")

                    def prepare_cache(*warm: Path) -> None:
                        if cache == "cold":
                            drop_caches(rammap, accept_eula)
                        else:
                            for target in warm:
                                read_files(target)

                    def compress() -> None:
                        if method == "zarlib":
                            compress_zarlib(binary, path, encrypted)
                        elif method == "zip":
                            compress_zip(tools["zip"], path, encrypted)
                        elif method == "7z":
                            compress_7z(
                                tools["7z"], path, encrypted, ENCRYPTED_PASSWORD
                            )
                        else:
                            compress_tar(
                                method,
                                tools["tar"],
                                tools["openssl"],
                                path,
                                encrypted,
                                certificate,
                                private_key,
                                temporary_directory,
                            )

                    compression_times: list[float] = []
                    extraction_times: list[float] = []
                    archive_bytes = 0
                    renamed_paths = 0
                    compression_resources: list[list[tuple[float, float, float]]] = []
                    extraction_resources: list[list[tuple[float, float, float]]] = []

                    monitor = ResourceMonitor()

                    for run in range(runs_per_test):
                        label = f" (run {run + 1}/{runs_per_test})" if runs_per_test > 1 else ""
                        print(
                            f"Compressing {method} (encrypted={encrypted}, "
                            f"{cache} cache){label}..."
                        )
                        path.unlink(missing_ok=True)
                        prepare_cache(SOURCE_DIR)
                        monitor.start()
                        compression_seconds = measure(compress)
                        monitor.stop()
                        compression_times.append(compression_seconds)
                        compression_resources.append(monitor.samples)
                        archive_bytes = path.stat().st_size

                        def _fmt_duration(seconds: float) -> str:
                            if seconds < 1:
                                return f"{seconds * 1000:.0f} ms"
                            if seconds < 60:
                                return f"{seconds:.2f} s"
                            return f"{seconds / 60:.1f} min"

                        print(
                            f"  Compressed: {archive_bytes / (1024 * 1024):.1f} MiB "
                            f"in {_fmt_duration(compression_seconds)}"
                        )
                        print(
                            f"Extracting {method} (encrypted={encrypted}){label}; "
                            f"archive is {archive_bytes / (1024 * 1024):.1f} MiB..."
                        )
                        clean_testresults()
                        prepare_cache(path)
                        try:
                            monitor.start()
                            extraction_seconds = measure(
                                lambda: extract_archive(
                                    method,
                                    encrypted,
                                    path,
                                    tools,
                                    binary,
                                    certificate,
                                    private_key,
                                    temporary_directory,
                                    ENCRYPTED_PASSWORD,
                                    len(symlinks),
                                )
                            )
                            monitor.stop()
                            extraction_times.append(extraction_seconds)
                            extraction_resources.append(monitor.samples)
                            print(
                                f"  Extracted in {_fmt_duration(extraction_seconds)}"
                            )
                            allow_renames = method in ("zarlib", "7z")
                            renamed_paths = verify_extraction(
                                expected, symlinks, allow_renames
                            )
                        finally:
                            clean_testresults()

                    avg_compression = sum(compression_times) / len(compression_times)
                    avg_extraction = sum(extraction_times) / len(extraction_times)
                    if runs_per_test > 1:
                        print(
                            f"  {method} (encrypted={encrypted}): "
                            f"compress {avg_compression:.2f}s avg "
                            f"({', '.join(f'{t:.2f}s' for t in compression_times)}), "
                            f"extract {avg_extraction:.2f}s avg "
                            f"({', '.join(f'{t:.2f}s' for t in extraction_times)})"
                        )

                    # Merge resource samples across runs: average CPU and RAM at each
                    # fractional-second offset (interpolated to a common 1 s grid).
                    def _merge_resource_samples(
                        run_samples: list[list[tuple[float, float, float]]],
                    ) -> list[tuple[float, float, float]]:
                        if not run_samples:
                            return []
                        max_len = max(len(s) for s in run_samples)
                        merged: list[tuple[float, float, float]] = []
                        for i in range(max_len):
                            cpu_sum = 0.0
                            ram_sum = 0.0
                            count = 0
                            for samples in run_samples:
                                if i < len(samples):
                                    cpu_sum += samples[i][1]
                                    ram_sum += samples[i][2]
                                    count += 1
                            merged.append(
                                (float(i), cpu_sum / count, ram_sum / count)
                            )
                        return merged

                    results.append(
                        {
                            "method": method,
                            "cache": cache,
                            "encrypted": encrypted,
                            "archive_bytes": archive_bytes,
                            "compression_seconds": avg_compression,
                            "extraction_seconds": avg_extraction,
                            "renamed_paths": renamed_paths,
                            "compression_resources": _merge_resource_samples(compression_resources),
                            "extraction_resources": _merge_resource_samples(extraction_resources),
                        }
                    )
                    path.unlink(missing_ok=True)

        write_report(
            results,
            source_bytes,
            len(expected),
            len(symlinks),
            versions,
            notes,
        )
        print(f"Report written to {REPORT_PATH}")
        print(f"Chart written to {CHART_PATH}")
    finally:
        clean_testresults()
        for method in methods:
            for encrypted in (False, True):
                archive_path(method, encrypted).unlink(missing_ok=True)
        try:
            ARCHIVE_DIR.rmdir()
        except OSError:
            pass


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo", default="cargo", help="cargo executable (default: cargo)")
    parser.add_argument(
        "--mode",
        choices=("hot", "cold", "both"),
        default="both",
        help="file cache state to benchmark (default: both; cold needs root/elevation)",
    )
    parser.add_argument(
        "--accept-sysinternals-eula",
        action="store_true",
        help="Windows: pass -accepteula to RAMMap, accepting the Sysinternals license",
    )
    args = parser.parse_args()
    try:
        tools = require_tools()
        run_benchmarks(tools, args.cargo, args.mode, args.accept_sysinternals_eula)
    except (OSError, RuntimeError, TimeoutError, tarfile.TarError) as error:
        print(f"benchmark: {error}", file=sys.stderr)
        return 1
    return 0


def tool_versions(tools: dict[str, str]) -> dict[str, str]:
    arguments = {
        "cargo": ["--version"],
        "zip": ["-v"],
        "unzip": ["-v"],
        "7z": [],
        "tar": ["--version"],
        "openssl": ["version"],
    }
    versions: dict[str, str] = {}
    for name, executable in tools.items():
        result = subprocess.run(
            [executable, *arguments[name]],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        if result.returncode:
            raise RuntimeError(f"Unable to read version from {name}")
        lines = result.stdout.decode("utf-8", errors="replace").splitlines()
        if name == "zip":
            version = next((line.strip() for line in lines if line.startswith("This is Zip")), "")
        else:
            version = next((line.strip() for line in lines if line.strip()), "")
        versions[name] = version or "version unavailable"
    return versions


if __name__ == "__main__":
    raise SystemExit(main())
