# luazarlib

Lua 5.4 bindings for ZAR, using the [`czarlib` C ABI](../czarlib/README.md).
The table API exposes all C JSON operations, including archive creation,
history, metadata, encryption, checking, listing, grep, and extraction. Seekable
archive streams and individual file readers and writers are also supported.

## Requirements and build

The provided Makefile targets Linux. Install a C compiler, `make`, `pkg-config`,
Lua 5.4 with 64-bit integers and development headers, and
[dkjson](https://dkolf.de/dkjson-lua/) for Lua 5.4. On Ubuntu:

```sh
sudo apt-get install build-essential pkg-config lua5.4 liblua5.4-dev lua-dkjson
cargo build -p czarlib
make -C luazarlib
make -C luazarlib test
```

`czarlib` is loaded dynamically, so its shared library must remain available.
From the repository root, run an example with:

```sh
export LUA_PATH="$PWD/luazarlib/?.lua;;"
export LUA_CPATH="$PWD/luazarlib/build/?.so;;"
export LD_LIBRARY_PATH="$PWD/target/debug${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
lua5.4 luazarlib/examples/basic.lua backup.zar README.md
lua5.4 luazarlib/examples/stream.lua
```

The basic example creates a new archive and fails if it already exists. The
streaming example uses temporary files. For a release native library, build
`cargo build --release -p czarlib` and pass `CZARLIB_DIR=../target/release` to
`make`; also point `LD_LIBRARY_PATH` at `target/release`.

## Table and JSON APIs

```lua
local zar = require("luazarlib")
local result, err = zar.create_archive {
    archive = "backup.zar",
    paths = { "documents", "README.md" },
    password = "obtain-this-securely",
    aead = "chacha20-poly1305",
    encryption_scope = "header-and-data",
}
assert(result, err)
local entries = assert(zar.list_archive { archive = "backup.zar", password = "obtain-this-securely" })
for _, entry in ipairs(entries) do print(entry.path) end
```

Each named operation accepts a request table and returns a decoded response,
or `nil, error_message` on encoding or operation failure. The operation names
and request and response fields are documented in the
[`czarlib` guide](../czarlib/README.md#c-abi-and-error-handling). They are:
`create_archive`, `add_archive`, `remove_archive`, `read_custom_metadata`,
`update_custom_metadata`, `update_commit_details`, `join_archive`, `undo_archive`,
`encrypt_archive_commits`, `decrypt_archive_commits`, `check_archive`,
`info_archive`, `cross_commit_references`, `list_archive`, `diff_archive`, `grep_archive`, `extract_archive`,
and `extract_archive_to_memory`.

`execute(operation, request)` supports the same table interface.
`execute_json(operation, request_json)` instead accepts and returns JSON
strings and returns `nil, error_message` on operation failure. Invalid argument
types and strings containing NUL raise Lua errors. JSON inputs and paths must
be UTF-8. Passwords in requests are plaintext: do not log them.

Lua `nil` cannot represent JSON null in a table; use `zar.null`. Null responses
also decode to that sentinel. Nonempty sequential tables encode as arrays.
Use `zar.array({})` for an empty JSON array and `zar.object({})` for an empty
JSON object; the top-level request and options are always objects. JSON
responses use dkjson's table representations; `extract_archive_to_memory`
returns numeric byte arrays, not Lua strings. Lua's numeric representation can
lose precision for JSON integers outside its signed integer range.

## Searching archive files

Search an existing `backup.zar` with one line of context on each side:

```lua
local zar = require("luazarlib")
local files = assert(zar.grep_archive {
    archive = "backup.zar",
    file_filter = "*.txt",
    pattern = "(?i)TODO|FIXME",
    before = 1,
    after = 1,
})
for _, file in ipairs(files) do
    for _, line in ipairs(file.lines) do
        print(file.path, line.line_number, line.is_match, table.concat(line.bytes, ","))
    end
end
```

The table API returns decoded results or `nil, error_message`. Results are
grouped by archive path and omit files without matches; `line_number` is
one-based, `is_match` distinguishes matches from context, and `bytes` is a
numeric table of raw bytes without the line terminator, not necessarily
UTF-8 text. See the
[grep contract](../czarlib/README.md#searching-archive-files) for glob,
regex, commit, encryption, integrity, and memory behavior.

## Streaming

```lua
local archive = assert(io.open("stream.zar", "w+b"))
local input = assert(io.open("input.bin", "rb"))
assert(zar.create_archive_from_readers(archive, {
    { path = "data/input.bin", reader = input },
}, { block_size = 65536 }))
assert(archive:flush())
local output = assert(io.open("output.bin", "wb"))
local count = assert(zar.extract_single_file_to_writer(archive, "data/input.bin", output))
assert(output:close())
assert(input:close())
assert(archive:close())
```

Creation requires an initially empty writable and seekable archive, not opened
in append mode. Each input names a regular file and supplies a reader.
Extraction requires a readable and seekable archive and a writer. Lua file
handles work directly; custom stream objects must provide these methods:

- `read(max_bytes)` returns a string of at most `max_bytes` bytes, or `nil`
  (or an empty string) at EOF; return `nil, message` or raise on error.
- `write(bytes)` returns a byte count (short writes are retried), or `true`
  or a stream object to indicate that all bytes were accepted. Return
  `nil, message` or raise on error.
- `seek(whence, offset)` returns the resulting nonnegative absolute position.
  `whence` is `set`, `cur`, or `end`, as with Lua file handles.

Creation returns `true` or `nil, error`. Extraction returns the byte count or
`nil, error, bytes_written`; a failure may leave partial output. Callbacks run
synchronously and must not yield. The caller owns, flushes, and closes all
streams. Do not modify the archive during extraction.

Options are the same as the C streaming API: `block_size`,
`compression_level`, `password`, `aead`, and `encryption_scope` for creation,
and `password`, `passwords`, and `to` for extraction. The nested
`encoding_options` form is also accepted. Streaming does not deduplicate or
preserve filesystem metadata. See the [C streaming guide](../czarlib/README.md#streaming)
for block-size and integrity constraints.
