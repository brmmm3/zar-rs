# csharpzarlib

C# (.NET 10) wrapper for ZAR. It P/Invokes the `czarlib` JSON C ABI and exposes
all nine operations: create, add, remove, join, check, info, list, extract and
in-memory extract. Request and response fields are documented in
[../czarlib/README.md](../czarlib/README.md).
Create and Add requests may include `.zip`, `.7z`, `.tar`, or `.tar.gz` source
paths; their contents are imported into the resulting ZAR using the source
archive's internal paths.

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
- `ZarLibrary.Execute(operation, request)` and `ExecuteJson(operation, json)` call
  any operation directly.
- Failures throw `ZarException` (`Status`: 1 request/archive error, 2 invalid
  pointer/allocation, 3 Rust panic, 0 wrapper-side validation).
- Calls are thread-safe. Encryption options (`password`, `passwords` per commit
  ID, `aead`, `encryption_scope`) behave as in the C wrapper.

## Tests and examples

```sh
cd ZarLib.Tests && dotnet test     # encrypted tests take about a minute in debug builds
cd examples/Basic && dotnet run
cd examples/Zarc && dotnet run -- c demo.zar somedir
cd examples/Zarc && dotnet run -- l demo.zar
```

The `Zarc` example supports `create|c`, `add|a`, `remove|r`, `join|j`,
`check|k`, `info|i`, `list|l`, `extract|e` (`--dry-run`) and `memory`.
