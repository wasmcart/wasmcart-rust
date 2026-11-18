#!/usr/bin/env node
// Generate wasmcart-wgpu/src/ffi.rs from an emdawnwebgpu package: every enum,
// flag, handle, callback type and struct in its webgpu.h, the extern block for
// the functions the host's glue provides (wasmcart's wgpu-wasi/imports.txt),
// and compile-time checks of every struct's field offsets and size against the
// package's own struct table (library_webgpu_generated_struct_info.js), which
// is what the JavaScript half reads cart memory with.
//
// usage: node scripts/gen-webgpu-ffi.mjs <emdawnwebgpu_pkg> <wasmcart repo>
//
// The package must be the release wasmcart pins (scripts/wgpu/emdawnwebgpu.json
// in the wasmcart repo); the release is recorded in the output.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const [pkg, wasmcart] = process.argv.slice(2);
if (!pkg || !wasmcart) {
  console.error('usage: node scripts/gen-webgpu-ffi.mjs <emdawnwebgpu_pkg> <wasmcart repo>');
  process.exit(2);
}
const here = path.dirname(fileURLToPath(import.meta.url));
const out = path.join(here, '..', 'wasmcart-wgpu', 'src', 'ffi.rs');

const header = fs.readFileSync(path.join(pkg, 'webgpu/include/webgpu/webgpu.h'), 'utf8');
const structInfoSrc = fs.readFileSync(path.join(pkg, 'webgpu/src/library_webgpu_generated_struct_info.js'), 'utf8');
const pin = JSON.parse(fs.readFileSync(path.join(wasmcart, 'scripts/wgpu/emdawnwebgpu.json'), 'utf8'));
const version = fs.existsSync(path.join(pkg, 'VERSION.txt'))
  ? fs.readFileSync(path.join(pkg, 'VERSION.txt'), 'utf8').trim() : '';
if (!version.includes(pin.release)) {
  console.error(`package is not the pinned release ${pin.release}: ${version || '(no VERSION.txt)'}`);
  process.exit(1);
}
const imports = new Set(fs.readFileSync(path.join(wasmcart, 'wgpu-wasi/imports.txt'), 'utf8').trim().split('\n'));

// Struct table the glue was generated with (wasm32).
const m = structInfoSrc.match(/const structInfo32 = \(\s*(\{[\s\S]*?\})\s*\);/);
if (!m) throw new Error('structInfo32 not found');
const structInfo = JSON.parse(m[1]).structs;

const strip = s => s.replace(/WGPU_NULLABLE/g, '').replace(/\bstruct\s+/g, '').replace(/\s+/g, ' ').trim();

const handles = [];
const enums = [];      // {name, values:[[n,v]]}
const flags = [];      // {name, values:[[n,v]]}
const callbacks = [];  // {name, ret, params}
const structs = new Map(); // name -> fields [{name, type}]
const funcs = [];      // {name, ret, params}

for (const mm of header.matchAll(/typedef struct (\w+)Impl\* (\w+) WGPU_OBJECT_ATTRIBUTE;/g)) handles.push(mm[2]);
for (const mm of header.matchAll(/typedef enum (\w+) \{([\s\S]*?)\} \1 WGPU_ENUM_ATTRIBUTE;/g)) {
  const values = [];
  for (const v of mm[2].matchAll(/(\w+) = (0x[0-9A-Fa-f]+|\d+)/g)) {
    if (v[1].endsWith('_Force32')) continue;
    values.push([v[1], v[2]]);
  }
  enums.push({ name: mm[1], values });
}
for (const mm of header.matchAll(/typedef WGPUFlags (\w+);/g)) {
  const name = mm[1];
  const values = [];
  for (const v of header.matchAll(new RegExp(`static const ${name} (\\w+) = (0x[0-9A-Fa-f]+);`, 'g'))) values.push([v[1], v[2]]);
  flags.push({ name, values });
}
for (const mm of header.matchAll(/typedef (\w+) \(\*(WGPU\w+Callback)\)\(([^)]*)\) WGPU_FUNCTION_ATTRIBUTE;/g)) {
  callbacks.push({ name: mm[2], ret: mm[1], params: splitParams(mm[3]) });
}
for (const mm of header.matchAll(/typedef struct (\w+) \{([\s\S]*?)\} \1 WGPU_STRUCTURE_ATTRIBUTE;/g)) {
  const fields = [];
  for (const line of mm[2].split('\n')) {
    const l = strip(line.replace(/\/\/.*$/, '').replace(/;$/, ''));
    if (!l) continue;
    const f = l.match(/^(.*?)\s*(\w+)$/);
    fields.push({ name: f[2], type: f[1] });
  }
  structs.set(mm[1], fields);
}
for (const mm of header.matchAll(/WGPU_EXPORT (.+?) (\w+)\(([^)]*)\)(?: WGPU_FUNCTION_ATTRIBUTE)?;/g)) {
  funcs.push({ ret: strip(mm[1]), name: mm[2], params: splitParams(mm[3]) });
}

function splitParams(s) {
  s = s.trim();
  if (!s || s === 'void') return [];
  return s.split(',').map(p => {
    const f = strip(p).match(/^(.*?)\s*(\w+)$/);
    return { name: f[2], type: f[1] };
  });
}

const prim = {
  uint8_t: 'u8', uint16_t: 'u16', uint32_t: 'u32', uint64_t: 'u64', int8_t: 'i8', int16_t: 'i16',
  int32_t: 'i32', int64_t: 'i64', int: 'i32', size_t: 'usize', float: 'f32', double: 'f64',
  char: 'c_char', void: 'c_void', WGPUBool: 'WGPUBool', WGPUFlags: 'WGPUFlags',
};
const rustKw = new Set(['type', 'ref', 'match', 'use', 'mod', 'fn', 'loop', 'move', 'self', 'where', 'impl']);
const fieldName = n => (rustKw.has(n) ? `r#${n}` : n);

function rustType(t) {
  t = strip(t);
  // pointer forms: "X const *", "X *", "X const * const *"
  const ptr = t.match(/^(.*?)\s*(const\s*)?\*$/);
  if (ptr) {
    const inner = ptr[1].trim();
    // a trailing "const" before * belongs to the pointee
    let pointee = inner, isConst = !!ptr[2];
    if (/\bconst$/.test(pointee)) { pointee = pointee.replace(/\s*const$/, ''); isConst = true; }
    return `${isConst ? '*const' : '*mut'} ${rustType(pointee)}`;
  }
  if (t.startsWith('const ')) return rustType(t.slice(6));
  return prim[t] || t;
}

// A by-value aggregate parameter: the wasm C ABI passes it by pointer, which
// is what the glue expects, so the import takes a pointer.
const isStruct = t => structs.has(strip(t));
function paramType(p) {
  return isStruct(p.type) ? `*const ${rustType(p.type)}` : rustType(p.type);
}

let o = '';
o += `// GENERATED by scripts/gen-webgpu-ffi.mjs from emdawnwebgpu ${pin.release}\n`;
o += `// (${version.split('\n')[0]}). Do not edit; regenerate.\n`;
o += '//\n// webgpu.h for the wasm32 cart side: types, the functions wasmcart hosts\n';
o += '// import into a cart (module `env`), and offset checks against the struct\n';
o += '// table the host glue reads cart memory with.\n';
o += '#![allow(non_camel_case_types, non_upper_case_globals, non_snake_case, dead_code, clippy::all)]\n\n';
o += 'use core::ffi::{c_char, c_void};\n\n';
o += `/// The emdawnwebgpu release this file was generated from (wasmcart's pin).\npub const EMDAWNWEBGPU_RELEASE: &str = "${pin.release}";\n\n`;
o += 'pub type WGPUFlags = u64;\npub type WGPUBool = u32;\n\n';
for (const [n, v] of [['WGPU_ARRAY_LAYER_COUNT_UNDEFINED', 'u32::MAX'], ['WGPU_COPY_STRIDE_UNDEFINED', 'u32::MAX'],
  ['WGPU_DEPTH_SLICE_UNDEFINED', 'u32::MAX'], ['WGPU_LIMIT_U32_UNDEFINED', 'u32::MAX'], ['WGPU_LIMIT_U64_UNDEFINED', 'u64::MAX'],
  ['WGPU_MIP_LEVEL_COUNT_UNDEFINED', 'u32::MAX'], ['WGPU_QUERY_SET_INDEX_UNDEFINED', 'u32::MAX'], ['WGPU_STRLEN', 'usize::MAX'],
  ['WGPU_WHOLE_MAP_SIZE', 'usize::MAX'], ['WGPU_WHOLE_SIZE', 'u64::MAX']]) {
  o += `pub const ${n}: ${v.split('::')[0]} = ${v};\n`;
}
o += 'pub const WGPU_DEPTH_CLEAR_VALUE_UNDEFINED: f32 = f32::NAN;\n\n';

o += '// ---- handles ----\n';
for (const h of handles) {
  o += `#[repr(C)] pub struct ${h}Impl { _private: [u8; 0] }\npub type ${h} = *mut ${h}Impl;\n`;
}
o += '\n// ---- enums ----\n';
for (const e of enums) {
  o += `pub type ${e.name} = u32;\n`;
  for (const [n, v] of e.values) o += `pub const ${n}: ${e.name} = ${v};\n`;
}
o += '\n// ---- flags ----\n';
for (const f of flags) {
  o += `pub type ${f.name} = WGPUFlags;\n`;
  for (const [n, v] of f.values) o += `pub const ${n}: ${f.name} = ${v};\n`;
}
o += '\n// ---- callbacks ----\n';
for (const c of callbacks) {
  const params = c.params.map(p => `${fieldName(p.name)}: ${rustType(p.type)}`).join(', ');
  const ret = c.ret === 'void' ? '' : ` -> ${rustType(c.ret)}`;
  o += `pub type ${c.name} = Option<unsafe extern "C" fn(${params})${ret}>;\n`;
}
o += 'pub type WGPUProc = Option<unsafe extern "C" fn()>;\n';
o += '\n// ---- structs ----\n';
for (const [name, fields] of structs) {
  o += `#[repr(C)]\n#[derive(Clone, Copy)]\npub struct ${name} {\n`;
  for (const f of fields) o += `    pub ${fieldName(f.name)}: ${rustType(f.type)},\n`;
  o += '}\n';
}
o += '\n// ---- functions the host glue provides (wgpu-wasi/imports.txt) ----\n';
o += '#[link(wasm_import_module = "env")]\nextern "C" {\n';
let imported = 0;
for (const f of funcs) {
  if (!imports.has(f.name)) continue;
  imported++;
  const params = f.params.map(p => `${fieldName(p.name)}: ${paramType(p)}`).join(', ');
  const ret = f.ret === 'void' ? '' : ` -> ${rustType(f.ret)}`;
  o += `    pub fn ${f.name}(${params})${ret};\n`;
}
o += '}\n';

o += '\n// ---- layout checks against the glue\'s struct table (wasm32 only) ----\n';
o += '#[cfg(target_arch = "wasm32")]\nconst _: () = {\n    use core::mem::{offset_of, size_of};\n';
let checked = 0;
for (const [name, info] of Object.entries(structInfo)) {
  const fields = structs.get(name);
  if (!fields) continue;
  o += `    assert!(size_of::<${name}>() == ${info.__size__});\n`;
  for (const [fname, off] of Object.entries(info)) {
    if (fname === '__size__') continue;
    if (!fields.some(f => f.name === fname)) continue;
    o += `    assert!(offset_of!(${name}, ${fieldName(fname)}) == ${off});\n`;
    checked++;
  }
}
o += '};\n';

fs.writeFileSync(out, o);
console.log(`wrote ${path.relative(process.cwd(), out)}: ${handles.length} handles, ${enums.length} enums, ${flags.length} flag sets, ${structs.size} structs, ${imported} imports, ${checked} offset checks`);
