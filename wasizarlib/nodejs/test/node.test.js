import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { createWasizarlib } from "../index.js";

test("Node.js executes archive operations through the WASI component", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "wasizarlib-node-"));

  try {
    await writeFile(path.join(directory, "input.txt"), "Node.js WASI support");
    const { execute } = await createWasizarlib({
      preopens: { "/data": directory },
    });
    const response = execute(
      "create_archive",
      JSON.stringify({
        archive: "/data/example.zar",
        paths: ["/data/input.txt"],
      }),
    );

    assert.ok(JSON.parse(response).block_size > 0);
    assert.ok((await readFile(path.join(directory, "example.zar"))).length > 0);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
