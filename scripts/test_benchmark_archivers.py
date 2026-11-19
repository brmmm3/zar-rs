import io
import subprocess
import tarfile
import threading
from contextlib import nullcontext
from types import SimpleNamespace
from unittest.mock import DEFAULT, patch

import pytest

from scripts import benchmark_archivers


@pytest.mark.parametrize("fails", [False, True])
def test_progress_flushes_updates_and_stops_on_exit(monkeypatch, fails) -> None:
    messages = []
    updated = threading.Event()

    def record(message, *, flush=False):
        messages.append((message, flush))
        if message.endswith(" s elapsed"):
            updated.set()

    monkeypatch.setattr(benchmark_archivers, "PROGRESS_INTERVAL_SECONDS", 0.01)
    monkeypatch.setattr(benchmark_archivers, "print", record, raising=False)

    with pytest.raises(RuntimeError, match="operation failed") if fails else nullcontext():
        with benchmark_archivers.report_progress("Preparing source data"):
            worker = next(
                thread
                for thread in threading.enumerate()
                if thread.name == "benchmark-progress"
            )
            assert messages[0] == ("Preparing source data...", True)
            assert updated.wait(timeout=2)
            if fails:
                raise RuntimeError("operation failed")

    assert not worker.is_alive()
    assert any(message.startswith("  Preparing source data: ") for message, _ in messages)
    assert all(flush for _, flush in messages)


@pytest.mark.parametrize(
    "caches, total_runs", [(["hot"], 14), (["cold"], 7), (["hot", "cold"], 21)]
)
def test_benchmark_progress_counts_enabled_runs(
    tmp_path, monkeypatch, capsys, caches, total_runs
) -> None:
    monkeypatch.setattr(benchmark_archivers, "TESTDATA", tmp_path)
    monkeypatch.setattr(benchmark_archivers, "TESTRESULTS", tmp_path / "output")
    monkeypatch.setattr(benchmark_archivers, "ARCHIVE_DIR", tmp_path / "archives")
    monkeypatch.setattr(benchmark_archivers, "os", SimpleNamespace(name="nt"))
    mocked = (
        "resolve_cache_modes", "ensure_source", "build_zarc", "source_manifest",
        "seven_zip_supports_zstd", "tool_versions", "run_command",
        "wait_between_benchmark_runs", "read_files", "drop_caches",
        "archive_path", "compress_zarlib", "compress_7z", "compress_tar",
        "extract_archive", "verify_extraction", "verify_partial_extraction",
        "clean_testresults", "write_report", "measure_with_disk_peak",
        "ResourceMonitor",
    )
    with patch.multiple(
        benchmark_archivers, **{name: DEFAULT for name in mocked}
    ) as mocks:
        mocks["resolve_cache_modes"].return_value = (caches, None)
        mocks["build_zarc"].return_value = tmp_path / "zarc"
        mocks["source_manifest"].return_value = ({"file": 1}, set(), 1)
        mocks["seven_zip_supports_zstd"].return_value = False
        mocks["archive_path"].return_value.stat.return_value.st_size = 1
        mocks["verify_extraction"].return_value = 0
        mocks["ResourceMonitor"].return_value.samples = []
        mocks["measure_with_disk_peak"].side_effect = (
            lambda action, roots: (benchmark_archivers.measure(action), 0, 0)
        )

        benchmark_archivers.run_benchmarks(
            {"tar": "tar", "7z": "7z", "openssl": "openssl"}, "cargo"
        )

        assert mocks["extract_archive"].call_count == total_runs * 2
        mocks["write_report"].assert_called_once()

    output = capsys.readouterr().out
    assert f"Running {total_runs} benchmark runs." in output
    assert f"[1/{total_runs}] zarlib (encrypted=False, {caches[0]} cache)" in output
    assert f"Completed {total_runs}/{total_runs} benchmark runs (100%)." in output
    assert output.count("Completed ") == total_runs
    assert output.count(": compressing...") == total_runs
    assert output.count(": extracting full archive") == total_runs
    assert output.count(": extracting 3 selected files...") == total_runs
    assert output.count(": verifying extracted files...") == total_runs
    assert "zarlib (encrypted=True" not in output


def _create_source_tarball(path, entries) -> None:
    with tarfile.open(path, "w:gz") as archive:
        for name, contents in entries:
            member = tarfile.TarInfo(name)
            member.size = len(contents)
            archive.addfile(member, io.BytesIO(contents))


@pytest.mark.parametrize("link_type", ["link", "symbolic link"])
def test_run_7z_extract_accepts_expected_dangerous_link_warnings(link_type) -> None:
    warning = f"ERROR: Dangerous {link_type} path was ignored : source : target"
    output = "\n".join([warning] * 12 + ["Sub items Errors: 12"])
    result = subprocess.CompletedProcess([], 2, output.encode())

    with patch.object(benchmark_archivers.subprocess, "run", return_value=result):
        benchmark_archivers.run_7z_extract(["7z"], benchmark_archivers.ROOT, 12)


def test_run_7z_extract_rejects_unexpected_errors() -> None:
    warnings = ["ERROR: Dangerous symbolic link path was ignored : source : target"] * 11
    output = "\n".join(warnings + ["ERROR: Unexpected error", "Sub items Errors: 12"])
    result = subprocess.CompletedProcess([], 2, output.encode())

    with (
        patch.object(benchmark_archivers.subprocess, "run", return_value=result),
        pytest.raises(RuntimeError, match="7z extraction failed"),
    ):
        benchmark_archivers.run_7z_extract(["7z"], benchmark_archivers.ROOT, 12)


def test_wait_between_benchmark_runs_sleeps_only_after_a_completed_run(
    monkeypatch,
) -> None:
    sleeps = []
    monkeypatch.setattr(benchmark_archivers.time, "sleep", sleeps.append)

    benchmark_archivers.wait_between_benchmark_runs(False)
    benchmark_archivers.wait_between_benchmark_runs(True)

    assert sleeps == [3]


def test_source_matches_tarball_tolerates_case_collisions_on_macos(
    tmp_path, monkeypatch
) -> None:
    monkeypatch.setattr(benchmark_archivers, "TESTDATA", tmp_path)
    monkeypatch.setattr(benchmark_archivers.sys, "platform", "darwin")
    source = tmp_path / benchmark_archivers.SOURCE_NAME
    source.mkdir()
    (source / "README").write_bytes(b"one")
    tarball = tmp_path / "source.tar.gz"
    _create_source_tarball(
        tarball,
        [
            (f"{benchmark_archivers.SOURCE_NAME}/README", b"one"),
            (f"{benchmark_archivers.SOURCE_NAME}/readme", b"longer"),
        ],
    )

    assert benchmark_archivers.source_matches_tarball(tarball)


def test_source_matches_tarball_rejects_non_collision_mismatches_on_macos(
    tmp_path, monkeypatch
) -> None:
    monkeypatch.setattr(benchmark_archivers, "TESTDATA", tmp_path)
    monkeypatch.setattr(benchmark_archivers.sys, "platform", "darwin")
    source = tmp_path / benchmark_archivers.SOURCE_NAME
    source.mkdir()
    (source / "README").write_bytes(b"one")
    tarball = tmp_path / "source.tar.gz"
    _create_source_tarball(
        tarball,
        [
            (f"{benchmark_archivers.SOURCE_NAME}/README", b"one"),
            (f"{benchmark_archivers.SOURCE_NAME}/readme", b"longer"),
            (f"{benchmark_archivers.SOURCE_NAME}/missing", b"missing"),
        ],
    )

    assert not benchmark_archivers.source_matches_tarball(tarball)


def test_write_report_shows_runtime_minimum_and_maximum(tmp_path, monkeypatch) -> None:
    report_path = tmp_path / "report.md"
    chart_path = tmp_path / "chart.svg"
    monkeypatch.setattr(benchmark_archivers, "BENCHMARKS_DIR", tmp_path)
    monkeypatch.setattr(benchmark_archivers, "REPORT_PATH", report_path)
    monkeypatch.setattr(benchmark_archivers, "CHART_PATH", chart_path)
    monkeypatch.setattr(
        benchmark_archivers,
        "collect_benchmark_hardware",
        lambda: benchmark_archivers.BenchmarkHardware("Linux", "CPU", ()),
    )
    results = [
        {
            "method": "tar",
            "cache": "hot",
            "encrypted": False,
            "archive_bytes": 1024 * 1024,
            "compression_min_seconds": 2.0,
            "compression_max_seconds": 3.0,
            "extraction_min_seconds": 4.0,
            "extraction_max_seconds": 5.0,
            "partial_extraction_min_seconds": 0.2,
            "partial_extraction_max_seconds": 0.3,
            "partial_temp_bytes": 0,
            "compression_resources": [],
            "extraction_resources": [],
        }
    ]

    benchmark_archivers.write_report(
        results,
        1024 * 1024,
        1,
        0,
        {"zarlib": "0.8.0", "zarc": "zarc 0.8.0"},
        [],
    )

    report = report_path.read_text(encoding="utf-8")
    chart = chart_path.read_text(encoding="utf-8")
    assert "`zarlib`: `0.8.0`" in report
    assert "`zarc`: `zarc 0.8.0`" in report
    assert "2.00-3.00 s" in report
    assert "4.00-5.00 s" in report
    assert "0.200-0.300 s" in report
    assert "2.00-3.00 s" in chart
    assert "4.00-5.00 s" in chart
    assert "Extraction time (3 files)" in chart
    assert "0.20-0.30 s" in chart
    assert "CPU during compression" in chart
    assert "RAM during compression" in chart
    assert "CPU during extraction" in chart
    assert "RAM during extraction" in chart
    assert "CPU &amp; RAM" not in chart


def test_tool_versions_includes_zarlib_and_zarc_versions(monkeypatch) -> None:
    def run(arguments, **kwargs):
        if "metadata" in arguments:
            return subprocess.CompletedProcess(
                arguments,
                0,
                b'{"packages":[{"name":"zarlib","version":"0.8.0"}]}',
            )
        if arguments[1:] == ["--version"]:
            return subprocess.CompletedProcess(arguments, 0, b"zarc 0.8.0\n")
        return subprocess.CompletedProcess(arguments, 0, b"cargo 1.99.1\n")

    monkeypatch.setattr(benchmark_archivers.subprocess, "run", run)

    versions = benchmark_archivers.tool_versions(
        {"cargo": "cargo", "zarc": "zarc"}
    )

    assert versions["zarlib"] == "0.8.0"
    assert versions["zarc"] == "zarc 0.8.0"
