#!/usr/bin/env bash
#
# Build both examples, pack them, run them, and check the output is not blank.
# This is the gate: "it built" and "it ran N frames" are not evidence that a
# cart renders anything. The 2D example is checked by reading its framebuffer
# back through the host; the GL example needs a real GL context, so it is
# checked separately by scripts/glcheck.mjs.
#
# Requires: rustup target add wasm32-unknown-unknown, and npx (for `wasmcart`).
set -euo pipefail
cd "$(dirname "$0")/.."
OUT="$(pwd)/scripts/out"
mkdir -p "$OUT"

WASMCART=${WASMCART:-npx --yes wasmcart}

# The compile-time asserts in src/lib.rs prove the structs are self-consistent.
# They cannot prove a CONSTANT is right, because they only compare the crate
# against itself. This compares it against wasmcart's own src/abi.js.
echo "── abi drift: constants must match the spec ──"
node scripts/abi-drift.mjs
echo

build_and_pack() {
  local dir=$1 name=$2
  echo "── $name ────────────────────────────────────────────"
  (cd "examples/$dir" && cargo build --release --target wasm32-unknown-unknown)
  local wasm="examples/$dir/target/wasm32-unknown-unknown/release/$name.wasm"
  echo "wasm: $(stat -c%s "$wasm") bytes"
  node -e "
    const fs=require('fs');
    const m=new WebAssembly.Module(fs.readFileSync('$wasm'));
    const ex=WebAssembly.Module.exports(m).map(e=>e.name);
    for (const need of ['memory','wc_get_info','wc_render'])
      if (!ex.includes(need)) { console.error('MISSING EXPORT: '+need); process.exit(1); }
    const im=WebAssembly.Module.imports(m);
    console.log('imports: ' + (im.length ? im.map(i=>i.module+'.'+i.name).join(' ') : '(none)'));
    const bad=im.filter(i=>!['env','gl','wasi_snapshot_preview1','wasi'].includes(i.module));
    if (bad.length) { console.error('ILLEGAL IMPORT MODULE: '+bad.map(b=>b.module).join(',')); process.exit(1); }
  "
  $WASMCART pack --wasm "$wasm" --name "$name" -o "$OUT/$name.wasc" >/dev/null
  echo "packed: $OUT/$name.wasc"
}

build_and_pack hello hello
$WASMCART "$OUT/hello.wasc" --frames 30 --shot "$OUT/hello.png"
node scripts/notblank.mjs "$OUT/hello.png"

build_and_pack hello_gl hello_gl
echo "hello_gl needs a GL context; checking with scripts/glcheck.mjs"
node scripts/glcheck.mjs "$OUT/hello_gl.wasc" "$OUT/hello_gl.png"

# Control that MUST fail. If the host accepts a cart with no wc_get_info, the
# harness above is not actually validating anything and every green run is
# meaningless.
echo "── control: cart with no wc_get_info must be REJECTED ──"
node -e "
  const fs=require('fs');
  const b=fs.readFileSync('$OUT/../../examples/hello/target/wasm32-unknown-unknown/release/hello.wasm');
  const i=b.indexOf(Buffer.from('wc_get_info'));
  if (i<0) { console.error('could not find the export name to corrupt'); process.exit(1); }
  b.write('wc_get_inXo', i);
  fs.writeFileSync('$OUT/broken.wasm', b);
"
$WASMCART pack --wasm "$OUT/broken.wasm" --name broken -o "$OUT/broken.wasc" >/dev/null
if $WASMCART "$OUT/broken.wasc" --frames 5 >/dev/null 2>&1; then
  echo "CONTROL DID NOT FAIL: the host accepted a cart with no wc_get_info."
  echo "The harness is broken; a green run above proves nothing."
  exit 1
fi
echo "control rejected as expected"

echo
echo "all checks passed"
