import sys
import zipfile

from scripts import pack_project


def test_pack_excludes_root_testdata(tmp_path, monkeypatch):
    root = tmp_path / "project"
    root.mkdir()
    (root / ".gitignore").write_text("!testdata/\n", encoding="utf-8")
    (root / "testdata").mkdir()
    (root / "testdata" / "sample.zar").write_bytes(b"dataset")
    legacy_icons = root / "zarlib-gui" / "assets" / "icons"
    legacy_icons.mkdir(parents=True)
    (legacy_icons / "old.png").write_bytes(b"old icon")
    crystal_icons = root / "zarlib-gui" / "assets" / "crystal"
    crystal_icons.mkdir()
    (crystal_icons / "new.png").write_bytes(b"new icon")
    nested = root / "src" / "testdata"
    nested.mkdir(parents=True)
    (nested / "fixture.txt").write_text("fixture", encoding="utf-8")
    output = root / "archive.zip"
    monkeypatch.setattr(pack_project, "ROOT", root)
    monkeypatch.setattr(sys, "argv", ["pack_project.py", "--output", str(output)])

    assert pack_project.main() == 0
    with zipfile.ZipFile(output) as archive:
        assert set(archive.namelist()) == {
            "project/.gitignore",
            "project/src/testdata/fixture.txt",
            "project/zarlib-gui/assets/crystal/new.png",
        }