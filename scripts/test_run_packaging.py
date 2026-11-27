import importlib.util
import os
import platform
import shutil
import subprocess
import tarfile
import tempfile
import textwrap
import unittest
from pathlib import Path
from unittest import mock

import run_packaging

spec = importlib.util.spec_from_file_location(
    "run_packaging_v3", Path(__file__).with_name("run_packaging-v3.py")
)
run_packaging_v3 = importlib.util.module_from_spec(spec)
spec.loader.exec_module(run_packaging_v3)


def workflow_step(workflow, name):
    text = (run_packaging.ROOT / ".github/workflows" / workflow).read_text(encoding="utf-8")
    step = text.split(f"      - name: {name}\n", 1)[1].split("\n      - name:", 1)[0]
    return textwrap.dedent(step.split("\n        run: |\n", 1)[1])


class PackagingDispatchTests(unittest.TestCase):
    @unittest.skipUnless(platform.system() == "Linux" and shutil.which("bash"), "requires Linux and bash")
    def test_v3_unix_archives_include_zarmount_only_on_linux(self):
        build = workflow_step("ci-v3.yml", "Build Unix binaries")
        package = workflow_step("ci-v3.yml", "Package binaries (Unix)")
        for target, name, include_mount in (
            ("x86_64-unknown-linux-gnu", "linux-x86_64", True),
            ("x86_64-unknown-linux-musl", "linux-x86_64-static", True),
            ("x86_64-apple-darwin", "macos-x86_64", False),
        ):
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                tools = root / "tools"
                tools.mkdir()
                cargo = tools / "cargo"
                cargo.write_text("#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$CARGO_ARGS\"\n", encoding="utf-8")
                cargo.chmod(0o755)
                binaries = root / "target" / target / "release"
                binaries.mkdir(parents=True)
                for binary in ("zarc", "zarg", "zarmount"):
                    (binaries / binary).write_text(binary, encoding="utf-8")
                env = os.environ | {
                    "PATH": f"{tools}{os.pathsep}{os.environ['PATH']}",
                    "CARGO_ARGS": str(root / "cargo-args"),
                    "TARGET": target,
                    "PLATFORM": name,
                }
                subprocess.run(["bash", "-e", "-c", build], cwd=root, env=env, check=True)
                args = (root / "cargo-args").read_text(encoding="utf-8").splitlines()
                self.assertEqual("zarmount" in args, include_mount)
                subprocess.run(["bash", "-e", "-c", package], cwd=root, env=env, check=True)
                with tarfile.open(root / f"dist/zar-rs-{name}-v3.tar.gz") as archive:
                    members = set(archive.getnames())
                self.assertEqual(f"zar-rs-{name}-v3/zarmount" in members, include_mount)

    @unittest.skipUnless(platform.system() == "Linux" and shutil.which("bash"), "requires Linux and bash")
    def test_v3_packaging_detects_and_builds_missing_linux_zarmount(self):
        download = workflow_step("packaging-v3.yml", "Download x86-64-v3 Linux binaries")
        build = workflow_step("packaging-v3.yml", "Build missing x86-64-v3 zarmount")
        for include_mount in (False, True):
            with self.subTest(include_mount=include_mount), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                tools = root / "tools"
                tools.mkdir()
                gh = tools / "gh"
                gh.write_text(
                    '#!/bin/sh\ncp "$TEST_ARCHIVE" reused/zar-rs-linux-x86_64-v3.tar.gz\n',
                    encoding="utf-8",
                )
                gh.chmod(0o755)
                source = root / "archive/zar-rs-linux-x86_64-v3"
                source.mkdir(parents=True)
                for binary in ("zarc", "zarg", *(("zarmount",) if include_mount else ())):
                    path = source / binary
                    path.write_text(binary, encoding="utf-8")
                    path.chmod(0o755)
                archive = root / "binaries.tar.gz"
                with tarfile.open(archive, "w:gz") as tar:
                    tar.add(source, arcname=source.name)
                output = root / "github-output"
                github_env = root / "github-env"
                env = os.environ | {
                    "PATH": f"{tools}{os.pathsep}{os.environ['PATH']}",
                    "TEST_ARCHIVE": str(archive),
                    "GITHUB_OUTPUT": str(output),
                    "GITHUB_ENV": str(github_env),
                    "REPO": "example/example",
                    "RELEASE_TAG": "1.0.0",
                }
                subprocess.run(["bash", "-e", "-c", download], cwd=root, env=env, check=True)
                self.assertIn(f"has_zarmount={str(include_mount).lower()}", output.read_text())
                binary_dir = root / "dist/reused/zar-rs-linux-x86_64-v3"
                self.assertIn(f"BIN_DIR={binary_dir}", github_env.read_text())
                if not include_mount:
                    cargo = tools / "cargo"
                    cargo.write_text(
                        "#!/bin/sh\n"
                        "mkdir -p target/x86_64-unknown-linux-gnu/release\n"
                        "printf zarmount > target/x86_64-unknown-linux-gnu/release/zarmount\n"
                        "chmod +x target/x86_64-unknown-linux-gnu/release/zarmount\n",
                        encoding="utf-8",
                    )
                    cargo.chmod(0o755)
                    subprocess.run(
                        ["bash", "-e", "-c", build], cwd=root,
                        env=env | {"BIN_DIR": str(binary_dir)}, check=True,
                    )
                self.assertEqual((binary_dir / "zarmount").read_text(), "zarmount")
                self.assertTrue(os.access(binary_dir / "zarmount", os.X_OK))

    @unittest.skipUnless(platform.system() == "Linux" and shutil.which("bash"), "requires Linux and bash")
    def test_flatpak_state_is_on_custom_staging_filesystem(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            tools = root / "tools"
            tools.mkdir()
            builder = tools / "flatpak-builder"
            builder.write_text(
                "#!/bin/sh\n"
                "expected=--state-dir=$DIST_DIR/work/flatpak/state\n"
                "for arg do [ \"$arg\" = \"$expected\" ] && exit 0; done\n"
                "echo 'state directory is not in the build staging directory' >&2\n"
                "exit 1\n",
                encoding="utf-8",
            )
            flatpak = tools / "flatpak"
            flatpak.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            builder.chmod(0o755)
            flatpak.chmod(0o755)
            env = os.environ | {
                "PATH": f"{tools}{os.pathsep}{os.environ['PATH']}",
                "DIST_DIR": str(root / "dist"),
            }
            subprocess.run(
                ["bash", str(run_packaging.ROOT / "packaging/linux/build-packages.sh"), "flatpak"],
                cwd=run_packaging.ROOT,
                env=env,
                check=True,
                capture_output=True,
                text=True,
            )

    def test_compiler_environment_matches_baseline_and_v3(self):
        with (
            mock.patch.object(run_packaging.platform, "machine", return_value="x86_64"),
            mock.patch.object(run_packaging.platform, "system", return_value="Linux"),
        ):
            for v3, cpu in ((False, "x86-64"), (True, "x86-64-v3")):
                env = run_packaging.compiler_environment(v3)
                self.assertEqual(env["RUSTFLAGS"], f"-C target-cpu={cpu}")
                self.assertEqual(env["CFLAGS"], f"-march={cpu}")
                self.assertEqual(env["CXXFLAGS"], f"-march={cpu}")

    def test_v3_flatpak_manifest_is_temporary_and_has_flags(self):
        original = run_packaging.ROOT / "packaging/linux/io.github.brmmm3.Zarg.yml"
        before = original.read_bytes()
        with run_packaging.v3_flatpak_manifest() as temporary:
            text = temporary.read_text(encoding="utf-8")
            self.assertIn('RUSTFLAGS: "-C target-cpu=x86-64-v3"', text)
            self.assertIn('CFLAGS: "-march=x86-64-v3"', text)
            self.assertIn('CXXFLAGS: "-march=x86-64-v3"', text)
            self.assertEqual(temporary.parent, original.parent)
        self.assertFalse(temporary.exists())
        self.assertEqual(original.read_bytes(), before)

    def test_v3_stages_local_packages_and_preserves_baseline(self):
        for system, method, extensions in (
            ("Linux", "build_linux_packages", ("deb", "rpm", "AppImage", "flatpak")),
            ("Windows", "build_windows_installers", ("exe",)),
            ("Darwin", "build_macos_packages", ("dmg", "pkg")),
        ):
            with self.subTest(system=system), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                (root / "dist").mkdir()
                baseline = root / "dist/package.deb"
                baseline.write_bytes(b"baseline")

                def build(*, v3, dist, extensions=extensions):
                    self.assertTrue(v3)
                    for extension in extensions:
                        (dist / f"package.{extension}").write_bytes(b"v3")

                with (
                    mock.patch.object(run_packaging, "ROOT", root),
                    mock.patch.object(run_packaging_v3.platform, "system", return_value=system),
                    mock.patch.object(run_packaging_v3.platform, "machine", return_value="x86_64"),
                    mock.patch.object(run_packaging, method, side_effect=build),
                ):
                    run_packaging_v3.build()
                    with self.assertRaisesRegex(run_packaging.PackagingFailure, "already exists"):
                        run_packaging_v3.build()
                self.assertEqual(baseline.read_bytes(), b"baseline")
                for extension in extensions:
                    self.assertEqual((root / "dist" / f"package-v3.{extension}").read_bytes(), b"v3")

    def test_v3_rejects_arm(self):
        with (
            mock.patch.object(run_packaging_v3.platform, "system", return_value="Linux"),
            mock.patch.object(run_packaging_v3.platform, "machine", return_value="aarch64"),
            self.assertRaises(run_packaging.PackagingFailure),
        ):
            run_packaging_v3.build()

    def test_v3_missing_package_leaves_baseline_untouched(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "dist").mkdir()
            baseline = root / "dist/package.deb"
            baseline.write_bytes(b"baseline")

            def incomplete_build(*, v3, dist):
                (dist / "package.deb").write_bytes(b"v3")

            with (
                mock.patch.object(run_packaging, "ROOT", root),
                mock.patch.object(run_packaging_v3.platform, "system", return_value="Linux"),
                mock.patch.object(run_packaging_v3.platform, "machine", return_value="x86_64"),
                mock.patch.object(run_packaging, "build_linux_packages", side_effect=incomplete_build),
                self.assertRaisesRegex(run_packaging.PackagingFailure, "one .rpm package"),
            ):
                run_packaging_v3.build()
            self.assertEqual(baseline.read_bytes(), b"baseline")
            self.assertFalse((root / "dist/package-v3.deb").exists())

    def test_runs_only_the_current_operating_system_branch(self):
        cases = (
            ("Linux", "build_linux_packages"),
            ("Windows", "build_windows_installers"),
            ("Darwin", "build_macos_packages"),
        )
        for system, selected in cases:
            with self.subTest(system=system):
                with (
                    mock.patch.object(run_packaging, "build_linux_packages") as linux,
                    mock.patch.object(
                        run_packaging, "build_windows_installers"
                    ) as windows,
                    mock.patch.object(run_packaging, "build_macos_packages") as macos,
                ):
                    run_packaging.build_for_system(system)
                calls = {
                    "build_linux_packages": linux.call_count,
                    "build_windows_installers": windows.call_count,
                    "build_macos_packages": macos.call_count,
                }
                self.assertEqual(calls[selected], 1)
                self.assertEqual(sum(calls.values()), 1)

    def test_rejects_an_unsupported_operating_system(self):
        with self.assertRaises(run_packaging.PackagingFailure):
            run_packaging.build_for_system("FreeBSD")

    def test_selects_only_the_native_windows_cpu_target(self):
        for machine, target in (
            ("AMD64", ("x64", "x86_64-pc-windows-msvc")),
            ("x86_64", ("x64", "x86_64-pc-windows-msvc")),
            ("ARM64", ("arm64", "aarch64-pc-windows-msvc")),
            ("aarch64", ("arm64", "aarch64-pc-windows-msvc")),
        ):
            with self.subTest(machine=machine):
                self.assertEqual(
                    run_packaging.windows_target_for_machine(machine), target
                )

    def test_rejects_an_unsupported_windows_cpu_architecture(self):
        with self.assertRaisesRegex(
            run_packaging.PackagingFailure, "unsupported Windows CPU architecture"
        ):
            run_packaging.windows_target_for_machine("x86")

    def test_windows_packaging_builds_only_the_detected_cpu_target(self):
        for machine, architecture, target in (
            ("AMD64", "x64", "x86_64-pc-windows-msvc"),
            ("ARM64", "arm64", "aarch64-pc-windows-msvc"),
        ):
            with self.subTest(machine=machine):
                with (
                    mock.patch.object(
                        run_packaging.platform, "machine", return_value=machine
                    ),
                    mock.patch.object(
                        run_packaging, "require", side_effect=lambda name: name
                    ),
                    mock.patch.object(
                        run_packaging, "workspace_version", return_value="0.14.0"
                    ),
                    mock.patch.object(
                        run_packaging, "inno_setup_compiler", return_value="ISCC.exe"
                    ),
                    mock.patch.object(run_packaging, "command") as command,
                ):
                    run_packaging.build_windows_installers()

                self.assertEqual(command.call_count, 4)
                self.assertEqual(command.call_args_list[1].args[0][3], target)
                self.assertIn(target, command.call_args_list[2].args[0])
                self.assertIn(
                    f"/DArch={architecture}", command.call_args_list[3].args[0]
                )


if __name__ == "__main__":
    unittest.main()
