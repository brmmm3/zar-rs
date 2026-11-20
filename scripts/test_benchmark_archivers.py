import json
import os
import shutil
import subprocess
import sys
import tarfile
import threading
import zipfile
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


def test_resource_monitor_excludes_existing_children_and_includes_workload_tree() -> None:
    sleeper = (
        "import sys,time; data=bytearray(int(sys.argv[1])*1024*1024); "
        "print('ready', flush=True); time.sleep(float(sys.argv[2]))"
    )
    parent_code = (
        "import subprocess,sys,time; data=bytearray(8*1024*1024); "
        f"p=subprocess.Popen([sys.executable,'-c',{sleeper!r},'16','2.5'],"
        "stdout=subprocess.PIPE,text=True); "
        "assert p.stdout.readline().strip() == 'ready'; "
        "print('ready', flush=True); p.wait()"
    )
    unrelated = subprocess.Popen(
        [sys.executable, "-c", sleeper, "128", "10"],
        stdout=subprocess.PIPE, text=True,
    )
    try:
        assert unrelated.stdout is not None
        assert unrelated.stdout.readline().strip() == "ready"
        monitor = benchmark_archivers.ResourceMonitor()
        monitor.start()
        try:
            workload = subprocess.Popen(
                [sys.executable, "-c", parent_code], stdout=subprocess.PIPE, text=True,
            )
            try:
                assert workload.stdout is not None
                assert workload.stdout.readline().strip() == "ready"
                assert workload.wait(timeout=6) == 0
            finally:
                if workload.poll() is None:
                    workload.terminate()
                    workload.wait(timeout=5)
        finally:
            monitor.stop()
        assert monitor.samples
        observed_mib = [sample[2] * 1024 for sample in monitor.samples]
        assert all(24 < value < 100 for value in observed_mib)
    finally:
        unrelated.terminate()
        unrelated.wait(timeout=5)


def test_resource_monitor_surfaces_sampling_failure(monkeypatch) -> None:
    monkeypatch.setattr(benchmark_archivers, "RESOURCE_SAMPLE_INTERVAL_SECONDS", 0.01)
    sampled = threading.Event()

    def fail_cpu(*args, **kwargs):
        sampled.set()
        raise RuntimeError("sampling failed")

    monkeypatch.setattr(benchmark_archivers.psutil, "cpu_percent", fail_cpu)
    monitor = benchmark_archivers.ResourceMonitor()
    monitor.start()
    assert sampled.wait(timeout=2)
    with pytest.raises(RuntimeError, match="sampling failed"):
        monitor.stop()


def test_no_install_skips_package_installation(monkeypatch) -> None:
    monkeypatch.setattr(benchmark_archivers.sys, "argv", ["benchmark_archivers.py", "--no-install"])
    with patch.multiple(
        benchmark_archivers, install_missing_tools=DEFAULT,
        require_tools=DEFAULT, run_benchmarks=DEFAULT,
    ) as mocks:
        assert benchmark_archivers.main() == 0
        mocks["install_missing_tools"].assert_not_called()
        mocks["run_benchmarks"].assert_called_once()


def test_main_exits_nonzero_after_failed_benchmarks(monkeypatch) -> None:
    monkeypatch.setattr(benchmark_archivers.sys, "argv", ["benchmark_archivers.py", "--no-install"])
    with patch.multiple(
        benchmark_archivers, require_tools=DEFAULT, run_benchmarks=DEFAULT,
    ) as mocks:
        mocks["run_benchmarks"].return_value = False
        assert benchmark_archivers.main() == 1


def test_main_passes_selected_dataset(monkeypatch) -> None:
    monkeypatch.setattr(
        benchmark_archivers.sys,
        "argv",
        ["benchmark_archivers.py", "--no-install", "--dataset", "icons"],
    )
    with patch.multiple(
        benchmark_archivers,
        require_tools=DEFAULT,
        run_benchmarks=DEFAULT,
    ) as mocks:
        mocks["require_tools"].return_value = {"tar": "tar"}

        assert benchmark_archivers.main() == 0

    mocks["run_benchmarks"].assert_called_once_with(
        {"tar": "tar"}, "cargo", "both", False, "icons"
    )


@pytest.fixture
def benchmark_runner(tmp_path, monkeypatch):
    monkeypatch.setattr(benchmark_archivers, "TESTDATA", tmp_path)
    monkeypatch.setattr(benchmark_archivers, "TESTRESULTS", tmp_path / "output")
    monkeypatch.setattr(benchmark_archivers, "DATASET_DIR", tmp_path / "output" / "datasets")
    monkeypatch.setattr(benchmark_archivers, "EXTRACTION_DIR", tmp_path / "output" / "extracted")
    monkeypatch.setattr(benchmark_archivers, "ARCHIVE_DIR", tmp_path / "archives")
    monkeypatch.setattr(
        benchmark_archivers, "DATASETS",
        ((benchmark_archivers.SOURCE_NAME, benchmark_archivers.PARTIAL_FILES),),
    )
    monkeypatch.setattr(benchmark_archivers, "BENCHMARK_CACHE_DIR", tmp_path / ".benchmark_cache")
    monkeypatch.setattr(benchmark_archivers, "BENCHMARKS_DIR", tmp_path)
    monkeypatch.setattr(benchmark_archivers, "REPORT_PATH", tmp_path / "report.md")
    monkeypatch.setattr(benchmark_archivers, "CHART_PATH", tmp_path / "chart.svg")
    monkeypatch.setattr(
        benchmark_archivers, "os", SimpleNamespace(name="nt", cpu_count=lambda: 4)
    )
    benchmark_archivers.os.environ = {}
    write_report = benchmark_archivers.write_report
    mocked = (
        "resolve_cache_modes", "ensure_dataset_source", "build_zarc", "source_manifest",
        "source_fingerprint",
        "seven_zip_supports_zstd", "tool_versions", "run_command",
        "wait_between_benchmark_runs", "read_files", "drop_caches",
        "archive_path", "compress_zarlib", "compress_zip", "compress_7z", "compress_tar",
        "extract_archive", "verify_extraction", "verify_partial_extraction",
        "clean_testresults", "write_report", "measure_with_disk_peak",
        "ResourceMonitor", "collect_benchmark_hardware",
    )
    with patch.multiple(
        benchmark_archivers, **{name: DEFAULT for name in mocked}
    ) as mocks:
        mocks["resolve_cache_modes"].return_value = (["hot"], None)
        mocks["ensure_dataset_source"].side_effect = (
            lambda binary, name: tmp_path / "output" / "datasets" / name
        )
        mocks["build_zarc"].return_value = tmp_path / "zarc"
        mocks["source_manifest"].return_value = ([("file", 1)], [], 1)
        mocks["source_fingerprint"].return_value = "source-hash"
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
    "dataset_name",
    [dataset_name for dataset_name, _ in benchmark_archivers.DATASETS],
)
def test_missing_dataset_archive_downloads_and_extraction_is_reused(
    tmp_path, monkeypatch, dataset_name
) -> None:
    testdata = tmp_path / "testdata"
    results = tmp_path / "testresults"
    dataset_dir = results / "datasets"
    extraction_dir = results / "extracted"
    results.mkdir()
    extraction_dir.mkdir()
    monkeypatch.setattr(benchmark_archivers, "TESTDATA", testdata)
    monkeypatch.setattr(benchmark_archivers, "TESTRESULTS", results)
    monkeypatch.setattr(benchmark_archivers, "DATASET_DIR", dataset_dir)
    monkeypatch.setattr(benchmark_archivers, "EXTRACTION_DIR", extraction_dir)
    downloads = []
    commands = []

    def download(url, destination):
        downloads.append(url)
        Path(destination).write_bytes(b"valid archive bytes")

    def run(command, *, cwd=None, input_data=None):
        commands.append(command)
        if command[1] == "extract":
            output = Path(command[command.index("--output") + 1])
            source = output / dataset_name
            (source / "init").mkdir(parents=True)
            (source / "Makefile").write_bytes(b"makefile")
            if dataset_name == benchmark_archivers.KERNEL_NAME:
                (source / "init" / "main.o").write_bytes(b"object")

    monkeypatch.setattr(benchmark_archivers.urllib.request, "urlretrieve", download)
    monkeypatch.setattr(benchmark_archivers, "run_command", run)
    source = benchmark_archivers.ensure_dataset_source(Path("zarc"), dataset_name)

    assert downloads == [
        f"{benchmark_archivers.DATASET_URL}/{dataset_name}.zar"
    ]
    assert (testdata / f"{dataset_name}.zar").read_bytes() == b"valid archive bytes"
    assert (source / "Makefile").is_file()
    if dataset_name == benchmark_archivers.KERNEL_NAME:
        assert (source / "init" / "main.o").is_file()

    extraction_dir.joinpath("old-run").mkdir()
    benchmark_archivers.clean_testresults()
    assert source.is_dir()
    assert not list(extraction_dir.iterdir())
    benchmark_archivers.ensure_dataset_source(Path("zarc"), dataset_name)
    assert sum(command[1] == "extract" for command in commands) == 1


def test_dataset_archive_download_replaces_local_lfs_pointer(tmp_path, monkeypatch) -> None:
    monkeypatch.setattr(benchmark_archivers, "TESTDATA", tmp_path)
    archive = tmp_path / f"{benchmark_archivers.SOURCE_NAME}.zar"
    archive.write_text(
        "version https://git-lfs.github.com/spec/v1\noid sha256:abc\nsize 3\n"
    )
    monkeypatch.setattr(
        benchmark_archivers.urllib.request,
        "urlretrieve",
        lambda _url, destination: Path(destination).write_bytes(b"zar"),
    )
    monkeypatch.setattr(benchmark_archivers, "run_command", lambda *_args, **_kwargs: None)

    assert benchmark_archivers.ensure_dataset_archive(
        Path("zarc"), benchmark_archivers.SOURCE_NAME
    ) == archive
    assert archive.read_bytes() == b"zar"


def test_missing_remote_dataset_reports_unpublished_lfs_object(tmp_path, monkeypatch) -> None:
    monkeypatch.setattr(benchmark_archivers, "TESTDATA", tmp_path)

    def fail_download(_url, _destination):
        raise benchmark_archivers.urllib.error.URLError("not found")

    monkeypatch.setattr(benchmark_archivers.urllib.request, "urlretrieve", fail_download)
    with pytest.raises(RuntimeError, match="Ensure the archive and its Git LFS object are published"):
        benchmark_archivers.ensure_dataset_archive(
            Path("zarc"), benchmark_archivers.SOURCE_NAME
        )

    assert not (tmp_path / f"{benchmark_archivers.SOURCE_NAME}.zar").exists()
    assert not (tmp_path / f".{benchmark_archivers.SOURCE_NAME}.zar.download").exists()


@pytest.mark.parametrize(
    "caches, total_runs", [(["hot"], 16), (["cold"], 8), (["hot", "cold"], 24)]
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
    assert "zarlib (encrypted=True" in output
    results = benchmark_runner["write_report"].call_args.args[0]
    assert any(item["method"] == "zarlib" and item["encrypted"] for item in results)


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
        "001-linux-4.0.1-zarlib-plain-hot-run-1.json"
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

    assert benchmark_runner["extract_archive"].call_count == 15 * 2
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


def test_benchmarks_reject_cache_when_source_fingerprint_changes(benchmark_runner) -> None:
    benchmark_runner["extract_archive"].side_effect = [None, None, KeyboardInterrupt()]
    tools = {"tar": "tar", "7z": "7z", "openssl": "openssl"}
    with pytest.raises(KeyboardInterrupt):
        benchmark_archivers.run_benchmarks(tools, "cargo")

    benchmark_runner["extract_archive"].reset_mock(side_effect=True)
    benchmark_runner["source_fingerprint"].return_value = "changed-source-hash"
    with pytest.raises(RuntimeError, match="Invalid benchmark cache"):
        benchmark_archivers.run_benchmarks(tools, "cargo")

    benchmark_runner["extract_archive"].assert_not_called()


def test_benchmarks_reject_system_memory_samples_from_old_cache(benchmark_runner) -> None:
    benchmark_runner["extract_archive"].side_effect = [None, None, KeyboardInterrupt()]
    tools = {"tar": "tar", "7z": "7z", "openssl": "openssl"}
    with pytest.raises(KeyboardInterrupt):
        benchmark_archivers.run_benchmarks(tools, "cargo")
    cache_file = next(benchmark_archivers.BENCHMARK_CACHE_DIR.iterdir())
    payload = json.loads(cache_file.read_text())
    assert payload["context"]["schema_version"] == 4
    payload["context"]["schema_version"] = 2
    cache_file.write_text(json.dumps(payload))
    benchmark_runner["extract_archive"].reset_mock(side_effect=True)
    with pytest.raises(RuntimeError, match="Invalid benchmark cache"):
        benchmark_archivers.run_benchmarks(tools, "cargo")
    benchmark_runner["extract_archive"].assert_not_called()


@pytest.mark.parametrize("phase", ["compression", "full extraction", "selected extraction"])
def test_program_failure_is_reported_and_other_runs_continue(benchmark_runner, phase, capsys) -> None:
    failure = benchmark_archivers.BenchmarkCommandError("program failed")
    if phase == "compression":
        benchmark_runner["compress_zarlib"].side_effect = [failure, None, None, None]
    else:
        benchmark_runner["extract_archive"].side_effect = (
            [failure] if phase == "full extraction" else [None, failure]
        ) + [None] * 30

    tools = {"tar": "tar", "7z": "7z", "openssl": "openssl"}
    assert benchmark_archivers.run_benchmarks(tools, "cargo") is False

    results = benchmark_runner["write_report"].call_args.args[0]
    assert results[0]["method"] == "zarlib"
    assert results[0]["failures"][0]["phase"] == phase
    assert results[-1]["method"] == "tar.gz"
    report = benchmark_archivers.REPORT_PATH.read_text()
    chart = benchmark_archivers.CHART_PATH.read_text()
    assert "FAILED" in report and phase in report and "program failed" in report
    assert "FAILED" in chart
    assert "FAILED" in capsys.readouterr().err
    assert not list(benchmark_archivers.BENCHMARK_CACHE_DIR.iterdir())


def test_program_failure_cache_survives_interruption(benchmark_runner) -> None:
    benchmark_runner["compress_zarlib"].side_effect = [
        benchmark_archivers.BenchmarkCommandError("program failed"), KeyboardInterrupt(),
    ]
    tools = {"tar": "tar", "7z": "7z", "openssl": "openssl"}
    with pytest.raises(KeyboardInterrupt):
        benchmark_archivers.run_benchmarks(tools, "cargo")

    cache = benchmark_archivers.BENCHMARK_CACHE_DIR
    payload = json.loads(next(cache.iterdir()).read_text())
    assert payload["result"]["phase"] == "compression"
    benchmark_runner["compress_zarlib"].reset_mock(side_effect=True)
    benchmark_archivers.run_benchmarks(tools, "cargo")
    assert benchmark_runner["write_report"].call_args.args[0][0]["failures"][0]["phase"] == "compression"
    assert benchmark_runner["compress_zarlib"].call_count == 3


def test_certificate_failure_only_marks_encrypted_tar_cases_failed(benchmark_runner) -> None:
    benchmark_runner["run_command"].side_effect = benchmark_archivers.BenchmarkCommandError(
        "certificate creation failed"
    )
    assert benchmark_archivers.run_benchmarks(
        {"tar": "tar", "7z": "7z", "openssl": "openssl"}, "cargo"
    ) is False
    results = benchmark_runner["write_report"].call_args.args[0]
    failed = [(item["method"], item["encrypted"]) for item in results if item.get("failures")]
    assert failed == [("tar", True), ("tar.gz", True)]
    assert "certificate creation failed" in benchmark_archivers.REPORT_PATH.read_text()


def test_command_failure_removes_password_from_report_and_cache(benchmark_runner, monkeypatch) -> None:
    monkeypatch.setattr(benchmark_archivers, "ENCRYPTED_PASSWORD", "secret-password")
    benchmark_runner["compress_zarlib"].side_effect = [
        benchmark_archivers.BenchmarkCommandError("command -psecret-password failed"),
        KeyboardInterrupt(),
    ]
    tools = {"tar": "tar", "7z": "7z", "openssl": "openssl"}
    with pytest.raises(KeyboardInterrupt):
        benchmark_archivers.run_benchmarks(tools, "cargo")
    cache = benchmark_archivers.BENCHMARK_CACHE_DIR
    cached = next(cache.iterdir()).read_text()
    assert "secret-password" not in cached
    assert "[redacted]" in cached
    benchmark_runner["compress_zarlib"].reset_mock(side_effect=True)
    assert benchmark_archivers.run_benchmarks(tools, "cargo") is False
    assert "secret-password" not in benchmark_archivers.REPORT_PATH.read_text()


def test_run_command_reports_real_nonzero_exit() -> None:
    with pytest.raises(benchmark_archivers.BenchmarkCommandError, match="exit code 7"):
        benchmark_archivers.run_command([sys.executable, "-c", "raise SystemExit(7)"])


def test_windows_zarlib_commands_receive_password_on_stdin(tmp_path, monkeypatch) -> None:
    monkeypatch.setattr(benchmark_archivers, "os", SimpleNamespace(name="nt"))
    monkeypatch.setattr(benchmark_archivers, "TESTRESULTS", tmp_path)
    monkeypatch.setattr(benchmark_archivers, "EXTRACTION_DIR", tmp_path / "extracted")
    monkeypatch.setattr(benchmark_archivers, "ENCRYPTED_PASSWORD", "test-secret")
    source = tmp_path / "source"
    archive = tmp_path / "archive.zar"
    binary = tmp_path / "zarc.exe"
    with patch.object(benchmark_archivers, "run_command") as run:
        benchmark_archivers.compress_zarlib(binary, archive, True, source)
        benchmark_archivers.extract_archive(
            "zarlib", True, archive, {}, binary, None, None, tmp_path, "", 0,
            source, ["file.txt"],
        )

    assert run.call_count == 2
    for call in run.call_args_list:
        assert "--password-stdin" in call.args[0]
        assert "test-secret" not in call.args[0]
        assert call.kwargs["input_data"] == b"test-secret\n"
    assert "source/file.txt" in run.call_args_list[1].args[0]


def test_missing_openssl_still_runs_unencrypted_tar(benchmark_runner) -> None:
    assert benchmark_archivers.run_benchmarks({"tar": "tar", "7z": "7z"}, "cargo") is True
    results = benchmark_runner["write_report"].call_args.args[0]
    assert any(item["method"] == "tar" and not item["encrypted"] for item in results)
    assert all(not item["encrypted"] for item in results if item["method"] in ("tar", "tar.gz"))


def test_run_benchmarks_prepares_all_datasets_even_if_first_fails(
    benchmark_runner, monkeypatch
) -> None:
    datasets = tuple(benchmark_archivers.DATASET_PARTIAL_FILES.items())
    monkeypatch.setattr(benchmark_archivers, "DATASETS", datasets)
    benchmark_archivers.BENCHMARK_CACHE_DIR.mkdir()
    run_dataset = patch.object(
        benchmark_archivers, "_run_benchmark_dataset",
        side_effect=[False, True, True, True, True],
    )
    with run_dataset as run:
        assert benchmark_archivers.run_benchmarks(
            {"tar": "tar", "7z": "7z", "openssl": "openssl"}, "cargo"
        ) is False

    expected_names = [name for name, _ in datasets]
    assert [
        call.args[1]
        for call in benchmark_runner["ensure_dataset_source"].call_args_list
    ] == expected_names
    assert [call.args[5].name for call in run.call_args_list] == expected_names
    assert [call.kwargs["partial_files"] for call in run.call_args_list] == [
        partial_files for _, partial_files in datasets
    ]
    assert set(expected_names) == {
        benchmark_archivers.SOURCE_NAME,
        benchmark_archivers.KERNEL_NAME,
        "duplicates",
        "icons",
        "small_archive",
    }


def test_run_benchmarks_only_prepares_selected_dataset_and_preserves_other_cache(
    benchmark_runner, monkeypatch
) -> None:
    datasets = tuple(benchmark_archivers.DATASET_PARTIAL_FILES.items())
    monkeypatch.setattr(benchmark_archivers, "DATASETS", datasets)
    cache_dir = benchmark_archivers.BENCHMARK_CACHE_DIR
    cache_dir.mkdir()
    selected_cache = cache_dir / "001-icons-zarlib-plain-hot-run-1.json"
    other_cache = cache_dir / "002-linux-4.0.1-zarlib-plain-hot-run-1.json"
    selected_cache.write_text("selected", encoding="utf-8")
    other_cache.write_text("other", encoding="utf-8")

    with patch.object(
        benchmark_archivers, "_run_benchmark_dataset", return_value=True
    ) as run_dataset:
        assert benchmark_archivers.run_benchmarks(
            {"tar": "tar", "7z": "7z", "openssl": "openssl"},
            "cargo",
            dataset_name="icons",
        ) is True

    assert [
        call.args[1]
        for call in benchmark_runner["ensure_dataset_source"].call_args_list
    ] == ["icons"]
    assert [call.args[5].name for call in run_dataset.call_args_list] == ["icons"]
    assert run_dataset.call_args.kwargs["partial_files"] == (
        benchmark_archivers.ICONS_PARTIAL_FILES
    )
    assert not selected_cache.exists()
    assert other_cache.read_text(encoding="utf-8") == "other"


def test_verification_failure_still_stops_benchmarks(benchmark_runner) -> None:
    benchmark_runner["verify_extraction"].side_effect = RuntimeError("verification failed")
    with pytest.raises(RuntimeError, match="verification failed"):
        benchmark_archivers.run_benchmarks(
            {"tar": "tar", "7z": "7z", "openssl": "openssl"}, "cargo"
        )
    benchmark_runner["write_report"].assert_not_called()


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
    assert len(files) == 16
    assert files[0].name == "001-linux-4.0.1-zarlib-plain-hot-run-1.json"
    assert files[-1].name == "016-linux-4.0.1-tar.gz-encrypted-hot-run-2.json"
    files[0].rename(cache / "099-linux-4.0.1-zarlib-plain-hot-run-1.json")
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
    assert capsys.readouterr().out.count("using cached results") == 16


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


@pytest.mark.parametrize("files", [[("file", 1)], [("init/main.o", 0), ("file", 1)]])
def test_precompiled_dataset_requires_nonempty_kernel_object(benchmark_runner, files) -> None:
    benchmark_runner["source_manifest"].return_value = (files, [], 1)
    with pytest.raises(RuntimeError, match="missing a nonempty init/main.o"):
        benchmark_archivers._run_benchmark_dataset(
            {"tar": "tar", "7z": "7z"}, Path("zarc"), ["hot"], None, False,
            Path(benchmark_archivers.KERNEL_NAME),
        )
    benchmark_runner["compress_zarlib"].assert_not_called()


def test_precompiled_dataset_tar_round_trip(tmp_path, monkeypatch) -> None:
    source = tmp_path / "datasets" / benchmark_archivers.KERNEL_NAME
    for member in (*benchmark_archivers.KERNEL_PARTIAL_FILES, "init/main.o", ".config"):
        path = source / member
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(member)
    output = tmp_path / "output"
    output.mkdir()
    monkeypatch.setattr(benchmark_archivers, "TESTRESULTS", output)
    monkeypatch.setattr(benchmark_archivers, "EXTRACTION_DIR", output)
    archive = tmp_path / "kernel.tar"
    benchmark_archivers.compress_tar("tar", "tar", "", archive, False, None, None, tmp_path, source)
    with tarfile.open(archive) as contents:
        for member in ("init/main.o", ".config"):
            assert contents.getmember(f"{source.name}/{member}").isfile()
    benchmark_archivers.extract_archive(
        "tar", False, archive, {"tar": "tar"}, tmp_path / "zarc",
        None, None, tmp_path, "", 0, source,
        list(benchmark_archivers.KERNEL_PARTIAL_FILES),
    )
    benchmark_archivers.verify_partial_extraction(source, benchmark_archivers.KERNEL_PARTIAL_FILES)


@pytest.mark.parametrize("platform", ["posix", "nt"])
@pytest.mark.parametrize("encrypted", [False, True])
def test_compress_zip_uses_platform_options(tmp_path, monkeypatch, platform, encrypted) -> None:
    source = tmp_path / "source"
    archive = tmp_path / "archive.zip"
    monkeypatch.setattr(benchmark_archivers, "os", SimpleNamespace(name=platform))
    with patch.multiple(benchmark_archivers, run_command=DEFAULT, run_with_password=DEFAULT) as mocks:
        benchmark_archivers.compress_zip("zip", archive, encrypted, source)

    command = ["zip", "-q", "-r", "-6"]
    if platform == "posix":
        command.append("-y")
    if encrypted and platform == "posix":
        command.append("-e")
    elif encrypted:
        command.extend(["-P", benchmark_archivers.ENCRYPTED_PASSWORD])
    command.extend([str(archive), source.name])
    if encrypted and platform == "posix":
        mocks["run_with_password"].assert_called_once_with(
            command, source.parent, benchmark_archivers.ENCRYPTED_PASSWORD
        )
    else:
        mocks["run_command"].assert_called_once_with(command, cwd=source.parent)


@pytest.mark.skipif(os.name != "nt" or not shutil.which("zip"), reason="requires Windows ZIP")
@pytest.mark.parametrize("encrypted", [False, True])
def test_windows_zip_round_trip(tmp_path, encrypted) -> None:
    source = tmp_path / "source"
    source.mkdir()
    (source / "file.txt").write_bytes(b"benchmark data")
    archive = tmp_path / "archive.zip"
    benchmark_archivers.compress_zip("zip", archive, encrypted, source)
    with zipfile.ZipFile(archive) as contents:
        password = benchmark_archivers.ENCRYPTED_PASSWORD.encode() if encrypted else None
        assert contents.read("source/file.txt", pwd=password) == b"benchmark data"


@pytest.mark.skipif(
    os.name != "posix" or not shutil.which("zip") or not shutil.which("unzip"),
    reason="requires POSIX symlinks and Info-ZIP",
)
def test_precompiled_dataset_zip_preserves_directory_symlinks(tmp_path, monkeypatch) -> None:
    source = tmp_path / "datasets" / benchmark_archivers.KERNEL_NAME
    (source / "include").mkdir(parents=True)
    (source / "include" / "generated.h").write_text("generated")
    (source / ".config").write_text("kernel config")
    (source / "links").symlink_to("include", target_is_directory=True)
    output = tmp_path / "output"
    output.mkdir()
    monkeypatch.setattr(benchmark_archivers, "TESTRESULTS", output)
    monkeypatch.setattr(benchmark_archivers, "EXTRACTION_DIR", output)
    archive = tmp_path / "kernel.zip"
    expected, symlinks, _ = benchmark_archivers.source_manifest(source)

    benchmark_archivers.compress_zip("zip", archive, False, source)
    benchmark_archivers.extract_archive(
        "zip", False, archive, {"unzip": "unzip"}, tmp_path / "zarc",
        None, None, tmp_path, "", 0, source,
    )
    assert (output / source.name / "links").is_symlink()
    assert benchmark_archivers.verify_extraction(expected, symlinks, False, source) == 0


@pytest.mark.parametrize("problem", ["missing", "extra", "size"])
def test_verify_extraction_rejects_mismatches(tmp_path, monkeypatch, problem) -> None:
    source = tmp_path / "source"
    source.mkdir()
    output = tmp_path / "output"
    extracted = output / source.name
    extracted.mkdir(parents=True)
    monkeypatch.setattr(benchmark_archivers, "TESTRESULTS", output)
    monkeypatch.setattr(benchmark_archivers, "EXTRACTION_DIR", output)
    if problem != "missing":
        (extracted / "file").write_bytes(b"x" if problem == "size" else b"abc")
    if problem == "extra":
        (extracted / "extra").write_bytes(b"x")

    with pytest.raises(RuntimeError, match="Extracted files do not match"):
        benchmark_archivers.verify_extraction([("file", 3)], [], False, source)


@pytest.mark.parametrize("missing_count", [1, 2])
def test_windows_verify_extraction_allows_at_most_one_missing_file(
    tmp_path, monkeypatch, capsys, missing_count
) -> None:
    source = tmp_path / "source"
    source.mkdir()
    output = tmp_path / "output"
    (output / source.name).mkdir(parents=True)
    monkeypatch.setattr(benchmark_archivers, "TESTRESULTS", output)
    monkeypatch.setattr(benchmark_archivers, "EXTRACTION_DIR", output)
    monkeypatch.setattr(
        benchmark_archivers,
        "os",
        SimpleNamespace(name="nt", walk=os.walk),
    )
    expected = [(f"file-{index}", 3) for index in range(missing_count)]

    if missing_count == 1:
        assert benchmark_archivers.verify_extraction(
            expected, [], False, source
        ) == 0
        assert "allowing one missing file on Windows: file-0" in capsys.readouterr().out
    else:
        with pytest.raises(RuntimeError, match="2 missing"):
            benchmark_archivers.verify_extraction(expected, [], False, source)


def test_verify_extraction_accepts_collision_renames(tmp_path, monkeypatch) -> None:
    source = tmp_path / "source"
    source.mkdir()
    output = tmp_path / "output"
    extracted = output / source.name
    extracted.mkdir(parents=True)
    (extracted / "file.txt").write_bytes(b"abc")
    (extracted / "File (1).txt").write_bytes(b"xyz")
    monkeypatch.setattr(benchmark_archivers, "TESTRESULTS", output)
    monkeypatch.setattr(benchmark_archivers, "EXTRACTION_DIR", output)

    assert benchmark_archivers.verify_extraction(
        [("file.txt", 3), ("File.txt", 3)], [], True, source,
    ) == 1


def test_source_fingerprint_detects_same_size_content_and_path_changes(tmp_path) -> None:
    source = tmp_path / "source"
    source.mkdir()
    path = source / "file"
    path.write_bytes(b"abc")
    original_stat = path.stat()
    original = benchmark_archivers.source_fingerprint(source)
    path.write_bytes(b"xyz")
    os.utime(path, ns=(original_stat.st_atime_ns, original_stat.st_mtime_ns))
    assert benchmark_archivers.source_fingerprint(source) != original
    path.write_bytes(b"abc")
    os.utime(path, ns=(original_stat.st_atime_ns, original_stat.st_mtime_ns))
    assert benchmark_archivers.source_fingerprint(source) == original
    path.rename(source / "other")
    assert benchmark_archivers.source_fingerprint(source) != original


@pytest.mark.skipif(os.name != "posix", reason="requires POSIX symlinks")
def test_source_fingerprint_detects_symlink_target_changes(tmp_path) -> None:
    source = tmp_path / "source"
    source.mkdir()
    link = source / "link"
    link.symlink_to("first")
    link_stat = link.lstat()
    original = benchmark_archivers.source_fingerprint(source)
    link.unlink()
    link.symlink_to("other")
    os.utime(
        link, ns=(link_stat.st_atime_ns, link_stat.st_mtime_ns), follow_symlinks=False,
    )
    assert benchmark_archivers.source_fingerprint(source) != original


@pytest.mark.parametrize("link_type", ["link", "symbolic link"])
def test_run_7z_extract_accepts_expected_dangerous_link_warnings(link_type) -> None:
    warning = f"ERROR: Dangerous {link_type} path was ignored : source : target"
    output = "\n".join([warning] * 12 + ["Sub items Errors: 12"])
    result = subprocess.CompletedProcess([], 2, output.encode())

    with patch.object(benchmark_archivers.subprocess, "run", return_value=result):
        benchmark_archivers.run_7z_extract(["7z"], benchmark_archivers.ROOT, 12)


def test_run_7z_extract_accepts_subset_of_source_symlink_warnings(capsys) -> None:
    warning = "ERROR: Dangerous link path was ignored : source : target"
    output = "\n".join([warning] * 47 + ["Sub items Errors: 47"])
    result = subprocess.CompletedProcess([], 2, output.encode())

    with patch.object(benchmark_archivers.subprocess, "run", return_value=result):
        benchmark_archivers.run_7z_extract(["7z"], benchmark_archivers.ROOT, 56)

    assert "7z ignored 47 of 56 expected symlink(s)." in capsys.readouterr().out


def test_run_7z_extract_rejects_unexpected_errors() -> None:
    warnings = ["ERROR: Dangerous symbolic link path was ignored : source : target"] * 11
    output = "\n".join(warnings + ["ERROR: Unexpected error", "Sub items Errors: 12"])
    result = subprocess.CompletedProcess([], 2, output.encode())

    with (
        patch.object(benchmark_archivers.subprocess, "run", return_value=result),
        pytest.raises(RuntimeError, match="7z extraction failed"),
    ):
        benchmark_archivers.run_7z_extract(["7z"], benchmark_archivers.ROOT, 12)


def test_run_7z_extract_rejects_more_warnings_than_source_symlinks() -> None:
    warning = "ERROR: Dangerous link path was ignored : source : target"
    output = "\n".join([warning] * 2 + ["Sub items Errors: 2"])
    result = subprocess.CompletedProcess([], 2, output.encode())

    with (
        patch.object(benchmark_archivers.subprocess, "run", return_value=result),
        pytest.raises(RuntimeError, match="7z extraction failed"),
    ):
        benchmark_archivers.run_7z_extract(["7z"], benchmark_archivers.ROOT, 1)


def test_wait_between_benchmark_runs_sleeps_only_after_a_completed_run(
    monkeypatch,
) -> None:
    sleeps = []
    monkeypatch.setattr(benchmark_archivers.time, "sleep", sleeps.append)

    benchmark_archivers.wait_between_benchmark_runs(False)
    benchmark_archivers.wait_between_benchmark_runs(True)

    assert sleeps == [3]


@pytest.mark.parametrize(
    "cpu_model, github_actions, expected",
    [
        ("AMD Ryzen 5 2400G", False, "-ryzen5"),
        ("Intel(R) Core(TM) i7-10700", False, "-core-i7"),
        ("Intel(R) Core(TM) Ultra 7 265HX", False, "-core-u7"),
        ("Intel Core u7 265HX", False, "-core-u7"),
        ("Generic CPU", False, ""),
        ("Generic CPU", True, "-gh"),
        ("AMD Ryzen 5 2400G", True, "-ryzen5-gh"),
    ],
)
def test_benchmark_result_postfix(cpu_model, github_actions, expected) -> None:
    assert benchmark_archivers.benchmark_result_postfix(
        cpu_model, github_actions
    ) == expected


def test_benchmark_result_paths_put_dataset_after_postfix(tmp_path, monkeypatch) -> None:
    monkeypatch.setattr(benchmark_archivers, "REPORT_PATH", tmp_path / "report.md")
    monkeypatch.setattr(benchmark_archivers, "CHART_PATH", tmp_path / "chart.svg")
    monkeypatch.setenv("GITHUB_ACTIONS", "true")

    report_path, chart_path = benchmark_archivers.benchmark_result_paths(
        "duplicates", "Intel(R) Core(TM) i7-10700"
    )

    assert report_path.name == "report-core-i7-gh-duplicates.md"
    assert chart_path.name == "chart-core-i7-gh-duplicates.svg"


def test_benchmark_result_paths_include_linux_4_0_1_dataset(
    tmp_path, monkeypatch
) -> None:
    monkeypatch.setattr(
        benchmark_archivers,
        "REPORT_PATH",
        tmp_path / "benchmark-report-linux-x86_64.md",
    )
    monkeypatch.setattr(
        benchmark_archivers,
        "CHART_PATH",
        tmp_path / "benchmark-charts-linux-x86_64.svg",
    )
    monkeypatch.delenv("GITHUB_ACTIONS", raising=False)

    report_path, chart_path = benchmark_archivers.benchmark_result_paths(
        benchmark_archivers.SOURCE_NAME, "Intel(R) Core(TM) i7-10700"
    )

    assert report_path.name == "benchmark-report-linux-x86_64-core-i7-linux-4.0.1.md"
    assert chart_path.name == "benchmark-charts-linux-x86_64-core-i7-linux-4.0.1.svg"


def test_archive_size_chart_deduplicates_cache_states() -> None:
    def result(method, encrypted, cache, size, failed=False):
        return {
            "method": method,
            "encrypted": encrypted,
            "cache": cache,
            "archive_bytes": size * 1024 * 1024,
            "compression_min_seconds": 1,
            "compression_max_seconds": 2,
            "extraction_min_seconds": 1,
            "extraction_max_seconds": 2,
            "partial_extraction_min_seconds": 1,
            "partial_extraction_max_seconds": 2,
            "failures": ["failed"] if failed else [],
        }

    results = [
        result("tar", False, "cold", 2),
        result("tar", True, "hot", 3, failed=True),
        result("zip", False, "cold", 4),
        result("tar", False, "hot", 1),
        result("tar", True, "cold", 5),
    ]
    chart = benchmark_archivers.svg_chart(
        results, 1024 * 1024, benchmark_archivers.BenchmarkHardware("Windows", "CPU", ())
    )
    archive_panel, timing_panels = chart.split('class="heading">Compression time', 1)
    assert archive_panel.count('class="label"') == 3
    assert "tar - plain</text>" in archive_panel
    assert "tar - encrypted</text>" in archive_panel
    assert "zip - plain</text>" in archive_panel
    assert "1.00 MiB" in archive_panel
    assert "5.00 MiB" in archive_panel
    assert "4.00 MiB" in archive_panel
    assert "FAILED" not in archive_panel
    assert " - hot" not in archive_panel
    assert " - cold" not in archive_panel
    assert "tar - plain - hot" in timing_panels
    assert "tar - plain - cold" in timing_panels


@pytest.mark.parametrize("cache", ["hot", "cold"])
@pytest.mark.parametrize("failed", [False, True])
def test_archive_size_chart_with_one_cache(cache, failed) -> None:
    result = {
        "method": "tar", "encrypted": False, "cache": cache,
        "archive_bytes": 1024 * 1024,
        "compression_min_seconds": 1, "compression_max_seconds": 2,
        "extraction_min_seconds": 1, "extraction_max_seconds": 2,
        "partial_extraction_min_seconds": 1, "partial_extraction_max_seconds": 2,
        "failures": ["failed"] if failed else [],
    }
    chart = benchmark_archivers.svg_chart(
        [result], 1024 * 1024, benchmark_archivers.BenchmarkHardware("Windows", "CPU", ())
    )
    archive_panel = chart.split('class="heading">Compression time', 1)[0]
    assert archive_panel.count('class="label"') == 1
    assert "tar - plain</text>" in archive_panel
    assert ("FAILED" in archive_panel) == failed
    assert ("1.00 MiB" in archive_panel) != failed
    assert " - hot" not in archive_panel
    assert " - cold" not in archive_panel


def test_write_report_shows_runtime_minimum_and_maximum(tmp_path, monkeypatch) -> None:
    report_path = tmp_path / "report.md"
    chart_path = tmp_path / "chart.svg"
    monkeypatch.setattr(benchmark_archivers, "BENCHMARKS_DIR", tmp_path)
    monkeypatch.setattr(benchmark_archivers, "REPORT_PATH", report_path)
    monkeypatch.setattr(benchmark_archivers, "CHART_PATH", chart_path)
    monkeypatch.delenv("GITHUB_ACTIONS", raising=False)
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
            "compression_resources": [(1.0, 10.0, 0.015), (2.0, 30.0, 0.025)],
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
    assert "| 20 % / 0.02 GiB | N/A |" in report
    assert "summed resident set size (RSS)" in report
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
