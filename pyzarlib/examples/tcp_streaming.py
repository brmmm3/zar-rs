"""Transfer one archive entry over TCP. For trusted networks only."""

import argparse
import os
import socket
import struct
from pathlib import Path

import pyzarlib

MAX_CHUNK = 65536


def read_exact(sock, length):
    chunks = bytearray()
    while len(chunks) < length:
        chunk = sock.recv(length - len(chunks))
        if not chunk:
            raise EOFError("connection closed before the transfer completed")
        chunks.extend(chunk)
    return bytes(chunks)


def read_length(sock):
    return struct.unpack("!I", read_exact(sock, 4))[0]


def send_length(sock, length):
    sock.sendall(struct.pack("!I", length))


class SocketWriter:
    def __init__(self, sock):
        self.sock = sock

    def write(self, data):
        for offset in range(0, len(data), MAX_CHUNK):
            chunk = data[offset:offset + MAX_CHUNK]
            send_length(self.sock, len(chunk))
            self.sock.sendall(chunk)
        return len(data)


def handle(conn, archive_path):
    length = read_length(conn)
    if not 0 < length <= MAX_CHUNK:
        raise ValueError("invalid entry path length")
    entry = read_exact(conn, length).decode("utf-8")
    with open(archive_path, "rb") as archive:
        count = pyzarlib.extract_file_to_stream(archive, entry, SocketWriter(conn))
    send_length(conn, 0)
    return entry, count


def serve(host, port, archive_path):
    with socket.create_server((host, port)) as listener:
        print(f"Listening on {listener.getsockname()}", flush=True)
        while True:
            conn, peer = listener.accept()
            with conn:
                try:
                    entry, count = handle(conn, archive_path)
                    print(f"{peer}: {entry} ({count} bytes)", flush=True)
                except (OSError, EOFError, UnicodeError, ValueError, RuntimeError) as error:
                    print(f"{peer}: {error}", flush=True)


def receive(host, port, entry, output_path):
    name = entry.encode("utf-8")
    if not 0 < len(name) <= MAX_CHUNK:
        raise ValueError("invalid entry path length")
    # Exclusive creation avoids replacing an existing file.
    output = open(output_path, "xb")
    try:
        with output:
            with socket.create_connection((host, port)) as conn:
                send_length(conn, len(name))
                conn.sendall(name)
                while True:
                    length = read_length(conn)
                    if length == 0:
                        break
                    if length > MAX_CHUNK:
                        raise ValueError("invalid data frame length")
                    output.write(read_exact(conn, length))
    except BaseException:
        os.unlink(output_path)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    server = commands.add_parser("server")
    server.add_argument("host")
    server.add_argument("port", type=int)
    server.add_argument("archive", type=Path)
    client = commands.add_parser("client")
    client.add_argument("host")
    client.add_argument("port", type=int)
    client.add_argument("entry")
    client.add_argument("output", type=Path)
    args = parser.parse_args()
    if args.command == "server":
        serve(args.host, args.port, args.archive)
    else:
        receive(args.host, args.port, args.entry, args.output)


if __name__ == "__main__":
    main()
