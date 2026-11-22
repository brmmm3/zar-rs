import importlib.util
import io
import socket
import struct
import threading
from pathlib import Path

import pytest

import pyzarlib


spec = importlib.util.spec_from_file_location(
    "tcp_streaming", Path(__file__).parents[1] / "examples" / "tcp_streaming.py"
)
tcp_streaming = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tcp_streaming)


def test_framed_writer_streams_multiple_chunks():
    left, right = socket.socketpair()
    data = b"x" * (tcp_streaming.MAX_CHUNK + 23)
    with left, right:
        worker = threading.Thread(target=tcp_streaming.SocketWriter(left).write, args=(data,))
        worker.start()
        received = bytearray()
        for expected in (tcp_streaming.MAX_CHUNK, 23):
            length = tcp_streaming.read_length(right)
            assert length == expected
            received.extend(tcp_streaming.read_exact(right, length))
        worker.join(timeout=5)
        assert not worker.is_alive()
        assert received == data


def test_client_rejects_truncated_transfer(tmp_path):
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        listener.listen()

        def truncated_server():
            conn, _ = listener.accept()
            with conn:
                length = tcp_streaming.read_length(conn)
                tcp_streaming.read_exact(conn, length)
                conn.sendall(struct.pack("!I", 3) + b"ab")

        worker = threading.Thread(target=truncated_server)
        worker.start()
        output = tmp_path / "output"
        with pytest.raises(EOFError):
            tcp_streaming.receive("127.0.0.1", listener.getsockname()[1], "file", output)
        worker.join(timeout=5)
        assert not worker.is_alive()
        assert not output.exists()


def test_streaming_roundtrip_over_socket(tmp_path):
    archive_path = tmp_path / "input.zar"
    expected = b"streamed content" * 10000
    with archive_path.open("w+b") as archive:
        pyzarlib.create_archive_from_streams(archive, [("dir/file", io.BytesIO(expected))])
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        listener.listen()

        def serve_one():
            conn, _ = listener.accept()
            with conn:
                tcp_streaming.handle(conn, archive_path)

        worker = threading.Thread(target=serve_one)
        worker.start()
        output = tmp_path / "output"
        tcp_streaming.receive("127.0.0.1", listener.getsockname()[1], "dir/file", output)
        worker.join(timeout=5)
        assert not worker.is_alive()
        assert output.read_bytes() == expected
        with pytest.raises(FileExistsError):
            tcp_streaming.receive("127.0.0.1", listener.getsockname()[1], "dir/file", output)
