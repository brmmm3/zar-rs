#!/usr/bin/env python3
"""Test ZAR archive and tree streaming over a 100 Mbit/s Linux loopback."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import select
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SAMPLE_INTERVAL = 0.5
MIB = 1024 * 1024


def run(command: list[str]) -> None:
    subprocess.run(command, cwd=ROOT, check=True)


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(MIB), b""):
            hasher.update(block)
    return hasher.hexdigest()


def tree_inventory(root: Path) -> tuple[dict[str, tuple[str, str]], int]:
    """Record directories, symlink targets and regular file hashes."""
    if not root.is_dir():
        raise ValueError(f"Extracted tree does not exist: {root}")
    inventory: dict[str, tuple[str, str]] = {}
    total = 0
    for parent, directories, files in os.walk(root, followlinks=False):
        for name in directories + files:
            path = Path(parent) / name
            relative = path.relative_to(root).as_posix()
            if path.is_symlink():
                inventory[relative] = ("symlink", os.readlink(path))
            elif path.is_dir():
                inventory[relative] = ("directory", "")
            elif path.is_file():
                total += path.stat().st_size
                inventory[relative] = ("file", digest(path))
            else:
                raise ValueError(f"Unsupported special file in extracted tree: {path}")
    return inventory, total


def process_stats(process: subprocess.Popen[bytes]) -> tuple[float, int] | None:
    """Return accumulated CPU seconds and RSS bytes for a Linux process."""
    try:
        stat = Path(f"/proc/{process.pid}/stat").read_text()
    except FileNotFoundError:
        if process.poll() is not None:
            return None
        raise
    fields = stat[stat.rfind(")") + 2 :].split()
    ticks = os.sysconf("SC_CLK_TCK")
    pages = os.sysconf("SC_PAGE_SIZE")
    return (int(fields[11]) + int(fields[12])) / ticks, int(fields[21]) * pages


def server_address(server: subprocess.Popen[bytes]) -> str:
    assert server.stderr is not None
    ready, _, _ = select.select([server.stderr], [], [], 30)
    if not ready:
        raise RuntimeError("Streaming server did not become ready within 30 seconds")
    line = server.stderr.readline().decode("utf-8", errors="replace").strip()
    prefix = "Listening on "
    if not line.startswith(prefix):
        raise RuntimeError(f"Streaming server failed to start: {line or 'no output'}")
    address = line[len(prefix) :]
    if not address.startswith("127.0.0.1:"):
        raise RuntimeError(f"Unexpected streaming server address: {address}")
    return address


def stop_process(process: subprocess.Popen[bytes] | None) -> None:
    if process is not None and process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


def run_case(
    mode: str,
    source: Path,
    temp: Path,
    server_binary: Path,
    client_binary: Path,
    reference: dict[str, tuple[str, str]],
    content_bytes: int,
    timeout: float,
) -> None:
    destination = temp / ("received.zar" if mode == "archive" else "received-tree")
    client_log = temp / f"{mode}.stderr"
    server: subprocess.Popen[bytes] | None = None
    client: subprocess.Popen[bytes] | None = None
    try:
        server = subprocess.Popen(
            [str(server_binary), "127.0.0.1:0", str(source)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
        )
        address = server_address(server)
        with client_log.open("wb") as errors:
            started = time.monotonic()
            client = subprocess.Popen(
                [str(client_binary), address, mode, str(destination)],
                stdout=subprocess.DEVNULL,
                stderr=errors,
            )
            processes = (server, client)
            previous = [process_stats(process) for process in processes]
            last_sample = started
            next_sample = started + SAMPLE_INTERVAL
            cpu_peaks = [0.0, 0.0]
            rss_peaks = [0, 0]
            combined_rss_peak = 0
            samples = 0
            print(f"\nTest: ZAR to {mode}", flush=True)
            print(
                "  time(s)  server CPU%  server MiB  client CPU%  client MiB",
                flush=True,
            )
            while True:
                now = time.monotonic()
                statuses = [
                    os.waitid(
                        os.P_PID, process.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT
                    )
                    for process in processes
                ]
                if any(
                    status is not None
                    and (status.si_code != os.CLD_EXITED or status.si_status != 0)
                    for status in statuses
                ):
                    break
                if all(status is not None for status in statuses):
                    break
                if now - started > timeout:
                    raise TimeoutError(f"{mode} test exceeded {timeout:g} seconds")
                if now >= next_sample:
                    interval = now - last_sample
                    current = [process_stats(process) for process in processes]
                    cpus: list[float] = []
                    rss: list[int] = []
                    for index, state in enumerate(current):
                        before = previous[index]
                        cpu = (
                            max(0.0, (state[0] - before[0]) / interval * 100)
                            if state is not None and before is not None
                            else 0.0
                        )
                        memory = state[1] if state is not None else 0
                        cpu_peaks[index] = max(cpu_peaks[index], cpu)
                        rss_peaks[index] = max(rss_peaks[index], memory)
                        cpus.append(cpu)
                        rss.append(memory)
                    combined_rss_peak = max(combined_rss_peak, sum(rss))
                    print(
                        f"{now - started:9.1f} {cpus[0]:12.1f} {rss[0] / MIB:11.1f}"
                        f" {cpus[1]:12.1f} {rss[1] / MIB:11.1f}",
                        flush=True,
                    )
                    previous = current
                    last_sample = now
                    next_sample += SAMPLE_INTERVAL
                    samples += 1
                time.sleep(min(0.05, max(0, next_sample - time.monotonic())))
            elapsed = time.monotonic() - started
            final_stats = [process_stats(process) for process in processes]
            if any(state is None for state in final_stats):
                raise RuntimeError("Could not read final process statistics")
            server_cpu, client_cpu = (
                state[0] for state in final_stats if state is not None
            )

        if not all(
            status is not None
            and status.si_code == os.CLD_EXITED
            and status.si_status == 0
            for status in statuses
        ):
            assert server.stderr is not None
            stop_process(server)
            stop_process(client)
            server_error = (
                server.stderr.read().decode("utf-8", errors="replace").strip()
            )
            client_error = client_log.read_text(
                encoding="utf-8", errors="replace"
            ).strip()
            raise RuntimeError(
                f"Streaming failed (server={server.returncode}, client={client.returncode})\n"
                f"Server: {server_error}\nClient: {client_error}"
            )
        server.wait()
        client.wait()
        if mode == "archive":
            if destination.stat().st_size != source.stat().st_size or digest(
                destination
            ) != digest(source):
                raise RuntimeError("Received archive does not match the source archive")
        else:
            actual, _ = tree_inventory(destination)
            if actual != reference:
                difference = next(
                    key
                    for key in sorted(actual.keys() | reference.keys())
                    if actual.get(key) != reference.get(key)
                )
                raise RuntimeError(
                    f"Extracted tree differs from the reference at {difference}"
                )

        print(f"\nSummary: ZAR to {mode}")
        print(f"Elapsed including client output: {elapsed:.2f} s")
        print(f"Compressed archive transferred: {source.stat().st_size:,} bytes")
        print(f"Uncompressed regular-file contents: {content_bytes:,} bytes")
        print(
            f"Effective uncompressed throughput: {content_bytes * 8 / elapsed / 1_000_000:.3f} Mbit/s"
        )
        print(
            f"Effective compressed throughput: {source.stat().st_size * 8 / elapsed / 1_000_000:.3f} Mbit/s"
        )
        for index, (label, cpu_seconds) in enumerate(
            (("Server", server_cpu), ("Client", client_cpu))
        ):
            print(
                f"{label} CPU: {cpu_seconds / elapsed * 100:.1f}% average, "
                f"{cpu_peaks[index]:.1f}% peak; RSS peak: {rss_peaks[index] / MIB:.1f} MiB"
            )
        print(
            f"Combined RSS peak: {combined_rss_peak / MIB:.1f} MiB ({samples} samples)"
        )
        print("CPU percentages use 100% per core; RSS and peaks are sampled estimates.")
        print("Output: verified", flush=True)
    finally:
        stop_process(client)
        stop_process(server)
        if server is not None and server.stderr is not None:
            server.stderr.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--archive",
        type=Path,
        default=ROOT / "testdata/linux-4.0.1.zar",
        help="ZAR dataset to transfer",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=3600,
        help="Timeout per test in seconds (default: 3600)",
    )
    parser.add_argument("--worker", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--reference", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--output-dir", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--target-dir", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if sys.platform != "linux":
        parser.error("this test requires Linux")
    if not 0 < args.timeout < float("inf"):
        parser.error("--timeout must be a positive finite number")
    archive = args.archive.resolve()
    if not archive.is_file():
        raise FileNotFoundError(f"Dataset archive not found: {archive}")
    if args.worker:
        if args.reference is None or args.output_dir is None or args.target_dir is None:
            parser.error(
                "internal worker requires reference, output and target directories"
            )
        run(["ip", "link", "set", "lo", "up"])
        run(
            [
                "tc",
                "qdisc",
                "add",
                "dev",
                "lo",
                "root",
                "tbf",
                "rate",
                "100mbit",
                "burst",
                "256kb",
                "latency",
                "400ms",
            ]
        )
        print("Network: isolated loopback, 100 Mbit/s TBF", flush=True)
        reference, content_bytes = tree_inventory(args.reference)
        server = args.target_dir / "release/examples/streaming_server"
        client = args.target_dir / "release/examples/streaming_tree_client"
        for mode in ("archive", "tree"):
            run_case(
                mode,
                archive,
                args.output_dir,
                server,
                client,
                reference,
                content_bytes,
                args.timeout,
            )
        return 0

    for tool in ("cargo", "unshare", "ip", "tc"):
        if shutil.which(tool) is None:
            raise RuntimeError(f"Required command not found: {tool}")
    metadata = subprocess.run(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
    )
    target_dir = Path(json.loads(metadata.stdout)["target_directory"])
    run(
        [
            "cargo",
            "build",
            "--release",
            "--locked",
            "-p",
            "zarlib",
            "--example",
            "streaming_server",
            "--example",
            "streaming_tree_client",
        ]
    )
    run(
        ["cargo", "build", "--release", "--locked", "-p", "zarlib-cli", "--bin", "zarc"]
    )
    with tempfile.TemporaryDirectory(prefix="zar-streaming-") as temporary:
        temp = Path(temporary)
        reference = temp / "reference"
        reference.mkdir()
        print(f"Preparing reference extraction from {archive}", flush=True)
        run(
            [
                str(target_dir / "release/zarc"),
                "extract",
                "--no-config",
                "--no-progress",
                "--collision",
                "rename",
                "--symlinks",
                str(archive),
                "--output",
                str(reference),
            ]
        )
        run(
            [
                "unshare",
                "--user",
                "--map-root-user",
                "--net",
                sys.executable,
                str(Path(__file__).resolve()),
                "--worker",
                "--archive",
                str(archive),
                "--reference",
                str(reference),
                "--output-dir",
                str(temp),
                "--target-dir",
                str(target_dir),
                "--timeout",
                str(args.timeout),
            ]
        )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (
        OSError,
        ValueError,
        RuntimeError,
        subprocess.CalledProcessError,
        TimeoutError,
    ) as error:
        print(f"Streaming test failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
