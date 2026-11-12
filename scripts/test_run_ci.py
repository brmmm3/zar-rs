import contextlib
import io
import subprocess
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

import run_ci


class RunnerTests(unittest.TestCase):
    def test_cargo_checks_do_not_fix_when_passing(self):
        for action, check in (
            (run_ci.cargo_fmt, ["cargo", "fmt", "--all", "--check"]),
            (
                run_ci.cargo_clippy,
                ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
            ),
        ):
            with self.subTest(action=action.__name__):
                with (
                    mock.patch.object(run_ci, "ensure_cargo_component"),
                    mock.patch.object(run_ci, "command") as command,
                ):
                    action()
                command.assert_called_once_with(check)

    def test_failed_cargo_checks_apply_fixes_and_recheck(self):
        cases = (
            (
                run_ci.cargo_fmt,
                ["cargo", "fmt", "--all", "--check"],
                ["cargo", "fmt", "--all"],
            ),
            (
                run_ci.cargo_clippy,
                ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
                [
                    "cargo", "clippy", "--workspace", "--all-targets", "--fix",
                    "--allow-dirty", "--allow-staged", "--", "-D", "warnings",
                ],
            ),
        )
        for action, check, fix in cases:
            with self.subTest(action=action.__name__):
                with (
                    mock.patch.object(run_ci, "ensure_cargo_component"),
                    mock.patch.object(
                        run_ci,
                        "command",
                        side_effect=[subprocess.CalledProcessError(1, check), None, None],
                    ) as command,
                ):
                    action()
                self.assertEqual(command.call_args_list, [mock.call(check), mock.call(fix), mock.call(check)])

    def test_cargo_fix_and_recheck_failures_propagate(self):
        for action in (run_ci.cargo_fmt, run_ci.cargo_clippy):
            for failure_index in (1, 2):
                with self.subTest(action=action.__name__, failure_index=failure_index):
                    results = [subprocess.CalledProcessError(1, "check"), None, None]
                    results[failure_index] = subprocess.CalledProcessError(1, "failure")
                    with (
                        mock.patch.object(run_ci, "ensure_cargo_component"),
                        mock.patch.object(run_ci, "command", side_effect=results) as command,
                        self.assertRaises(subprocess.CalledProcessError),
                    ):
                        action()
                    self.assertEqual(command.call_count, failure_index + 1)

    def test_failed_prerequisite_blocks_only_dependent_jobs(self):
        runner = run_ci.Runner()
        called = []

        def fail():
            raise run_ci.JobFailure("missing tool")

        with (
            contextlib.redirect_stdout(io.StringIO()),
            contextlib.redirect_stderr(io.StringIO()),
        ):
            runner.run("first", fail)
            runner.run("dependent", lambda: called.append("dependent"), ("first",))
            runner.run("independent", lambda: called.append("independent"))
        self.assertEqual(runner.results["first"], ("FAIL", "missing tool"))
        self.assertEqual(runner.results["dependent"][0], "SKIP")
        self.assertEqual(called, ["independent"])

    def test_require_respects_no_install(self):
        with (
            mock.patch.object(run_ci, "AUTO_INSTALL_ENABLED", False),
            mock.patch.object(run_ci.shutil, "which", return_value=None),
            mock.patch.object(run_ci, "install_missing_tool") as install,
            self.assertRaises(run_ci.JobFailure),
        ):
            run_ci.require("clang")
        install.assert_not_called()

    def test_linux_missing_tool_selects_distribution_package(self):
        with (
            mock.patch.object(run_ci.platform, "system", return_value="Linux"),
            mock.patch.object(
                run_ci.shutil,
                "which",
                side_effect=lambda name: "/usr/bin/apt-get" if name == "apt-get" else None,
            ),
            mock.patch.object(run_ci, "install_linux_packages") as install,
        ):
            run_ci.install_missing_tool("mvn")
        install.assert_called_once_with(["maven"])

    def test_windows_missing_tool_prefers_chocolatey(self):
        with (
            mock.patch.object(run_ci.platform, "system", return_value="Windows"),
            mock.patch.object(
                run_ci.shutil,
                "which",
                side_effect=lambda name: {
                    "winget": "winget.exe",
                    "choco": "choco.exe",
                }.get(name),
            ),
            mock.patch.object(run_ci, "command") as command,
            mock.patch.object(run_ci, "refresh_windows_path") as refresh_path,
        ):
            run_ci.install_missing_tool("mvn")
        self.assertEqual(command.call_args.args[0][:3], ["choco.exe", "install", "maven"])
        refresh_path.assert_called_once()

    def test_windows_missing_tool_falls_back_to_winget(self):
        with (
            mock.patch.object(run_ci.platform, "system", return_value="Windows"),
            mock.patch.object(
                run_ci.shutil,
                "which",
                side_effect=lambda name: "winget.exe" if name == "winget" else None,
            ),
            mock.patch.object(run_ci, "command") as command,
            mock.patch.object(run_ci, "refresh_windows_path"),
        ):
            run_ci.install_missing_tool("mvn")
        self.assertIn("Apache.Maven", command.call_args.args[0])

    def test_windows_python_package_prefers_chocolatey(self):
        with (
            mock.patch.object(run_ci.platform, "system", return_value="Windows"),
            mock.patch.object(
                run_ci.shutil,
                "which",
                side_effect=lambda name: {
                    "winget": "winget.exe",
                    "choco": "choco.exe",
                }.get(name),
            ),
            mock.patch.object(run_ci, "command") as command,
            mock.patch.object(run_ci, "refresh_windows_path"),
        ):
            run_ci.install_missing_tool("python-3.9")
        self.assertEqual(
            command.call_args.args[0],
            ["choco.exe", "install", "python39", "--yes", "--no-progress"],
        )

    def test_windows_python_installs_missing_version_and_uses_launcher(self):
        with (
            mock.patch.object(run_ci, "AUTO_INSTALL_ENABLED", True),
            mock.patch.object(run_ci.shutil, "which", side_effect=[None, "py.exe"]),
            mock.patch.object(
                run_ci.subprocess,
                "run",
                return_value=mock.Mock(
                    returncode=0, stdout="C:\\Python39\\python.exe\n"
                ),
            ) as run,
            mock.patch.object(run_ci, "install_missing_tool") as install,
            mock.patch.object(run_ci, "refresh_windows_path") as refresh,
        ):
            executable = run_ci.windows_python("3.9")
        self.assertEqual(executable, "C:\\Python39\\python.exe")
        install.assert_called_once_with("python-3.9")
        refresh.assert_called_once()
        run.assert_called_once_with(
            ["py.exe", "-3.9", "-c", "import sys; print(sys.executable)"],
            text=True,
            capture_output=True,
            check=False,
        )

    def test_wheel_matrix_covers_all_declared_python_versions(self):
        self.assertEqual(
            run_ci.WHEEL_VERSIONS,
            ("3.9", "3.10", "3.11", "3.12", "3.13", "3.14", "3.15"),
        )

    def test_ruff_installer_adds_python_user_scripts(self):
        with tempfile.TemporaryDirectory() as temp:
            user_base = Path(temp) / "user"
            scripts = user_base / ("Scripts" if run_ci.os.name == "nt" else "bin")
            scripts.mkdir(parents=True)
            with (
                mock.patch.object(run_ci.subprocess, "run", return_value=mock.Mock(stdout=f"{user_base}\n")),
                mock.patch.object(run_ci, "command") as command,
                mock.patch.dict(run_ci.os.environ, {"PATH": ""}),
            ):
                run_ci.install_missing_tool("ruff")
                self.assertIn(str(scripts), run_ci.os.environ["PATH"])
        self.assertEqual(command.call_args.args[0][-2:], ["--user", "ruff"])

    def test_binding_helpers_issue_expected_commands(self):
        for system, variable in (("Linux", "LD_LIBRARY_PATH"), ("Darwin", "DYLD_LIBRARY_PATH")):
            for existing in ("", "/existing"):
                with self.subTest(system=system, existing=existing):
                    with (
                        mock.patch.object(run_ci, "ROOT", Path("/repo")),
                        mock.patch.object(run_ci, "require", side_effect=lambda name: name),
                        mock.patch.object(run_ci, "command") as command,
                        mock.patch.dict(run_ci.os.environ, {variable: existing}),
                    ):
                        run_ci.go_tests(system)
                    self.assertEqual(
                        [call.args[0] for call in command.call_args_list],
                        [
                            ["cargo", "build", "--locked", "-p", "czarlib", "--target-dir", str(Path("/repo/target"))],
                            ["go", "test", "./..."],
                        ],
                    )
                    self.assertEqual(command.call_args.kwargs["cwd"], Path("/repo/gozarlib"))
                    self.assertEqual(
                        command.call_args.kwargs["env"][variable],
                        str(Path("/repo/target/debug")) + (run_ci.os.pathsep + existing if existing else ""),
                    )

        with (
            mock.patch.object(run_ci, "require", return_value="dotnet"),
            mock.patch.object(run_ci, "command") as command,
        ):
            run_ci.csharp_tests()
        self.assertEqual(
            [call.args[0] for call in command.call_args_list],
            [
                ["cargo", "build", "--locked", "--release", "-p", "czarlib"],
                [
                    "dotnet",
                    "restore",
                    "csharpzarlib/ZarLib.Tests/ZarLib.Tests.csproj",
                    "--source",
                    "https://api.nuget.org/v3/index.json",
                ],
                [
                    "dotnet",
                    "test",
                    "csharpzarlib/ZarLib.Tests/ZarLib.Tests.csproj",
                    "--configuration",
                    "Release",
                    "--no-restore",
                ],
            ],
        )
        native_name = {
            "Windows": "czarlib.dll",
            "Darwin": "libczarlib.dylib",
            "Linux": "libczarlib.so",
        }[run_ci.platform.system()]
        self.assertEqual(
            command.call_args_list[-1].kwargs["env"]["ZAR_LIBRARY"],
            str(run_ci.ROOT / "target/release" / native_name),
        )

    def test_lint_helpers_run_check_modes(self):
        with (
            mock.patch.object(run_ci, "require", side_effect=lambda name: name),
            mock.patch.object(run_ci, "command") as command,
        ):
            run_ci.ruff_check()
            run_ci.rumdl_check()
            run_ci.zizmor_check()
        self.assertEqual(
            [call.args[0] for call in command.call_args_list],
            [
                ["ruff", "check", "."],
                ["rumdl", "check", "."],
                ["zizmor", "--offline", "--min-severity=low", "."],
            ],
        )

    def test_linux_package_install_uses_detected_manager(self):
        with (
            mock.patch.object(run_ci, "AUTO_INSTALL_ENABLED", True),
            mock.patch.object(
                run_ci.shutil,
                "which",
                side_effect=lambda name: "/usr/bin/apt-get" if name == "apt-get" else None,
            ),
            mock.patch.object(run_ci.os, "geteuid", return_value=0, create=True),
            mock.patch.object(run_ci.subprocess, "run") as run,
            mock.patch.object(run_ci, "command") as command,
        ):
            run_ci.install_linux_packages(["clang", "lld", "llvm"])
        run.assert_called_once_with(
            ["apt-get", "update"], env=mock.ANY, check=False
        )
        command.assert_called_once_with(
            ["apt-get", "install", "-y", "clang", "lld", "llvm"], env=mock.ANY
        )

    def test_linux_matrix_runs_matching_jobs_in_order(self):
        called = []
        with (
            mock.patch.object(run_ci.platform, "system", return_value="Linux"),
            mock.patch.object(run_ci, "host_arch", return_value="x86_64"),
            mock.patch.object(
                run_ci,
                "command",
                side_effect=lambda *args, **kwargs: called.append(args[0]),
            ),
            mock.patch.object(run_ci, "c_tests"),
            mock.patch.object(run_ci, "cxx_api_tests") as cxx_api,
            mock.patch.object(run_ci, "go_tests") as go_tests,
            mock.patch.object(run_ci, "csharp_tests") as csharp_tests,
            mock.patch.object(run_ci, "ruff_check") as ruff_check,
            mock.patch.object(run_ci, "rumdl_check") as rumdl_check,
            mock.patch.object(run_ci, "zizmor_check") as zizmor_check,
            mock.patch.object(run_ci, "wasi"),
            mock.patch.object(run_ci, "python_tests") as python_tests,
            mock.patch.object(run_ci, "java_bindings") as java,
            mock.patch.object(run_ci, "binaries") as binaries,
            mock.patch.object(run_ci, "wheels") as wheels,
            mock.patch.object(run_ci, "sdist") as sdist,
            contextlib.redirect_stdout(io.StringIO()) as output,
        ):
            result = run_ci.main([])
        self.assertEqual(result, 0)
        self.assertEqual(
            [call.args[0] for call in python_tests.call_args_list],
            list(run_ci.PYTHON_VERSIONS),
        )
        self.assertEqual(
            [call.args[0] for call in java.call_args_list],
            ["jzarlib-ffm", "jzarlib-jni"],
        )
        self.assertEqual(
            [call.args[0] for call in binaries.call_args_list],
            [
                "linux-x86_64",
                "linux-x86_64-static",
                "windows-x86_64",
                "windows-aarch64",
            ],
        )
        self.assertTrue(
            all(
                call.kwargs.get("cross_windows")
                for call in binaries.call_args_list[2:]
            )
        )
        cxx_api.assert_called_once()
        go_tests.assert_called_once_with("Linux")
        csharp_tests.assert_called_once()
        ruff_check.assert_called_once()
        rumdl_check.assert_called_once()
        zizmor_check.assert_called_once()
        self.assertEqual(called[0], ["cargo", "fmt", "--all", "--check"])
        wheels.assert_called_once()
        sdist.assert_called_once()
        self.assertNotIn("SKIP build-windows-x86_64", output.getvalue())

    def test_windows_runs_core_and_c_library_jobs(self):
        called = []
        with (
            mock.patch.object(run_ci.platform, "system", return_value="Windows"),
            mock.patch.object(run_ci, "host_arch", return_value="x86_64"),
            mock.patch.object(
                run_ci,
                "command",
                side_effect=lambda *args, **kwargs: called.append(args[0]),
            ),
            mock.patch.object(run_ci, "c_tests") as c_tests,
            mock.patch.object(run_ci, "cxx_api_tests"),
            mock.patch.object(run_ci, "go_tests"),
            mock.patch.object(run_ci, "csharp_tests"),
            mock.patch.object(run_ci, "ruff_check"),
            mock.patch.object(run_ci, "rumdl_check"),
            mock.patch.object(run_ci, "zizmor_check"),
            mock.patch.object(run_ci, "wasi"),
            mock.patch.object(run_ci, "python_tests"),
            mock.patch.object(run_ci, "java_bindings"),
            mock.patch.object(run_ci, "binaries"),
            mock.patch.object(run_ci, "wheels"),
            mock.patch.object(run_ci, "windows_nuget"),
            contextlib.redirect_stdout(io.StringIO()) as output,
        ):
            result = run_ci.main([])
        self.assertEqual(result, 0)
        self.assertEqual(
            called[:4],
            [
                ["cargo", "fmt", "--all", "--check"],
                [
                    "cargo",
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--",
                    "-D",
                    "warnings",
                ],
                ["cargo", "test", "--workspace"],
                ["cargo", "test", "--locked", "-p", "zcopy"],
            ],
        )
        c_tests.assert_called_once_with("Windows")
        self.assertIn(
            ["cargo", "build", "--locked", "--release", "-p", "cxxzarlib"],
            called,
        )
        self.assertIn("SKIP wasizarlib", output.getvalue())
        self.assertNotIn("SKIP cargo-fmt", output.getvalue())

    def test_windows_c_tests_build_library_and_skip_posix_harness(self):
        with (
            mock.patch.object(run_ci, "require") as require,
            mock.patch.object(run_ci, "command") as command,
            contextlib.redirect_stdout(io.StringIO()) as output,
        ):
            run_ci.c_tests("Windows")
        command.assert_called_once_with(
            ["cargo", "build", "--locked", "--release", "-p", "czarlib"]
        )
        require.assert_not_called()
        self.assertIn("SKIP C API integration tests", output.getvalue())

    def test_windows_cross_toolchain_installs_missing_dependencies(self):
        with (
            mock.patch.object(run_ci, "ensure_rustup_item") as rustup_item,
            mock.patch.object(run_ci, "install_linux_packages") as install,
            mock.patch.object(run_ci.shutil, "which", return_value=None),
            mock.patch.object(run_ci, "require", side_effect=lambda name: name),
            mock.patch.object(run_ci, "command") as command,
        ):
            run_ci.ensure_windows_cross_toolchain("x86_64-pc-windows-msvc")
        self.assertEqual(
            [call.args for call in rustup_item.call_args_list],
            [
                ("target", "x86_64-pc-windows-msvc"),
                ("component", "llvm-tools"),
            ],
        )
        install.assert_called_once_with(["clang", "lld", "llvm"])
        command.assert_called_once_with(["cargo", "install", "--locked", "cargo-xwin"])

    def test_missing_cargo_fmt_installs_rustfmt_component(self):
        with (
            mock.patch.object(
                run_ci.subprocess, "run", return_value=mock.Mock(returncode=1)
            ),
            mock.patch.object(run_ci, "ensure_rustup_item") as ensure,
        ):
            run_ci.ensure_cargo_component("fmt")
        ensure.assert_called_once_with("component", "rustfmt")

    def test_require_installs_missing_linux_tool_and_rechecks_path(self):
        with (
            mock.patch.object(run_ci, "AUTO_INSTALL_ENABLED", True),
            mock.patch.object(
                run_ci.shutil, "which", side_effect=[None, "/usr/bin/clang"]
            ),
            mock.patch.object(run_ci, "install_missing_tool") as install,
        ):
            path = run_ci.require("clang")
        self.assertEqual(path, "/usr/bin/clang")
        install.assert_called_once_with("clang")

    def test_failure_is_reported_and_returns_nonzero(self):
        with (
            mock.patch.object(run_ci.platform, "system", return_value="Linux"),
            mock.patch.object(run_ci, "host_arch", return_value="x86_64"),
            mock.patch.object(
                run_ci, "command", side_effect=run_ci.JobFailure("unavailable")
            ),
            mock.patch.object(run_ci, "c_tests"),
            mock.patch.object(run_ci, "cxx_api_tests"),
            mock.patch.object(run_ci, "go_tests"),
            mock.patch.object(run_ci, "csharp_tests"),
            mock.patch.object(run_ci, "ruff_check"),
            mock.patch.object(run_ci, "rumdl_check"),
            mock.patch.object(run_ci, "zizmor_check"),
            mock.patch.object(run_ci, "python_tests"),
            mock.patch.object(run_ci, "java_bindings"),
            contextlib.redirect_stdout(io.StringIO()) as output,
            contextlib.redirect_stderr(io.StringIO()),
        ):
            result = run_ci.main([])
        self.assertEqual(result, 1)
        self.assertIn(
            "SKIP wasizarlib: prerequisite job did not pass", output.getvalue()
        )
        self.assertIn("FAIL cargo-fmt: unavailable", output.getvalue())

    def test_python_override_must_match_requested_runtime(self):
        with mock.patch.object(run_ci, "python_identity", return_value="3.14"):
            self.assertEqual(run_ci.find_python("3.14", {"3.14": "/test/python"}), "/test/python")
            with self.assertRaises(run_ci.JobFailure):
                run_ci.find_python("3.14t", {"3.14t": "/test/python"})

    def test_pyenv_installs_missing_python_version(self):
        prefix = str(Path(tempfile.gettempdir()) / "pyenv" / "versions" / "3.14.9")
        calls = []

        def subprocess_run(args, **kwargs):
            calls.append(args)
            if args[1] == "latest":
                return mock.Mock(stdout="3.14.9\n")
            if args[1] == "prefix" and len([call for call in calls if call[1] == "prefix"]) == 1:
                return mock.Mock(returncode=1, stdout="")
            return mock.Mock(returncode=0, stdout=f"{prefix}\n")

        with (
            mock.patch.object(run_ci, "AUTO_INSTALL_ENABLED", True),
            mock.patch.object(run_ci, "require", return_value="pyenv"),
            mock.patch.object(run_ci.subprocess, "run", side_effect=subprocess_run),
            mock.patch.object(run_ci, "install_linux_packages") as install,
            mock.patch.object(run_ci, "command") as command,
        ):
            interpreter = run_ci.pyenv_python("3.14")
        expected = Path(prefix) / ("python.exe" if run_ci.os.name == "nt" else "bin/python")
        self.assertEqual(interpreter, str(expected))
        install.assert_not_called()
        command.assert_called_once_with(
            ["pyenv", "install", "--skip-existing", "3.14.9"]
        )

    def test_python_tests_install_pytest_in_isolated_venv(self):
        with (
            mock.patch.object(run_ci, "find_python", return_value="/pyenv/python"),
            mock.patch.object(run_ci, "require", return_value="maturin"),
            mock.patch.object(run_ci, "command") as command,
        ):
            run_ci.python_tests("3.14", {})
        commands = [call.args[0] for call in command.call_args_list]
        pip_install = next(args for args in commands if "pip" in args)
        self.assertEqual(pip_install[-2:], ["install", "pytest>=8"])

    def test_java_version_installs_required_jdk(self):
        javac = Path(tempfile.gettempdir()) / "jdk25" / "bin" / "javac.exe"
        with (
            mock.patch.object(run_ci, "AUTO_INSTALL_ENABLED", True),
            mock.patch.object(run_ci, "require", return_value="javac21.exe"),
            mock.patch.object(run_ci.shutil, "which", return_value=str(javac)),
            mock.patch.object(
                run_ci.subprocess,
                "run",
                side_effect=[
                    mock.Mock(returncode=0, stdout="", stderr="javac 21.0.12"),
                    mock.Mock(returncode=0, stdout="javac 25.0.1", stderr=""),
                ],
            ) as run,
            mock.patch.object(run_ci, "install_missing_tool") as install,
            mock.patch.object(run_ci, "refresh_windows_path") as refresh,
            mock.patch.dict(run_ci.os.environ),
        ):
            run_ci.ensure_java25()
            self.assertEqual(
                run_ci.os.environ["JAVA_HOME"], str(javac.parent.parent.resolve())
            )
        install.assert_called_once_with("javac")
        refresh.assert_called_once()
        self.assertEqual(
            run.call_args_list[-1].args[0], [str(javac), "-version"]
        )

    def test_windows_msvc_environment_activates_build_tools(self):
        with (
            mock.patch.dict(
                run_ci.os.environ,
                {"ProgramFiles(x86)": "C:\\Program Files (x86)"},
            ),
            mock.patch.object(Path, "is_file", return_value=True),
            mock.patch.object(
                run_ci.subprocess,
                "run",
                side_effect=[
                    mock.Mock(returncode=0, stdout="C:\\VS\\BuildTools\n"),
                    mock.Mock(
                        returncode=0,
                        stdout="Path=C:\\VS\\VC\\bin;C:\\Windows\\System32\nINCLUDE=C:\\VS\\include\n",
                    ),
                ],
            ) as run,
            mock.patch.object(run_ci.shutil, "which", return_value="C:\\VS\\VC\\bin\\cl.exe") as which,
        ):
            compiler, env = run_ci.windows_msvc_environment()
        self.assertEqual(compiler, "C:\\VS\\VC\\bin\\cl.exe")
        self.assertEqual(env["Path"], "C:\\VS\\VC\\bin;C:\\Windows\\System32")
        self.assertEqual(env["INCLUDE"], "C:\\VS\\include")
        self.assertEqual(run.call_count, 2)
        which.assert_called_once_with("cl", path=env["Path"])

    def test_artifacts_contain_directory_and_file(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            folder = root / "artifact"
            folder.mkdir()
            (folder / "file").write_bytes(b"contents")
            tar = root / "artifact.tar.gz"
            zip_path = root / "artifact.zip"
            run_ci.archive_directory(folder, tar)
            run_ci.archive_directory(folder, zip_path, windows=True)
            with tarfile.open(tar) as archive:
                self.assertEqual(
                    archive.extractfile("artifact/file").read(), b"contents"
                )
            with zipfile.ZipFile(zip_path) as archive:
                self.assertEqual(archive.read("artifact/file"), b"contents")

    def test_binary_build_packages_only_expected_files(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            release = root / "target/x86_64-unknown-linux-gnu/release"
            release.mkdir(parents=True)
            (release / "zarc").write_bytes(b"cli")
            (release / "zarg").write_bytes(b"gui")
            (release / "zcopy").write_bytes(b"copy")
            (release / "unrelated").write_bytes(b"not packaged")
            with (
                mock.patch.object(run_ci, "ROOT", root),
                mock.patch.object(run_ci, "DIST", root / "dist"),
                mock.patch.object(run_ci, "command") as build,
            ):
                run_ci.binaries(
                    "linux-x86_64", "x86_64-unknown-linux-gnu", False, False
                )
            build.assert_called_once()
            self.assertEqual(
                build.call_args.kwargs["env"]["RUSTFLAGS"], "-C target-cpu=x86-64-v3"
            )
            self.assertEqual(
                build.call_args.kwargs["env"]["CFLAGS"], "-march=x86-64-v3"
            )
            self.assertEqual(
                build.call_args.kwargs["env"]["CXXFLAGS"], "-march=x86-64-v3"
            )
            artifact = root / "dist/zar-rs-linux-x86_64.tar.gz"
            with tarfile.open(artifact) as archive:
                self.assertEqual(
                    set(archive.getnames()),
                    {
                        "zar-rs-linux-x86_64",
                        "zar-rs-linux-x86_64/zarc",
                        "zar-rs-linux-x86_64/zarg",
                        "zar-rs-linux-x86_64/zcopy",
                    },
                )
                self.assertEqual(
                    archive.extractfile("zar-rs-linux-x86_64/zarc").read(), b"cli"
                )
                self.assertEqual(
                    archive.extractfile("zar-rs-linux-x86_64/zcopy").read(), b"copy"
                )

    def test_static_binary_preserves_x86_cpu_target(self):
        with (
            mock.patch.object(run_ci, "require"),
            mock.patch.object(run_ci, "command") as build,
            mock.patch.object(run_ci, "copy_required"),
            mock.patch.object(run_ci, "archive_directory"),
        ):
            run_ci.binaries(
                "linux-x86_64-static", "x86_64-unknown-linux-musl", True, False
            )
        env = build.call_args.kwargs["env"]
        self.assertEqual(
            env["RUSTFLAGS"], "-C target-cpu=x86-64-v3 -C target-feature=+crt-static"
        )
        self.assertEqual(env["CFLAGS"], "-march=x86-64-v3")
        self.assertEqual(env["CXXFLAGS"], "-march=x86-64-v3")
        self.assertEqual(env["CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER"], "musl-gcc")

    def test_x86_wheel_build_uses_v3_compiler_flags(self):
        def build_wheel(args, **kwargs):
            self.assertEqual(kwargs["cwd"], run_ci.ROOT / "pyzarlib")
            (Path(args[4]) / "test.whl").touch()

        with (
            mock.patch.object(run_ci, "require", return_value="maturin"),
            mock.patch.object(run_ci, "find_python", return_value="python"),
            mock.patch.object(run_ci, "WHEEL_VERSIONS", ("3.11",)),
            mock.patch.object(run_ci, "host_arch", return_value="x86_64"),
            mock.patch.object(run_ci.platform, "system", return_value="Linux"),
            mock.patch.object(run_ci, "command", side_effect=build_wheel) as build,
            mock.patch.object(run_ci, "copy_required"),
        ):
            run_ci.wheels({})
        env = build.call_args.kwargs["env"]
        self.assertEqual(env["RUSTFLAGS"], "-C target-cpu=x86-64-v3")
        self.assertEqual(env["CFLAGS"], "-march=x86-64-v3")
        self.assertEqual(env["CXXFLAGS"], "-march=x86-64-v3")

    def test_wheel_flags_do_not_apply_unix_x86_options_to_other_targets(self):
        def build_wheel(args, **kwargs):
            self.assertEqual(kwargs["cwd"], run_ci.ROOT / "pyzarlib")
            (Path(args[4]) / "test.whl").touch()

        for system, arch, rustflags in (
            ("Linux", "aarch64", None),
            ("Windows", "aarch64", None),
            ("Windows", "x86_64", "-C target-cpu=x86-64-v3"),
        ):
            with (
                self.subTest(system=system, arch=arch),
                mock.patch.dict(run_ci.os.environ, {}, clear=True),
                mock.patch.object(run_ci, "require", return_value="maturin"),
                mock.patch.object(run_ci, "find_python", return_value="python"),
                mock.patch.object(run_ci, "WHEEL_VERSIONS", ("3.11",)),
                mock.patch.object(run_ci, "host_arch", return_value=arch),
                mock.patch.object(run_ci.platform, "system", return_value=system),
                mock.patch.object(run_ci, "command", side_effect=build_wheel) as build,
                mock.patch.object(run_ci, "copy_required"),
            ):
                run_ci.wheels({})
            env = build.call_args.kwargs["env"]
            self.assertEqual(env.get("RUSTFLAGS"), rustflags)
            self.assertNotIn("CFLAGS", env)
            self.assertNotIn("CXXFLAGS", env)

    def test_windows_cross_binary_uses_xwin_and_packages_executables(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            release = root / "target/x86_64-pc-windows-msvc/release"
            release.mkdir(parents=True)
            (release / "zarc.exe").write_bytes(b"cli")
            (release / "zarg.exe").write_bytes(b"gui")
            (release / "zcopy.exe").write_bytes(b"copy")
            with (
                mock.patch.object(run_ci, "ROOT", root),
                mock.patch.object(run_ci, "DIST", root / "dist"),
                mock.patch.object(run_ci, "ensure_windows_cross_toolchain"),
                mock.patch.object(run_ci, "command") as build,
            ):
                run_ci.binaries(
                    "windows-x86_64",
                    "x86_64-pc-windows-msvc",
                    False,
                    False,
                    cross_windows=True,
                )
            self.assertEqual(
                build.call_args.args[0][:3], ["cargo", "xwin", "build"]
            )
            self.assertEqual(
                build.call_args.kwargs["env"]["XWIN_CROSS_COMPILER"], "clang-cl"
            )
            with zipfile.ZipFile(root / "dist/zar-rs-windows-x86_64.zip") as archive:
                self.assertEqual(
                    set(archive.namelist()),
                    {
                        "zar-rs-windows-x86_64/zarc.exe",
                        "zar-rs-windows-x86_64/zarg.exe",
                        "zar-rs-windows-x86_64/zcopy.exe",
                    },
                )

    def test_missing_and_ambiguous_artifacts_fail(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            with self.assertRaises(run_ci.JobFailure):
                run_ci.one_file(root, "*.jar")
            (root / "one.jar").touch()
            (root / "two.jar").touch()
            with self.assertRaises(run_ci.JobFailure):
                run_ci.one_file(root, "*.jar")
            with self.assertRaises(run_ci.JobFailure):
                run_ci.copy_required(root / "absent", root / "out")


if __name__ == "__main__":
    unittest.main()
