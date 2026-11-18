// Check a wgpu cart's wasm against the WebGPU tier's import and export lists
// (wasmcart's wgpu-wasi/imports.txt and exports.txt): every import is a
// function from `env`, named in imports.txt or a wasmcart host function
// (wc_*); nothing from wasm-bindgen; and every export the host glue calls
// back into is present.
//
// Usage: node scripts/wgpuimports.mjs cart.wasm
import fs from 'node:fs';
import path from 'node:path';
import { wasmcartDir } from './wasmcart-dir.mjs';

const wasm = process.argv[2];
const dir = wasmcartDir();
const list = (f) => new Set(fs.readFileSync(path.join(dir, 'wgpu-wasi', f), 'utf8').trim().split('\n'));
const allowed = list('imports.txt');
const required = list('exports.txt');

const m = new WebAssembly.Module(fs.readFileSync(wasm));
const imports = WebAssembly.Module.imports(m);
const exports = new Set(WebAssembly.Module.exports(m).map((e) => e.name));
const bad = imports.filter(
  (i) => i.module !== 'env' || i.kind !== 'function' || (!allowed.has(i.name) && !i.name.startsWith('wc_')),
);
const bindgen = imports.filter((i) => /wbindgen|wbg/.test(i.module + i.name));
const missing = [...required].filter((n) => !exports.has(n));
for (const need of ['memory', 'wc_get_info', 'wc_init', 'wc_render']) if (!exports.has(need)) missing.push(need);

console.log(`${path.basename(wasm)}: ${imports.length} imports (${imports.filter((i) => i.name.startsWith('wc_')).map((i) => i.name).join(' ')} + WebGPU), ${exports.size} exports`);
if (bad.length) console.error('ILLEGAL IMPORTS: ' + bad.map((i) => `${i.module}.${i.name}`).join(' '));
if (bindgen.length) console.error('WASM-BINDGEN IMPORTS: ' + bindgen.map((i) => `${i.module}.${i.name}`).join(' '));
if (missing.length) console.error('MISSING EXPORTS: ' + missing.join(' '));
if (bad.length || bindgen.length || missing.length) process.exit(1);
