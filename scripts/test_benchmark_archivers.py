import io
import json
import subprocess
import tarfile
import threading
from contextlib import nullcontext
from pathlib import Path
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


@pytest.fixture
def benchmark_runner(tmp_path, monkeypatch):
    monkeypatch.setattr(benchmark_archivers, "TESTDATA", tmp_path)
    monkeypatch.setattr(benchmark_archivers, "TESTRESULTS", tmp_path / "output")
    monkeypatch.setattr(benchmark_archivers, "ARCHIVE_DIR", tmp_path / "archives")
    monkeypatch.setattr(benchmark_archivers, "BENCHMARK_CACHE_DIR", tmp_path / ".benchmark_cache")
    monkeypatch.setattr(benchmark_archivers, "BENCHMARKS_DIR", tmp_path)
    monkeypatch.setattr(benchmark_archivers, "REPORT_PATH", tmp_path / "report.md")
    monkeypatch.setattr(benchmark_archivers, "CHART_PATH", tmp_path / "chart.svg")
    monkeypatch.setattr(
        benchmark_archivers, "os", SimpleNamespace(name="nt", cpu_count=lambda: 4)
    )
    write_report = benchmark_archivers.write_report
    mocked = (
        "resolve_cache_modes", "ensure_source", "build_zarc", "source_manifest",
        "seven_zip_supports_zstd", "tool_versions", "run_command",
        "wait_between_benchmark_runs", "read_files", "drop_caches",
        "archive_path", "compress_zarlib", "compress_7z", "compress_tar",
        "extract_archive", "verify_extraction", "verify_partial_extraction",
        "clean_testresults", "write_report", "measure_with_disk_peak",
        "ResourceMonitor", "collect_benchmark_hardware",
    )
    with patch.multiple(
        benchmark_archivers, **{name: DEFAULT for name in mocked}
    ) as mocks:
        mocks["resolve_cache_modes"].return_value = (["hot"], None)
        mocks["build_zarc"].return_value = tmp_path / "zarc"
        mocks["source_manifest"].return_value = ([("file", 1)], [], 1)
        mocks["tool_versions"].return_value = {"zarc": "zarc 0.15.0"}
        mocks["seven_zip_supports_zstd"].return_value = False
        mocks["archive_path"].return_value.stat.return_value.st_size = 1
        mocks["verify_extraction"].return_value = 0
        mocks["ResourceMonitor"].return_value.samples = [(0.0, 10.0, 1.0)]
        mocks["collect_benchmark_hardware"].return_value = (
            benchmark_archivers.BenchmarkHardware("Windows", "CPU", ())
        )
        mocks["write_report"].side_effect = write_report
        mocks["measure_with_disk_peak"].side_effect = (
            lambda action, roots: (benchmark_archivers.measure(action), 0, 0)
        )
        yield mocks


@pytest.mark.parametrize(
    "caches, total_runs", [(["hot"], 14), (["cold"], 7), (["hot", "cold"], 21)]
)
def test_benchmark_progress_counts_enabled_runs(
    benchmark_runner, capsys, caches, total_runs
) -> None:
    benchmark_runner["resolve_cache_modes"].return_value = (caches, None)
    benchmark_archivers.run_benchmarks(
        {"tar": "tar", "7z": "7z", "openssl": "openssl"}, "cargo"
    )

    assert benchmark_runner["extract_archive"].call_count == total_runs * 2
    benchmark_runner["write_report"].assert_called_once()
    assert not list(benchmark_archivers.BENCHMARK_CACHE_DIR.iterdir())

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


def test_benchmarks_resume_after_interruption(benchmark_runner, monkeypatch, capsys) -> None:
    times = iter((1.0, 2.0, 0.25, 4.0))

    def measure(action):
        action()
        return next(times)

    monkeypatch.setattr(benchmark_archivers, "measure", measure)
    benchmark_runner["extract_archive"].side_effect = [
        None, None, KeyboardInterrupt(),
    ]
    tools = {"tar": "tar", "7z": "7z", "openssl": "openssl"}
    with pytest.raises(KeyboardInterrupt):
        benchmark_archivers.run_benchmarks(tools, "cargo")

    cache = benchmark_archivers.BENCHMARK_CACHE_DIR
    assert [path.name for path in cache.iterdir()] == [
        "001-zarlib-plain-hot-run-1.json"
    ]
    payload = json.loads(next(cache.iterdir()).read_text(encoding="utf-8"))
    assert payload["result"]["compression_seconds"] == 1.0
    assert payload["result"]["partial_extraction_seconds"] == 0.25
    assert payload["result"]["extraction_resources"] == [[0.0, 10.0, 1.0]]
    benchmark_runner["write_report"].assert_not_called()
    capsys.readouterr()

    def resume_measure(action):
        action()
        return 5.0

    monkeypatch.setattr(benchmark_archivers, "measure", resume_measure)
    benchmark_runner["extract_archive"].reset_mock(side_effect=True)
    benchmark_archivers.run_benchmarks(tools, "cargo")

    assert benchmark_runner["extract_archive"].call_count == 13 * 2
    first_result = benchmark_runner["write_report"].call_args.args[0][0]
    assert first_result["compression_min_seconds"] == 1.0
    assert first_result["compression_max_seconds"] == 5.0
    assert first_result["extraction_min_seconds"] == 2.0
    assert first_result["partial_extraction_min_seconds"] == 0.25
    assert first_result["compression_resources"] == [(0.0, 10.0, 1.0)]
    report = benchmark_archivers.REPORT_PATH.read_text(encoding="utf-8")
    assert "1.00-5.00 s" in report
    assert "0.250-5.000 s" in report
    assert benchmark_archivers.CHART_PATH.is_file()
    assert not list(cache.iterdir())
    assert "using cached results" in capsys.readouterr().out


def test_report_failure_preserves_cache_and_retry_skips_all_runs(
    benchmark_runner, capsys
) -> None:
    write_report = benchmark_runner["write_report"].side_effect
    benchmark_runner["write_report"].side_effect = RuntimeError("report failed")
    tools = {"tar": "tar", "7z": "7z", "openssl": "openssl"}
    with pytest.raises(RuntimeError, match="report failed"):
        benchmark_archivers.run_benchmarks(tools, "cargo")

    results = benchmark_runner["write_report"].call_args.args[0]
    cache = benchmark_archivers.BENCHMARK_CACHE_DIR
    files = sorted(cache.iterdir())
    assert len(files) == 14
    assert files[0].name == "001-zarlib-plain-hot-run-1.json"
    assert files[-1].name == "014-tar.gz-encrypted-hot-run-2.json"
    files[0].rename(cache / "099-zarlib-plain-hot-run-1.json")
    capsys.readouterr()
    for mock in benchmark_runner.values():
        mock.reset_mock()
    benchmark_runner["write_report"].side_effect = write_report

    benchmark_archivers.run_benchmarks(tools, "cargo")

    for name in (
        "compress_zarlib", "compress_7z", "compress_tar", "extract_archive",
        "wait_between_benchmark_runs", "read_files", "drop_caches",
    ):
        benchmark_runner[name].assert_not_called()
    assert benchmark_runner["write_report"].call_args.args[0] == results
    assert benchmark_archivers.REPORT_PATH.is_file()
    assert benchmark_archivers.CHART_PATH.is_file()
    assert not list(cache.iterdir())
    assert capsys.readouterr().out.count("using cached results") == 14


@pytest.mark.parametrize(
    "corruption", ["json", "missing", "duration", "resources", "context"]
)
def test_invalid_benchmark_cache_is_reported(benchmark_runner, corruption) -> None:
    cache = benchmark_archivers.BENCHMARK_CACHE_DIR
    cache.mkdir()
    result = benchmark_archivers.BenchmarkRunResult(
        1.0, 2.0, 0.25, 1024, 0, 0, [(0.0, 10.0, 1.0)], [],
    )
    context = {"schema_version": 1}
    benchmark_archivers.save_benchmark_run(1, "zarlib-plain-hot-run-1", context, result)
    path = next(cache.iterdir())
    payload = json.loads(path.read_text(encoding="utf-8"))
    if corruption == "json":
        path.write_text("{", encoding="utf-8")
    else:
        if corruption == "missing":
            del payload["result"]["extraction_seconds"]
        elif corruption == "duration":
            payload["result"]["compression_seconds"] = float("nan")
        elif corruption == "resources":
            payload["result"]["compression_resources"] = [[0.0, 10.0]]
        else:
            payload["context"]["schema_version"] = 2
        path.write_text(json.dumps(payload), encoding="utf-8")

    with pytest.raises(RuntimeError, match="Invalid benchmark cache"):
        benchmark_archivers.load_benchmark_run("zarlib-plain-hot-run-1", context)
    assert path.is_file()


def test_benchmark_cache_write_is_atomic(benchmark_runner) -> None:
    cache = benchmark_archivers.BENCHMARK_CACHE_DIR
    cache.mkdir()
    result = benchmark_archivers.BenchmarkRunResult(1.0, 2.0, 0.25, 1024, 0, 0, [], [])
    benchmark_archivers.save_benchmark_run(1, "zarlib-plain-hot-run-1", {}, result)
    path = next(cache.iterdir())
    original = path.read_bytes()
    result.compression_seconds = 3.0

    with (
        patch.object(Path, "replace", side_effect=OSError("disk error")),
        pytest.raises(OSError, match="disk error"),
    ):
        benchmark_archivers.save_benchmark_run(1, "zarlib-plain-hot-run-1", {}, result)

    assert path.read_bytes() == original
    assert list(cache.iterdir()) == [path]


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
