import io
from concurrent.futures import ThreadPoolExecutor

import pytest

import pyzarlib


class ShortReader(io.BytesIO):
    def read(self, size=-1):
        assert 0 < size <= 65536
        return super().read(min(size, 17))


class ShortWriter(io.BytesIO):
    def write(self, data):
        return super().write(data[:11])


def test_stream_roundtrip_with_short_io_and_empty_files():
    payload = bytes(range(256)) * 600
    archive = ShortWriter()
    inputs = [ShortReader(payload), ShortReader(b"")]
    pyzarlib.create_archive_from_streams(
        archive, [("dir/data", inputs[0]), ("empty", inputs[1])], block_size=65536
    )
    for path, expected in [("dir/data", payload), ("empty", b"")]:
        output = ShortWriter()
        assert pyzarlib.extract_file_to_stream(archive, path, output) == len(expected)
        assert output.getvalue() == expected
        assert not output.closed
    assert not archive.closed
    assert all(not stream.closed for stream in inputs)

    empty_archive = io.BytesIO()
    pyzarlib.create_archive_from_streams(empty_archive, [])
    with pytest.raises(RuntimeError, match="not a file"):
        pyzarlib.extract_file_to_stream(empty_archive, "missing", io.BytesIO())


@pytest.mark.parametrize("scope", ["header", "data", "header-and-data"])
@pytest.mark.parametrize("aead", ["aes-256-gcm", "chacha20-poly1305"])
def test_stream_encryption_and_passwords(scope, aead):
    archive = io.BytesIO()
    pyzarlib.create_archive_from_streams(
        archive, [("file", io.BytesIO(b"secret"))],
        password=b"password", encryption_scope=scope, aead=aead
    )
    for kwargs in ({}, {"password": b"wrong"}):
        with pytest.raises(RuntimeError):
            pyzarlib.extract_file_to_stream(archive, "file", io.BytesIO(), **kwargs)
    output = io.BytesIO()
    assert pyzarlib.extract_file_to_stream(
        archive, "file", output, passwords={"00000001": b"password"}, to="00000001"
    ) == 6
    assert output.getvalue() == b"secret"


def test_historical_commit_stream_extraction(tmp_path, monkeypatch):
    monkeypatch.chdir(tmp_path)
    path = tmp_path / "source"
    archive_path = tmp_path / "archive.zar"
    path.write_bytes(b"old")
    pyzarlib.create_archive(archive_path, [path])
    path.write_bytes(b"new")
    pyzarlib.add_archive(archive_path, [path])
    archived_path = pyzarlib.list_archive(archive_path)[0]["path"]
    with archive_path.open("rb") as archive:
        for commit, expected in [("00000001", b"old"), (None, b"new")]:
            output = io.BytesIO()
            pyzarlib.extract_file_to_stream(archive, archived_path, output, to=commit)
            assert output.getvalue() == expected


def test_stream_creation_is_compatible_with_path_operations(tmp_path):
    archive_path = tmp_path / "streamed.zar"
    with archive_path.open("w+b") as archive:
        pyzarlib.create_archive_from_streams(
            archive, [("folder/file", io.BytesIO(b"payload"))]
        )
    assert pyzarlib.check_archive(archive_path)["commits"] == 1
    assert pyzarlib.extract_archive_to_memory(archive_path)["folder/file"] == b"payload"


class MarkerError(Exception):
    pass


class BrokenStream(io.BytesIO):
    def __init__(self, method, value=None, raises=False):
        super().__init__()
        self.method = method
        self.value = value
        self.raises = raises

    def callback(self):
        if self.raises:
            raise MarkerError(self.method)
        return self.value

    def read(self, size=-1):
        return self.callback() if self.method == "read" else super().read(size)

    def write(self, data):
        return self.callback() if self.method == "write" else super().write(data)

    def seek(self, offset, whence=0):
        return self.callback() if self.method == "seek" else super().seek(offset, whence)


@pytest.mark.parametrize("method", ["read", "write", "seek"])
def test_stream_callback_exceptions_propagate(method):
    broken = BrokenStream(method, raises=True)
    with pytest.raises(MarkerError, match=method):
        if method == "read":
            pyzarlib.create_archive_from_streams(io.BytesIO(), [("f", broken)])
        else:
            pyzarlib.create_archive_from_streams(broken, [("f", io.BytesIO(b"x"))])

    archive = io.BytesIO()
    pyzarlib.create_archive_from_streams(archive, [("f", io.BytesIO(b"x"))])
    with pytest.raises(MarkerError, match=method):
        if method == "write":
            pyzarlib.extract_file_to_stream(archive, "f", broken)
        else:
            broken = BrokenStream(method, raises=True)
            broken.write(archive.getvalue())
            pyzarlib.extract_file_to_stream(broken, "f", io.BytesIO())


@pytest.mark.parametrize("value", [None, "text", b"x" * 65537])
def test_invalid_read_results(value):
    with pytest.raises((TypeError, ValueError)):
        pyzarlib.create_archive_from_streams(
            io.BytesIO(), [("f", BrokenStream("read", value))], block_size=65536
        )


@pytest.mark.parametrize("value", [None, 0, -1, True, 999999, "1"])
def test_invalid_write_results(value):
    with pytest.raises((TypeError, ValueError)):
        pyzarlib.create_archive_from_streams(BrokenStream("write", value), [])


@pytest.mark.parametrize("value", [None, -1, True, "0"])
def test_invalid_seek_results(value):
    with pytest.raises((TypeError, ValueError)):
        pyzarlib.create_archive_from_streams(BrokenStream("seek", value), [])


def test_invalid_options_and_partial_archive():
    for kwargs in ({"block_size": 0}, {"compression_level": 23}, {"aead": "invalid"},
                   {"encryption_scope": "invalid"}):
        with pytest.raises(ValueError):
            pyzarlib.create_archive_from_streams(io.BytesIO(), [], **kwargs)
    with pytest.raises(RuntimeError):
        pyzarlib.create_archive_from_streams(io.BytesIO(b"not empty"), [])
    archive = io.BytesIO()
    with pytest.raises(MarkerError):
        pyzarlib.create_archive_from_streams(archive, [("f", BrokenStream("read", raises=True))])
    assert archive.getvalue()

    class PartialOutput(io.BytesIO):
        def write(self, data):
            if self.tell():
                raise MarkerError("output")
            return super().write(data[:1])

    archive = io.BytesIO()
    pyzarlib.create_archive_from_streams(archive, [("f", io.BytesIO(b"content"))])
    output = PartialOutput()
    with pytest.raises(MarkerError, match="output"):
        pyzarlib.extract_file_to_stream(archive, "f", output)
    assert output.getvalue() == b"c"


def test_streams_are_not_flushed():
    class NoFlush(io.BytesIO):
        def flush(self):
            raise MarkerError("flush")

    archive = NoFlush()
    pyzarlib.create_archive_from_streams(archive, [("f", io.BytesIO(b"ok"))])
    output = NoFlush()
    pyzarlib.extract_file_to_stream(archive, "f", output)
    assert output.getvalue() == b"ok"


def test_independent_streams_are_thread_safe():
    def roundtrip(index):
        data = str(index).encode() * 10000
        archive = io.BytesIO()
        pyzarlib.create_archive_from_streams(archive, [("f", io.BytesIO(data))])
        output = io.BytesIO()
        pyzarlib.extract_file_to_stream(archive, "f", output)
        return output.getvalue() == data

    with ThreadPoolExecutor(max_workers=4) as executor:
        assert all(executor.map(roundtrip, range(8)))
