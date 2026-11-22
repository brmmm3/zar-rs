// Transfer one entry over TCP. For trusted networks only.
import { closeSync, fstatSync, openSync, readSync, writeSync } from "node:fs";
import { mkdtemp, open, rm } from "node:fs/promises";
import net from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createWasizarlib } from "../index.js";

const MAX = 65536;
const z = await createWasizarlib();

function seekableFile(fd) {
  let position = 0;
  return {
    read(max) {
      const buffer = Buffer.alloc(max);
      const count = readSync(fd, buffer, 0, max, position);
      position += count;
      return buffer.subarray(0, count);
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

class Reader {
  constructor(socket) {
    this.iterator = socket[Symbol.asyncIterator]();
    this.pending = Buffer.alloc(0);
  }
  async read(length) {
    const result = Buffer.alloc(length);
    let offset = 0;
    while (offset < length) {
      if (!this.pending.length) {
        const next = await this.iterator.next();
        if (next.done) throw new Error("connection closed before transfer completed");
        this.pending = next.value;
      }
      const count = Math.min(length - offset, this.pending.length);
      this.pending.copy(result, offset, 0, count);
      this.pending = this.pending.subarray(count);
      offset += count;
    }
    return result;
  }
  async length() { return (await this.read(4)).readUInt32BE(); }
}

function send(socket, bytes) {
  return new Promise((resolve, reject) => {
    socket.write(bytes, (error) => error ? reject(error) : resolve());
  });
}

async function sendLength(socket, length) {
  const header = Buffer.alloc(4);
  header.writeUInt32BE(length);
  await send(socket, header);
}

async function handle(socket, archivePath) {
  const reader = new Reader(socket);
  const length = await reader.length();
  if (!length || length > MAX) throw new Error("invalid entry path length");
  const entry = new TextDecoder("utf-8", { fatal: true }).decode(await reader.read(length));
  // WASI callbacks are synchronous; stage the entry before using Node's async socket.
  const directory = await mkdtemp(join(tmpdir(), "zar-tcp-"));
  const staged = join(directory, "entry");
  try {
    const archive = openSync(archivePath, "r");
    try {
      const output = openSync(staged, "wx");
      try {
        z.extractToStream(seekableFile(archive), entry, {
          write(bytes) { return writeSync(output, bytes); },
        });
      } finally { closeSync(output); }
    } finally { closeSync(archive); }
    const file = await open(staged, "r");
    try {
      for await (const chunk of file.createReadStream({ highWaterMark: MAX, autoClose: false })) {
        await sendLength(socket, chunk.length);
        await send(socket, chunk);
      }
    } finally { await file.close(); }
    await sendLength(socket, 0);
    socket.end();
    console.log(`Sent ${entry}`);
  } finally { await rm(directory, { recursive: true }); }
}

async function receive(host, port, entry, path) {
  const name = Buffer.from(entry);
  if (!name.length || name.length > MAX) throw new Error("invalid entry path length");
  const file = await open(path, "wx");
  try {
    const socket = net.createConnection({ host, port });
    try {
      const reader = new Reader(socket);
      await sendLength(socket, name.length);
      await send(socket, name);
      while (true) {
        const length = await reader.length();
        if (!length) break;
        if (length > MAX) throw new Error("invalid data frame length");
        await file.writeFile(await reader.read(length));
      }
    } finally { socket.destroy(); }
    await file.close();
  } catch (error) {
    await file.close();
    await rm(path);
    throw error;
  }
}

const [mode, host, portText, ...args] = process.argv.slice(2);
const port = Number(portText);
if (mode === "server" && args.length === 1 && host && Number.isInteger(port) && port > 0 && port <= 65535) {
  net.createServer((socket) => {
    handle(socket, args[0]).catch((error) => {
      console.error(`Request failed: ${error.message}`);
      socket.destroy();
    });
  }).listen(port, host, () => console.log(`Listening on ${host}:${port}`));
} else if (mode === "client" && args.length === 2 && host && Number.isInteger(port) && port > 0 && port <= 65535) {
  await receive(host, port, args[0], args[1]);
} else {
  throw new Error("usage: tcp-streaming.mjs server HOST PORT ARCHIVE | client HOST PORT ENTRY OUTPUT");
}
