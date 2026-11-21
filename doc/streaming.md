# Streaming support

This guide describes `zarlib` streaming, the TCP examples, and a supplied
Linux bandwidth-test run. For API details and more examples, see the
[developer guide](./zarlib-developer-guide.md#streaming-regular-files).

## Library APIs

`create_archive_from_readers` consumes named regular-file `Read` inputs in
blocks and writes a single-commit ZAR to an empty, seekable `Write + Seek`
destination. It supports optional encryption but does not deduplicate inputs
or collect filesystem metadata. `extract_single_file_to_writer` reads a
seekable archive and writes one selected regular file to a `Write` destination.
It validates selected data and the file checksum; it does not necessarily
validate unselected blocks. Use `check_archive_with_recovery` to check the
whole archive.

The archive must be seekable because its headers follow its data. These APIs
buffer blocks and metadata rather than requiring a whole file in memory. An
error may leave partial output, so callers should stage output when atomic
publication is needed.

## TCP examples

The [streaming server](../zarlib/examples/streaming_server.rs) has two modes:

1. **File:** send one regular file, framed by an eight-byte big-endian size,
   to one connection. The
   [tree client](../zarlib/examples/streaming_tree_client.rs) can save a
   received ZAR or stage it and extract its tree.
2. **Directory:** serve requests for archives directly inside a directory.
   The [selection client](../zarlib/examples/streaming_client.rs) lists
   archives or requests regular files from one named ZAR. Patterns and archive
   names are case-sensitive; repeated `--select` patterns are supported, and
   omitting them selects all regular files. Matching paths are preserved.

For directory mode, start the server and run the client in another terminal:

```sh
cargo run --release -p zarlib --example streaming_server -- 127.0.0.1:7878 testdata
cargo run --release -p zarlib --example streaming_client -- 127.0.0.1:7878 --list
cargo run --release -p zarlib --example streaming_client -- 127.0.0.1:7878 \
  --zar linux-4.0.1.zar --output output --select '*.txt'
```

The client defaults to the current output directory and refuses existing files
unless `--force` is used. It displays progress on an interactive terminal;
`--no-progress` disables it. The server logs requests unless `--quiet` is used.
The directory protocol sends selected file contents uncompressed, preparing one
file at a time; files larger than 8 MiB are staged temporarily. Case-distinct
paths remain distinct on the server, but can collide on a case-insensitive
client filesystem. With `--force`, such a collision can overwrite an earlier
received file. Failed transfers may leave partial output.

These are demonstrations, not authenticated or encrypted network services.
Use trusted networks and directories. Directory mode requires matching
`ZARREQ03` client/server support; the server also accepts older `ZARREQ01`
and `ZARREQ02` requests. See the
[protocol and limitations](./zarlib-developer-guide.md#tcp-streaming-example).

## Bandwidth test

On Linux, run the [test script](../scripts/test_streaming_bandwidth.py) from the
repository root:

```sh
python scripts/test_streaming_bandwidth.py
```

The script requires `cargo`, `unshare`, `ip`, and `tc`, permission to create
unprivileged user and network namespaces, and sufficient temporary disk space.
It builds release examples, extracts a reference tree, and runs two transfers
over an isolated, rate-limited loopback. `--archive PATH` selects another
existing archive; `--timeout SECONDS` sets the timeout per transfer.

The two cases use **file mode**, not directory selection mode:

- **ZAR to archive:** the server sends the ZAR; the client writes a ZAR file.
  The test compares its size and SHA-256 hash with the source.
- **ZAR to tree:** the server sends the ZAR; the client stages the complete
  archive and then extracts it. The test compares paths, regular-file content
  hashes, directories, and symlink targets with the reference extraction.
  Both extractions use the existing `rename` collision policy.

The timed interval includes client output and, in tree mode, extraction; it
excludes reference extraction, building, and final verification. The script
samples each process's CPU and resident set size (RSS) approximately every
0.5 seconds. CPU is expressed as 100% per core, so values above 100% indicate
multiple cores. RSS peaks are sampled estimates; the combined peak is the
maximum sum observed at one sample, not the sum of individual peak values.

Throughput uses decimal Mbit/s and the complete timed interval:

```text
effective throughput = bytes * 8 / elapsed_seconds / 1,000,000
```

The compressed figure uses transferred ZAR bytes; the uncompressed figure uses
the size of the original regular-file contents. The latter is an effective
content rate, **not** the number of uncompressed bits sent over the network.
The reported elapsed time is rounded, so recalculating from it may differ
slightly from the printed throughput.

## Supplied Linux 4.0.1 run

This is one user-supplied run, not a statistical benchmark. The reference
extraction of `testdata/linux-4.0.1.zar` reported **2.89 s**. Both cases
transferred **120,475,375 compressed bytes**, representing **550,572,641
uncompressed regular-file bytes** (approximately 4.57 times as many bytes).
The log labels the network as isolated loopback with a **100 Mbit/s TBF**.
Both outputs were reported as verified.

### Summary

| Measurement | ZAR to archive | ZAR to tree |
| --- | ---: | ---: |
| Elapsed including client output | 9.65 s | 11.48 s |
| Effective compressed throughput | 99.867 Mbit/s | 83.989 Mbit/s |
| Effective uncompressed throughput | 456.394 Mbit/s | 383.830 Mbit/s |
| Server CPU average / sampled peak | 1.0% / 2.0% | 0.9% / 2.0% |
| Client CPU average / sampled peak | 3.1% / 6.0% | 47.8% / 445.4% |
| Server / client sampled RSS peak | 2.7 / 2.6 MiB | 2.8 / 261.4 MiB |
| Combined sampled RSS peak | 5.2 MiB | 261.4 MiB |
| Samples | 19 | 22 |
| Output | verified | verified |

Archive mode measures transferring and saving the compressed stream. Tree
mode includes extraction after receipt, so its longer elapsed time and lower
effective rate include decompression and filesystem work. Its later client
samples show a rise in CPU and RSS: the 445.4% sampled peak corresponds to
approximately 4.45 cores during that interval. Server RSS reaching zero is
consistent with the server having finished while the client continues. These
are observations from this run, not guarantees for other machines or data.

### Samples: ZAR to archive

| Time (s) | Server CPU % | Server RSS MiB | Client CPU % | Client RSS MiB |
| ---: | ---: | ---: | ---: | ---: |
| 0.5 | 2.0 | 2.7 | 2.0 | 2.6 |
| 1.0 | 0.0 | 2.7 | 4.0 | 2.6 |
| 1.5 | 2.0 | 2.7 | 2.0 | 2.6 |
| 2.0 | 0.0 | 2.7 | 4.0 | 2.6 |
| 2.5 | 2.0 | 2.7 | 2.0 | 2.6 |
| 3.0 | 0.0 | 2.7 | 6.0 | 2.6 |
| 3.5 | 2.0 | 2.7 | 2.0 | 2.6 |
| 4.0 | 0.0 | 2.7 | 4.0 | 2.6 |
| 4.5 | 2.0 | 2.7 | 2.0 | 2.6 |
| 5.0 | 2.0 | 2.7 | 4.0 | 2.6 |
| 5.5 | 0.0 | 2.7 | 2.0 | 2.6 |
| 6.0 | 0.0 | 2.7 | 2.0 | 2.6 |
| 6.5 | 2.0 | 2.7 | 6.0 | 2.6 |
| 7.0 | 0.0 | 2.7 | 2.0 | 2.6 |
| 7.5 | 2.0 | 2.7 | 4.0 | 2.6 |
| 8.0 | 0.0 | 2.7 | 2.0 | 2.6 |
| 8.5 | 2.0 | 2.7 | 4.0 | 2.6 |
| 9.0 | 0.0 | 2.7 | 2.0 | 2.6 |
| 9.5 | 2.0 | 0.0 | 2.0 | 2.6 |

### Samples: ZAR to tree

| Time (s) | Server CPU % | Server RSS MiB | Client CPU % | Client RSS MiB |
| ---: | ---: | ---: | ---: | ---: |
| 0.5 | 0.0 | 2.8 | 2.0 | 2.8 |
| 1.0 | 2.0 | 2.8 | 2.0 | 2.8 |
| 1.5 | 2.0 | 2.8 | 4.0 | 2.8 |
| 2.0 | 0.0 | 2.8 | 2.0 | 2.8 |
| 2.5 | 2.0 | 2.8 | 2.0 | 2.8 |
| 3.0 | 0.0 | 2.8 | 2.0 | 2.8 |
| 3.5 | 0.0 | 2.8 | 2.0 | 2.8 |
| 4.0 | 2.0 | 2.8 | 4.0 | 2.8 |
| 4.5 | 2.0 | 2.8 | 2.0 | 2.8 |
| 5.0 | 0.0 | 2.8 | 2.0 | 2.8 |
| 5.5 | 2.0 | 2.8 | 2.0 | 2.8 |
| 6.0 | 0.0 | 2.8 | 2.0 | 2.8 |
| 6.5 | 0.0 | 2.8 | 4.0 | 2.8 |
| 7.0 | 2.0 | 2.8 | 2.0 | 2.8 |
| 7.5 | 2.0 | 2.8 | 2.0 | 2.8 |
| 8.0 | 0.0 | 2.8 | 2.0 | 2.8 |
| 8.5 | 2.0 | 2.8 | 4.0 | 2.8 |
| 9.0 | 0.0 | 2.8 | 2.0 | 2.8 |
| 9.5 | 2.0 | 0.0 | 4.0 | 2.8 |
| 10.0 | 0.0 | 0.0 | 100.0 | 91.4 |
| 10.5 | 0.0 | 0.0 | 96.0 | 136.4 |
| 11.0 | 0.0 | 0.0 | 445.4 | 261.4 |
