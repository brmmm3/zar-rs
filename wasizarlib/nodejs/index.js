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
 * @returns {Promise<object>} Synchronous archive operations.
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
  let handles;
  const call = (handle, method, argument, validate) => {
    try {
      const stream = handles?.[handle];
      if (!stream || typeof stream[method] !== "function") {
        throw new Error(`stream ${handle} does not support ${method}`);
      }
      const result = stream[method](argument);
      if (!validate(result)) throw new Error(`invalid synchronous ${method} result`);
      return result;
    } catch (error) {
      return { tag: "err", val: String(error?.message ?? error) };
    }
  };
  const imports = runtime.getImportObject();
  imports["zar:wasizarlib/streams"] = {
    read: (handle, max) => call(handle, "read", max,
      (value) => value instanceof Uint8Array && value.byteLength <= max),
    write: (handle, bytes) => call(handle, "write", bytes,
      (value) => Number.isInteger(value) && value >= 0 && value <= bytes.length),
    seek: (handle, position) => call(handle, "seek", position,
      (value) => typeof value === "bigint" && value >= 0n && value <= 0xffffffffffffffffn),
  };
  const component = await componentModule.instantiate(
    loadCoreModule,
    imports,
  );
  const operations = component["zar:wasizarlib/operations@0.5.0"];

  if (!operations || typeof operations.execute !== "function") {
    throw new Error("wasizarlib component did not export operations.execute");
  }

  const withStreams = (streams, run) => {
    if (handles) throw new Error("stream operations cannot be reentered");
    handles = streams;
    try {
      return run();
    } finally {
      handles = undefined;
    }
  };
  return {
    execute(operation, requestJson) {
      if (handles) throw new Error("component operations cannot be reentered");
      return operations.execute(operation, requestJson);
    },
    diffArchive(request) {
      if (handles) throw new Error("component operations cannot be reentered");
      return JSON.parse(operations.execute("diff_archive", JSON.stringify(request)));
    },
    createFromStreams(archive, inputs, {
      encoding_options: encoding = {},
      password,
      aead = "aes-256-gcm",
      encryption_scope = "header-and-data",
    } = {}) {
      const streams = [archive, ...inputs.map((input) => input.reader)];
      const options = {
        blockSize: encoding.block_size ?? 8 * 1024 * 1024,
        compressionLevel: encoding.compression_level ?? 3,
        encryption: password === undefined ? undefined : {
          password, algorithm: aead, scope: encryption_scope,
        },
      };
      return withStreams(streams, () => operations.createFromStreams(
        0, inputs.map(({ path }, index) => ({ path, handle: index + 1 })), options,
      ));
    },
    extractToStream(archive, path, output, { password, passwords = {}, to } = {}) {
      if (Object.hasOwn(passwords, "default")) {
        if (password !== undefined) throw new Error("specify the default password only once");
        password = passwords.default;
      }
      return withStreams([archive, output], () => operations.extractToStream(
        0, path, 1, password,
        Object.entries(passwords).filter(([commit]) => commit !== "default")
          .map(([commit, value]) => ({ commit, password: value })),
        to,
      ));
    },
  };
}
