import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { closeSync, fstatSync, openSync, readSync, writeSync } from "node:fs";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { createWasizarlib } from "../index.js";

function seekableFile(fd) {
  let position = 0;
  return {
    read(max) {
      assert.ok(max <= 65536);
      const buffer = Buffer.alloc(max);
      const count = readSync(fd, buffer, 0, max, position);
      position += count;
      return buffer.subarray(0, count);
    },
    write(bytes) {
      assert.ok(bytes.length <= 65536);
      const count = writeSync(fd, bytes, 0, Math.min(bytes.length, 8191), position);
      position += count;
      return count;
    },
    seek({ tag, val }) {
      const base = tag === "start" ? 0n : tag === "current" ? BigInt(position) : BigInt(fstatSync(fd).size);
      const next = base + val;
      if (next < 0n || next > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error("invalid seek");
      position = Number(next);
      return next;
    },
  };
}

test("bounded streams round-trip encrypted data and propagate host errors", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "wasizarlib-stream-"));
  const fd = openSync(path.join(directory, "stream.zar"), "w+");
  try {
    const zar = await createWasizarlib({ preopens: { "/data": directory } });
    const archive = seekableFile(fd);
    let remaining = 1_000_123;
    let reads = 0;
    const expected = createHash("sha256");
    const reader = {
      read(max) {
        assert.ok(max <= 65536);
        reads++;
        const bytes = Buffer.alloc(Math.min(max, remaining, 7001), 0x5a);
        remaining -= bytes.length;
        expected.update(bytes);
        return bytes;
      },
    };
    zar.createFromStreams(archive, [{ path: "nested/file.bin", reader }], {
      encoding_options: { block_size: 65536, compression_level: 1 },
      password: "secret", encryption_scope: "header-and-data",
    });
    assert.ok(reads > 100);
    const actual = createHash("sha256");
    let written = 0;
    const output = {
      write(bytes) {
        assert.ok(bytes.length <= 65536);
        const part = bytes.subarray(0, 4093);
        actual.update(part);
        written += part.length;
        return part.length;
      },
    };
    assert.throws(() => zar.extractToStream(archive, "nested/file.bin", output, { password: "wrong" }));
    assert.equal(zar.extractToStream(archive, "nested/file.bin", output, {
      passwords: { "00000001": "secret" }, to: "00000001",
    }), 1_000_123n);
    assert.equal(written, 1_000_123);
    assert.equal(actual.digest("hex"), expected.digest("hex"));
    assert.throws(() => zar.extractToStream(archive, "missing", output, { password: "secret" }));
    assert.throws(() => zar.extractToStream(archive, "nested/file.bin", {
      write() { throw new Error("sink failed"); },
    }, { password: "secret" }), /sink failed/);
    const saved = Buffer.alloc(1);
    readSync(fd, saved, 0, 1, 30);
    writeSync(fd, Buffer.from([saved[0] ^ 1]), 0, 1, 30);
    assert.throws(() => zar.extractToStream(archive, "nested/file.bin", {
      write(bytes) { return bytes.length; },
    }, { password: "secret" }));
    writeSync(fd, saved, 0, 1, 30);
    assert.throws(() => zar.createFromStreams(archive, [], {}), /empty/);
  } finally {
    closeSync(fd);
    await rm(directory, { recursive: true, force: true });
  }
});

test("input failures, invalid host results and commit selection are surfaced", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "wasizarlib-stream-errors-"));
  const fd = openSync(path.join(directory, "stream.zar"), "w+");
  try {
    const zar = await createWasizarlib({ preopens: { "/data": directory } });
    const archive = seekableFile(fd);
    assert.throws(() => zar.createFromStreams(archive, [
      { path: "dup", reader: { read: () => Buffer.alloc(0) } },
      { path: "dup", reader: { read: () => Buffer.alloc(0) } },
    ]), /duplicate/);
    assert.throws(() => zar.createFromStreams(archive, [], {
      encoding_options: { block_size: 0 },
    }), /block size/);
    assert.throws(() => zar.createFromStreams({ seek() { throw new Error("seek failed"); } }, []), /seek failed/);
    assert.throws(() => zar.createFromStreams(archive, [{
      path: "a", reader: { read() { throw new Error("source failed"); } },
    }]), /source failed/);
    for (const [name, read, message] of [
      ["oversized", (max) => Buffer.alloc(max + 1), /invalid synchronous read/],
      ["reentrant", () => zar.execute("check_archive", "{}"), /cannot be reentered/],
    ]) {
      const badFd = openSync(path.join(directory, name), "w+");
      try {
        assert.throws(() => zar.createFromStreams(seekableFile(badFd), [
          { path: "a", reader: { read } },
        ]), message);
      } finally {
        closeSync(badFd);
      }
    }
    // The failed destination is partial: use a fresh archive for the next operation.
    const freshFd = openSync(path.join(directory, "fresh.zar"), "w+");
    try {
      const fresh = seekableFile(freshFd);
      let once = false;
      zar.createFromStreams(fresh, [{ path: "first", reader: {
        read() { if (once) return Buffer.alloc(0); once = true; return Buffer.from("one"); },
      } }]);
      await writeFile(path.join(directory, "second"), "two");
      zar.execute("add_archive", JSON.stringify({ archive: "/data/fresh.zar", paths: ["/data/second"] }));
      const entries = JSON.parse(zar.execute("list_archive", JSON.stringify({ archive: "/data/fresh.zar" })));
      const secondPath = entries.find((entry) => entry.path.endsWith("second")).path;
      const updatedFd = openSync(path.join(directory, "fresh.zar"), "r");
      try {
        const updated = seekableFile(updatedFd);
        const parts = [];
        const sink = { write(bytes) { parts.push(Buffer.from(bytes)); return bytes.length; } };
        assert.throws(() => zar.extractToStream(updated, secondPath, sink, { to: "00000001" }));
        assert.equal(zar.extractToStream(updated, secondPath, sink, { to: "00000002" }), 3n);
        assert.equal(Buffer.concat(parts).toString(), "two");
        assert.throws(() => zar.extractToStream(updated, "first", { write: () => Promise.resolve(1) }), /invalid synchronous write/);
        assert.throws(() => zar.extractToStream(updated, "first", { write: () => 0 }), /failed to write whole buffer/);
      } finally {
        closeSync(updatedFd);
      }
    } finally {
      closeSync(freshFd);
    }
  } finally {
    closeSync(fd);
    await rm(directory, { recursive: true, force: true });
  }
});
