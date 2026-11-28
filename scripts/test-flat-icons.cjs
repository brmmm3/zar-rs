const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { test } = require('node:test');
const { chromium } = require('playwright');

const root = path.resolve(__dirname, '../zarlib-gui/assets');

test('icon themes match names and sizes but have distinct visible artwork', async () => {
  const browser = await chromium.launch({
    headless: true,
    executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH || undefined,
  });

  test('lock and unlock icons have distinct shackles at button size', async () => {
    const browser = await chromium.launch({
      headless: true,
      executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH || undefined,
    });
    try {
      const page = await browser.newPage();
      for (const theme of ['crystal', 'flat']) {
        const icons = await Promise.all(['document-decrypt.png', 'document-encrypt.png'].map(
          (name) => fs.readFile(path.join(root, theme, 'actions', name)),
        ));
        const alpha = await page.evaluate(async (images) => {
          return Promise.all(images.map(async (data) => {
            const image = new Image();
            image.src = `data:image/png;base64,${data}`;
            await image.decode();
            const canvas = document.createElement('canvas');
            canvas.width = canvas.height = 16;
            const context = canvas.getContext('2d');
            context.drawImage(image, 0, 0, 16, 16);
            return context.getImageData(13, 8, 1, 1).data[3];
          }));
        }, icons.map((icon) => icon.toString('base64')));
        assert.ok(alpha[0] < 128, `${theme}: unlocked shackle is not open`);
        assert.ok(alpha[1] > 200, `${theme}: locked shackle is not closed`);
      }
    } finally {
      await browser.close();
    }
  });
  try {
    const page = await browser.newPage();
    let count = 0;
    for (const category of ['actions', 'devices', 'mimetypes', 'places', 'status']) {
      const source = path.join(root, 'crystal', category);
      const flat = path.join(root, 'flat', category);
      const names = (await fs.readdir(source)).filter((name) => name.endsWith('.png')).sort();
      const generated = (await fs.readdir(flat)).filter((name) => name.endsWith('.png')).sort();
      assert.deepEqual(generated, names, category);
      for (const name of names) {
        const original = await fs.readFile(path.join(source, name));
        const result = await fs.readFile(path.join(flat, name));
        assert.equal(result.toString('hex', 0, 8), '89504e470d0a1a0a', name);
        assert.ok(original.readUInt32BE(16) >= 48, `${name}: width below 48`);
        assert.ok(original.readUInt32BE(20) >= 48, `${name}: height below 48`);
        assert.equal(result.readUInt32BE(16), original.readUInt32BE(16), name);
        assert.equal(result.readUInt32BE(20), original.readUInt32BE(20), name);
        const pixels = await page.evaluate(async ([crystal, flat]) => {
          async function decode(data) {
            const image = new Image();
            image.src = `data:image/png;base64,${data}`;
            await image.decode();
            const canvas = document.createElement('canvas');
            canvas.width = image.width;
            canvas.height = image.height;
            const context = canvas.getContext('2d');
            context.drawImage(image, 0, 0);
            return context.getImageData(0, 0, image.width, image.height).data;
          }
          const rgba = await decode(flat);
          const other = await decode(crystal);
          let opaque = 0;
          let transparent = 0;
          let crystalOpaque = 0;
          let crystalTransparent = 0;
          let different = 0;
          for (let i = 3; i < rgba.length; i += 4) {
            if (rgba[i] === 0) transparent++;
            if (rgba[i] > 0) opaque++;
            if (other[i] === 0) crystalTransparent++;
            if (other[i] > 0) crystalOpaque++;
            if (rgba[i] > 0 && (rgba[i - 3] !== other[i - 3] ||
                rgba[i - 2] !== other[i - 2] || rgba[i - 1] !== other[i - 1])) different++;
          }
          return { opaque, transparent, crystalOpaque, crystalTransparent, different };
        }, [original.toString('base64'), result.toString('base64')]);
        assert.ok(pixels.opaque > 0, `${name}: empty`);
        assert.ok(pixels.transparent > 0, `${name}: no transparency`);
        assert.ok(pixels.crystalOpaque > 0, `${name}: Crystal icon is empty`);
        assert.ok(pixels.crystalTransparent > 0, `${name}: Crystal icon has no transparency`);
        assert.ok(pixels.different > 0, `${name}: themes look identical`);
        if (category === 'actions' && ['dialog-close.png', 'dialog-cancel.png', 'dialog-ok.png'].includes(name)) {
          const mount = await fs.readFile(path.join(root, '../../zarmount/assets/crystal/actions', name));
          assert.deepEqual(mount, original, `${name}: zarmount icon differs`);
        }
        count++;
      }
    }
    assert.ok(count > 0);
  } finally {
    await browser.close();
  }
});
