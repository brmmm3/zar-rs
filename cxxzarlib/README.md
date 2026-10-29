# cxxzarlib

`cxxzarlib` provides a C++ interface to `zarlib` using the
[CXX bridge](https://cxx.rs/). It exposes the same JSON operation dispatcher as
[`czarlib`](../czarlib/README.md), but uses `rust::Str`, `rust::String`, and
`rust::Error` at the C++ boundary instead of manually managed C strings.
All nine C-wrapper operations are supported: `create_archive`, `add_archive`,
`remove_archive`, `join_archive`, `check_archive`, `info_archive`,
`list_archive`, `extract_archive`, and `extract_archive_to_memory`. Request
fields, validation, output shapes, password handling, filters, and archive
semantics are shared with `czarlib`.

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
BRIDGE_INCLUDE=$(find target/debug/build/cxxzarlib \
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
/tmp/zarc-cxx join backup.zar --from 00000001 --through 00000002
```

The CXX include directory is part of the installed `cxx` crate. If you use a
non-default Cargo target directory, update the bridge-header path accordingly.

## Tests

The native C++ integration test exercises the bridge and all supported
operations, including create/add/remove/join, info/list/check, filesystem and
in-memory extraction, filters, existing-file behavior, AES-GCM and
ChaCha20-Poly1305 encryption, per-commit passwords, error handling, and
concurrent calls.

```sh
cargo build -p cxxzarlib
BRIDGE_INCLUDE=$(find target/debug/build/cxxzarlib \
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
