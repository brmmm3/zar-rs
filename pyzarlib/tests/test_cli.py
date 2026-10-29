import json
from pathlib import Path

import pyzarlib
import pyzarlib.cli as pyzarlib_cli
from pyzarlib.cli import main


def test_python_zarc_cli_aliases_cover_archive_commands(
    tmp_path: Path, monkeypatch, capsys
) -> None:
    monkeypatch.chdir(tmp_path)
    Path("input.txt").write_text("initial", encoding="utf-8")
    archive = "example.zar"

    assert main(["c", archive, "input.txt"]) == 0
    assert "Archive size:" in capsys.readouterr().out
    assert main(["i", "--json", archive]) == 0
    info_json = capsys.readouterr().out
    assert json.loads(info_json.splitlines()[0])["commit_count"] == 1

    Path("added.txt").write_text("added", encoding="utf-8")
    assert main(["a", archive, "added.txt"]) == 0
    capsys.readouterr()
    assert main(["k", archive]) == 0
    assert "2 commit(s)" in capsys.readouterr().out

    commit_ids = [commit["id"] for commit in pyzarlib.info_archive(archive)]
    assert main(
        ["j", archive, "--from", commit_ids[0], "--through", commit_ids[1]]
    ) == 0
    capsys.readouterr()

    assert main(["l", archive]) == 0
    listing = capsys.readouterr().out
    assert "input.txt" in listing
    assert "added.txt" in listing

    assert main(["e", archive, "--to", "restore", "--dry-run"]) == 0
    dry_run = capsys.readouterr().out
    assert "write restore/input.txt" in dry_run
    assert "write restore/added.txt" in dry_run

    assert main(["r", archive, "added.txt"]) == 0
    capsys.readouterr()
    assert main(["list", archive]) == 0
    assert "added.txt" not in capsys.readouterr().out


def test_python_zarc_cli_filelist_filters_inputs(
    tmp_path: Path, monkeypatch, capsys
) -> None:
    monkeypatch.chdir(tmp_path)
    Path("keep.txt").write_text("keep", encoding="utf-8")
    Path("drop.txt").write_text("drop", encoding="utf-8")
    Path("drop.bin").write_bytes(b"drop")
    Path("filters.txt").write_text("*.txt\n-drop.txt\n", encoding="utf-8")

    assert main(
        [
            "create",
            "--filelist",
            "filters.txt",
            "filtered.zar",
            "keep.txt",
            "drop.bin",
        ]
    ) == 0
    capsys.readouterr()
    assert [entry["path"] for entry in pyzarlib.list_archive("filtered.zar")] == [
        "keep.txt"
    ]


def test_python_zarc_cli_creates_and_extracts_chacha_encrypted_archive(
    tmp_path: Path, monkeypatch, capsys
) -> None:
    monkeypatch.chdir(tmp_path)
    Path("secret.txt").write_text("private", encoding="utf-8")
    Path("second.txt").write_text("another secret", encoding="utf-8")
    prompted_passwords = iter(
        [
            "first-password",
            "second-password",
            "first-password",
            "first-password",
            "second-password",
            "first-password",
            "second-password",
        ]
    )
    monkeypatch.setattr(
        pyzarlib_cli.getpass, "getpass", lambda _prompt: next(prompted_passwords)
    )

    assert main(
        [
            "create",
            "--password",
            "--aead",
            "chacha20-poly1305",
            "--encryption-scope",
            "data",
            "encrypted.zar",
            "secret.txt",
        ]
    ) == 0
    capsys.readouterr()

    assert main(
        [
            "add",
            "--password",
            "--password-for",
            "00000001",
            "encrypted.zar",
            "second.txt",
        ]
    ) == 0
    capsys.readouterr()

    password_options = [
        "--password-for",
        "00000001",
        "--password-for",
        "00000002",
    ]
    assert main(["check", *password_options, "encrypted.zar"]) == 0
    assert "2 commit(s)" in capsys.readouterr().out
    assert main(
        ["extract", *password_options, "--to", "restored", "encrypted.zar"]
    ) == 0
    capsys.readouterr()
    assert Path("restored/secret.txt").read_text(encoding="utf-8") == "private"
    assert Path("restored/second.txt").read_text(encoding="utf-8") == "another secret"
