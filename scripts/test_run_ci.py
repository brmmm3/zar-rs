import contextlib
import io
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

import run_ci


class RunnerTests(unittest.TestCase):
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
            ["linux-x86_64", "linux-x86_64-static"],
        )
        self.assertEqual(called[0], ["cargo", "fmt", "--all", "--check"])
        wheels.assert_called_once()
        sdist.assert_called_once()
        self.assertIn("SKIP build-windows-x86_64", output.getvalue())

    def test_failure_is_reported_and_returns_nonzero(self):
        with (
            mock.patch.object(run_ci.platform, "system", return_value="Linux"),
            mock.patch.object(run_ci, "host_arch", return_value="x86_64"),
            mock.patch.object(
                run_ci, "command", side_effect=run_ci.JobFailure("unavailable")
            ),
            mock.patch.object(run_ci, "c_tests"),
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
            self.assertEqual(
                run_ci.find_python("3.14", {"3.14": "/test/python"}), "/test/python"
            )
            with self.assertRaises(run_ci.JobFailure):
                run_ci.find_python("3.14t", {"3.14t": "/test/python"})

    def test_python_discovery_falls_back_when_first_has_no_pytest(self):
        with tempfile.TemporaryDirectory() as temp:
            interpreter = Path(temp) / "versions/3.14.5/bin/python"
            interpreter.parent.mkdir(parents=True)
            interpreter.touch()

            def check(args, **kwargs):
                if args[0] == "/first/python":
                    raise run_ci.subprocess.CalledProcessError(1, args)
                return mock.Mock()

            with (
                mock.patch.dict(run_ci.os.environ, {"PYENV_ROOT": temp}),
                mock.patch.object(run_ci.shutil, "which", return_value="/first/python"),
                mock.patch.object(run_ci, "python_identity", return_value="3.14"),
                mock.patch.object(run_ci.subprocess, "run", side_effect=check),
            ):
                self.assertEqual(
                    run_ci.find_python("3.14", {}, require_pytest=True),
                    str(interpreter),
                )

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
            artifact = root / "dist/zar-rs-linux-x86_64.tar.gz"
            with tarfile.open(artifact) as archive:
                self.assertEqual(
                    set(archive.getnames()),
                    {
                        "zar-rs-linux-x86_64",
                        "zar-rs-linux-x86_64/zarc",
                        "zar-rs-linux-x86_64/zarg",
                    },
                )
                self.assertEqual(
                    archive.extractfile("zar-rs-linux-x86_64/zarc").read(), b"cli"
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
