import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import check_rust_coverage as coverage


class CoverageCheckTests(unittest.TestCase):
    def test_requires_more_than_95_percent_for_every_crate(self):
        root = coverage.ROOT
        entries = [
            {
                "filename": str(root / crate / "src/main.rs"),
                "summary": {"lines": {"count": 100, "covered": covered}},
            }
            for crate, covered in zip(coverage.CRATES, (96, 95, 100))
        ]
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "coverage.json"
            path.write_text(json.dumps({"data": [{"files": entries}]}), encoding="utf-8")
            command = [sys.executable, str(root / "scripts/check_rust_coverage.py"), str(path)]
            failing = subprocess.run(command, capture_output=True, text=True, check=False)
            self.assertEqual(failing.returncode, 1)
            self.assertIn("zarlib-cli: 95/100 lines (95.00%)", failing.stdout)
            entries[1]["summary"]["lines"]["covered"] = 96
            path.write_text(json.dumps({"data": [{"files": entries}]}), encoding="utf-8")
            passing = subprocess.run(command, capture_output=True, text=True, check=False)
            self.assertEqual(passing.returncode, 0)

    def test_rejects_missing_or_duplicate_source_entries(self):
        entry = {
            "filename": str(coverage.ROOT / "zarlib/src/lib.rs"),
            "summary": {"lines": {"count": 10, "covered": 9}},
        }
        with self.assertRaisesRegex(ValueError, "no instrumented source lines"):
            coverage.summarize({"data": [{"files": [entry]}]})
        with self.assertRaisesRegex(ValueError, "duplicate coverage entry"):
            coverage.summarize({"data": [{"files": [entry, entry]}]})
        entry["filename"] = "/src/zarlib/src/lib.rs"
        with self.assertRaisesRegex(ValueError, "outside the workspace"):
            coverage.summarize({"data": [{"files": [entry]}]})


if __name__ == "__main__":
    unittest.main()
