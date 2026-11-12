const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { test } = require('node:test');
const { chromium } = require('playwright');

const root = path.resolve(__dirname, '../zarlib-gui/assets');

test('flat icons match every source name and size and have transparent backgrounds', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    let count = 0;
    for (const category of ['actions', 'devices', 'mimetypes', 'places', 'status']) {
      const source = path.join(root, 'icons', category);
      const flat = path.join(root, 'flat', category);
      const names = (await fs.readdir(source)).filter((name) => name.endsWith('.png')).sort();
      const generated = (await fs.readdir(flat)).filter((name) => name.endsWith('.png')).sort();
      assert.deepEqual(generated, names, category);
      for (const name of names) {
        const original = await fs.readFile(path.join(source, name));
        const result = await fs.readFile(path.join(flat, name));
        const svg = await fs.readFile(path.join(flat, name.replace(/\.png$/, '.svg')), 'utf8');
        assert.equal(result.toString('hex', 0, 8), '89504e470d0a1a0a', name);
        assert.equal(result.readUInt32BE(16), original.readUInt32BE(16), name);
        assert.equal(result.readUInt32BE(20), original.readUInt32BE(20), name);
        assert.match(svg, /<svg .+viewBox="0 0 24 24"/);
        const pixels = await page.evaluate(async (data) => {
          const image = new Image();
          image.src = `data:image/png;base64,${data}`;
          await image.decode();
          const canvas = document.createElement('canvas');
          canvas.width = image.width;
          canvas.height = image.height;
          const context = canvas.getContext('2d');
          context.drawImage(image, 0, 0);
          const rgba = context.getImageData(0, 0, image.width, image.height).data;
          let opaque = 0;
          let transparent = 0;
          for (let i = 3; i < rgba.length; i += 4) {
            if (rgba[i] === 0) transparent++;
            if (rgba[i] > 0) opaque++;
          }
          return { opaque, transparent };
        }, result.toString('base64'));
        assert.ok(pixels.opaque > 0, `${name}: empty`);
        assert.ok(pixels.transparent > 0, `${name}: no transparency`);
        count++;
      }
    }
    assert.equal(count, 63);
  } finally {
    await browser.close();
  }
});
