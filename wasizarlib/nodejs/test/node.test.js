import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, unlink, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { createWasizarlib } from "../index.js";

function tarFile(name, contents) {
  const data = Buffer.from(contents);
  const header = Buffer.alloc(512);
  header.write(name, 0, "utf8");
  header.write("0000644\0", 100, "ascii");
  header.write("0000000\0", 108, "ascii");
  header.write("0000000\0", 116, "ascii");
  header.write(`${data.length.toString(8).padStart(11, "0")}\0`, 124, "ascii");
  header.write("00000000000\0", 136, "ascii");
  header.fill(0x20, 148, 156);
  header.write("0", 156, "ascii");
  header.write("ustar\0", 257, "ascii");
  header.write("00", 263, "ascii");
  const checksum = header.reduce((sum, byte) => sum + byte, 0);
  header.write(`${checksum.toString(8).padStart(6, "0")}\0 `, 148, "ascii");
  return Buffer.concat([header, data, Buffer.alloc((512 - data.length % 512) % 512)]);
}

test("WASI extracts external tar archives", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "wasizarlib-tar-"));
  try {
    await writeFile(
      path.join(directory, "input.tar"),
      Buffer.concat([tarFile("first.txt", "first"), tarFile("second.txt", "second"), Buffer.alloc(1024)]),
    );
    const { execute } = await createWasizarlib({ preopens: { "/data": directory } });
    execute("extract_archive", JSON.stringify({ archive: "/data/input.tar", destination: "/data/output" }));
    assert.equal(await readFile(path.join(directory, "output/first.txt"), "utf8"), "first");
    assert.equal(await readFile(path.join(directory, "output/second.txt"), "utf8"), "second");
    await writeFile(path.join(directory, "output/first.txt"), "preserved");
    await unlink(path.join(directory, "output/second.txt"));
    execute("extract_archive", JSON.stringify({
      archive: "/data/input.tar",
      destination: "/data/output",
      existing: "skip",
    }));
    assert.equal(await readFile(path.join(directory, "output/first.txt"), "utf8"), "preserved");
    assert.equal(await readFile(path.join(directory, "output/second.txt"), "utf8"), "second");
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

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

test("WASI exposes commit encryption and decryption operations", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "wasizarlib-transform-"));

  try {
    const first = path.join(directory, "first.txt");
    const second = path.join(directory, "second.txt");
    const archive = "/data/source.zar";
    const output = "/data/transformed.zar";
    await writeFile(first, "first");
    await writeFile(second, "second");
    const { execute } = await createWasizarlib({
      preopens: { "/data": directory },
    });

    execute("create_archive", JSON.stringify({ archive, paths: ["/data/first.txt"] }));
    execute("add_archive", JSON.stringify({ archive, paths: ["/data/second.txt"] }));
    execute(
      "encrypt_archive_commits",
      JSON.stringify({
        archive,
        from_id: "00000002",
        to_id: "00000002",
        output,
        new_password: "current password",
        encryption_scope: "header-and-data",
        strict: true,
      }),
    );
    const encryptedInfo = JSON.parse(execute("info_archive", JSON.stringify({ archive: output })));
    assert.equal(encryptedInfo[1].header_encrypted, true);
    assert.equal(encryptedInfo[1].data_encrypted, true);

    execute(
      "decrypt_archive_commits",
      JSON.stringify({
        archive: output,
        from_id: "00000002",
        to_id: "00000002",
        password: "current password",
        strict: true,
      }),
    );
    const decryptedInfo = JSON.parse(execute("info_archive", JSON.stringify({ archive: output })));
    assert.equal(decryptedInfo[1].header_encrypted, false);
    assert.equal(decryptedInfo[1].data_encrypted, false);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
