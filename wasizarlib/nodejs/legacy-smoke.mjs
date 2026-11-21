import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { filesystem } from "@bytecodealliance/preview2-shim";
import { WASIShim } from "@bytecodealliance/preview2-shim/instantiation";
import { instantiate } from "./dist/legacy/wasizarlib-legacy.js";

const directory = new URL("./dist/legacy/", import.meta.url);
const runtime = new WASIShim({
  filesystem: filesystem.createFilesystem({ preopens: {} }),
  sandbox: { env: {}, args: [], enableNetwork: false },
});
const component = await instantiate(async (path) => {
  const bytes = await readFile(new URL(path, directory));
  return WebAssembly.compile(bytes);
}, runtime.getImportObject());

const operations = component["zar:wasizarlib/operations@0.5.0"];
assert.equal(typeof operations.execute, "function");
assert.throws(() => operations.execute("check_archive", "not-json"), /invalid request JSON/);
console.log("JSON-only component instantiated without streaming imports");
