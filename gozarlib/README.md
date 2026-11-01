# gozarlib

`gozarlib` is the Go wrapper for the Rust `zarlib` archive library. It calls
the `czarlib` C ABI through cgo and provides both a generic JSON dispatcher
and Go functions for every C-wrapper operation:

- `CreateArchive`
- `AddArchive`
- `RemoveArchive`
- `JoinArchive`
- `CheckArchive`
- `InfoArchive`
- `ListArchive`
- `ExtractArchive`
- `ExtractArchiveToMemory`

The request fields, validation, response shapes, encryption and archive
semantics are shared with `czarlib`. See
[`czarlib/README.md`](../czarlib/README.md) for all request options, filter
formats, encryption scope, per-commit password behavior, and limitations.
Create and Add requests may include `.zip`, `.7z`, `.tar`, or `.tar.gz` source
paths; their contents are imported into the resulting ZAR using the source
archive's internal paths.

## Requirements and build

Install Go with cgo enabled, a C compiler, and the Rust toolchain. Build the
native C ABI library from the repository root before running Go commands:

```sh
cargo build -p czarlib
cd gozarlib
go test ./...
```

The cgo configuration links `target/debug/libczarlib.so` on Linux and
`target/debug/libczarlib.dylib` on macOS, with a runtime search path pointing
to the repository's `target/debug` directory. For a release build, run
`cargo build --release -p czarlib`, then adjust the `-L` and rpath settings in
`gozarlib.go` to point to `target/release`.

The Go package currently uses the dynamically linked C ABI library; make sure
it remains available at runtime. cgo must not be disabled.

## Go API

Pass request values as Go maps or structs. The response is JSON and can be
decoded into application-specific types, or inspected as `json.RawMessage`.
Errors from request validation and archive operations are returned as Go
`error` values.

```go
response, err := gozarlib.CreateArchive(map[string]any{
    "archive": "backup.zar",
    "paths":   []string{"src", "README.md"},
    "block_size": 8 * 1024 * 1024,
})
if err != nil {
    return err
}
fmt.Println(string(response))
```

`ExecuteJSON(operation, requestJSON)` is also available for callers that
already have a JSON request. It rejects invalid JSON and non-object requests
before crossing the C boundary. `ExtractArchiveToMemory` returns a JSON object
whose values are byte arrays; decode it into `map[string][]byte`:

```go
response, err := gozarlib.ExtractArchiveToMemory(map[string]any{
    "archive": "backup.zar",
    "offset":  10,
    "size_limit": 4096,
})
if err != nil {
    return err
}
var files map[string][]byte
if err := json.Unmarshal(response, &files); err != nil {
    return err
}
```

Password fields in request values are plaintext credentials. Obtain them
securely, do not log password-bearing requests, and clear mutable credential
buffers when practical.

## Examples

The direct API example creates an archive, lists and checks it, extracts
contents to memory, and displays archive information:

```sh
go run ./examples/basic backup.zar ./README.md
```

The `zarc`-style example supports the C example's commands and aliases, plus
`memory` for in-memory extraction:

```sh
go run ./examples/zarc create backup.zar src/ README.md
go run ./examples/zarc add backup.zar config.toml
go run ./examples/zarc list backup.zar
go run ./examples/zarc info backup.zar
go run ./examples/zarc check backup.zar
go run ./examples/zarc extract backup.zar restored
go run ./examples/zarc memory backup.zar
go run ./examples/zarc remove backup.zar obsolete.txt
go run ./examples/zarc join backup.zar --from 00000001 --through 00000002
```

The tests cover all nine operations, filters, extraction policies, memory
offset/size limits, both supported AEAD algorithms, per-commit passwords,
validation and failure cases, and concurrent calls.
