// Writes app/src-tauri/icons/icon.ico: one plain 32 by 32 icon (a teal square
// with a white bar), as an uncompressed 32 bit bitmap. Tauri needs an .ico to
// build on Windows; this avoids a design tool or `tauri icon`.
// Run from the spike folder: node tools/make-icon.mjs

import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const size = 32;
const pixels = Buffer.alloc(size * size * 4);
for (let y = 0; y < size; y++) {
  for (let x = 0; x < size; x++) {
    // Bitmap rows go from the bottom up; colours are blue, green, red, alpha.
    const i = ((size - 1 - y) * size + x) * 4;
    const bar = y >= 13 && y <= 18 && x >= 6 && x <= 25;
    const [r, g, b] = bar ? [255, 255, 255] : [0, 128, 128];
    pixels[i] = b;
    pixels[i + 1] = g;
    pixels[i + 2] = r;
    pixels[i + 3] = 255;
  }
}
const andMask = Buffer.alloc((size / 8) * size); // all zero: every pixel shown

const header = Buffer.alloc(40);
header.writeUInt32LE(40, 0); // header size
header.writeInt32LE(size, 4); // width
header.writeInt32LE(size * 2, 8); // height, doubled for the AND mask
header.writeUInt16LE(1, 12); // planes
header.writeUInt16LE(32, 14); // bits per pixel
header.writeUInt32LE(0, 16); // no compression
header.writeUInt32LE(pixels.length + andMask.length, 20);

const image = Buffer.concat([header, pixels, andMask]);

const dir = Buffer.alloc(6);
dir.writeUInt16LE(0, 0);
dir.writeUInt16LE(1, 2); // type: icon
dir.writeUInt16LE(1, 4); // one image

const entry = Buffer.alloc(16);
entry.writeUInt8(size, 0);
entry.writeUInt8(size, 1);
entry.writeUInt8(0, 2); // no palette
entry.writeUInt8(0, 3);
entry.writeUInt16LE(1, 4); // planes
entry.writeUInt16LE(32, 6); // bits per pixel
entry.writeUInt32LE(image.length, 8);
entry.writeUInt32LE(6 + 16, 12); // offset of the image

const here = dirname(fileURLToPath(import.meta.url));
const out = join(here, '..', 'app', 'src-tauri', 'icons', 'icon.ico');
writeFileSync(out, Buffer.concat([dir, entry, image]));
console.log(`wrote ${out}`);
