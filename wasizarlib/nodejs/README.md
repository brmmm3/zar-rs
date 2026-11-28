# @zar-rs/wasizarlib-node

Node.js adapter for the ZAR WASI Preview 2 component. Requires Node.js 22.20
or later. The adapter uses Jco-generated JavaScript and the
`@bytecodealliance/preview2-shim` runtime.

Build from this directory in a `zar-rs` checkout:

```sh
npm ci --include=optional
npm run build
npm test
npm pack
```

`npm run build` builds the Rust component and transpiles it for JavaScript.
The packed package includes those generated files and can be installed from
the resulting tarball.

Create an instance with only the directories it needs preopened:

```js
import { createWasizarlib } from "@zar-rs/wasizarlib-node";

const { execute } = await createWasizarlib({
  preopens: { "/data": "/srv/zar-data" },
});
const response = execute(
  "check_archive",
  JSON.stringify({ archive: "/data/backup.zar" }),
);
console.log(JSON.parse(response));
```

Operation errors are thrown. Archive paths must refer to configured preopens.
The adapter exposes no environment variables, arguments, or network access to
the component.

`diffArchive({ archive, start, end, password, passwords })` compares commit
snapshots and returns the decoded category maps described in the
[C diff schema](../../czarlib/README.md). Password fields are optional.
`updateCommitDetails(request)` edits commit details using the
[C request schema](../../czarlib/README.md).

## Searching archive files

For an existing `/srv/zar-data/backup.zar`, search `.txt` files with one line
of context before and after each match:

```js
import { createWasizarlib } from "@zar-rs/wasizarlib-node";

const { execute } = await createWasizarlib({
  preopens: { "/data": "/srv/zar-data" },
});
const files = JSON.parse(execute("grep_archive", JSON.stringify({
  archive: "/data/backup.zar",
  file_filter: "*.txt",
  pattern: "(?i)TODO|FIXME",
  before: 1,
  after: 1,
})));
for (const file of files) {
  for (const line of file.lines) {
    console.log(file.path, line.line_number, line.is_match, Buffer.from(line.bytes));
  }
}
```

`execute` is synchronous and throws on errors. Results are grouped by path;
files without matches are omitted. `line_number` is one-based, `is_match`
distinguishes matches from context, and `bytes` contains raw byte values
without the line terminator. The example uses a `Buffer` to preserve bytes
that may not be UTF-8. WASI searches are sequential. See the
[grep contract](../../czarlib/README.md#searching-archive-files) for glob,
regex, commit, encryption, integrity, and memory behavior.

## Synchronous streaming

`createFromStreams(archive, inputs, options)` and
`extractToStream(archive, path, output, options)` use host callbacks rather
than JSON-encoded file contents. A runnable file-backed example is included:

```sh
node examples/stream.mjs input.txt new-archive.zar new-output.txt
```

Inputs are `{ path, reader }` objects; `reader.read(maxBytes)` synchronously
returns a `Uint8Array` of at most `maxBytes` (empty means EOF). An output has
`write(bytes)` returning the number consumed; short writes are retried, zero
progress is an error. The archive object additionally implements `seek(pos)`,
where `pos` is `{ tag: "start" | "current" | "end", val: bigint }` and the
return value is the new absolute position as a nonnegative `bigint`. Creation
requires an empty, seekable writable archive; extraction requires a seekable
readable archive. Callbacks receive at most 64 KiB at a time; don't retain the
write buffer after returning. Each handle needs its own independent cursor.

Creation options use `{ encoding_options: { block_size, compression_level },
password, aead, encryption_scope }`. Defaults are 8 MiB, 3,
`"aes-256-gcm"`, and `"header-and-data"`; encryption is enabled only when
`password` is supplied. `block_size` must be 64 KiB–32 MiB (automatic size 0
is not supported); `compression_level` is -7 through 22. `aead` also accepts
`"chacha20-poly1305"`; scope accepts `"data"`, `"header"`, or
`"header-and-data"`. Extraction accepts `{ password, passwords, to }`:
`passwords` maps eight-digit hexadecimal commit IDs to passwords and can
contain `default` instead of `password`. `to` selects a commit; omitting it
uses the latest. A commit-specific password overrides the default.
`extractToStream` returns the extracted byte count as a `bigint`.

These are blocking callbacks, not Node `Readable`/`Writable` streams or Promise
APIs; async callbacks and reentrant component calls are unsupported. Errors
are thrown, and an error can leave a partial archive or output. The caller
must flush and close streams and clean up partial files. Callback streams are
explicit host-granted capabilities outside the WASI preopen sandbox; `execute`
continues to use only preopened paths. Streaming bounds data transfers, not
archive indexes, compression block/workspace, or header memory. See the
[component documentation](../README.md#streaming) for commit and integrity
semantics.

The WASI operation dispatcher supports archive-level custom key/value metadata:

```js
execute(
  "update_custom_metadata",
  JSON.stringify({
    archive: "/data/backup.zar",
    set: { project: "example", revision: 3 },
  }),
);
const metadata = JSON.parse(
  execute("read_custom_metadata", JSON.stringify({ archive: "/data/backup.zar" })),
);
```

Read returns a JSON object; update takes a `set` object and/or `remove` array
and appends a metadata-only commit. Values are strings, signed integers, or
finite numbers, and updates require an unencrypted archive.
