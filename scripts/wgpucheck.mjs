// Run the wgpu example carts on wasmcart's Node host (WebGPU through
// webgpu-node) and check what they drew, pixel by pixel.
//
// "It loaded and ran N frames" says nothing about a GPU cart: a frame that
// is all clear colour, or all black, passes that. So each cart is checked at
// known pixels against colours the cart can only produce by working: the
// clear colour in the corners, each vertex colour near its vertex, a moving
// picture between two frames, and the log lines that prove async results
// (buffer readback, error scope, compilation info) came back. Every check
// runs on a core device and again in compatibility mode, the tier's floor.
//
// Controls that must fail, so a green run means something:
// - the same host with WebGPU turned off (wgpu: false) must refuse the cart;
// - the triangle's pixel checks run against the scene's picture must fail.
//
// Usage: node scripts/wgpucheck.mjs <triangle.wasc> <scene.wasc> <outdir>
// WASMCART_DIR points at a wasmcart checkout; otherwise an install is used.
import { pathToFileURL } from 'node:url';
import path from 'node:path';
import fs from 'node:fs';
import { encodePng } from './png.mjs';
import { wasmcartDir as wasmcartDir_ } from './wasmcart-dir.mjs';

const [, , trianglePath, scenePath, outDir] = process.argv;
if (!trianglePath || !scenePath || !outDir) {
  console.error('usage: node scripts/wgpucheck.mjs <triangle.wasc> <scene.wasc> <outdir>');
  process.exit(2);
}
fs.mkdirSync(outDir, { recursive: true });

const wasmcartDir = wasmcartDir_();
const { CartHost } = await import(pathToFileURL(path.join(wasmcartDir, 'index.js')));

let failures = 0;
const fail = (msg) => {
  console.error(`  FAIL ${msg}`);
  failures++;
};
const pass = (msg) => console.log(`  ok   ${msg}`);

/** Run a cart for `frames`, yielding to the event loop between frames, and
 * return the shots taken at `shotsAt` plus every log line. */
async function run(cart, { frames, shotsAt, featureLevel, wgpu }) {
  const host = new CartHost();
  const opts = { adapterOptions: { featureLevel } };
  if (wgpu === false) opts.wgpu = false;
  await host.load(fs.readFileSync(cart), opts);
  if (!host.usesWgpu) throw new Error('the cart did not run on WebGPU');
  const shots = {};
  const log = [];
  for (let f = 1; f <= frames; f++) {
    host.runFrame([{ connected: true, buttons: 0 }]);
    if (host.gpuLost) throw new Error(`GPU lost: ${JSON.stringify(host.gpuLost)}`);
    await new Promise((r) => setImmediate(r));
    if (shotsAt.includes(f)) {
      const g = await host.readGpuFrame();
      shots[f] = g;
    }
  }
  log.push(...host.drainDebugEvents().log.map((l) => l.text));
  host.destroy();
  return { shots, log };
}

const px = (g, x, y) => {
  const i = (y * g.width + x) * 4;
  return [g.data[i], g.data[i + 1], g.data[i + 2]];
};
const near = (a, b, tol) => a.every((v, i) => Math.abs(v - b[i]) <= tol);
const srgb = (l) => Math.round(255 * (l <= 0.0031308 ? 12.92 * l : 1.055 * Math.pow(l, 1 / 2.4) - 0.055));

function save(g, name) {
  const file = path.join(outDir, name);
  fs.writeFileSync(file, encodePng(g.data, g.width, g.height));
  return file;
}

/** The triangle's checks; returns the list of failed ones (empty = pass). */
function triangleChecks(g) {
  const bad = [];
  const clear = [0, 0, srgb(0.125)];
  for (const [x, y] of [[4, 4], [635, 4], [4, 475], [635, 475], [320, 450]]) {
    if (!near(px(g, x, y), clear, 3)) bad.push(`clear at (${x},${y}) is ${px(g, x, y)}, want ${clear}`);
  }
  // Near each vertex its colour dominates; the centre is a mix of all three.
  const dominant = (c, k) => c[k] > 180 && c.every((v, i) => i === k || v < c[k] - 60);
  const tip = px(g, 320, 80), left = px(g, 100, 370), right = px(g, 540, 370), mid = px(g, 320, 280);
  if (!dominant(tip, 0)) bad.push(`top vertex (320,80) is ${tip}, want red`);
  if (!dominant(left, 1)) bad.push(`left vertex (100,370) is ${left}, want green`);
  if (!dominant(right, 2)) bad.push(`right vertex (540,370) is ${right}, want blue`);
  if (!mid.every((v) => v > 90)) bad.push(`centre (320,280) is ${mid}, want a mix of all three`);
  return bad;
}

function sceneChecks(a, b) {
  const bad = [];
  const clear = [srgb(0.02), srgb(0.05), srgb(0.03)];
  for (const [x, y] of [[4, 4], [635, 4], [4, 475], [635, 475]]) {
    if (!near(px(a, x, y), clear, 3)) bad.push(`clear at (${x},${y}) is ${px(a, x, y)}, want ${clear}`);
  }
  // The cube covers the centre: not the clear colour, and textured (several
  // distinct colours in a 64x64 block).
  const colours = new Set();
  let covered = 0;
  for (let y = 208; y < 272; y += 2) {
    for (let x = 288; x < 352; x += 2) {
      const c = px(a, x, y);
      colours.add(c.map((v) => v >> 4).join(','));
      if (!near(c, clear, 6)) covered++;
    }
  }
  if (covered < 1000) bad.push(`cube covers ${covered}/1024 centre samples`);
  if (colours.size < 12) bad.push(`centre has ${colours.size} colours, want a texture`);
  // It moves: the two shots differ in many pixels.
  let changed = 0;
  for (let i = 0; i < a.data.length; i += 4) {
    if (Math.abs(a.data[i] - b.data[i]) + Math.abs(a.data[i + 1] - b.data[i + 1]) + Math.abs(a.data[i + 2] - b.data[i + 2]) > 30) changed++;
  }
  if (changed < 5000) bad.push(`only ${changed} pixels changed between frames 30 and 60`);
  return bad;
}

const report = (what, bad) => (bad.length ? bad.forEach((b) => fail(`${what}: ${b}`)) : pass(what));

for (const level of ['core', 'compatibility']) {
  console.log(`── ${level} device ──`);
  const t = await run(trianglePath, { frames: 10, shotsAt: [10], featureLevel: level });
  console.log(`  shot ${save(t.shots[10], `wgpu_triangle_${level}.png`)}`);
  report(`triangle pixels (${level})`, triangleChecks(t.shots[10]));

  const s = await run(scenePath, { frames: 60, shotsAt: [30, 60], featureLevel: level });
  console.log(`  shot ${save(s.shots[30], `wgpu_scene_${level}.png`)}`);
  report(`scene pixels (${level})`, sceneChecks(s.shots[30], s.shots[60]));
  for (const want of [/^scene: ready$/, /^scene: readback ok /, /^scene: errors none$/, /^scene: shader messages 0$/]) {
    if (s.log.some((l) => want.test(l))) pass(`scene log ${want} (${level})`);
    else fail(`scene log ${want} missing (${level}); log: ${JSON.stringify(s.log)}`);
  }
  const core = s.log.find((l) => l.startsWith('scene: device'));
  const wantCore = level === 'core' ? 'core=true' : 'core=false';
  if (core?.includes(wantCore)) pass(`device reports ${wantCore}`);
  else fail(`device line ${JSON.stringify(core)} lacks ${wantCore}`);

  // Control: the triangle's checks on the scene's picture must fail.
  if (triangleChecks(s.shots[30]).length === 0) fail('CONTROL: triangle checks passed on the scene picture; the checks prove nothing');
  else pass(`control: triangle checks reject the scene picture (${level})`);
}

// Control: a host without WebGPU must refuse a WebGPU-only cart at load.
console.log('── control: host with wgpu: false ──');
try {
  await run(trianglePath, { frames: 1, shotsAt: [], featureLevel: 'core', wgpu: false });
  fail('CONTROL: a host with WebGPU disabled ran the cart');
} catch (e) {
  pass(`refused: ${String(e.message).split('\n')[0].slice(0, 120)}`);
}

if (failures) {
  console.error(`wgpucheck: ${failures} failure(s)`);
  process.exit(1);
}
console.log('wgpucheck: all checks passed');
process.exit(0);
