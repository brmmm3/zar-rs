import { closeSync, fstatSync, openSync, readSync, writeSync } from "node:fs";
import { basename } from "node:path";
import { createWasizarlib } from "../index.js";

const [inputPath, archivePath, outputPath] = process.argv.slice(2);
if (!inputPath || !archivePath || !outputPath) {
  throw new Error("usage: node examples/stream.mjs INPUT NEW_ARCHIVE NEW_OUTPUT");
}

function seekableFile(fd) {
  let position = 0;
  return {
    read(max) {
      const buffer = Buffer.alloc(max);
      const count = readSync(fd, buffer, 0, max, position);
      position += count;
      return buffer.subarray(0, count);
    },
    write(bytes) {
      const count = writeSync(fd, bytes, 0, bytes.length, position);
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

const z = await createWasizarlib();
const input = openSync(inputPath, "r");
try {
  const archiveFd = openSync(archivePath, "wx+");
  try {
    const archive = seekableFile(archiveFd);
    const name = basename(inputPath);
    z.createFromStreams(archive, [{
      path: name,
      reader: {
        read(max) {
          const bytes = Buffer.alloc(max);
          const count = readSync(input, bytes, 0, max, null);
          return bytes.subarray(0, count);
        },
      },
    }]);
    const output = openSync(outputPath, "wx");
    try {
      const count = z.extractToStream(archive, name, {
        write(bytes) { return writeSync(output, bytes); },
      });
      console.log(`Extracted ${count} bytes`);
    } finally {
      closeSync(output);
    }
  } finally {
    closeSync(archiveFd);
  }
} finally {
  closeSync(input);
}
