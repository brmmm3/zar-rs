import subprocess
from unittest.mock import patch

import pytest

from scripts import benchmark_archivers


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
