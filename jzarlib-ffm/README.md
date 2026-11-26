# jzarlib-ffm

`jzarlib-ffm` is the Java Foreign Function & Memory API binding to `czarlib`. It
requires Java 25 or newer and exposes the JSON operations for create, add,
remove, custom metadata, join, undo, encrypt/decrypt commit ranges, check, info,
list, grep, filesystem extract, and in-memory extract. See
[`czarlib/README.md`](../czarlib/README.md) for all request fields, filters,
encryption settings, and support boundaries. Create and Add requests may include
`.zip`, `.7z`, `.tar`, `.tar.gz`, or `.tgz` source paths; their contents are
imported into the resulting ZAR using the source archive's internal paths.

`ZarLibrary.crossCommitReferences(requestJson)` returns the shared JSON
reference inventory, and `infoArchive` reports per-commit reference counts.
`ZarLibrary.grepArchive(requestJson)` searches files using the C JSON schema.

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

## Streaming

The streaming methods require a `czarlib` built with the streaming C ABI.
They process data incrementally: entry contents come from `InputStream`s and
one extracted entry is written to an `OutputStream`. The archive itself uses
a `SeekableByteChannel` because ZAR needs random access. Creation requires an
empty channel positioned at zero, opened for reading and writing; extraction
requires a readable, seekable channel.

```java
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.List;
import org.zar.jarlib.ffm.ZarLibrary;

try (var archive = Files.newByteChannel(Path.of("backup.zar"),
        StandardOpenOption.CREATE, StandardOpenOption.TRUNCATE_EXISTING,
        StandardOpenOption.READ, StandardOpenOption.WRITE);
     var input = Files.newInputStream(Path.of("source.bin"));
     var output = Files.newOutputStream(Path.of("restored.bin"))) {
    ZarLibrary.createArchiveFromReaders(archive,
        List.of(new ZarLibrary.StreamInput("source.bin", input)),
        "{\"encoding_options\":{\"block_size\":65536}}");
    long bytes = ZarLibrary.extractSingleFileToWriter(
        archive, "source.bin", output, null);
}
```

Both operations are synchronous and leave channels and streams open; callers
must close and, if appropriate, flush them. On callback failure, `ZarException`
retains the original Java exception as its cause. Do not concurrently use or
close the supplied objects during a call. A failed operation may leave a
partial archive or output.
Streaming creation writes regular-file entries without filesystem metadata or
deduplication; entry paths are archive-relative.

The optional JSON object for creation accepts `encoding_options` (including
`block_size` and `compression_level`), `password`, `aead`, and
`encryption_scope`; extraction accepts `password`, `passwords`, and `to`.
Pass `null` for defaults. Passwords are plaintext and should not be logged.

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

## Searching archive files

With native access enabled, the C library configured as above, and an existing
`backup.zar`:

```java
import org.zar.jarlib.ffm.ZarLibrary;

public class GrepExample {
    public static void main(String[] args) throws Exception {
        String result = ZarLibrary.grepArchive(
            "{\"archive\":\"backup.zar\",\"file_filter\":\"*.txt\"," +
            "\"pattern\":\"(?i)TODO|FIXME\",\"before\":1,\"after\":1}");
        System.out.println(result);
    }
}
```

The result is JSON grouped by archive path, with files without matches
omitted. Each line has a one-based `line_number`, raw `bytes` as an array of
numbers without the terminator, and `is_match` to distinguish matches from
context. Parse the JSON without assuming the bytes are UTF-8. Request and
archive errors throw `ZarException`. See the
[grep contract](../czarlib/README.md#searching-archive-files) for glob,
regex, commit, encryption, integrity, and memory behavior.

## Examples

Compile the examples with Maven and run them from this module directory:

```sh
mvn package
java --enable-native-access=ALL-UNNAMED \
  -cp target/classes org.zar.jarlib.ffm.examples.BasicExample backup.zar README.md
java --enable-native-access=ALL-UNNAMED \
  -cp target/classes org.zar.jarlib.ffm.examples.ZarcExample create backup.zar src/
java --enable-native-access=ALL-UNNAMED \
  -cp target/classes org.zar.jarlib.ffm.examples.StreamingExample backup.zar README.md restored.md
```

`ZarcExample` supports `create|c`, `add|a`, `remove|r`, `join|j`, `check|k`,
`info|i`, `list|l`, `extract|e`, and `memory`. The integration tests cover
archive operations, custom metadata, option/error handling, encryption,
extraction, and concurrent calls.
