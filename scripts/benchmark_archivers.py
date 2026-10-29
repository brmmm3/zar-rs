#!/usr/bin/env python3
"""Benchmark zarlib, zip, 7z, and tar.gz using the Linux 4.0.1 source tree."""

from __future__ import annotations

import argparse
import datetime as dt
import fcntl
from collections import deque
import html
import os
import pty
import re
import secrets
import select
import shutil
import subprocess
import sys
import tarfile
import termios
import tempfile
import time
import urllib.request
from pathlib import Path, PurePosixPath
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
TESTDATA = ROOT / "testdata"
TESTRESULTS = ROOT / "testresults"
SOURCE_NAME = "linux-4.0.1"
SOURCE_ARCHIVE = TESTDATA / f"{SOURCE_NAME}.tar.gz"
SOURCE_DIR = TESTDATA / SOURCE_NAME
ARCHIVE_DIR = TESTDATA / "benchmark-archives"
REPORT_PATH = ROOT / "doc" / "benchmark-report.md"
CHART_PATH = ROOT / "doc" / "benchmark-charts.svg"
SOURCE_URL = "https://cdn.kernel.org/pub/linux/kernel/v4.x/linux-4.0.1.tar.gz"
ENCRYPTED_PASSWORD = secrets.token_urlsafe(24)
PROMPT_RE = re.compile(rb"(?im)^[^\r\n]*(?:password|passphrase)[^\r\n]*:\s*$")
COLLISION_SUFFIX_RE = re.compile(r" \(\d+\)(?=\.[^.]+$|$)")


def require_tools() -> dict[str, str]:
    names = ("cargo", "zip", "unzip", "7z", "tar", "openssl")
    missing = [name for name in names if shutil.which(name) is None]
    if missing:
        raise RuntimeError(f"Required benchmark tools are unavailable: {', '.join(missing)}")
    return {name: shutil.which(name) or name for name in names}


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
    with tarfile.open(tarball, "r:gz") as archive:
        for member in archive:
            path = TESTDATA.joinpath(*PurePosixPath(member.name).parts)
            if member.isdir():
                if path.is_symlink() or not path.is_dir():
                    return False
            elif member.issym():
                if not path.is_symlink() or os.readlink(path) != member.linkname:
                    return False
            elif member.islnk():
                if path.is_symlink() or not path.is_file():
                    return False
            elif not path.is_file() or path.is_symlink() or path.stat().st_size != member.size:
                return False
    return True


def ensure_source(tar_executable: str) -> None:
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
                "--overwrite",
            ]
        )
    if not SOURCE_DIR.is_dir():
        raise RuntimeError(f"Source extraction did not create {SOURCE_DIR}")
    if not source_matches_tarball(SOURCE_ARCHIVE):
        raise RuntimeError(f"Source folder does not match {SOURCE_ARCHIVE.name}")


def clean_testresults() -> None:
    if TESTRESULTS.is_symlink():
        raise RuntimeError(f"Refusing to clean symlinked results directory: {TESTRESULTS}")
    TESTRESULTS.mkdir(parents=True, exist_ok=True)
    for entry in TESTRESULTS.iterdir():
        if entry.is_symlink() or entry.is_file():
            entry.unlink()
        elif entry.is_dir():
            shutil.rmtree(entry)
        else:
            entry.unlink()


def source_manifest() -> tuple[list[tuple[str, int]], list[str], int]:
    files: list[tuple[str, int]] = []
    symlinks: list[str] = []
    total_size = 0
    for directory, dirnames, filenames in os.walk(SOURCE_DIR, followlinks=False):
        base = Path(directory)
        kept_directories: list[str] = []
        for name in dirnames:
            path = base / name
            relative = path.relative_to(SOURCE_DIR).as_posix()
            if path.is_symlink():
                symlinks.append(relative)
            else:
                kept_directories.append(name)
        dirnames[:] = kept_directories

        for name in filenames:
            path = base / name
            relative = path.relative_to(SOURCE_DIR).as_posix()
            if path.is_symlink():
                symlinks.append(relative)
                continue
            size = path.stat().st_size
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
    for directory, dirnames, filenames in os.walk(extracted_root, followlinks=False):
        base = Path(directory)
        kept_directories: list[str] = []
        for name in dirnames:
            path = base / name
            if path.is_symlink():
                continue
            kept_directories.append(name)
            actual_dirs.add(path.relative_to(extracted_root).as_posix())
        dirnames[:] = kept_directories

        for name in filenames:
            path = base / name
            relative = path.relative_to(extracted_root).as_posix()
            if path.is_symlink() or relative in symlinks:
                continue
            actual_files[relative] = path.stat().st_size

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
        raise RuntimeError(
            "Extracted files do not match the source tree: "
            f"{len(missing)} missing, {len(extra)} unexpected, {len(changed)} size mismatches; "
            f"examples: missing={missing[:3]}, extra={extra[:3]}, changed={changed[:3]}"
        )
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
        "tar.gz": ".tar.gz",
    }
    name = f"{SOURCE_NAME}-{method}{'-encrypted' if encrypted else ''}{suffixes[method]}"
    if method == "tar.gz" and encrypted:
        name += ".cms"
    return ARCHIVE_DIR / name


def build_zarc(cargo: str) -> Path:
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
    command = [zip_executable, "-q", "-r", "-6", "-y"]
    if encrypted:
        command.append("-e")
    command.extend([str(path), SOURCE_NAME])
    if encrypted:
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
    tar_executable: str,
    openssl: str,
    path: Path,
    encrypted: bool,
    certificate: Path | None,
    private_key: Path | None,
    temporary_directory: Path,
) -> None:
    if not encrypted:
        run_command(
            [
                tar_executable,
                "--create",
                "--gzip",
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
    compressed = temporary_directory / "encrypted-source.tar.gz"
    run_command(
        [
            tar_executable,
            "--create",
            "--gzip",
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
        command = [tools["unzip"], "-q", "-o", str(path), "-d", str(TESTRESULTS)]
        if encrypted:
            run_with_password(command, TESTDATA, ENCRYPTED_PASSWORD)
        else:
            run_command(command, cwd=TESTDATA)
    elif method == "7z":
        command = [tools["7z"], "x", "-bd", "-y", f"-o{TESTRESULTS}"]
        if encrypted:
            command.append(f"-p{password}")
        command.append(str(path))
        run_7z_extract(command, TESTDATA, expected_link_warnings)
    elif method == "tar.gz":
        compressed = path
        if encrypted:
            if certificate is None or private_key is None:
                raise RuntimeError("The temporary OpenSSL certificate was not initialized")
            compressed = temporary_directory / "decrypted-source.tar.gz"
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
                "--gzip",
                "--file",
                str(compressed),
                "--directory",
                str(TESTRESULTS),
                "--no-same-owner",
            ]
        )
    else:
        raise RuntimeError(f"Unknown archive format: {method}")


def svg_chart(results: list[dict[str, object]], source_bytes: int) -> str:
    width = 1120
    label_width = 250
    plot_width = 700
    row_height = 30
    panel_height = 285
    top = 40
    height = top + panel_height * 3 + 30
    colors = {
        "zarlib": "#2563eb",
        "zip": "#16a34a",
        "7z": "#9333ea",
        "tar.gz": "#ea580c",
    }

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

    archive_panel = chart_panel("Archive size", "archive_bytes", "MiB", top, 1024 * 1024)
    compression_panel = chart_panel(
        "Compression time", "compression_seconds", "s", top + panel_height
    )
    extraction_panel = chart_panel(
        "Extraction time", "extraction_seconds", "s", top + panel_height * 2
    )
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        f'viewBox="0 0 {width} {height}" role="img" aria-labelledby="title description">\n'
        f'<title id="title">Linux 4.0.1 archive benchmark charts</title>\n'
        f'<desc id="description">Archive size, compression time, and extraction time '
        f'for four archive formats with and without encryption. Source size: '
        f'{source_bytes / (1024 * 1024):.1f} MiB.</desc>\n'
        """<style>
  text { font-family: system-ui, sans-serif; fill: #172033; }
  .heading { font-size: 19px; font-weight: 700; }
  .label { font-size: 13px; }
  .axis { font-size: 11px; fill: #64748b; }
  .value { font-size: 12px; font-weight: 600; }
  .grid { stroke: #dbe3ee; stroke-width: 1; }
</style>
<rect width="100%" height="100%" fill="#ffffff"/>
"""
        + archive_panel
        + "\n"
        + compression_panel
        + "\n"
        + extraction_panel
        + "\n</svg>\n"
    )


def write_report(
    results: list[dict[str, object]],
    source_bytes: int,
    source_files: int,
    symlink_count: int,
    tools: dict[str, str],
) -> None:
    CHART_PATH.write_text(svg_chart(results, source_bytes), encoding="utf-8")
    timestamp = dt.datetime.now(dt.timezone.utc).astimezone().isoformat(timespec="seconds")
    table_rows = []
    for item in results:
        encryption = "Yes" if item["encrypted"] else "No"
        size_mib = int(item["archive_bytes"]) / (1024 * 1024)
        compression_speed = source_bytes / 1024 / 1024 / float(item["compression_seconds"])
        extraction_speed = source_bytes / 1024 / 1024 / float(item["extraction_seconds"])
        table_rows.append(
            "| {method} | {encrypted} | {size:.1f} MiB | {compress:.2f} s "
            "({compress_speed:.1f} MiB/s) | {extract:.2f} s ({extract_speed:.1f} MiB/s) |".format(
                method=item["method"],
                encrypted=encryption,
                size=size_mib,
                compress=float(item["compression_seconds"]),
                compress_speed=compression_speed,
                extract=float(item["extraction_seconds"]),
                extract_speed=extraction_speed,
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

![Archive size, compression time, and extraction time charts](./benchmark-charts.svg)

## Results

Source: [`linux-4.0.1.tar.gz`](https://cdn.kernel.org/pub/linux/kernel/v4.x/linux-4.0.1.tar.gz)  
Uncompressed source size: {source_bytes / (1024 * 1024):.1f} MiB  
Regular files verified: {source_files:,}  
Symbolic links excluded from path/size verification: {symlink_count}

| Format | Encryption | Archive size | Compression | Extraction |
|---|---:|---:|---:|---:|
{chr(10).join(table_rows)}

Throughput uses the uncompressed regular-file size. Each extraction was run into
a freshly emptied `testresults/` directory, then checked against the source tree
for matching regular-file paths and sizes. {collision_note} The results
directory was emptied again after each check. Symlink entries are excluded
because `zarlib` currently reports and skips unsupported input symlinks. ZIP
and tar preserve links; 7z preserves them in its archive but may skip unsafe
relative links during extraction, as noted above.

## Method

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


def run_benchmarks(tools: dict[str, str], cargo: str) -> None:
    if os.name != "posix":
        raise RuntimeError("Password prompting automation currently requires a Linux/POSIX terminal")
    TESTRESULTS.mkdir(parents=True, exist_ok=True)
    ensure_source(tools["tar"])
    binary = build_zarc(cargo)
    expected, symlinks, source_bytes = source_manifest()
    if not expected or not source_bytes:
        raise RuntimeError(f"No regular source files were found in {SOURCE_DIR}")
    print(
        f"Benchmarking {len(expected):,} files ({source_bytes / (1024 * 1024):.1f} MiB)"
    )

    methods = ("zarlib", "zip", "7z", "tar.gz")
    results: list[dict[str, object]] = []
    versions = tool_versions(tools)
    if ARCHIVE_DIR.exists():
        raise RuntimeError(
            f"Refusing to overwrite an existing benchmark archive directory: {ARCHIVE_DIR}"
        )
    ARCHIVE_DIR.mkdir()
    try:
        with tempfile.TemporaryDirectory(prefix="zar-benchmark-", dir=TESTDATA) as temporary:
            temporary_directory = Path(temporary)
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

            for method in methods:
                for encrypted in (False, True):
                    path = archive_path(method, encrypted)
                    path.unlink(missing_ok=True)
                    print(f"Compressing {method} (encrypted={encrypted})...")

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
                                tools["tar"],
                                tools["openssl"],
                                path,
                                encrypted,
                                certificate,
                                private_key,
                                temporary_directory,
                            )

                    compression_seconds = measure(compress)
                    archive_bytes = path.stat().st_size
                    print(
                        f"Extracting {method} (encrypted={encrypted}); "
                        f"archive is {archive_bytes / (1024 * 1024):.1f} MiB..."
                    )
                    clean_testresults()
                    try:
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
                        renamed_paths = verify_extraction(
                            expected, symlinks, method == "zarlib"
                        )
                    finally:
                        clean_testresults()

                    results.append(
                        {
                            "method": method,
                            "encrypted": encrypted,
                            "archive_bytes": archive_bytes,
                            "compression_seconds": compression_seconds,
                            "extraction_seconds": extraction_seconds,
                            "renamed_paths": renamed_paths,
                        }
                    )
                    path.unlink(missing_ok=True)

        write_report(
            results,
            source_bytes,
            len(expected),
            len(symlinks),
            versions,
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
    args = parser.parse_args()
    try:
        tools = require_tools()
        run_benchmarks(tools, args.cargo)
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
