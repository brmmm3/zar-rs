# jzarlib-jni

`jzarlib-jni` is the Java Native Interface binding to the `czarlib` C ABI. It
provides the same nine archive operations and JSON contract as the C wrapper:
create, add, remove, join, check, info, list, filesystem extract, and in-memory
extract. See [`czarlib/README.md`](../czarlib/README.md) for operation request
fields, filters, encryption options, and limitations.

## Requirements, build, and tests

Install a JDK, a C compiler, Cargo, and Maven. Build the JNI shared library and
run the integration tests from the repository root:

```sh
./jzarlib-jni/build-native.sh
cd jzarlib-jni
mvn test
```

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
`info|i`, `list|l`, `extract|e`, and `memory`. Tests cover every operation,
request failures, filters/options, extraction, encryption, and concurrent
calls.
