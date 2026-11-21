import subprocess
import sys
from pathlib import Path

import pytest
from test_streaming_bandwidth import (
    digest,
    process_stats,
    server_address,
    tree_inventory,
)


def test_tree_inventory_checks_contents_directories_and_symlinks(
    tmp_path: Path,
) -> None:
    tree = tmp_path / "tree"
    (tree / "nested").mkdir(parents=True)
    (tree / "nested" / "data").write_bytes(b"payload")
    (tree / "empty").mkdir()
    (tree / "link").symlink_to("nested/data")
    inventory, size = tree_inventory(tree)
    assert size == 7
    assert inventory["empty"] == ("directory", "")
    assert inventory["link"] == ("symlink", "nested/data")
    assert inventory["nested/data"] == ("file", digest(tree / "nested" / "data"))
    (tree / "nested" / "data").write_bytes(b"changed")
    assert tree_inventory(tree)[0] != inventory
    with pytest.raises(ValueError, match="does not exist"):
        tree_inventory(tmp_path / "missing")


@pytest.mark.skipif(sys.platform != "linux", reason="requires Linux procfs")
def test_process_stats_and_server_readiness() -> None:
    process = subprocess.Popen(
        [
            sys.executable,
            "-c",
            "import sys,time; print('Listening on 127.0.0.1:1234', file=sys.stderr, flush=True); time.sleep(10)",
        ],
        stderr=subprocess.PIPE,
    )
    try:
        assert server_address(process) == "127.0.0.1:1234"
        stats = process_stats(process)
        assert stats is not None
        assert stats[0] >= 0
        assert stats[1] > 0
    finally:
        process.terminate()
        process.wait(timeout=5)
        assert process.stderr is not None
        process.stderr.close()
    assert process_stats(process) is None
