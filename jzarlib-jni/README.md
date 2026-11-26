# jzarlib-jni

`jzarlib-jni` is the Java Native Interface binding to the `czarlib` C ABI. It
provides the archive operations and JSON contract as the C wrapper: create,
add, remove, custom metadata, join, undo, encrypt/decrypt commit ranges, check,
info, list, grep, filesystem extract, and in-memory extract. See
[`czarlib/README.md`](../czarlib/README.md) for operation request fields,
filters, encryption options, and limitations. Create and Add requests may
include `.zip`, `.7z`, `.tar`, `.tar.gz`, or `.tgz` source paths; their contents
are imported into the resulting ZAR using the source archive's internal paths.

`ZarLibrary.crossCommitReferences(requestJson)` returns the shared JSON
reference inventory, and `infoArchive` reports per-commit reference counts.
`ZarLibrary.grepArchive(requestJson)` searches files using the C JSON schema.

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
On macOS, the JNI library looks for `libczarlib.dylib` alongside itself or in
the repository's `target/release` directory.

## Maven Central bundle

The Maven coordinates are `io.github.brmmm3:jzarlib-jni`. To prepare a bundle
for manual upload to the Central Publisher Portal, configure a GPG signing key
and publish its public key, then run:

```sh
./jzarlib-jni/build-native.sh
cd jzarlib-jni
mvn -Pcentral deploy
```

Upload `target/central-publishing/central-bundle.zip`, not the JAR. The
`central` profile builds sources and Javadoc, signs the artifacts, and creates
the Maven-layout bundle with checksums without uploading it. See
[Sonatype's publishing requirements](https://central.sonatype.org/publish/requirements/).
The normal `mvn deploy` still targets GitHub Packages.

The JAR does not include the JNI shim or `czarlib`; consumers must provide
both native libraries for their platform.

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

### Searching archive files

With the JNI library configured as above and an existing `backup.zar`:

```java
import org.zar.jarlib.jni.ZarLibrary;

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

### Streaming

`createArchiveFromReaders` consumes `StreamInput` records (archive-relative
path and `InputStream`) into an **empty**, readable, writable and seekable
`SeekableByteChannel`. `extractSingleFileToWriter` reads a seekable archive
channel and writes one file to an `OutputStream`, returning the byte count.
Neither operation buffers whole files, closes or flushes caller-owned streams
or channels. They run synchronously. An exception leaves possibly partial
output; discard it on failure. Java callback exceptions are propagated
unchanged. Channels must make progress on reads and writes, support arbitrary
seeks, and not be opened in append mode. The archive begins at offset zero
and must not change during the call.

```java
try (var archive = Files.newByteChannel(Path.of("new.zar"),
        StandardOpenOption.CREATE_NEW, StandardOpenOption.READ, StandardOpenOption.WRITE);
     var source = Files.newInputStream(Path.of("photo.jpg"))) {
    ZarLibrary.createArchiveFromReaders(archive,
        List.of(new ZarLibrary.StreamInput("photos/photo.jpg", source)),
        "{\"encoding_options\":{\"block_size\":65536}}");
    try (var destination = Files.newOutputStream(Path.of("restored.jpg"))) {
        long bytes = ZarLibrary.extractSingleFileToWriter(
            archive, "photos/photo.jpg", destination, "{}");
    }
}
```

The final argument is a JSON object. Creation supports `encoding_options`
(including `block_size` and `compression_level`), `password`, `aead`, and
`encryption_scope`; extraction supports `password`, per-commit `passwords`,
and `to`. Streaming creation handles regular files, not filesystem metadata,
and does not perform cross-file deduplication. Use a fixed block size (64 KiB
to 32 MiB), not automatic sizing. Failures in the native operation throw
`ZarException`; I/O callbacks can throw `IOException` or their original
unchecked exception. Keep password-bearing options private.

## Examples

```sh
mvn package
java --enable-native-access=ALL-UNNAMED -Djava.library.path=target/native -cp target/classes \
  org.zar.jarlib.jni.examples.BasicExample backup.zar README.md
java --enable-native-access=ALL-UNNAMED -Djava.library.path=target/native -cp target/classes \
  org.zar.jarlib.jni.examples.ZarcExample create backup.zar src/
java --enable-native-access=ALL-UNNAMED -cp target/classes \
  org.zar.jarlib.jni.examples.StreamingExample README.md new.zar restored.md
```

The CLI example supports `create|c`, `add|a`, `remove|r`, `join|j`, `check|k`,
`info|i`, `list|l`, `extract|e`, and `memory`. Tests cover archive operations,
custom metadata, request failures, filters/options, extraction, encryption,
and concurrent calls.
