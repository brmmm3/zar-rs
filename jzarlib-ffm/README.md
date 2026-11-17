# jzarlib-ffm

`jzarlib-ffm` is the Java Foreign Function & Memory API binding to `czarlib`. It
requires Java 25 or newer and exposes the JSON operations for create, add,
remove, custom metadata, join, undo, encrypt/decrypt commit ranges, check, info,
list, filesystem extract, and in-memory extract. See
[`czarlib/README.md`](../czarlib/README.md) for all request fields, filters,
encryption settings, and support boundaries. Create and Add requests may include
`.zip`, `.7z`, `.tar`, `.tar.gz`, or `.tgz` source paths; their contents are
imported into the resulting ZAR using the source archive's internal paths.

`ZarLibrary.crossCommitReferences(requestJson)` returns the shared JSON
reference inventory, and `infoArchive` reports per-commit reference counts.

The FFM wrapper provides `readCustomMetadata` and `updateCustomMetadata` for
archive-level custom key/value metadata. Reading returns the effective values
as a JSON object; updating accepts a `set` object and/or `remove` array and
appends a commit without file changes. Values are strings, signed integers,
or finite numbers, and updates require an unencrypted archive. This is
separate from per-file permissions and timestamps; see the
[`czarlib` guide](../czarlib/README.md) for the request schema.

## Build and test

Build the C ABI shared library, then run the Java tests:

```sh
cargo build --release -p czarlib
cd jzarlib-ffm
mvn test
```

By default the Java binding loads `../target/release/libczarlib.so` on Linux
(or the platform-specific name) relative to the module working directory.
Override it with `-Dzar.library=/absolute/path/to/libczarlib.so`. The test
runtime enables native access for the FFM API.

## Maven Central bundle

The Maven coordinates are `io.github.brmmm3:jzarlib-ffm`. To prepare a
bundle for manual upload to the Central Publisher Portal, configure a GPG
signing key and publish its public key, then run:

```sh
cargo build --release -p czarlib
cd jzarlib-ffm
mvn -Pcentral deploy
```

Upload `target/central-publishing/central-bundle.zip`, not the JAR. The
`central` profile builds sources and Javadoc, signs the artifacts, and creates
the Maven-layout bundle with checksums without uploading it. See
[Sonatype's publishing requirements](https://central.sonatype.org/publish/requirements/).
The normal `mvn deploy` still targets GitHub Packages.

The JAR does not include `czarlib`; consumers must provide the native library
for their platform and set `-Dzar.library` as described above.

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
`info|i`, `list|l`, `extract|e`, and `memory`. The integration tests cover
archive operations, custom metadata, option/error handling, encryption,
extraction, and concurrent calls.
