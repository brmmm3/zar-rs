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
