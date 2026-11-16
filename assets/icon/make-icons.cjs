// Regenerates the app icons from netlix.svg:
//   netlix.png  256 px  window icon (embedded in the binary) and Linux AppImage icon
//   netlix.ico          Windows executable and installer (16 to 256 px)
//   netlix.icns         macOS app bundle (drawn smaller, on Apple's 824/1024 icon grid)
//   packaging/windows/wizard-*.bmp  installer images (side panel and header), per DPI
// Needs sharp: `npm install --prefix /tmp/sharp sharp`, then
//   NODE_PATH=/tmp/sharp/node_modules node assets/icon/make-icons.cjs
const fs = require('node:fs');
const path = require('node:path');
const sharp = require('sharp');

const dir = __dirname;
const svg = fs.readFileSync(path.join(dir, 'netlix.svg'), 'utf8');
// macOS: the tile is 824 of 1024 px, centred (Apple's app icon template).
const scale = 824 / 944;
const offset = 512 - 512 * scale;
const macSvg = svg
  .replace('<rect x="40"', `<g transform="translate(${offset} ${offset}) scale(${scale})"><rect x="40"`)
  .replace('</svg>', '</g></svg>');

const png = (source, size) =>
  sharp(Buffer.from(source)).resize(size, size).png({ compressionLevel: 9 }).toBuffer();

function ico(images) {
  const header = Buffer.alloc(6 + 16 * images.length);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(images.length, 4);
  let offset = header.length;
  images.forEach(({ size, data }, i) => {
    const e = 6 + 16 * i;
    header.writeUInt8(size >= 256 ? 0 : size, e);
    header.writeUInt8(size >= 256 ? 0 : size, e + 1);
    header.writeUInt16LE(1, e + 4); // planes
    header.writeUInt16LE(32, e + 6); // bits per pixel
    header.writeUInt32LE(data.length, e + 8);
    header.writeUInt32LE(offset, e + 12);
    offset += data.length;
  });
  return Buffer.concat([header, ...images.map((image) => image.data)]);
}

/** 24-bit bottom-up BMP from raw RGB pixels (Inno Setup wizard images). */
function bmp(width, height, rgb) {
  const row = Math.ceil((width * 3) / 4) * 4;
  const out = Buffer.alloc(54 + row * height);
  out.write('BM', 0, 'ascii');
  out.writeUInt32LE(out.length, 2);
  out.writeUInt32LE(54, 10);
  out.writeUInt32LE(40, 14);
  out.writeInt32LE(width, 18);
  out.writeInt32LE(height, 22);
  out.writeUInt16LE(1, 26);
  out.writeUInt16LE(24, 28);
  out.writeUInt32LE(row * height, 34);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const src = ((height - 1 - y) * width + x) * 3;
      const dst = 54 + y * row + x * 3;
      out[dst] = rgb[src + 2];
      out[dst + 1] = rgb[src + 1];
      out[dst + 2] = rgb[src];
    }
  }
  return out;
}

async function writeBmp(source, width, height, file, background) {
  const { data, info } = await sharp(Buffer.from(source))
    .resize(width, height)
    .flatten({ background })
    .removeAlpha()
    .raw()
    .toBuffer({ resolveWithObject: true });
  fs.writeFileSync(file, bmp(info.width, info.height, data));
}

// The network motif alone (no tile), for the installer panel.
const motif = svg.slice(svg.indexOf('<g stroke'), svg.lastIndexOf('</svg>'));
const panelSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="328" height="628" viewBox="0 0 328 628">
  <defs>
    <linearGradient id="sky" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#1e1914"/>
      <stop offset="0.62" stop-color="#322921"/>
      <stop offset="0.8" stop-color="#863b24"/>
      <stop offset="0.92" stop-color="#b24e30"/>
      <stop offset="1" stop-color="#dcafa2"/>
    </linearGradient>
  </defs>
  <rect width="328" height="628" fill="url(#sky)"/>
  <g transform="translate(54 120) scale(0.215)">${motif}</g>
</svg>`;

function icns(entries) {
  const chunks = entries.map(({ type, data }) => {
    const head = Buffer.alloc(8);
    head.write(type, 0, 'ascii');
    head.writeUInt32BE(data.length + 8, 4);
    return Buffer.concat([head, data]);
  });
  const body = Buffer.concat(chunks);
  const head = Buffer.alloc(8);
  head.write('icns', 0, 'ascii');
  head.writeUInt32BE(body.length + 8, 4);
  return Buffer.concat([head, body]);
}

(async () => {
  fs.writeFileSync(path.join(dir, 'netlix.png'), await png(svg, 256));

  const icoSizes = [16, 24, 32, 48, 64, 128, 256];
  const icoImages = [];
  for (const size of icoSizes) icoImages.push({ size, data: await png(svg, size) });
  fs.writeFileSync(path.join(dir, 'netlix.ico'), ico(icoImages));

  const icnsTypes = [
    ['icp4', 16], ['icp5', 32], ['icp6', 64], ['ic07', 128], ['ic08', 256], ['ic09', 512],
    ['ic10', 1024], ['ic11', 32], ['ic12', 64], ['ic13', 256], ['ic14', 512],
  ];
  const icnsEntries = [];
  for (const [type, size] of icnsTypes) icnsEntries.push({ type, data: await png(macSvg, size) });
  fs.writeFileSync(path.join(dir, 'netlix.icns'), icns(icnsEntries));

  // Inno Setup picks the size closest to the display scale (100, 125, 150, 200 %).
  const win = path.join(dir, '..', '..', 'packaging', 'windows');
  for (const [scale, suffix] of [[1, '100'], [1.25, '125'], [1.5, '150'], [2, '200']]) {
    const w = Math.round(164 * scale);
    const h = Math.round(314 * scale);
    await writeBmp(panelSvg, w, h, path.join(win, `wizard-${suffix}.bmp`), '#1e1914');
    const s = Math.round(55 * scale);
    await writeBmp(svg, s, s, path.join(win, `wizard-small-${suffix}.bmp`), '#ffffff');
  }
  console.log('netlix.png, netlix.ico, netlix.icns and installer images written');
})();
