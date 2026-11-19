import contextlib
import io
import json
from pathlib import Path
import shutil
import tempfile
import tomllib
import unittest
import xml.etree.ElementTree as ET

import update_version


class VersionUpdateTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for source in update_version.manifest_paths(update_version.ROOT):
            target = self.root / source.relative_to(update_version.ROOT)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, target)

    def run_update(self, *args):
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(
            io.StringIO()
        ) as stderr:
            result = update_version.main(list(args), root=self.root)
        return result, stderr.getvalue()

    def snapshot(self):
        return {path: path.read_bytes() for path in self.root.rglob("*") if path.is_file()}

    def test_updates_packages_without_changing_dependencies_or_hashes(self):
        before = self.snapshot()
        self.assertEqual(self.run_update("3.2.1")[0], 0)
        manifests = update_version.manifest_paths(self.root)
        workspace = tomllib.loads((self.root / "Cargo.toml").read_text())
        self.assertEqual(workspace["workspace"]["package"]["version"], "3.2.1")
        local_names = set()
        for path in manifests:
            if path.name == "Cargo.toml" and path.parent != self.root:
                data = tomllib.loads(path.read_text())
                local_names.add(data["package"]["name"])
                self.assertEqual(data["package"]["version"], {"workspace": True})
                dependency = data.get("dependencies", {}).get("zarlib", {})
                if "version" in dependency:
                    self.assertEqual(dependency["version"], "3.2.1")
        lock_path = self.root / "Cargo.lock"
        old_lock = tomllib.loads(before[lock_path].decode())
        new_lock = tomllib.loads(lock_path.read_text())
        for old, new in zip(old_lock["package"], new_lock["package"], strict=True):
            if old["name"] in local_names and "source" not in old:
                self.assertEqual(new["version"], "3.2.1")
                self.assertEqual({**old, "version": "3.2.1"}, new)
            else:
                self.assertEqual(old, new)
        python = tomllib.loads((self.root / "pyzarlib/pyproject.toml").read_text())
        self.assertEqual(python["project"]["version"], "3.2.1")
        for flavor in ("ffm", "jni"):
            project = ET.parse(self.root / f"jzarlib-{flavor}/pom.xml").getroot()
            self.assertEqual(
                project.find("{http://maven.apache.org/POM/4.0.0}version").text, "3.2.1"
            )
        dotnet = ET.parse(self.root / "csharpzarlib/ZarLib/ZarLib.csproj")
        self.assertEqual(dotnet.find("./PropertyGroup/Version").text, "3.2.1")
        for filename in ("package.json", "package-lock.json"):
            path = self.root / "wasizarlib/nodejs" / filename
            old = json.loads(before[path])
            new = json.loads(path.read_bytes())
            self.assertEqual(new["version"], "3.2.1")
            old["version"] = "3.2.1"
            if filename == "package-lock.json":
                old["packages"][""]["version"] = "3.2.1"
            self.assertEqual(old, new)
        for name, nuspec in (("zar-rs", "zarg.nuspec"), ("zar-rs-v3", "zarc.nuspec")):
            path = self.root / f"packaging/windows/scoop/{name}.json"
            old = json.loads(before[path])
            new = json.loads(path.read_bytes())
            self.assertEqual(new["version"], "3.2.1")
            for architecture, values in new["architecture"].items():
                self.assertIn("/download/3.2.1/", values["url"])
                self.assertEqual(values["hash"], old["architecture"][architecture]["hash"])
            package = self.root / "packaging/windows/chocolatey" / name
            metadata = ET.parse(package / nuspec).getroot().find(
                "{http://schemas.microsoft.com/packaging/2015/06/nuspec.xsd}metadata"
            )
            self.assertEqual(
                metadata.find(
                    "{http://schemas.microsoft.com/packaging/2015/06/nuspec.xsd}version"
                ).text,
                "3.2.1",
            )
            path = package / "tools/chocolateyInstall.ps1"
            self.assertEqual(
                path.read_bytes(),
                update_version.release_urls(before[path].decode(), "3.2.1").encode(),
            )
        self.assertEqual(update_version.plan_updates(self.root, "3.2.1"), {})

    def test_dry_run_and_invalid_versions_do_not_modify_files(self):
        before = self.snapshot()
        result, warning = self.run_update("3.2.1", "--dry-run")
        self.assertEqual(result, 0)
        self.assertIn("checksums are unchanged", warning)
        self.assertEqual(before, self.snapshot())
        for version in ("v1.2.3", "1.2", "01.2.3", "1.2.3-beta", "1.2.3\n"):
            with self.subTest(version=version):
                self.assertEqual(self.run_update(version)[0], 1)
                self.assertEqual(before, self.snapshot())

    def test_malformed_manifest_aborts_before_writing_other_files(self):
        path = self.root / "wasizarlib/nodejs/package.json"
        path.write_text("{", encoding="utf-8")
        before = self.snapshot()
        result, error = self.run_update("3.2.1")
        self.assertEqual(result, 1)
        self.assertIn("version update failed", error)
        self.assertEqual(before, self.snapshot())

    def test_preserves_line_endings_and_historical_metadata(self):
        windows_files = [
            self.root / "Cargo.toml",
            self.root / "packaging/windows/scoop/zar-rs.json",
            self.root / "packaging/windows/chocolatey/zar-rs/zarg.nuspec",
        ]
        for path in windows_files:
            path.write_bytes(path.read_bytes().replace(b"\r\n", b"\n").replace(b"\n", b"\r\n"))
        historical = self.root / "packaging/windows/winget/Example/0.1.0/Example.yaml"
        historical.parent.mkdir(parents=True)
        historical.write_text("PackageVersion: 0.1.0\n", encoding="utf-8")
        self.assertEqual(self.run_update("3.2.1")[0], 0)
        for path in windows_files:
            self.assertNotIn(b"\n", path.read_bytes().replace(b"\r\n", b""))
        self.assertEqual(historical.read_text(), "PackageVersion: 0.1.0\n")


if __name__ == "__main__":
    unittest.main()
