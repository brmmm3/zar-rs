# wasizarlib

`wasizarlib` packages the `zarlib` archive operations as a WebAssembly Component
Model component targeting WASI Preview 2. Its WIT interface exports an
`execute` function that accepts an operation name and a JSON request and returns
a JSON response or an error. The operation names and request/response schemas
match [`czarlib`](../czarlib/README.md), so all archive operations are available
without introducing a second API implementation, including the
`encrypt_archive_commits` and `decrypt_archive_commits` range operations. As
with the other bindings, `create_archive` and `add_archive` requests may include
`.zip`, `.7z`, `.tar`, `.tar.gz`, or `.tgz` paths; source entry paths are
imported into the resulting ZAR. Encrypted ZIP/7z sources and ZIP/7z links are
not supported. TAR symbolic links are preserved where the host supports them,
hard links are imported as regular files, and TAR special entries are not
supported.

Use `execute("cross_commit_references", requestJson)` to enumerate references.
The request, result, and `info_archive` reference counts use the C JSON schema.

The WASI operation interface supports archive-level custom key/value metadata
through `read_custom_metadata` and `update_custom_metadata`. Call
`execute("read_custom_metadata", "{\"archive\":\"/data/backup.zar\"}")` to
read it, and
`execute("update_custom_metadata", "{\"archive\":\"/data/backup.zar\",\"set\":{\"project\":\"example\",\"revision\":3}}")`
to set values.

Read returns a JSON object; update takes a `set` object and/or `remove` array
and appends a metadata-only commit. Values are strings, signed integers, or
finite numbers, and updates require an unencrypted archive.

## Build

Install the WASI Preview 2 target and build the component from the repository
root. The build also needs `clang` to compile the native Zstandard dependency
for WASI:

```sh
rustup target add wasm32-wasip2
cargo build --release --locked -p wasizarlib --target wasm32-wasip2
```

The component is written to
`target/wasm32-wasip2/release/wasizarlib.wasm`. This default build keeps the
original JSON-only WIT interface. To build the streaming component instead:

```sh
cargo build --release --locked -p wasizarlib --features streaming --target wasm32-wasip2
```

The latter command overwrites the same output path. The release archive
contains both `wasizarlib.wasm` (JSON-only) and
`wasizarlib-streaming.wasm`.

## Node.js

Node.js does not run this Preview 2 component through its built-in
`node:wasi` API, which implements Preview 1. The [`nodejs`](./nodejs/)
integration uses Bytecode Alliance Jco to transpile the component and the
Preview 2 shim to run it. It requires Node.js 22.20 or later, Rust with the
`wasm32-wasip2` target, and `clang`.

From `wasizarlib/nodejs`:

```sh
npm ci --include=optional
npm run build
npm test
```

To create an installable npm tarball after building:

```sh
npm pack
npm install ./zar-rs-wasizarlib-node-0.9.0.tgz
```

The Node API creates an independent WASI instance for each caller. Grant file
access only through explicit preopens:

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

The exported function returns the response JSON string and throws on operation
errors. Archive paths must refer to preopened directories. The shim's
filesystem capabilities are enforced per instance; do not preopen directories
the component does not need.

## Streaming

With the `streaming` feature enabled, the WIT world imports
`zar:wasizarlib/streams` and exports
`create-from-streams` and `extract-to-stream`. The host assigns operation-local
u32 handles and implements synchronous `read`, `write`, and `seek` callbacks.
Each read and write request is at most 64 KiB; reads return up to the requested
count (empty means EOF), writes return the number consumed (including short
writes), and seek returns the new absolute u64 position. Callback errors are
returned as operation errors. A zero-byte write cannot make progress and fails.
Hosts must honor the requested bounds, keep handles valid during the call, and
provide an independently positioned stream for each handle. They must not
reenter the same component instance from a callback.
Hosts must provide this import when instantiating the streaming component,
even if they only use `execute`; the default JSON-only build does not require
it. The Node adapter supplies the import.

Creation requires an initially empty, seekable, writable archive (not append
mode); its input readers need not seek. It creates one new commit (`00000001`)
of regular files, validates archive-relative paths, and does not deduplicate or
preserve filesystem metadata. Extraction needs a seekable, readable ZAR and a
writable output. It reads the selected file as of `to`, or the latest commit
when omitted. Locked headers through that commit cause an error. The default
password applies to all commits unless overridden by a commit-specific password.
All streams start at archive offset zero as needed; the archive must not change
during an operation.

Only the streamed file data is bounded; archive metadata, indexes, the
configured compression block (64 KiB–32 MiB), and compression/encryption
workspace consume additional memory. Calls are synchronous, not asynchronous
WASI I/O, so slow hosts block the calling thread. Partial archives or outputs
may remain after any error; callers own flushing, closing, and cleanup. Avoid
logging passwords. Unlike `execute` paths, host-provided handles represent
explicit capabilities chosen by the host and are not restricted by WASI
preopens. The existing path-based API remains sandboxed by its preopens.

The Node adapter exposes `createFromStreams` and `extractToStream`; see
[`nodejs/README.md`](./nodejs/README.md) and the runnable
[`nodejs/examples/stream.mjs`](./nodejs/examples/stream.mjs).

## Runtime capabilities

The component performs filesystem operations through Rust's standard library.
Configure the server-side WASI runtime to preopen only the directories the
component needs; paths in requests are resolved within those granted
capabilities. Do not pass password-bearing requests to logs.

External archive extraction creates a temporary directory in the destination
or its nearest existing parent. That directory must be writable and accessible
through a WASI preopen.

## WIT interface

The streaming interface is defined in [`wit/world.wit`](./wit/world.wit); the
default JSON-only interface is in [`wit/legacy/world.wit`](./wit/legacy/world.wit):

```wit
execute: func(operation: string, request-json: string) -> result<string, string>;
```

For example, `check_archive` accepts the same JSON request as the native C
binding:

```json
{"archive":"backup.zar"}
```
