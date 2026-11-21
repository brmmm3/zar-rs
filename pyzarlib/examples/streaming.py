"""Create a ZAR archive and extract a file using binary streams."""

import io

import pyzarlib


def main():
    archive = io.BytesIO()
    pyzarlib.create_archive_from_streams(
        archive,
        [("hello.txt", io.BytesIO(b"Hello, streaming ZAR!\n")),
         ("empty.txt", io.BytesIO())],
        block_size=65536,
    )
    output = io.BytesIO()
    size = pyzarlib.extract_file_to_stream(archive, "hello.txt", output)
    print(f"Extracted {size} bytes: {output.getvalue().decode().strip()}")


if __name__ == "__main__":
    main()
