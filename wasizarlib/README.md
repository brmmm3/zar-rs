# wasizarlib

`wasizarlib` packages the `zarlib` archive operations as a WebAssembly
Component Model component targeting WASI Preview 2. Its WIT interface exports
one `execute` function that accepts an operation name and a JSON request and
returns a JSON response or an error. The operation names and request/response
schemas match [`czarlib`](../czarlib/README.md), so all archive operations are
available without introducing a second API implementation.

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

## Runtime capabilities

The component performs filesystem operations through Rust's standard library.
Configure the server-side WASI runtime to preopen only the directories the
component needs; paths in requests are resolved within those granted
capabilities. Do not pass password-bearing requests to logs.

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
