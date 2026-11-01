# czarlib

`czarlib` is the C ABI wrapper for the Rust `zarlib` archive library. It is
packaged as a workspace member in `czarlib/` and builds shared, static, and Rust
libraries. The public header is [`include/czarlib.h`](./include/czarlib.h).

The interface uses one operation dispatcher with JSON requests and responses.
This keeps the exported ABI small and stable while providing the same archive
operations and result fields as `pyzarlib`.

## Build

From the repository root:

```sh
cargo build -p czarlib
```

The shared library is `target/debug/libczarlib.so` on Linux,
`target/debug/libczarlib.dylib` on macOS, and
`target/debug/czarlib.dll` on Windows. The static library is also produced.

Build and run the C integration tests:

```sh
cargo build -p czarlib
cc -std=c11 -Wall -Wextra -Werror -I czarlib/include \
  czarlib/tests/test_c_api.c -L target/debug -lczarlib \
  -Wl,-rpath,"$PWD/target/debug" -pthread -o /tmp/czarlib-test
/tmp/czarlib-test
```

Build the `zarc`-style C example:

```sh
cc -std=c11 -Wall -Wextra -Werror -I czarlib/include \
  czarlib/examples/zarc.c -L target/debug -lczarlib \
  -Wl,-rpath,"$PWD/target/debug" -o /tmp/zarc-c
/tmp/zarc-c --help
```

## C ABI and error handling

Call `czarlib_execute_json(operation, request_json, &response, &error)`. On
success it returns `0` and sets `response` to an allocated JSON string. On
request or archive failure it returns `1` and sets `error`. Both output values
are initialized to null before the operation starts. Release either returned
string with `czarlib_string_free`; do not call `free()` on it.

The operation dispatcher does not retain pointers from the request and is safe
to call concurrently from multiple threads. Input and output strings must be
UTF-8. Passwords are UTF-8 strings in JSON; callers using non-UTF-8 password
bytes should use the Rust or Python APIs. A request containing a password
contains plaintext credentials; avoid logging it and clear mutable request
buffers after use.

`create_archive` and `add_archive` import the contents of `.zip` and `.7z`
paths in their `paths` arrays, preserving the entry paths inside each source
archive. This behavior is shared by the C, C++, Go, C#, Java, Python, and WASI
bindings. Encrypted source archives and symbolic links are not supported.

```c
#include "czarlib.h"
#include <stdio.h>

int main(void) {
    const char *request =
        "{\"archive\":\"backup.zar\",\"paths\":[\"src\",\"README.md\"]}";
    char *response = NULL;
    char *error = NULL;
    int status = czarlib_execute_json(
        "create_archive", request, &response, &error);
    if (status != 0) {
        fprintf(stderr, "create failed: %s\n", error ? error : "wrapper error");
        czarlib_string_free(error);
        return status;
    }
    puts(response);
    czarlib_string_free(response);
    return 0;
}
```

All requests are JSON objects. File paths are strings; `paths` must be an
array. Numeric and boolean values are validated rather than silently coerced.
The output shapes follow the corresponding Python API:

| Operation | Required request fields | Useful optional fields |
|---|---|---|
| `create_archive` | `archive`, `paths` | `block_size`, `compression_level`, `strict`, `overwrite`, `save_metadata`, `save_dates`, `include_special_files`, `verbose`, `password`, `aead`, `encryption_scope`, `include_files`, `exclude_files`, `include_folders`, `exclude_folders` |
| `add_archive` | `archive`, `paths` | Same encoding, scan, and filter fields as create; `password`, `passwords`, `aead`, `encryption_scope` |
| `remove_archive` | `archive`, `paths` | — |
| `join_archive` | `archive`, `from_id`, `through_id` | `block_size`, `compression_level` |
| `check_archive` | `archive` | `recover`, `password`, `passwords` |
| `info_archive` | `archive` | `verbose` |
| `list_archive` | `archive` | `through`, `password`, `passwords` |
| `extract_archive` | `archive`, `destination` | `dry_run`, `through`, `password`, `passwords`, `collision`, `existing`, `restore_metadata`, `restore_symlinks`, `restore_devices`, file/folder filters |
| `extract_archive_to_memory` | `archive` | `through`, `password`, `passwords`, `size_limit`, `offset`, file/folder filters |

Supported block sizes are `0` (automatic) or 64 KiB through 32 MiB;
compression levels are -7 through 22. Filter values are arrays of fnmatch
patterns. The `remove_archive` paths are archive-relative names, not filesystem
paths to delete. Join endpoints are inclusive eight-hex-digit commit IDs.

For password encryption, set `"password"` and optionally `"aead"` and
`"encryption_scope"` on a `create_archive` or `add_archive` request. The `aead`
value is `"aes-256-gcm"` (the default) or `"chacha20-poly1305"`. The scope is
`"data"`, `"header"`, or `"header-and-data"` (the default). Data-only encryption
leaves the logical header readable, so `info_archive` and `list_archive` can
inspect it without a password, while extraction of its data blocks still
requires the password. `info_archive` reports each commit's header/data
encryption status and AEAD algorithm.

The optional `"passwords"` object maps eight-hex-digit commit IDs to passwords.
The `"password"` field remains a default password and is tried for every
encrypted commit without a specific entry. For example, callers can append a
new commit with `"password":"new-password"` and unlock an earlier commit with
`"passwords":{"00000001":"old-password"}`. Use the same `password` value alone
when all encrypted commits share a password. Read operations accept this same
shape.

```c
char *response = NULL;
char *error = NULL;
int status = czarlib_execute_json(
    "create_archive",
    "{\"archive\":\"secret.zar\",\"paths\":[\"secrets\"],"
    "\"password\":\"read-from-a-secure-prompt\","
    "\"aead\":\"chacha20-poly1305\"}",
    &response,
    &error
);
if (status != 0) {
    fprintf(stderr, "create failed: %s\n", error ? error : "wrapper error");
    czarlib_string_free(error);
    return status;
}
czarlib_string_free(response);

status = czarlib_execute_json(
    "extract_archive",
    "{\"archive\":\"secret.zar\",\"destination\":\"restored\","
    "\"password\":\"read-from-a-secure-prompt\"}",
    &response,
    &error
);
if (status != 0) {
    fprintf(stderr, "extract failed: %s\n", error ? error : "wrapper error");
    czarlib_string_free(error);
    return status;
}
czarlib_string_free(response);
```

In production code, populate password fields from a secure prompt, do not
hardcode it in the request, and avoid logging the JSON request because it
contains the plaintext password. Fresh key and nonce material is generated
internally and the password-derived wrapping key uses Argon2id. `add_archive`
can append a new encrypted commit when supplied credentials for any encrypted
history it validates. `remove_archive` and `join_archive` still reject
encrypted archives because encrypted stateful TOCs are not rewritten.

`extract_archive_to_memory` returns a JSON object mapping archive paths to
arrays of byte values, since JSON has no byte-string type. Its optional
`offset` applies to every file and `size_limit` caps the number of bytes
returned per file after the offset.

## Examples

The small example program is a C counterpart to the Python `zarc-py` example.
It supports the familiar command names and aliases:

```sh
/tmp/zarc-c create backup.zar src/ README.md
/tmp/zarc-c add backup.zar config.toml
/tmp/zarc-c list backup.zar
/tmp/zarc-c info backup.zar
/tmp/zarc-c check backup.zar
/tmp/zarc-c extract backup.zar restored
/tmp/zarc-c remove backup.zar obsolete.txt
/tmp/zarc-c join backup.zar --from 00000001 --through 00000002
```

The examples and integration test also demonstrate overwrite, file filters,
commit selection, extraction policies, in-memory extraction, encryption,
validation failures, and concurrent calls. Native `zarlib` tests remain in the
Rust crate and are still run by `cargo test -p zarlib`.
