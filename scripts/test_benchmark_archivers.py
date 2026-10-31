import io
import subprocess
import tarfile
from unittest.mock import patch

import pytest

from scripts import benchmark_archivers


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

    with patch.object(benchmark_archivers.subprocess, "run", return_value=result):
        with pytest.raises(RuntimeError, match="7z extraction failed"):
            benchmark_archivers.run_7z_extract(["7z"], benchmark_archivers.ROOT, 12)


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
        results, 1024 * 1024, 1, 0, {}, []
    )

    report = report_path.read_text(encoding="utf-8")
    chart = chart_path.read_text(encoding="utf-8")
    assert "2.00-3.00 s" in report
    assert "4.00-5.00 s" in report
    assert "0.200-0.300 s" in report
    assert "2.00-3.00 s" in chart
    assert "4.00-5.00 s" in chart
