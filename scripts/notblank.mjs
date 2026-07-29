// Fail if a PNG is a single flat colour.
//
// A cart that loads, returns frames, and renders nothing is the standard
// silent failure. "ran 30 frames" does not distinguish it from a working cart,
// so the screenshot has to be inspected. This is the mechanical half; the
// human half is opening the PNG and looking at it, which no script replaces.
import fs from 'node:fs';
import zlib from 'node:zlib';

const png = fs.readFileSync(process.argv[2]);
if (png.readUInt32BE(0) !== 0x89504e47) throw new Error('not a PNG');

let pos = 8;
let w = 0, h = 0, bitDepth = 0, colorType = 0;
const idat = [];
while (pos < png.length) {
  const len = png.readUInt32BE(pos);
  const type = png.toString('ascii', pos + 4, pos + 8);
  const data = png.subarray(pos + 8, pos + 8 + len);
  if (type === 'IHDR') {
    w = data.readUInt32BE(0); h = data.readUInt32BE(4);
    bitDepth = data[8]; colorType = data[9];
  } else if (type === 'IDAT') idat.push(data);
  else if (type === 'IEND') break;
  pos += 12 + len;
}
if (bitDepth !== 8) throw new Error(`unsupported bit depth ${bitDepth}`);
const channels = { 0: 1, 2: 3, 3: 1, 4: 2, 6: 4 }[colorType];
if (!channels) throw new Error(`unsupported color type ${colorType}`);

// Un-filter the scanlines.
const raw = zlib.inflateSync(Buffer.concat(idat));
const stride = w * channels;
const out = Buffer.alloc(stride * h);
for (let y = 0; y < h; y++) {
  const filter = raw[y * (stride + 1)];
  const line = raw.subarray(y * (stride + 1) + 1, y * (stride + 1) + 1 + stride);
  for (let x = 0; x < stride; x++) {
    const a = x >= channels ? out[y * stride + x - channels] : 0;
    const b = y > 0 ? out[(y - 1) * stride + x] : 0;
    const c = x >= channels && y > 0 ? out[(y - 1) * stride + x - channels] : 0;
    let v = line[x];
    if (filter === 1) v += a;
    else if (filter === 2) v += b;
    else if (filter === 3) v += (a + b) >> 1;
    else if (filter === 4) {
      const p = a + b - c, pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
      v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
    }
    out[y * stride + x] = v & 0xff;
  }
}

const counts = new Map();
for (let i = 0; i < out.length; i += channels) {
  const k = `${out[i]},${out[i + 1] ?? 0},${out[i + 2] ?? 0}`;
  counts.set(k, (counts.get(k) || 0) + 1);
}
const top = [...counts].sort((a, b) => b[1] - a[1]);
const share = (100 * top[0][1]) / (w * h);
console.log(`${process.argv[2]}: ${w}x${h}, ${counts.size} distinct colours, ` +
  `dominant rgb(${top[0][0]}) ${share.toFixed(1)}%`);

// A cart drawing a legible scene has structure. One colour at 92%+ of the
// frame is the threshold the rest of the org uses for "nearly blank".
if (counts.size < 2 || share >= 92) {
  console.error('NEARLY BLANK: this frame is one flat colour. The cart is not rendering.');
  process.exit(1);
}
