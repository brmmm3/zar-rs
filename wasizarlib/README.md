# wasizarlib

`wasizarlib` packages the `zarlib` archive operations as a WebAssembly Component
Model component targeting WASI Preview 2. Its WIT interface exports one
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
`target/wasm32-wasip2/release/wasizarlib.wasm`.

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

## Runtime capabilities

The component performs filesystem operations through Rust's standard library.
Configure the server-side WASI runtime to preopen only the directories the
component needs; paths in requests are resolved within those granted
capabilities. Do not pass password-bearing requests to logs.

External archive extraction creates a temporary directory in the destination
or its nearest existing parent. That directory must be writable and accessible
through a WASI preopen.

## WIT interface

The exported interface is defined in [`wit/world.wit`](./wit/world.wit):

```wit
execute: func(operation: string, request-json: string) -> result<string, string>;
```

For example, `check_archive` accepts the same JSON request as the native C
binding:

```json
{"archive":"backup.zar"}
```
