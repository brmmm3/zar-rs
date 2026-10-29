from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
import sys
import sysconfig

import pytest

import pyzarlib


def create_sample_archive(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    monkeypatch.chdir(tmp_path)
    (tmp_path / "input" / "nested").mkdir(parents=True)
    (tmp_path / "input" / "alpha.txt").write_bytes(b"alpha contents")
    (tmp_path / "input" / "nested" / "beta.bin").write_bytes(bytes(range(128)))
    (tmp_path / "input" / "empty.txt").write_bytes(b"")
    archive = tmp_path / "sample.zar"
    pyzarlib.create_archive(
        archive,
        [Path("input")],
        block_size=64 * 1024,
    )
    return archive


def test_create_list_check_and_extract_round_trip(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    archive = create_sample_archive(tmp_path, monkeypatch)

    entries = {entry["path"]: entry for entry in pyzarlib.list_archive(archive)}
    assert entries["input/alpha.txt"]["type"] == "file"
    assert entries["input/alpha.txt"]["size"] == len(b"alpha contents")
    assert entries["input/nested"]["type"] == "directory"
    assert entries["input/empty.txt"]["size"] == 0

    report = pyzarlib.check_archive(archive)
    assert report["commits"] == 1
    assert report["logical_state_readable"] is True
    assert report["encrypted_blocks"] == 0
    assert len(pyzarlib.info_archive(archive)) == 1

    plan = pyzarlib.extract_archive(archive, tmp_path / "dry", dry_run=True)
    assert plan["dry_run"] is True
    assert {Path(item["path"]).name for item in plan["files"]} == {
        "alpha.txt",
        "beta.bin",
        "empty.txt",
    }
    assert not (tmp_path / "dry").exists()

    destination = tmp_path / "restored"
    pyzarlib.extract_archive(archive, destination)
    assert (destination / "input" / "alpha.txt").read_bytes() == b"alpha contents"
    assert (destination / "input" / "nested" / "beta.bin").read_bytes() == bytes(
        range(128)
    )
    assert (destination / "input" / "empty.txt").read_bytes() == b""


def test_create_rejects_invalid_encoding_options(tmp_path: Path) -> None:
    source = tmp_path / "source.txt"
    source.write_text("data", encoding="utf-8")

    with pytest.raises(ValueError, match="block_size"):
        pyzarlib.create_archive(tmp_path / "bad-size.zar", [source], block_size=1)
    with pytest.raises(ValueError, match="compression_level"):
        pyzarlib.create_archive(
            tmp_path / "bad-level.zar",
            [source],
            compression_level=23,
        )


def test_create_overwrite_and_input_filters(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.chdir(tmp_path)
    source = tmp_path / "input"
    source.mkdir()
    (source / "keep.txt").write_text("keep", encoding="utf-8")
    (source / "drop.bin").write_bytes(b"drop")
    archive = tmp_path / "filtered.zar"

    pyzarlib.create_archive(
        archive,
        [Path("input")],
        include_files=["*.txt"],
        block_size=64 * 1024,
    )
    assert {entry["path"] for entry in pyzarlib.list_archive(archive)} == {
        "input",
        "input/keep.txt",
    }

    with pytest.raises(RuntimeError):
        pyzarlib.create_archive(archive, [Path("input")], block_size=64 * 1024)
    pyzarlib.create_archive(
        archive,
        [Path("input")],
        overwrite=True,
        block_size=64 * 1024,
    )
    assert {entry["path"] for entry in pyzarlib.list_archive(archive)} == {
        "input",
        "input/keep.txt",
        "input/drop.bin",
    }


def test_add_replaces_files_and_remove_tombstones(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    archive = create_sample_archive(tmp_path, monkeypatch)
    replacement = tmp_path / "input" / "alpha.txt"
    replacement.write_bytes(b"replaced")
    added = tmp_path / "input" / "nested" / "new.txt"
    added.write_bytes(b"new")

    pyzarlib.add_archive(archive, [Path("input/alpha.txt"), Path("input/nested/new.txt")])
    assert pyzarlib.extract_archive_to_memory(archive)["input/alpha.txt"] == b"replaced"
    assert pyzarlib.extract_archive_to_memory(archive)["input/nested/new.txt"] == b"new"

    pyzarlib.remove_archive(archive, ["input/nested"])
    paths = {entry["path"] for entry in pyzarlib.list_archive(archive)}
    assert "input/nested" not in paths
    assert "input/nested/beta.bin" not in paths
    assert "input/nested/new.txt" not in paths
    assert pyzarlib.check_archive(archive)["commits"] == 3


def test_add_and_remove_failures_leave_archive_unchanged(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    archive = create_sample_archive(tmp_path, monkeypatch)
    before = archive.read_bytes()
    added = tmp_path / "extra.txt"
    added.write_text("extra", encoding="utf-8")

    with pytest.raises(RuntimeError):
        pyzarlib.remove_archive(archive, ["not-in-archive"])
    assert archive.read_bytes() == before

    encrypted = tmp_path / "encrypted.zar"
    pyzarlib.create_archive(
        encrypted,
        [Path("input/alpha.txt")],
        password=b"secret",
        block_size=64 * 1024,
    )
    encrypted_before = encrypted.read_bytes()
    with pytest.raises(RuntimeError):
        pyzarlib.add_archive(encrypted, [added])
    assert encrypted.read_bytes() == encrypted_before


def test_list_and_extract_stop_at_selected_commit(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    archive = create_sample_archive(tmp_path, monkeypatch)
    original_commit = pyzarlib.info_archive(archive)[0]["id"]
    new_file = tmp_path / "input" / "later.txt"
    new_file.write_text("later", encoding="utf-8")
    pyzarlib.add_archive(archive, [Path("input/later.txt")])

    assert "input/later.txt" not in {
        entry["path"] for entry in pyzarlib.list_archive(archive, through=original_commit)
    }
    plan = pyzarlib.extract_archive(
        archive,
        tmp_path / "through",
        through=original_commit,
        dry_run=True,
    )
    assert "later.txt" not in {Path(item["path"]).name for item in plan["files"]}

    with pytest.raises(RuntimeError, match="not found"):
        pyzarlib.list_archive(archive, through="ffffffff")


def test_join_compacts_selected_range_and_preserves_suffix(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    archive = create_sample_archive(tmp_path, monkeypatch)
    first = pyzarlib.info_archive(archive)[0]["id"]
    added_one = tmp_path / "input" / "one.txt"
    added_one.write_text("one", encoding="utf-8")
    pyzarlib.add_archive(archive, [Path("input/one.txt")])
    added_two = tmp_path / "input" / "two.txt"
    added_two.write_text("two", encoding="utf-8")
    pyzarlib.add_archive(archive, [Path("input/two.txt")])
    before = pyzarlib.info_archive(archive)
    through = before[1]["id"]
    suffix_id = before[2]["id"]

    pyzarlib.join_archive(archive, first, through)

    after = pyzarlib.info_archive(archive)
    assert len(after) == 2
    assert after[-1]["id"] == suffix_id
    assert {entry["path"] for entry in pyzarlib.list_archive(archive)} >= {
        "input/one.txt",
        "input/two.txt",
    }
    assert pyzarlib.check_archive(archive)["commits"] == 2


def test_extract_existing_file_policies_and_filters(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    archive = create_sample_archive(tmp_path, monkeypatch)
    destination = tmp_path / "existing"
    target = destination / "input" / "alpha.txt"
    target.parent.mkdir(parents=True)
    target.write_bytes(b"keep existing")

    with pytest.raises(RuntimeError, match="already exists"):
        pyzarlib.extract_archive(archive, destination)
    assert target.read_bytes() == b"keep existing"

    pyzarlib.extract_archive(archive, destination, existing="skip")
    assert target.read_bytes() == b"keep existing"
    pyzarlib.extract_archive(archive, destination, existing="overwrite")
    assert target.read_bytes() == b"alpha contents"

    filtered = pyzarlib.extract_archive(
        archive,
        tmp_path / "filtered-out",
        dry_run=True,
        include_files=["*.txt"],
    )
    assert {Path(item["path"]).name for item in filtered["files"]} == {
        "alpha.txt",
        "empty.txt",
    }


def test_memory_extraction_returns_full_files_and_slices(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    archive = create_sample_archive(tmp_path, monkeypatch)
    full = pyzarlib.extract_archive_to_memory(archive)
    assert full["input/alpha.txt"] == b"alpha contents"
    assert full["input/nested/beta.bin"] == bytes(range(128))
    assert full["input/empty.txt"] == b""

    sliced = pyzarlib.extract_archive_to_memory(
        archive,
        offset=13,
        size_limit=20,
    )
    assert sliced["input/alpha.txt"] == b"s"
    assert sliced["input/nested/beta.bin"] == bytes(range(13, 33))
    assert sliced["input/empty.txt"] == b""

    beyond_end = pyzarlib.extract_archive_to_memory(
        archive,
        offset=1000,
    )
    assert beyond_end == {
        "input/alpha.txt": b"",
        "input/nested/beta.bin": b"",
        "input/empty.txt": b"",
    }

    filtered = pyzarlib.extract_archive_to_memory(
        archive,
        include_files=["*.txt"],
        exclude_files=["input/empty.txt"],
    )
    assert filtered == {"input/alpha.txt": b"alpha contents"}


def test_encrypted_archive_operations_authenticate_password(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    archive = create_sample_archive(tmp_path, monkeypatch)
    encrypted = tmp_path / "encrypted.zar"
    pyzarlib.create_archive(
        encrypted,
        [Path("input/alpha.txt")],
        password=b"correct password",
        block_size=64 * 1024,
    )

    report = pyzarlib.check_archive(encrypted, password=b"correct password")
    assert report["commits"] == 1
    assert pyzarlib.list_archive(encrypted, password=b"correct password")[0]["path"] == (
        "input/alpha.txt"
    )
    assert pyzarlib.extract_archive_to_memory(
        encrypted,
        password=b"correct password",
    ) == {"input/alpha.txt": b"alpha contents"}

    with pytest.raises(RuntimeError):
        pyzarlib.check_archive(encrypted, password=b"wrong password")
    with pytest.raises(RuntimeError):
        pyzarlib.extract_archive(encrypted, tmp_path / "locked")
    assert not (tmp_path / "locked").exists()


@pytest.mark.parametrize("aead", ["aes-256-gcm", "chacha20-poly1305"])
def test_selected_aead_can_create_check_and_extract(
    tmp_path: Path, aead: str
) -> None:
    source = tmp_path / "source.txt"
    source.write_bytes(b"encrypted example")
    archive = tmp_path / f"{aead}.zar"
    password = b"sample-password"

    result = pyzarlib.create_archive(
        archive,
        [source],
        password=password,
        aead=aead,
        block_size=64 * 1024,
    )
    assert result["aead"] == aead
    assert pyzarlib.check_archive(archive, password=password)["commits"] == 1
    assert pyzarlib.list_archive(archive, password=password)[0]["path"] == "source.txt"
    assert pyzarlib.extract_archive_to_memory(
        archive,
        password=password,
    ) == {"source.txt": b"encrypted example"}

    destination = tmp_path / "restored"
    pyzarlib.extract_archive(
        archive,
        destination,
        password=password,
    )
    assert (destination / "source.txt").read_bytes() == b"encrypted example"

def test_commits_accept_individual_passwords_and_data_only_headers(tmp_path: Path) -> None:
    first = tmp_path / "first.txt"
    second = tmp_path / "second.txt"
    first.write_bytes(b"first")
    second.write_bytes(b"second")
    archive = tmp_path / "mixed.zar"

    pyzarlib.create_archive(
        archive,
        [first],
        password=b"first-password",
        encryption_scope="data",
        block_size=64 * 1024,
    )
    info = pyzarlib.info_archive(archive)
    assert info[0]["id"] == "00000001"
    assert info[0]["header_encrypted"] is False
    assert info[0]["data_encrypted"] is True
    assert info[0]["encryption_algorithm"] == "aes-256-gcm"

    pyzarlib.add_archive(
        archive,
        [second],
        password=b"second-password",
        aead="chacha20-poly1305",
        passwords={"00000001": b"first-password"},
        block_size=64 * 1024,
    )
    commit_passwords = {
        "00000001": b"first-password",
        "00000002": b"second-password",
    }
    assert len(pyzarlib.list_archive(archive, passwords=commit_passwords)) == 2
    assert pyzarlib.extract_archive_to_memory(
        archive, passwords=commit_passwords
    ) == {"first.txt": b"first", "second.txt": b"second"}
    with pytest.raises(RuntimeError):
        pyzarlib.extract_archive_to_memory(archive, password=b"first-password")


def test_extension_is_safe_for_concurrent_python_threads(tmp_path: Path) -> None:
    payloads = [bytes([index]) * (64 * 1024 + index) for index in range(8)]

    def create_check_and_read(index: int, payload: bytes) -> tuple[int, bytes]:
        source = tmp_path / f"thread-{index}.bin"
        archive = tmp_path / f"thread-{index}.zar"
        source.write_bytes(payload)
        pyzarlib.create_archive(archive, [source], block_size=64 * 1024)
        assert pyzarlib.check_archive(archive)["commits"] == 1
        return index, pyzarlib.extract_archive_to_memory(archive)[source.name]

    with ThreadPoolExecutor(max_workers=8) as executor:
        results = list(executor.map(create_check_and_read, range(8), payloads))

    assert results == list(enumerate(payloads))


def test_free_threaded_interpreter_keeps_gil_disabled() -> None:
    if sysconfig.get_config_var("Py_GIL_DISABLED") != 1:
        pytest.skip("requires a free-threaded CPython build")
    assert not sys._is_gil_enabled()
