import { readFile } from "node:fs/promises";
import { filesystem } from "@bytecodealliance/preview2-shim";
import { WASIShim } from "@bytecodealliance/preview2-shim/instantiation";

const generatedDirectory = new URL("./dist/component/", import.meta.url);
const componentModule = await import(
  new URL("./dist/component/wasizarlib.js", import.meta.url)
);

async function loadCoreModule(path) {
  const bytes = await readFile(new URL(path, generatedDirectory));
  return WebAssembly.compile(bytes);
}

/**
 * Instantiate wasizarlib with only the host directories explicitly preopened.
 *
 * @param {{ preopens?: Record<string, string> }} options
 * @returns {Promise<{ execute: (operation: string, requestJson: string) => string }>}
 */
export async function createWasizarlib({ preopens = {} } = {}) {
  const runtime = new WASIShim({
    filesystem: filesystem.createFilesystem({
      preopens,
      enableMutateDirectory: true,
    }),
    sandbox: {
      env: {},
      args: [],
      enableNetwork: false,
    },
  });
  const component = await componentModule.instantiate(
    loadCoreModule,
    runtime.getImportObject(),
  );
  const operations = component["zar:wasizarlib/operations@0.5.0"];

  if (!operations || typeof operations.execute !== "function") {
    throw new Error("wasizarlib component did not export operations.execute");
  }

  return { execute: operations.execute };
}
