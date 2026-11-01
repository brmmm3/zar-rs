# jzarlib-ffm

`jzarlib-ffm` is the Java Foreign Function & Memory API binding to `czarlib`.
It requires Java 25 or newer and exposes the same nine JSON operations:
create, add, remove, join, check, info, list, filesystem extract, and
in-memory extract. See [`czarlib/README.md`](../czarlib/README.md) for all
request fields, filters, encryption settings, and support boundaries.
Create and Add requests may include `.zip`, `.7z`, `.tar`, or `.tar.gz` source
paths; their contents are imported into the resulting ZAR using the source
archive's internal paths.

## Build and test

Build the C ABI shared library, then run the Java tests:

```sh
cargo build -p czarlib
cd jzarlib-ffm
mvn test
```

By default the Java binding loads `../target/debug/libczarlib.so` on Linux
(or the platform-specific name) relative to the module working directory.
Override it with `-Dzar.library=/absolute/path/to/libczarlib.so`. The test
runtime enables native access for the FFM API.

## API

Each operation is available as a static Java method accepting a UTF-8 JSON
object string, for example:

```java
String response = ZarLibrary.createArchive(
    "{\"archive\":\"backup.zar\",\"paths\":[\"src\"]}");
```

`ZarLibrary.executeJson(operation, request)` is the generic dispatcher.
Success returns the JSON response string; request and archive failures throw
`ZarException`. In-memory extraction returns a JSON object whose values are
byte arrays. Password values in request JSON are plaintext secrets; obtain
them securely and do not log those request strings.

## Examples

Compile the examples with Maven and run them from this module directory:

```sh
mvn package
java --enable-native-access=ALL-UNNAMED \
  -cp target/classes org.zar.jarlib.ffm.examples.BasicExample backup.zar README.md
java --enable-native-access=ALL-UNNAMED \
  -cp target/classes org.zar.jarlib.ffm.examples.ZarcExample create backup.zar src/
```

`ZarcExample` supports `create|c`, `add|a`, `remove|r`, `join|j`, `check|k`,
`info|i`, `list|l`, `extract|e`, and `memory`. The integration tests cover all
nine operations, option/error handling, encryption, extraction, and concurrent
calls.
