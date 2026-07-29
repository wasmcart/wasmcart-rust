# wasmcart-rust

Rust bindings for the [wasmcart](https://github.com/monteslu/wasmcart) cart ABI.

A wasmcart cart is a freestanding `wasm32-unknown-unknown` module. There is no
runtime to embed, no JS glue, and no `wasm-bindgen`: this crate is a set of
`#[repr(C)]` structs, constants, and one macro. Rust carts are the smallest you
can build with any language wasmcart supports.

```
examples/hello      7,677 bytes wasm   1 import  (env.wc_log)
examples/hello_gl   1,905 bytes wasm  22 imports (env.wc_log + 21 gl.*)
```

## Install

```toml
[dependencies]
wasmcart = "0.1"

[lib]
crate-type = ["cdylib"]

[profile.release]
opt-level = "z"
lto = true
panic = "abort"
strip = true
codegen-units = 1
```

```sh
rustup target add wasm32-unknown-unknown
```

## A whole cart

```rust
#![no_std]

use wasmcart::*;

wc_cart!(320, 240);

#[no_mangle]
pub extern "C" fn wc_init() {
    wc_log("hello from rust");
}

#[no_mangle]
pub extern "C" fn wc_render() {
    let buf = fb();
    for (i, px) in buf.iter_mut().enumerate() {
        *px = 0xFF00_0000 | (i as u32 & 0xFFFF);
    }
}
```

`wc_cart!(w, h)` declares every buffer the ABI needs, the live `wc_info_t`
struct, `wc_get_info`, and a panic handler. You write `wc_init` and
`wc_render`. Nothing else is required, and a cart author writes no `unsafe`
around a mutable static.

## Build and run

Verified with `rustc 1.89.0` and `wasmcart` 0.12.2:

```sh
cargo build --release --target wasm32-unknown-unknown

npx wasmcart pack \
  --wasm target/wasm32-unknown-unknown/release/my_cart.wasm \
  --name my_cart -o my_cart.wasc

npx wasmcart my_cart.wasc
```

`--frames 30 --shot out.png` runs headless and writes a screenshot, which is
how you check a cart actually renders. See [Testing](#testing).

## What `wc_cart!` gives you

Accessors, all safe, all returning references into the cart's static buffers:

| function | what |
|---|---|
| `fb()` | framebuffer, `width * height` `u32` pixels, `0xAARRGGBB` |
| `fb_all()` | the whole allocation, `max_width * max_height` |
| `audio()` | interleaved stereo `f32` ring, `audio_cap * 2` samples |
| `audio_cursor()` | the cart's write cursor, in stereo frames |
| `pads()` | `&[WcPad; 4]`, written by the host each frame |
| `time()` | `&WcTime`: `time_ms`, `delta_ms`, `frame` |
| `host_info()` | `&WcHostInfo`, written once before `wc_init` |
| `pointers()` | `&[WcPointer; 10]` (needs `WC_FLAG_POINTER`) |
| `keys()` | `&[u8; 32]` bitmask (needs `WC_FLAG_KEYBOARD`) |
| `save()` | save-data bytes (needs `save_size`) |
| `info()` | the live `WcInfo` |
| `set_size(w, h)` | change the reported resolution, refused if unbacked |

Options, all optional, all after the width and height:

```rust
wc_cart!(640, 480, gpu_api: WC_GPU_API_WEBGL2);
wc_cart!(320, 240, audio_cap: 2048, flags: WC_FLAG_POINTER);
wc_cart!(320, 240, max_width: 1920, max_height: 1080);
wc_cart!(320, 240, save_size: 512);
wc_cart!(320, 240, panic_handler: false);
```

They are the fields of `CartConfig`; anything omitted takes its default.
`audio_cap` defaults to 4096 stereo frames and audio is `f32`
(`WC_FLAG_AUDIO_F32` is always set), matching `wc_cart.h`.

### Adapting to the host's resolution

`max_width`/`max_height` size the framebuffer; `width`/`height` are what the
cart reports. Give yourself headroom, then adopt the host's preference in
`wc_init`:

```rust
wc_cart!(640, 480, max_width: 1920, max_height: 1080);

#[no_mangle]
pub extern "C" fn wc_init() {
    let hi = host_info();
    set_size(hi.preferred_width, hi.preferred_height);
}
```

`set_size` returns `false` and changes nothing if the framebuffer cannot back
the request, which is the same call the host makes: it rejects a resize its
memory does not cover rather than reporting a frame that does not exist.

## Panics

The generated panic handler formats the message into a fixed 512-byte buffer
(no allocator) and sends it through `wc_log` before trapping:

```
[cart] [rust panic] panicked at src/lib.rs:8:15:
index out of bounds: the len is 3 but the index is 10
wasmcart-play: unreachable
```

That is a file, a line, and a reason, which is more than a C cart gets for the
equivalent mistake. The trap is deliberate: the obvious `loop {}` never returns
from `wc_render`, so the host gets no frame and no error and the player simply
stops and has to be killed. Trapping hands control back with the reason already
logged.

It costs one import, `env.wc_log`, which is inside the ABI's allowed `env`
module. Pass `panic_handler: false` to write your own.

## GL carts

Set `gpu_api: WC_GPU_API_WEBGL2` and use `wasmcart::gl`:

```rust
use wasmcart::gl::*;

wc_cart!(640, 480, gpu_api: WC_GPU_API_WEBGL2);

const VS: &[u8] = shader_source!(
    "#version 300 es\n",
    "layout(location=0) in vec2 a_pos;\n",
    "void main() { gl_Position = vec4(a_pos, 0.0, 1.0); }\n",
);
```

`gpu_api` and the imports must agree: the host detects GL from the wasm import
section before instantiation and treats `gpu_api` as authoritative afterwards.

The GL bindings are raw `extern "C"` declarations, `unsafe` to call. That is on
purpose. A cart doing GL is writing or porting GL code, and a safe wrapper here
would be a second API to learn that still could not make `glDrawArrays` safe.
Only functions you call end up in the import section, so a triangle imports 21
symbols, not the whole surface.

Shaders must be `#version 300 es` or `#version 100`; desktop GL version strings
are not supported and the host does not translate them. A VAO must be bound in
ES 3.0 core or every draw is silently invalid and you see only the clear colour.

## Examples

```sh
cd examples/hello    && cargo build --release --target wasm32-unknown-unknown
cd examples/hello_gl && cargo build --release --target wasm32-unknown-unknown
```

- **`examples/hello`**: 2D. An animated gradient plus a d-pad-movable square.
- **`examples/hello_gl`**: GL. A spinning RGB triangle over a dark clear.

`template/` is a copyable starter: copy it, rename the package, run
`./build.sh`.

## Testing

**"It built" and "it ran 30 frames" are not evidence.** A cart that loads,
returns frames, and renders nothing looks identical to a working one from the
outside. Take the screenshot and look at it.

```sh
./scripts/test.sh
```

That builds both examples, checks their exports and that every import is in an
allowed module (`env`, `gl`, `wasi*`), packs them, runs them, and fails if a
frame is one flat colour. The GL example goes through `scripts/glcheck.mjs`,
which drives a real headless `webgl-node` context and reads the result back
with `glReadPixels`, because the terminal player has no context to give a GL
cart.

The script ends with a control that **must** fail: a copy of `hello` with
`wc_get_info` renamed, which the host has to reject with
`Cart must export wc_get_info`. If the control passes, the harness is not
validating anything and the green run above it means nothing.

## `no_std`, and the `std` feature

`#![no_std]` is the default and the right choice. On `wasm32-unknown-unknown`,
`std` drags in panic machinery and expects an allocator you have to supply
yourself, for a cart that has no filesystem, no threads, and no OS.

The `std` feature exists for carts that genuinely want `Vec`/`String`. It is
not enabled by default, it needs your own `#[global_allocator]`, and it
suppresses the generated panic handler (`std` brings its own).

## ABI notes

`WcInfo` is 17 `u32` fields and the order is load-bearing. The v3 tail
(`pointer_ptr` at 56, `keys_ptr` at 60, `gpu_api` at 64) is what hand-written
bindings get wrong. The struct sizes are asserted at compile time against the
host's `src/abi.js`, so a drifted binding is a build error rather than a cart
that renders garbage:

```
WcPad      16 bytes
WcTime     24 bytes (20 of fields, padded to align 8)
WcHostInfo 20 bytes
WcPointer   8 bytes
WcInfo     68 bytes
```

`wc_get_info` must return a pointer to a **live** struct, never a copy: the
host re-reads it after `wc_init` so a cart can adapt its resolution. The macro
handles this.

## Non-goals

- **No `wasm-bindgen`.** It targets JS-host interop and introduces imports the
  validator rejects. The whole point is a freestanding module.
- **No runtime, no engine, no allocator.** This is a binding.

## License

MIT
