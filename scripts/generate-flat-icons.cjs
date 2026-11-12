#!/usr/bin/env node
// Generate the flat zarg icon set. Requires the repository's Playwright package.
const fs = require('node:fs/promises');
const path = require('node:path');
const { chromium } = require('playwright');

const root = path.resolve(__dirname, '..');
const source = path.join(root, 'zarlib-gui/assets/crystal');
const output = path.join(root, 'zarlib-gui/assets/flat');
const ink = '#27364f';
const blue = '#2563eb';
const cyan = '#0891b2';
const green = '#16a36a';
const amber = '#e99a24';
const red = '#e5484d';
const purple = '#7c57cb';
const pale = '#e9eff8';
const white = '#ffffff';

const stroke = (content, color = ink, width = 1.8) =>
  `<g fill="none" stroke="${color}" stroke-width="${width}" stroke-linecap="round" stroke-linejoin="round">${content}</g>`;
const text = (value, x = 12, y = 16, size = 8, color = white) =>
  `<text x="${x}" y="${y}" fill="${color}" font-family="DejaVu Sans,sans-serif" font-weight="700" font-size="${size}" text-anchor="middle">${value}</text>`;
const badge = (symbol, color = blue) =>
  `<circle cx="18" cy="18" r="5.5" fill="${white}"/><circle cx="18" cy="18" r="4.6" fill="${color}"/>${symbol}`;
const plus = stroke('<path d="M18 15.8v4.4m-2.2-2.2h4.4"/>', white, 1.5);
const minus = stroke('<path d="M15.8 18h4.4"/>', white, 1.5);
const check = stroke('<path d="m15.7 18 1.5 1.5 3.1-3.1"/>', white, 1.5);
const down = stroke('<path d="M18 15.5v4.4m-1.8-1.8 1.8 1.8 1.8-1.8"/>', white, 1.5);
const cross = stroke('<path d="m16.4 16.4 3.2 3.2m0-3.2-3.2 3.2"/>', white, 1.5);
const doc = `<path d="M5 2h9l5 5v15H5z" fill="${pale}"/><path d="M14 2v5h5" fill="#c6d6ef"/>`;
const folder = `<path d="M2 5h8l2 2h10v12a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2z" fill="${amber}"/><path d="M2 9h20v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2z" fill="#f3b84e"/>`;
const box = `<rect x="3" y="6" width="18" height="15" rx="2" fill="#e9a23c"/><path d="M3 9h18V6H3z" fill="#c98126"/><path d="M10 6h4v6h-4z" fill="${pale}"/>`;
const lock = `<rect x="7" y="10" width="10" height="10" rx="2" fill="${blue}"/>${stroke('<path d="M9 10V7a3 3 0 1 1 6 0v3M12 14v2"/>', white, 1.5)}`;
const shield = (color) => `<path d="M12 2 20 5v6c0 5-3 8-8 11-5-3-8-6-8-11V5z" fill="${color}"/>`;
const circle = (color, content) => `<circle cx="12" cy="12" r="9" fill="${color}"/>${content}`;
const documentLabel = (label, color = blue) =>
  `${doc}<rect x="3" y="13" width="18" height="7" rx="2" fill="${color}"/>${text(label, 12, 18.1, label.length > 3 ? 5 : 6)}`;
const strokes = (paths, color = ink, width = 1.8) => stroke(`<path d="${paths}"/>`, color, width);
const glyphs = {
  'application-exit': `${strokes('M10 4H5v16h5 M13 8l4 4-4 4 M8 12h9')}`,
  'archive-extract': box + badge(down, blue),
  'archive-insert-directory': folder + badge(plus, green),
  'archive-insert': box + badge(plus, green),
  'archive-remove': box + badge(minus, red),
  'archive-test': box + badge(check, green),
  benchmark: `<path d="M4 19a8 8 0 1 1 16 0" fill="${pale}"/>${strokes('M4 19a8 8 0 1 1 16 0M12 17l4-5M8 16h0M12 11h0M17 16h0', blue, 2)}`,
  'change-commits': `${strokes('M7 4v16M7 8h7l3-3M7 16h7l3 3', blue)}<g fill="${cyan}"><circle cx="7" cy="4" r="2"/><circle cx="7" cy="20" r="2"/><circle cx="17" cy="5" r="2"/><circle cx="17" cy="19" r="2"/></g>`,
  configure: `${circle(pale, '')}${strokes('M5 7h14M5 12h14M5 17h14', ink, 2)}<g fill="${blue}"><circle cx="9" cy="7" r="2"/><circle cx="15" cy="12" r="2"/><circle cx="10" cy="17" r="2"/></g>`,
  'dialog-cancel': circle(red, strokes('M8 8l8 8M16 8l-8 8', white, 2)),
  'dialog-ok-apply': circle(green, strokes('m7 12 3 3 7-7', white, 2)),
  'dialog-ok': circle(blue, strokes('m7 12 3 3 7-7', white, 2)),
  'dialog-password': lock,
  'dialog-warning': `<path d="M12 2 23 21H1z" fill="${amber}"/>${strokes('M12 8v6M12 17h0', white, 2)}`,
  'document-decrypt': doc + badge(stroke('<path d="M16 17v-1a2 2 0 0 1 4 0M15.5 17h5v3h-5z"/>', white, 1), green),
  'document-encrypt': doc + badge(stroke('<path d="M16 17v-1a2 2 0 0 1 4 0v1m-4.5 0h5v3h-5z"/>', white, 1), blue),
  'document-new': doc + badge(plus, green),
  'document-open-folder': folder + badge(stroke('<path d="M15.5 18h5m-2-2 2 2-2 2"/>', white, 1.4)),
  'document-open': folder + `<path d="m3 11 3-3h16l-3 11H2z" fill="#f7c765"/>`,
  'document-preview-archive': box + badge(stroke('<circle cx="17.5" cy="17.5" r="1.7"/><path d="m19 19 1.3 1.3"/>', white, 1.2)),
  'document-properties': doc + badge(stroke('<path d="M18 17.4v2.3M18 15.7h0"/>', white, 1.5)),
  'document-save': `<path d="M4 3h14l3 3v15H3V3z" fill="${blue}"/><path d="M7 3h9v6H7zM7 14h10v7H7z" fill="${pale}"/><path d="M13 4h2v4h-2z" fill="${ink}"/>`,
  'edit-clear': `<path d="m5 15 8-9 6 5-8 9H8z" fill="${purple}"/>${strokes('m5 15 6 5h9', ink)}`,
  'edit-select-all': `${strokes('M4 7V4h3M17 4h3v3M20 17v3h-3M7 20H4v-3', blue, 2)}${strokes('m8 12 3 3 5-6', ink, 2)}`,
  error: circle(red, strokes('M12 6v7M12 17h0', white, 2.2)),
  'folder-new': folder + badge(plus, green),
  'go-up': circle(blue, strokes('M12 18V6m-5 5 5-5 5 5', white, 2)),
  'help-about': circle(blue, `${text('i', 12, 17.5, 15)}`),
  'join-commits': `${strokes('M6 5v3a6 6 0 0 0 6 6v4M18 5v3a6 6 0 0 1-6 6', blue, 2)}<g fill="${cyan}"><circle cx="6" cy="5" r="2"/><circle cx="18" cy="5" r="2"/><circle cx="12" cy="19" r="2"/></g>`,
  'list-add': `${strokes('M4 6h11M4 11h11M4 16h7', ink, 2)}${badge(plus, green)}`,
  'list-remove': `${strokes('M4 6h11M4 11h11M4 16h7', ink, 2)}${badge(minus, red)}`,
  'online-help': circle(cyan, `${strokes('M3 12h18M12 3c-4 4-4 14 0 18M12 3c4 4 4 14 0 18', white, 1.2)}${badge(text('?', 18, 20.5, 7), blue)}`),
  publish: `${strokes('M12 16V4m-4 4 4-4 4 4M4 16v4h16v-4', blue, 2)}`,
  question: circle(blue, text('?', 12, 17.5, 15)),
  tools: `${strokes('M5 4a5 5 0 0 0 6 6l8 8-2 2-8-8a5 5 0 0 1-6-6l3 2 2-2z', ink, 2)}`,
  'trash-empty': `<path d="M6 7h12l-1 14H7z" fill="${blue}"/>${strokes('M4 6h16M9 4h6M10 10v7M14 10v7', ink, 1.6)}`,
  undo: `${strokes('M8 7 4 11l4 4M5 11h8a6 6 0 0 1 6 6v2', blue, 2.3)}`,
  'view-filter': `<path d="M3 4h18l-7 8v6l-4 2v-8z" fill="${blue}"/>`,
  'view-refresh': `${strokes('M20 10a8 8 0 0 0-14-3L4 9m0-4v4h4M4 14a8 8 0 0 0 14 3l2-2m0 4v-4h-4', blue, 2)}`,
  'drive-harddisk': `<rect x="3" y="4" width="18" height="16" rx="3" fill="#64748b"/><rect x="5" y="6" width="14" height="8" rx="1" fill="${pale}"/><circle cx="17" cy="17" r="1.3" fill="${green}"/>`,
  'application-octet-stream': documentLabel('BIN', '#64748b'),
  'application-pdf': documentLabel('PDF', red),
  'application-vnd.ms-excel': documentLabel('X', green),
  'application-vnd.ms-powerpoint': documentLabel('P', '#dc653c'),
  'application-vnd.ms-word': documentLabel('W', blue),
  'application-x-archive': box,
  'application-x-javascript': documentLabel('JS', '#c18a00'),
  'audio-x-generic': `${doc}${strokes('M12 8v8a2 2 0 1 1-2-2h2M12 8l4-1v6a2 2 0 1 1-2-2h2', purple, 1.8)}`,
  'emblem-symbolic-link': `${strokes('M10 15 8 17a4 4 0 0 1-6-6l4-4a4 4 0 0 1 6 0M14 9l2-2a4 4 0 0 1 6 6l-4 4a4 4 0 0 1-6 0M8 16l8-8', blue, 2)}`,
  'image-x-generic': `${doc}<rect x="7" y="8" width="10" height="10" rx="1" fill="${cyan}"/><circle cx="14" cy="10.5" r="1.3" fill="${white}"/><path d="m7 17 3.2-4 2.4 2.4 1.5-1.5L17 17z" fill="${white}"/>`,
  'text-plain': `${doc}${strokes('M8 10h8M8 13h8M8 16h6M8 19h5', '#64809f', 1.4)}`,
  'text-x-csrc': documentLabel('C', blue),
  'text-x-java': documentLabel('JAVA', red),
  'text-x-python': documentLabel('PY', blue),
  'text-x-script': documentLabel('SH', green),
  unknown: documentLabel('?', '#64748b'),
  'video-x-generic': `${doc}<rect x="7" y="8" width="10" height="9" rx="1.5" fill="${purple}"/><path d="m11 10 4 2.5-4 2.5z" fill="${white}"/>`,
  bookmarks: `<path d="M6 3h12v18l-6-4-6 4z" fill="${blue}"/><path d="m12 6 .8 1.7 1.9.3-1.4 1.3.4 1.9-1.7-.9-1.7.9.3-1.9L9.3 8l1.9-.3z" fill="${white}"/>`,
  folder,
  'dialog-error': circle(red, strokes('M12 6v7M12 17h0', white, 2.2)),
  'security-high': shield(green) + strokes('m8 12 3 3 5-6', white, 2),
  'security-low': shield(red) + strokes('M12 7v6M12 16h0', white, 2),
  'security-medium': shield(amber) + strokes('M12 7v6M12 16h0', white, 2),
};

async function main() {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ deviceScaleFactor: 1 });
    const categories = ['actions', 'devices', 'mimetypes', 'places', 'status'];
    let count = 0;
    for (const category of categories) {
      const directory = path.join(source, category);
      const destination = path.join(output, category);
      await fs.mkdir(destination, { recursive: true });
      for (const filename of (await fs.readdir(directory)).filter((name) => name.endsWith('.png')).sort()) {
        const name = filename.slice(0, -4);
        const artwork = glyphs[name];
        if (!artwork) throw new Error(`Missing flat icon design: ${category}/${filename}`);
        const png = await fs.readFile(path.join(directory, filename));
        if (png.toString('hex', 0, 8) !== '89504e470d0a1a0a') throw new Error(`Invalid PNG: ${filename}`);
        const width = png.readUInt32BE(16);
        const height = png.readUInt32BE(20);
        const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 24 24">${artwork}</svg>`;
        await fs.writeFile(path.join(destination, `${name}.svg`), `${svg}\n`);
        await page.setViewportSize({ width, height });
        await page.setContent(`<style>html,body{margin:0;width:100%;height:100%;overflow:hidden;background:transparent}</style>${svg}`);
        await page.evaluate(() => document.fonts.ready);
        await page.screenshot({ path: path.join(destination, filename), omitBackground: true });
        count += 1;
      }
    }
    console.log(`Generated ${count} PNG and SVG icons in ${output}`);
  } finally {
    await browser.close();
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
