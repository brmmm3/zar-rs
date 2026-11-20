import contextlib
import io
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

import update_checksums


class ChecksumUpdateTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for package in ("zar-rs", "zar-rs-v3"):
            for relative in (
                f"packaging/windows/scoop/{package}.json",
                f"packaging/windows/chocolatey/{package}/tools/chocolateyInstall.ps1",
            ):
                target = self.root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(update_checksums.ROOT / relative, target)
        self.version = json.loads((self.root / "packaging/windows/scoop/zar-rs.json").read_bytes())["version"]
        self.checksums = {name: digit * 64 for name, digit in zip(update_checksums.ASSETS, "123", strict=True)}

    def snapshot(self):
        return {path: path.read_bytes() for path in self.root.rglob("*") if path.is_file()}

    def run_update(self, *args):
        with patch.object(update_checksums, "fetch_checksums", return_value=self.checksums), contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()) as stderr:
            result = update_checksums.main(list(args), root=self.root)
        return result, stderr.getvalue()

    def test_updates_both_package_managers_and_is_idempotent(self):
        self.assertEqual(self.run_update(self.version)[0], 0)
        baseline = json.loads((self.root / "packaging/windows/scoop/zar-rs.json").read_bytes())
        optimized = json.loads((self.root / "packaging/windows/scoop/zar-rs-v3.json").read_bytes())
        self.assertEqual(baseline["architecture"]["64bit"]["hash"], self.checksums["x86_64"])
        self.assertEqual(baseline["architecture"]["arm64"]["hash"], self.checksums["aarch64"])
        self.assertEqual(optimized["architecture"]["64bit"]["hash"], self.checksums["x86_64-v3"])
        for package, assets in (("zar-rs", ("x86_64", "aarch64")), ("zar-rs-v3", ("x86_64-v3",))):
            script = (self.root / f"packaging/windows/chocolatey/{package}/tools/chocolateyInstall.ps1").read_text()
            for asset in assets:
                self.assertIn(self.checksums[asset], script)
        self.assertEqual(update_checksums.plan_updates(self.root, self.version, self.checksums), {})

    def test_dry_run_and_bad_version_do_not_write(self):
        before = self.snapshot()
        self.assertEqual(self.run_update("--dry-run")[0], 0)
        self.assertEqual(self.run_update("1.2.3-beta")[0], 1)
        self.assertEqual(self.run_update("1.2.3")[0], 1)
        self.assertEqual(before, self.snapshot())

    def test_missing_asset_or_malformed_installer_aborts_before_writing(self):
        path = self.root / "packaging/windows/chocolatey/zar-rs-v3/tools/chocolateyInstall.ps1"
        path.write_text(path.read_text().replace("$checksum =", "$digest ="))
        before = self.snapshot()
        self.assertEqual(self.run_update()[0], 1)
        self.assertEqual(before, self.snapshot())
        with self.assertRaises(ValueError):
            update_checksums.plan_updates(self.root, self.version, {"x86_64": "1" * 64})

    def test_fetch_failure_does_not_write(self):
        before = self.snapshot()
        with patch.object(update_checksums, "fetch_checksums", side_effect=OSError("network error")), contextlib.redirect_stderr(io.StringIO()) as stderr:
            self.assertEqual(update_checksums.main([], root=self.root), 1)
        self.assertIn("network error", stderr.getvalue())
        self.assertEqual(before, self.snapshot())

    def test_preserves_line_endings(self):
        for path in self.snapshot():
            path.write_bytes(path.read_bytes().replace(b"\r\n", b"\n").replace(b"\n", b"\r\n"))
        self.assertEqual(self.run_update()[0], 0)
        for path, content in self.snapshot().items():
            with self.subTest(path=path):
                self.assertNotIn(b"\n", content.replace(b"\r\n", b""))

    def test_parses_only_valid_complete_release_assets(self):
        parser = update_checksums.ReleaseAssets(self.version)
        for architecture, name in update_checksums.ASSETS.items():
            parser.feed(f'<a href="/brmmm3/zar-rs/releases/download/{self.version}/{name}"></a>')
            parser.feed(f'<clipboard-copy aria-label="Copy to clipboard digest for {name}" value="sha256:{self.checksums[architecture]}"></clipboard-copy>')
        self.assertEqual(parser.checksums(), self.checksums)
        with self.assertRaises(ValueError):
            parser.feed(f'<clipboard-copy aria-label="Copy to clipboard digest for {name}" value="sha256:{self.checksums[architecture]}"></clipboard-copy>')
        with self.assertRaises(ValueError):
            update_checksums.ReleaseAssets(self.version).checksums()


if __name__ == "__main__":
    unittest.main()