import unittest
from unittest import mock

import run_packaging


class PackagingDispatchTests(unittest.TestCase):
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
                        run_packaging, "workspace_version", return_value="0.12.0"
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
