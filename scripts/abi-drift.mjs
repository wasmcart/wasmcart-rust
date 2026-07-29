#!/usr/bin/env node
/*
 * abi-drift.mjs -- check this binding's constants against the spec.
 *
 * A binding written in Rust cannot #include wasmcart.h, so every constant here
 * is a hand transcription, and a transcription drifts. It happened: wasmcart
 * 0.16.0 merged the WebSocket and data-channel flags into one peer flag, and
 * this crate still declared WC_FLAG_NET_WS and WC_FLAG_NET_DC afterwards.
 * Nothing broke, because the surviving flag kept the same bit, but the binding
 * named a flag the spec no longer had.
 *
 * The compile-time asserts in src/lib.rs catch a struct whose SIZE is wrong.
 * They cannot catch a constant whose VALUE is wrong, because they only compare
 * the crate against itself. This compares it against src/abi.js in the wasmcart
 * repo, which is the machine-readable source of truth the host itself reads.
 *
 *   node scripts/abi-drift.mjs [--wasmcart <path-to-wasmcart-checkout>]
 *
 * Resolution order for the spec: --wasmcart, $WASMCART_DIR, the installed
 * `wasmcart` package, then ../wasmcart. Skips with a clear message if none
 * resolve, so the check never fails a machine that simply lacks a checkout.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const CRATE = path.join(HERE, '..');

function specDir() {
  const i = process.argv.indexOf('--wasmcart');
  if (i >= 0 && process.argv[i + 1]) return process.argv[i + 1];
  if (process.env.WASMCART_DIR) return process.env.WASMCART_DIR;
  try {
    return path.dirname(fileURLToPath(import.meta.resolve('wasmcart')));
  } catch { /* not installed */ }
  return path.join(CRATE, '..', 'wasmcart');
}

const dir = specDir();
let abi;
try {
  abi = await import(path.join(dir, 'src', 'abi.js'));
} catch {
  console.log(`skip  abi-drift   no wasmcart checkout at ${dir}`);
  console.log('      pass --wasmcart <path>, set WASMCART_DIR, or npm i wasmcart');
  process.exit(0);
}

const src = readFileSync(path.join(CRATE, 'src', 'lib.rs'), 'utf8');

/* Read `pub const NAME: u32 = <expr>;` and evaluate the shift form the crate
 * uses. Deliberately not a full expression parser: anything it cannot read is
 * reported as unreadable rather than silently skipped. */
function rustConst(name) {
  const m = src.match(new RegExp(`pub const ${name}\\s*:\\s*u32\\s*=\\s*([^;]+);`));
  if (!m) return undefined;
  const e = m[1].trim();
  const shift = e.match(/^1\s*<<\s*(\d+)$/);
  if (shift) return 1 << Number(shift[1]);
  if (/^\d+$/.test(e)) return Number(e);
  return NaN;   // present but not in a form we read
}

/* spec name -> crate name */
const CONSTS = {
  ABI_VERSION: 'WC_ABI_VERSION',
  FLAG_AUDIO_F32: 'WC_FLAG_AUDIO_F32',
  FLAG_NET_PEER: 'WC_FLAG_NET_PEER',
  FLAG_POINTER: 'WC_FLAG_POINTER',
  FLAG_KEYBOARD: 'WC_FLAG_KEYBOARD',
  FLAG_DEBUG: 'WC_FLAG_DEBUG',
  FLAG_DETERMINISTIC: 'WC_FLAG_DETERMINISTIC',
  GPU_API_NONE: 'WC_GPU_API_NONE',
  GPU_API_WEBGL2: 'WC_GPU_API_WEBGL2',
};

/* Names the spec has RETIRED. A binding that still declares one is telling its
 * users about a flag that no longer exists, which is the drift that prompted
 * this script. */
const RETIRED = ['WC_FLAG_NET_WS', 'WC_FLAG_NET_DC'];

const problems = [];
let checked = 0;

for (const [specName, rustName] of Object.entries(CONSTS)) {
  const want = abi[specName];
  if (want === undefined) continue;      // spec does not define it; nothing to check
  const got = rustConst(rustName);
  if (got === undefined) {
    problems.push(`${rustName} is missing; spec has ${specName} = ${want}`);
  } else if (Number.isNaN(got)) {
    problems.push(`${rustName} is not in a form this check can read`);
  } else if (got !== want) {
    problems.push(`${rustName} = ${got}, spec says ${want}`);
  } else {
    checked++;
  }
}

for (const dead of RETIRED) {
  if (rustConst(dead) !== undefined) {
    problems.push(`${dead} is retired from the spec but still declared here`);
  }
}

/* Struct sizes. lib.rs already asserts these at compile time against literals;
 * this is what ties those literals to the spec. */
const SIZES = {
  PAD_SIZE: /size_of::<WcPad>\(\)\s*==\s*(\d+)/,
  HOST_INFO_SIZE: /size_of::<WcHostInfo>\(\)\s*==\s*(\d+)/,
  POINTER_SIZE: /size_of::<WcPointer>\(\)\s*==\s*(\d+)/,
  INFO_SIZE: /size_of::<WcInfo>\(\)\s*==\s*(\d+)/,
};
for (const [specName, re] of Object.entries(SIZES)) {
  const want = abi[specName];
  if (want === undefined) continue;
  const m = src.match(re);
  if (!m) { problems.push(`no compile-time assert found for ${specName}`); continue; }
  if (Number(m[1]) !== want) {
    problems.push(`${specName}: crate asserts ${m[1]}, spec says ${want}`);
  } else checked++;
}

/* The v3 tail offsets are the ones a hand transcription gets wrong. */
if (abi.INFO_FIELDS_V3) {
  const order = [
    'version', 'width', 'height', 'fb_ptr', 'audio_ptr', 'audio_cap',
    'audio_write_ptr', 'input_ptr', 'save_ptr', 'save_size', 'time_ptr',
    'host_info_ptr', 'flags', 'audio_sample_rate', 'pointer_ptr', 'keys_ptr',
    'gpu_api',
  ];
  const body = src.match(/pub struct WcInfo\s*\{([^}]*)\}/s);
  if (!body) problems.push('could not find `pub struct WcInfo` to check field order');
  else {
    const fields = [...body[1].matchAll(/pub\s+([a-z_0-9]+)\s*:/g)].map((m) => m[1]);
    for (const [specField, off] of Object.entries(abi.INFO_FIELDS_V3)) {
      const name = specField.toLowerCase();
      const idx = fields.indexOf(name);
      if (idx < 0) { problems.push(`WcInfo is missing field ${name}`); continue; }
      if (idx * 4 !== off) {
        problems.push(`WcInfo.${name} is at byte ${idx * 4}, spec says ${off}`);
      } else checked++;
    }
    if (fields.join(',') !== order.join(',')) {
      problems.push(`WcInfo field order differs from the spec:\n    got  ${fields.join(' ')}\n    want ${order.join(' ')}`);
    } else checked++;
  }
}

if (problems.length) {
  console.error(`FAIL  abi-drift   ${problems.length} problem(s) against ${dir}`);
  for (const p of problems) console.error(`      ${p}`);
  process.exit(1);
}
console.log(`ok    abi-drift    ${checked} constants match the spec at ${dir}`);
