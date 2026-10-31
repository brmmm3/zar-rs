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
