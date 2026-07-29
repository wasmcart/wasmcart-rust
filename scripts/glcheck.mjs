// Run a GL cart through a real headless GL context and prove it drew something.
//
// The bundled terminal player refuses GL carts (it has no context to give
// them), so a GL cart cannot be checked the same way a 2D one is. This drives
// CartHost directly with a webgl-node context, reads the framebuffer back with
// glReadPixels, and writes a PNG so the result can be looked at rather than
// merely counted.
//
// Usage: node scripts/glcheck.mjs cart.wasc out.png [frames]
//
// wasmcart and webgl-node are resolved from wherever wasmcart is installed
// (webgl-node is one of its dependencies), so this works against a global,
// local, or npx install without this repo depending on either.
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import path from 'node:path';
import fs from 'node:fs';
import zlib from 'node:zlib';

const [, , cartPath, outPng, framesArg] = process.argv;
if (!cartPath || !outPng) {
  console.error('usage: node scripts/glcheck.mjs cart.wasc out.png [frames]');
  process.exit(2);
}
const FRAMES = Number(framesArg || 30);
const W = 640, H = 480;

const require = createRequire(import.meta.url);
// WASMCART_DIR points at a wasmcart checkout; otherwise resolve an install.
let wasmcartDir = process.env.WASMCART_DIR;
if (!wasmcartDir) {
  try {
    wasmcartDir = path.dirname(require.resolve('wasmcart/package.json'));
  } catch {
    console.error('wasmcart is not installed here. Try: npm i wasmcart');
    console.error('(or set WASMCART_DIR to a wasmcart checkout)');
    process.exit(2);
  }
}
const { CartHost } = await import(pathToFileURL(path.join(wasmcartDir, 'index.js')));
const glReq = createRequire(path.join(wasmcartDir, 'package.json'));
const { createWebGL2Context } = await import(pathToFileURL(glReq.resolve('webgl-node')));

const gl = createWebGL2Context(W, H).gl;
const host = new CartHost();
await host.load(cartPath, { glBackend: gl });
console.log(`usesGL=${host.usesGL} size=${host.info.width}x${host.info.height} gpuApi=${host.info.gpuApi}`);
if (!host.usesGL) {
  console.error('cart did not register as a GL cart (check gpu_api and the gl imports)');
  process.exit(1);
}

for (let i = 0; i < FRAMES; i++) host.runFrame([{ connected: true, buttons: 0 }]);

const px = new Uint8Array(W * H * 4);
gl.readPixels(0, 0, W, H, gl.RGBA, gl.UNSIGNED_BYTE, px);

const counts = new Map();
for (let i = 0; i < px.length; i += 4) {
  const k = `${px[i]},${px[i + 1]},${px[i + 2]}`;
  counts.set(k, (counts.get(k) || 0) + 1);
}
const top = [...counts].sort((a, b) => b[1] - a[1]);
const share = (100 * top[0][1]) / (W * H);
console.log(`${counts.size} distinct colours, dominant rgb(${top[0][0]}) ${share.toFixed(1)}%`);

// ── PNG out (GL origin is bottom-left, so flip) ─────────────────────────
const raw = Buffer.alloc((W * 4 + 1) * H);
for (let y = 0; y < H; y++) {
  raw[y * (W * 4 + 1)] = 0;
  Buffer.from(px.buffer, (H - 1 - y) * W * 4, W * 4).copy(raw, y * (W * 4 + 1) + 1);
}
let TAB = null;
function crc32(buf) {
  if (!TAB) {
    TAB = new Int32Array(256);
    for (let n = 0; n < 256; n++) {
      let c = n;
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      TAB[n] = c;
    }
  }
  let c = -1;
  for (const b of buf) c = TAB[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ -1) >>> 0;
}
const chunk = (type, data) => {
  const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type), data]);
  const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
};
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(W, 0); ihdr.writeUInt32BE(H, 4);
ihdr[8] = 8; ihdr[9] = 6;
fs.writeFileSync(outPng, Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk('IHDR', ihdr),
  chunk('IDAT', zlib.deflateSync(raw)),
  chunk('IEND', Buffer.alloc(0)),
]));
console.log('wrote', outPng);

host.destroy();
if (counts.size < 2 || share >= 92) {
  console.error('NEARLY BLANK: only the clear colour is on screen. The draw call did not land.');
  process.exit(1);
}
process.exit(0);
