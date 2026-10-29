# cxxzarlib

`cxxzarlib` provides a C++ interface to `zarlib` using the
[CXX bridge](https://cxx.rs/). It exposes the same JSON operation dispatcher as
[`czarlib`](../czarlib/README.md), but uses `rust::Str`, `rust::String`, and
`rust::Error` at the C++ boundary instead of manually managed C strings.

## Build

From the repository root:

```sh
cargo build -p cxxzarlib
```

Cargo generates the bridge header below
`target/debug/build/cxxzarlib-*/out/cxxbridge/include/cxxzarlib/src/lib.rs.h`
and a static library at `target/debug/libcxxzarlib.a`.

The C++ example uses the header and library as follows:

```sh
BRIDGE_INCLUDE=$(find target/debug/build/cxxzarlib \
  -path '*/out/cxxbridge/include' -type d -print -quit)
CXX_INCLUDE=$(find "$HOME/.cargo/registry/src" \
  -path '*/cxx-*/include' -type d -print -quit)
c++ -std=c++14 -Wall -Wextra -Werror \
  -I "$BRIDGE_INCLUDE" -I "$CXX_INCLUDE" \
  cxxzarlib/examples/info.cpp target/debug/libcxxzarlib.a \
  -ldl -lpthread -lm -o /tmp/cxxzarlib-info
/tmp/cxxzarlib-info archive.zar
```

The CXX include directory is part of the installed `cxx` crate. If you use a
non-default Cargo target directory, update the bridge-header path accordingly.

## API

The generated header declares:

```cpp
rust::String cxxzarlib::execute_json(
    rust::Str operation,
    rust::Str request_json
);
```

Call it with any operation and JSON request supported by the C binding. The
supported operations and fields are documented in
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
