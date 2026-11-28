# cxxzarlib

`cxxzarlib` provides a C++ interface to `zarlib` using the
[CXX bridge](https://cxx.rs/). It exposes the same JSON operation dispatcher as
[`czarlib`](../czarlib/README.md), but uses `rust::Str`, `rust::String`, and
`rust::Error` at the C++ boundary instead of manually managed C strings. Its
generic dispatcher supports all `czarlib` operations, including custom metadata
read and update. Request fields, validation, output shapes, password handling,
filters, and archive semantics are shared with `czarlib`. Create and Add
requests may include `.zip`, `.7z`, `.tar`, `.tar.gz`, or `.tgz` source paths;
their contents are imported into the resulting ZAR using the source archive's
internal paths.

Call `execute_json("cross_commit_references", request)` to enumerate references;
the request and result are described in the C binding guide.
Call `execute_json("grep_archive", request)` to search files; its JSON schema
is documented in the C binding guide.
Call `execute_json("diff_archive", request)` to compare commits using the
[C diff schema](../czarlib/README.md).
Call `execute_json("update_commit_details", request)` to edit commit details
using the [C request schema](../czarlib/README.md).

The C++ wrapper's `execute_json` dispatcher supports archive-level custom
key/value metadata through `read_custom_metadata` and
`update_custom_metadata`. For example:

```cpp
auto response = cxxzarlib::execute_json(
    "update_custom_metadata",
    R"({"archive":"backup.zar","set":{"project":"example","revision":3}})");
```

`read_custom_metadata` returns the effective values as a JSON object.
`update_custom_metadata` accepts a `set` object and/or `remove` array, returns
JSON `null`, and appends a commit without file changes. Values are strings,
signed integers, or finite numbers; metadata updates require an unencrypted
archive. See the [`czarlib` guide](../czarlib/README.md) for the full request
shape.

## Build

From the repository root:

```sh
cargo build -p cxxzarlib
```

Cargo generates the bridge header below
`target/debug/build/cxxzarlib-*/out/cxxbridge/include/cxxzarlib/src/lib.rs.h`
and a static library at `target/debug/libcxxzarlib.a`.

Build the C++ command-line example, which supports the same commands and
shortcuts as the C example:

```sh
BRIDGE_INCLUDE=$(find target/debug/build/cxxzarlib-* \
  -path '*/out/cxxbridge/include' -type d -print -quit)
CXX_INCLUDE=$(find "$HOME/.cargo/registry/src" \
  -path '*/cxx-*/include' -type d -print -quit)
c++ -std=c++14 -Wall -Wextra -Werror \
  -I "$BRIDGE_INCLUDE" -I "$CXX_INCLUDE" \
  cxxzarlib/examples/zarc.cpp target/debug/libcxxzarlib.a \
  -ldl -lpthread -lm -o /tmp/zarc-cxx
```

Example commands:

```sh
/tmp/zarc-cxx create backup.zar src/ README.md
/tmp/zarc-cxx add backup.zar config.toml
/tmp/zarc-cxx list backup.zar
/tmp/zarc-cxx info backup.zar
/tmp/zarc-cxx check backup.zar
/tmp/zarc-cxx extract backup.zar restored
/tmp/zarc-cxx remove backup.zar obsolete.txt
/tmp/zarc-cxx join backup.zar --from 00000001 --to 00000002
```

The CXX include directory is part of the installed `cxx` crate. If you use a
non-default Cargo target directory, update the bridge-header path accordingly.

## Tests

The native C++ integration test exercises the bridge and supported operations,
including custom metadata, create/add/remove/join/undo, info/list/check,
filesystem and in-memory extraction, commit encryption/decryption, filters,
existing-file behavior, AES-GCM and ChaCha20-Poly1305 encryption, per-commit
passwords, error handling, and concurrent calls.

```sh
cargo build -p cxxzarlib
BRIDGE_INCLUDE=$(find target/debug/build/cxxzarlib-* \
  -path '*/out/cxxbridge/include' -type d -print -quit)
CXX_INCLUDE=$(find "$HOME/.cargo/registry/src" \
  -path '*/cxx-*/include' -type d -print -quit)
c++ -std=c++17 -Wall -Wextra -Werror \
  -I "$BRIDGE_INCLUDE" -I "$CXX_INCLUDE" \
  cxxzarlib/tests/test_cpp_api.cpp target/debug/libcxxzarlib.a \
  -ldl -lpthread -lm -o /tmp/cxxzarlib-test
/tmp/cxxzarlib-test
```

The separate `info.cpp` example demonstrates direct API use. Compile it with
the same include paths and linker flags, substituting
`cxxzarlib/examples/info.cpp` and `/tmp/cxxzarlib-info` in the command above.
The `zarc.cpp` example provides command-style use with `create|c`, `add|a`,
`remove|r`, `join|j`, `check|k`, `info|i`, `list|l`, and `extract|e`.

## API

The generated header declares:

```cpp
rust::String cxxzarlib::execute_json(
    rust::Str operation,
    rust::Str request_json
);
```

Call it with any operation and JSON request supported by the C binding. Every
operation and its fields are documented in
[`czarlib/README.md`](../czarlib/README.md). For example:

```cpp
#include "cxxzarlib/src/lib.rs.h"
#include "rust/cxx.h"
#include <iostream>

int main() {
    try {
        auto response = cxxzarlib::execute_json(
            "list_archive",
            R"({"archive":"backup.zar"})");
        std::cout << response.c_str() << '\n';
    } catch (const rust::Error &error) {
        std::cerr << "archive operation failed: " << error.what() << '\n';
        return 1;
    }
}
```

Malformed requests, unsupported operations, and archive-operation failures are
reported as `rust::Error` exceptions. The bridge does not retain either input
string. Password-bearing JSON requests contain plaintext credentials: obtain
passwords securely and do not log those request strings.

## Searching archive files

For an existing `backup.zar`, search `.txt` paths for a case-insensitive
regex and include one line of context on each side:

```cpp
#include "cxxzarlib/src/lib.rs.h"
#include "rust/cxx.h"
#include <iostream>

int main() {
    try {
        auto result = cxxzarlib::execute_json(
            "grep_archive",
            R"({"archive":"backup.zar","file_filter":"*.txt","pattern":"(?i)TODO|FIXME","before":1,"after":1})");
        std::cout << result.c_str() << '\n';
    } catch (const rust::Error &error) {
        std::cerr << "grep failed: " << error.what() << '\n';
        return 1;
    }
}
```

The response is JSON grouped by archive path. Each line has a one-based
`line_number`, a `bytes` array, and `is_match` to distinguish matches from
context; files without matches are omitted. Use a JSON parser to consume the
result and preserve the byte arrays rather than assuming UTF-8. See the
[grep contract](../czarlib/README.md#searching-archive-files) for glob,
regex, commit, encryption, integrity, and memory behavior.

## C++ streams

The native wrapper in `include/cxxzarlib/stream.hpp` provides
`create_archive_from_readers` and
`extract_single_file_to_writer` using standard C++ streams. It uses the C
streaming ABI in `czarlib`, is built into `libcxxzarlib.a`, and does not
change the existing CXX bridge.

```cpp
#include "cxxzarlib/stream.hpp"
#include <sstream>

std::istringstream input("hello");
std::stringstream archive(std::ios::in | std::ios::out | std::ios::binary);
cxxzarlib::create_archive_from_readers(archive, {{"hello.txt", input}});
std::ostringstream output;
auto bytes = cxxzarlib::extract_single_file_to_writer(archive, "hello.txt", output);
```

Both calls are synchronous and retain no references. Input paths are UTF-8
archive paths; the readers and archive/output streams must remain alive during
the call. The archive must start at offset zero and be seekable: creation
requires an initially empty writable archive (not append mode), and extraction
requires a readable archive. Use binary mode with file streams. Inputs and
extraction output need not be seekable. Creation handles regular files, not
filesystem metadata or deduplication. Transfers are bounded to 64 KiB per C++
callback; the Rust implementation buffers compression blocks, not entire files.

An optional JSON object uses the C streaming options: creation accepts
`encoding_options` (including `block_size` and `compression_level`), `password`,
`aead`, and `encryption_scope`; extraction accepts `password`, `passwords`,
and `to`. For example:

```cpp
auto options = R"({"encoding_options":{"block_size":65536},"password":"secret","aead":"aes-256-gcm","encryption_scope":"header-and-data"})";
```

Streaming creation requires a nonzero block size from 64 KiB through 32 MiB;
see the [`czarlib` guide](../czarlib/README.md) for all option values and
password semantics. Calls return normally on success; extraction returns the
number of bytes written. Invalid arguments throw `std::invalid_argument` and
operation, stream, and seek failures throw `std::runtime_error`. C++ exceptions
never cross the C callback boundary. An error can leave partial output: discard
it or stage it before publishing. The caller owns, flushes, and closes all
streams; flush errors are the caller's responsibility. Paths and JSON cannot
contain embedded NUL bytes.

Build and run the complete streaming example and focused native test from the
repository root:

```sh
cargo build -p cxxzarlib
c++ -std=c++14 -Wall -Wextra -Werror \
  -I cxxzarlib/include cxxzarlib/examples/stream.cpp \
  target/debug/libcxxzarlib.a -ldl -lpthread -lm -o cxxzarlib/stream-example
./cxxzarlib/stream-example
c++ -std=c++14 -Wall -Wextra -Werror \
  -I cxxzarlib/include cxxzarlib/tests/test_stream.cpp \
  target/debug/libcxxzarlib.a -ldl -lpthread -lm -o cxxzarlib/stream-test
./cxxzarlib/stream-test
BRIDGE_INCLUDE=$(find target/debug/build/cxxzarlib-* \
  -path '*/out/cxxbridge/include' -type d -print -quit)
CXX_INCLUDE=$(find "$HOME/.cargo/registry/src" \
  -path '*/cxx-*/include' -type d -print -quit)
c++ -std=c++14 -Wall -Wextra -Werror \
  -I cxxzarlib/include -I "$BRIDGE_INCLUDE" -I "$CXX_INCLUDE" \
  cxxzarlib/tests/test_stream_bridge.cpp target/debug/libcxxzarlib.a \
  -ldl -lpthread -lm -o cxxzarlib/stream-bridge-test
./cxxzarlib/stream-bridge-test
```

`examples/stream.cpp` is a runnable version of the snippet. The existing
`execute_json` API and its build instructions remain unchanged.
