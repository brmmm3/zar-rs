# csharpzarlib

C# (.NET 10) wrapper for ZAR. It P/Invokes the `czarlib` JSON C ABI and exposes
archive operations for create, add, remove, custom metadata, join, undo,
encrypt/decrypt commit ranges, check, info, list, extract and in-memory extract.
Request and response fields are documented in
[../czarlib/README.md](../czarlib/README.md).
Create and Add requests may include `.zip`, `.7z`, `.tar`, `.tar.gz`, or `.tgz`
source paths; their contents are imported into the resulting ZAR using the
source archive's internal paths.

`ZarLibrary.CrossCommitReferences(request)` returns the shared JSON reference
inventory. `InfoArchive` reports per-commit cross-reference counts.

The C# wrapper provides `ReadCustomMetadata` and `UpdateCustomMetadata`:

```csharp
ZarLibrary.UpdateCustomMetadata(new
{
    archive = "backup.zar",
    set = new Dictionary<string, object>
    {
        ["project"] = "example",
        ["revision"] = 3,
    },
});
var metadata = ZarLibrary.ReadCustomMetadata(new { archive = "backup.zar" });
```

`ReadCustomMetadata` returns a dictionary of `JsonElement` values.
`UpdateCustomMetadata` accepts a `set` object and/or `remove` array and appends
a commit without file changes. Values are strings, signed integers, or finite
numbers; updates require an unencrypted archive. See the
[`czarlib` guide](../czarlib/README.md) for the request schema.

## NuGet and native library

The default `ZarLib` NuGet package contains the managed wrapper only. Install
or build a compatible `czarlib` shared library separately for the application's
operating system and architecture. Its filename must be `czarlib.dll` on
Windows, `libczarlib.so` on Linux, or `libczarlib.dylib` on macOS. Put it in
the application directory or set `ZAR_LIBRARY` to its absolute path. Otherwise
native calls fail with a library-loading error.

The release workflow also builds the `ZarLib.x64` and `ZarLib.arm64` packages
with native binaries for `win-x64` and `win-arm64`, respectively. Packages
built with `NativeLibraryPath` include only the library for the specified
`RuntimeIdentifier`; other runtimes still need their own compatible library.
For example, to bundle a locally built Linux x64 library:

```sh
cargo build --release -p czarlib
dotnet pack csharpzarlib/ZarLib/ZarLib.csproj -c Release \
  -p:RuntimeIdentifier=linux-x64 \
  -p:NativeLibraryPath="$PWD/target/release/libczarlib.so"
```

Build the native library for the selected runtime and architecture before
packing; the packaging process does not cross-compile or verify its architecture.

## Build the native library

```sh
cargo build -p czarlib          # target/debug/libczarlib.so
```

The library is located via `ZAR_LIBRARY` (full path), the application directory,
or `target/{debug,release}` in any parent directory of the working or
application directory.

## API

```csharp
using Zar;

ZarLibrary.CreateArchive(new { archive = "a.zar", paths = new[] { "input" }, password = "secret" });
string listing = ZarLibrary.ListArchive(new { archive = "a.zar", password = "secret" });
Dictionary<string, byte[]> files = ZarLibrary.ExtractArchiveToMemory(new { archive = "a.zar", password = "secret" });
```

- Requests are JSON strings or any object serializable to a JSON object (null
  properties are omitted). Responses are JSON strings.
- `ZarLibrary.Execute(operation, request)` and `ExecuteJson(operation, json)`
  call any operation directly. `ReadCustomMetadata` returns a typed map of
  `JsonElement` values; `UpdateCustomMetadata` takes `set` and/or `remove`
  fields in its request object.
- Failures throw `ZarException` (`Status`: 1 request/archive error, 2 invalid
  pointer/allocation, 3 Rust panic, 0 wrapper-side validation).
- Calls are thread-safe. Encryption options (`password`, `passwords` per commit
  ID, `aead`, `encryption_scope`) behave as in the C wrapper.
- `EncryptArchiveCommits` uses `new_password` for transformed commits and
  `password`/`passwords` to unlock already-encrypted commits.
  `DecryptArchiveCommits` uses the latter fields to unlock commits being
  decrypted.

## Tests and examples

```sh
cd ZarLib.Tests && dotnet test     # encrypted tests take about a minute in debug builds
cd examples/Basic && dotnet run
cd examples/Zarc && dotnet run -- c demo.zar somedir
cd examples/Zarc && dotnet run -- l demo.zar
```

The `Zarc` example supports `create|c`, `add|a`, `remove|r`, `join|j`,
`check|k`, `info|i`, `list|l`, `extract|e` (`--dry-run`) and `memory`.
