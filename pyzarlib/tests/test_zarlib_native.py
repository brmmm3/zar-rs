import subprocess
from pathlib import Path


def test_all_zarlib_native_unit_tests() -> None:
    repository_root = Path(__file__).resolve().parents[2]
    subprocess.run(
        ["cargo", "test", "-p", "zarlib"],
        cwd=repository_root,
        check=True,
    )
