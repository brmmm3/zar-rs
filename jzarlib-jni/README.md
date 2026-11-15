# jzarlib-jni

`jzarlib-jni` is the Java Native Interface binding to the `czarlib` C ABI. It
provides the archive operations and JSON contract as the C wrapper: create,
add, remove, custom metadata, join, undo, encrypt/decrypt commit ranges, check,
info, list, filesystem extract, and in-memory extract. See
[`czarlib/README.md`](../czarlib/README.md) for operation request fields,
filters, encryption options, and limitations. Create and Add requests may
include `.zip`, `.7z`, `.tar`, `.tar.gz`, or `.tgz` source paths; their contents
are imported into the resulting ZAR using the source archive's internal paths.

The JNI wrapper provides `readCustomMetadata` and `updateCustomMetadata` for
archive-level custom key/value metadata. Reading returns the effective values
as a JSON object; updating accepts a `set` object and/or `remove` array and
appends a commit without file changes. Values are strings, signed integers,
or finite numbers, and updates require an unencrypted archive. This is
separate from per-file permissions and timestamps; see the
[`czarlib` guide](../czarlib/README.md) for the request schema.

## Requirements, build, and tests

Install JDK 25 or newer, a C compiler, Cargo, and Maven. Build the JNI shared
library and run the integration tests from the repository root:

```sh
./jzarlib-jni/build-native.sh
cd jzarlib-jni
mvn test
```

Use `./jzarlib-jni/build-native.sh --skip-czarlib-build` to link against an
existing release build of `czarlib` in `target/release`.

The script builds `czarlib`, compiles the JNI C shim, and links it against the
Rust shared library. Linux and macOS are supported by the build script. The
default Java loader path is `target/native/libzarlib_jni.so` (or the platform
equivalent) relative to the module working directory. Override it with
`-Dzar.jni.library=/absolute/path/to/the/JNI/library`.

## API

Each operation has a static Java method accepting the same JSON object request
as `czarlib`, for example:

```java
String response = ZarLibrary.createArchive(
    "{\"archive\":\"backup.zar\",\"paths\":[\"src\"]}");
```

`ZarLibrary.executeJson(operation, request)` is the generic dispatcher.
Success returns a JSON string; operation or request failures throw
`ZarException`. The JNI boundary explicitly copies UTF-8 bytes from Java,
converts returned UTF-8 into Java strings, and releases C ABI response/error
strings using `czarlib_string_free`.

Use `readCustomMetadata` to read the effective metadata map and
`updateCustomMetadata` to set and/or remove keys:

```java
ZarLibrary.updateCustomMetadata(
    "{\"archive\":\"backup.zar\",\"set\":{\"project\":\"example\",\"revision\":3}}");
String metadata = ZarLibrary.readCustomMetadata("{\"archive\":\"backup.zar\"}");
```

The read result is a JSON object. The update request takes a `set` object and
an optional `remove` array; it appends a metadata-only commit. Values are
strings, signed integers, or finite numbers, and updates require an unencrypted
archive.

In-memory extraction returns JSON arrays of byte values. Passwords in request
JSON are plaintext credentials: obtain them securely and do not log password-
bearing requests.

## Examples

```sh
mvn package
java --enable-native-access=ALL-UNNAMED -Djava.library.path=target/native -cp target/classes \
  org.zar.jarlib.jni.examples.BasicExample backup.zar README.md
java --enable-native-access=ALL-UNNAMED -Djava.library.path=target/native -cp target/classes \
  org.zar.jarlib.jni.examples.ZarcExample create backup.zar src/
```

The CLI example supports `create|c`, `add|a`, `remove|r`, `join|j`, `check|k`,
`info|i`, `list|l`, `extract|e`, and `memory`. Tests cover archive operations,
custom metadata, request failures, filters/options, extraction, encryption,
and concurrent calls.
