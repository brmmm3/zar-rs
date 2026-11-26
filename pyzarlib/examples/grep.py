"""Search a temporary ZAR archive and print matches with context."""

from pathlib import Path
from tempfile import TemporaryDirectory

import pyzarlib


def main():
    with TemporaryDirectory() as directory:
        source = Path(directory) / "notes.txt"
        source.write_bytes(b"first line\nTODO: check backup\r\nafter\n")
        archive = Path(directory) / "backup.zar"
        pyzarlib.create_archive(archive, [source])

        results = pyzarlib.grep_archive(
            archive, "*.txt", r"(?i)\b(todo|fixme)\b", before=1, after=1
        )
        for file in results:
            for line in file["lines"]:
                separator = ":" if line["is_match"] else "-"
                print(
                    f"{file['path']}{separator}{line['line_number']}"
                    f"{separator}{line['bytes']!r}"
                )


if __name__ == "__main__":
    main()
