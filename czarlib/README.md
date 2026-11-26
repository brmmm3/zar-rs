# czarlib

`czarlib` is the C ABI wrapper for the Rust `zarlib` archive library. It is
packaged as a workspace member in `czarlib/` and builds shared, static, and Rust
libraries. The public header is [`include/czarlib.h`](./include/czarlib.h).

The interface provides a JSON operation dispatcher and callback-based streaming
functions.

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

## Streaming

`czarlib_create_archive_from_readers` consumes named callback readers and writes
to an empty, seekable archive. `czarlib_extract_single_file_to_writer` reads a
seekable archive and writes one regular file to a callback writer. Callbacks are
synchronous and are not retained; short reads and writes are supported. See the
[header](./include/czarlib.h) for callback signatures, seek origins, status
codes, and pointer ownership. Callers must flush and close their streams; a
failure may leave partial output. Do not modify the archive during extraction.

The options argument is a UTF-8 JSON object. Creation accepts `block_size`
and `compression_level` either at the top level or in `encoding_options`, plus
`password`, `aead`, and `encryption_scope`. Block size must be 64 KiB to
32 MiB; automatic sizing (`0`) is not supported. Extraction accepts `password`,
per-commit `passwords`, and `to`. Creation does not deduplicate or preserve
filesystem metadata. See the [Rust streaming guide](../doc/zarlib-developer-guide.md#streaming-regular-files)
for format and integrity limitations.

Build and run the [file-backed example](./examples/stream.c):

```sh
cc -std=c11 -Wall -Wextra -Werror -I czarlib/include \
  czarlib/examples/stream.c -L target/debug -lczarlib \
  -Wl,-rpath,"$PWD/target/debug" -o /tmp/czarlib-stream
/tmp/czarlib-stream input.bin archive.zar data.bin restored.bin
```

The example overwrites the archive and output files. Its `fseek` adapter
rejects offsets outside the platform's `long` range.

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

`create_archive` and `add_archive` import the contents of `.zip`, `.7z`, `.rar`, `.iso`, `.tar`,
`.tar.gz`, `.tgz`, and `.gz` paths in their `paths` arrays, preserving the entry paths
inside each source archive. This behavior is shared by the C, C++, Go, C#, Java,
Python, and WASI bindings. Encrypted ZIP/7z/RAR sources and ZIP/7z/RAR links are not
supported. Multi-volume RAR archives and RAR special entries are rejected.
ISO import reads ISO 9660, preferring Rock Ridge over Joliet and the primary
namespace. ISO links, special entries, and UDF-only images are not supported.
ISO import is limited to 100,000 entries, 16 MiB of directory metadata, and
64 directory levels.
TAR symbolic links are preserved where the host supports them, hard
links are imported as regular files, and TAR special entries are not supported.

The C operation dispatcher also supports archive-level custom key/value
metadata:

```sh
zarc meta backup.zar --set project=example
zarc meta backup.zar
zarc meta backup.zar --remove project
```

The equivalent C requests use `read_custom_metadata` with
`{"archive":"backup.zar"}` or `update_custom_metadata` with a `set` object
and/or `remove` array:

```c
char *metadata = NULL;
char *error = NULL;
int status = czarlib_execute_json(
    "update_custom_metadata",
    "{\"archive\":\"backup.zar\",\"set\":{\"project\":\"example\",\"revision\":3}}",
    &metadata,
    &error
);
```

`read_custom_metadata` returns a JSON object; `update_custom_metadata`
returns JSON `null`. Values are strings, signed integers, or finite numbers.
Each update appends a commit without file changes and requires an unencrypted
archive. This metadata is separate from per-file permissions and timestamps.

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
| `create_archive` | `archive`, `paths` | `block_size`, `compression_level`, `strict`, `overwrite`, `save_metadata`, `save_modification_time`, `save_creation_time`, `save_access_time`, `include_special_files`, `verbose`, `password`, `aead`, `encryption_scope`, `include_files`, `exclude_files`, `include_folders`, `exclude_folders` |
| `add_archive` | `archive`, `paths` | Same encoding, scan, and filter fields as create; `password`, `passwords`, `aead`, `encryption_scope` |
| `remove_archive` | `archive`, `paths` | — |
| `join_archive` | `archive`, `from_id`, `to_id` | `block_size`, `compression_level` |
| `undo_archive` | `archive`, `commits` | — |
| `encrypt_archive_commits` | `archive`, `from_id`, `to_id`, `new_password` | `output`, `aead`, `encryption_scope`, `password`, `passwords`, `strict` |
| `decrypt_archive_commits` | `archive`, `from_id`, `to_id` | `output`, `password`, `passwords`, `strict` |
| `check_archive` | `archive` | `recover`, `password`, `passwords` |
| `info_archive` | `archive` | `verbose`, `password`, `passwords` |
| `cross_commit_references` | `archive` | `password`, `passwords` |
| `list_archive` | `archive` | `to`, `password`, `passwords` |
| `grep_archive` | `archive`, `file_filter`, `pattern` | `before`, `after`, `to`, `password`, `passwords` |
| `extract_archive` | `archive`, `destination` | `dry_run`, `to`, `password`, `passwords`, `collision`, `existing`, `restore_metadata`, `restore_symlinks`, `restore_devices`, file/folder filters |
| `extract_archive_to_memory` | `archive` | `to`, `password`, `passwords`, `size_limit`, `offset`, file/folder filters |

## Searching archive files

`grep_archive` searches regular files without extracting them. `archive`,
`file_filter`, and `pattern` are required JSON strings. The case-sensitive
`file_filter` glob selects archive-relative paths before searching; `"*"`
selects all regular files and `"*.txt"` also matches files in subdirectories.
`pattern` is a line-based `fancy_regex` byte regex, not a C regex. It supports
lookaround and backreferences; use `(?i)` for case-insensitive matching.
`before` and `after` are non-negative context-line counts, both defaulting
to zero. Overlapping context is returned once.

For an existing `backup.zar`, this complete C example prints the JSON result:

```c
#include "czarlib.h"
#include <stdio.h>

int main(void) {
    char *response = NULL;
    char *error = NULL;
    int status = czarlib_execute_json(
        "grep_archive",
        "{\"archive\":\"backup.zar\",\"file_filter\":\"*.txt\","
        "\"pattern\":\"(?i)TODO|FIXME\",\"before\":1,\"after\":1}",
        &response, &error);
    if (status != 0) {
        fprintf(stderr, "grep failed: %s\n", error ? error : "wrapper error");
    } else {
        puts(response);
    }
    czarlib_string_free(response);
    czarlib_string_free(error);
    return status;
}
```

Results are ordered by path and line number, for example:

```json
[{"path":"file.txt","lines":[{"line_number":1,"bytes":[104,105],"is_match":true}]}]
```

`line_number` is one-based; `bytes` is an array of byte values without the
LF or CRLF terminator, not a text string. `is_match` distinguishes matches
from context. Files without matches are omitted and no matches returns `[]`.
Non-UTF-8 data is preserved, although regex matching is Unicode-aware and
cannot match invalid bytes individually. Inline Unicode mode changes such as
`(?-u:...)` are not supported.

`to` optionally selects an eight-hex-digit commit ID (latest by default).
For encrypted archives, `password` is a UTF-8 fallback and `passwords` maps
commit IDs to UTF-8 passwords; a specific password takes precedence. Unlike
partial listing, grep fails if required headers or selected data cannot be
unlocked. Invalid regexes, authentication failures, and integrity errors are
reported as errors; keep the archive unchanged during the search.

Selected files are fully read and checked, including files with no matches;
unselected block payloads are not checked. Native searches run in parallel;
WASI searches sequentially. The JSON
dispatcher returns the complete result, not a stream; memory includes
returned lines, context, decoded blocks, and the longest line. See the
[Rust guide](../doc/zarlib-developer-guide.md#searching-files) for details.

## Other options

Timestamp options are opt-in. Creation- or access-time recording fails if a stored entry
has no available timestamp. Access times depend on filesystem policies and may
change during reads. Imported archives use staging-time filesystem
timestamps; extraction does not restore them. The old `save_dates` key is
rejected.

Supported block sizes are `0` (automatic) or 64 KiB through 32 MiB;
compression levels are -7 through 22. Filter values are arrays of fnmatch
patterns. The `remove_archive` paths are archive-relative names, not filesystem
paths to delete. Join endpoints are inclusive eight-hex-digit commit IDs.
`undo_archive` removes the given positive number of trailing commits. It
validates and atomically replaces the archive, and rejects any request that
would remove the first commit.

For password encryption, set `"password"` and optionally `"aead"` and
`"encryption_scope"` on a `create_archive` or `add_archive` request. The `aead`
value is `"aes-256-gcm"` (the default) or `"chacha20-poly1305"`. The scope is
`"data"`, `"header"`, or `"header-and-data"` (the default). Data-only encryption
leaves the logical header readable, so `info_archive` and `list_archive` can
inspect it without a password, while extraction of its data blocks still
requires the password. `info_archive` reports each commit's header/data
encryption status and AEAD algorithm.

Listing and extraction skip commits whose encrypted headers have no supplied
password, so their results can be a partial view of the archive. Extraction
still fails when a visible commit's data is encrypted and its password is
unavailable. An incorrect password fails authentication.

The optional `"passwords"` object maps eight-hex-digit commit IDs to passwords.
The `"password"` field remains a default password and is tried for every
encrypted commit without a specific entry. For example, callers can append a
new commit with `"password":"new-password"` and unlock an earlier commit with
`"passwords":{"00000001":"old-password"}`. Use the same `password` value alone
when all encrypted commits share a password. Read operations accept this same
shape.

`cross_commit_references` returns an array of objects with `referencing_commit`,
`source_commit`, and `path`. It requires passwords for all encrypted headers,
so the result is complete. `info_archive` includes a
`cross_commit_reference_count` for each commit; it is `null` when that header
is locked. The C++, Go, C#, Java, and WASI bindings share these JSON shapes.

`encrypt_archive_commits` and `decrypt_archive_commits` transform the inclusive
`from_id`–`to_id` range in physical commit order. Encryption uses
`new_password` for the transformed commits; `password` and `passwords` supply
the current passwords for commits that are already encrypted. Decryption uses
those same current-password fields. Both operations work in place by default.
Set `output` to write the result to another path; it must not identify the
input archive. If an encrypted commit in the selected range has no current
password, `strict: true` fails the operation, while the default `false` leaves
that commit unchanged. Wrong passwords fail in either mode. Rewriting is
staged and validated before the destination is replaced.

```json
{
  "archive": "backup.zar",
  "from_id": "00000002",
  "to_id": "00000003",
  "output": "encrypted-copy.zar",
  "new_password": "read-from-a-secure-prompt",
  "aead": "chacha20-poly1305",
  "encryption_scope": "header-and-data",
  "passwords": {"00000002": "old-commit-password"},
  "strict": true
}
```

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
/tmp/zarc-c join backup.zar --from 00000001 --to 00000002
```

The examples and integration test also demonstrate overwrite, file filters,
commit selection, extraction policies, in-memory extraction, encryption,
validation failures, and concurrent calls. Native `zarlib` tests remain in the
Rust crate and are still run by `cargo test -p zarlib`.
