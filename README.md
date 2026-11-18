# wasmcart-rust

Rust bindings for the [wasmcart](https://github.com/wasmcart/wasmcart) cart ABI.

A wasmcart cart is a freestanding `wasm32-unknown-unknown` module. There is no
runtime to embed, no JS glue, and no `wasm-bindgen`: this crate is a set of
`#[repr(C)]` structs, constants, and one macro. Rust carts are the smallest you
can build with any language wasmcart supports.

```
examples/hello      7,677 bytes wasm   1 import  (env.wc_log)
examples/hello_gl   1,905 bytes wasm  22 imports (env.wc_log + 21 gl.*)
examples/wgpu_triangle  127 KB wasm  wgpu over the WebGPU imports (env.wc_log + 97 WebGPU)
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
  --name my_cart --version 0.1.0 -o my_cart.wasc

npx wasmcart my_cart.wasc
```

The cart's version is the crate's: `--version` takes `version` from
`Cargo.toml` (`template/build.sh` reads it with `cargo metadata`). Without
`--version`, wasmcart-pack 0.39.0 or later takes it from the git checkout
instead.

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

## WebGPU carts, with wgpu

`wasmcart-wgpu` (in `wasmcart-wgpu/`) is a [wgpu](https://wgpu.rs) backend for
wasmcart's WebGPU tier. It implements wgpu's `custom` backend traits over the
WebGPU functions a wasmcart host provides as wasm imports (the frozen
emdawnwebgpu import set, SPEC.md "WebGPU"), so a cart, or an engine built on
wgpu, uses the ordinary wgpu API and runs on every host with WebGPU: browser,
Node and native. No wasm-bindgen, no web-sys, no JavaScript in the cart.

```toml
[dependencies]
wasmcart = { version = "0.1", features = ["std"] }   # std: wgpu needs an allocator
wasmcart-wgpu = { git = "https://github.com/wasmcart/wasmcart-rust" }
```

```rust
use wasmcart::*;
use wasmcart_wgpu::{wgpu, Gpu};

wc_cart!(640, 480, gpu_api: WC_GPU_API_WEBGPU);

static mut GPU: Option<Gpu> = None;

#[no_mangle]
pub extern "C" fn wc_init() {
    // The host's device and the cart's "#canvas", synchronously.
    let gpu = Gpu::new(640, 480).expect("no WebGPU on this host");
    // gpu.device / gpu.queue / gpu.surface are plain wgpu objects:
    // make shaders, pipelines and buffers as in any wgpu program.
    unsafe { GPU = Some(gpu) };
}

#[no_mangle]
pub extern "C" fn wc_render() {
    #[allow(static_mut_refs)]
    let Some(gpu) = (unsafe { GPU.as_ref() }) else { return };
    let Some(frame) = gpu.frame() else { return };
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &frame.view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(gpu.clear_color(wgpu::Color::BLUE)),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
    gpu.queue.submit([enc.finish()]);
    gpu.present(frame); // releases the frame; the host presents the canvas
}
```

What differs from wgpu on a desktop:

- **Nothing blocks.** `map_async`, error scopes, `on_submitted_work_done` and
  shader compilation info complete between frames, when the host delivers
  them; `Device::poll` returns at once. Poll futures each frame
  (`wasmcart_wgpu::poll_once`, or `now` for one that may already be done).
  `request_adapter` and a `request_device` the host's device satisfies are
  ready on the first poll, so set-up fits in `wc_init`.
- **One surface**: the host's canvas. `Gpu::new` configures it;
  `wasmcart_wgpu::canvas_surface` (or `create_surface(CanvasWindow)`) gives it
  to code that sets surfaces up itself. Never present it yourself; the host
  does after `wc_render`.
- **Compatibility mode** is the tier's floor. There the canvas has no sRGB
  view, so `Gpu::srgb` is false: use `gpu.view_format()` for pipeline targets,
  `gpu.clear_color(linear)` for clears, and have shaders encode sRGB when the
  `SRGB_ENCODE` pipeline constant (`gpu.srgb_constants()`) says so. The
  examples do. `wasmcart_wgpu::is_core(&device)` tells the two apart.
- **Shaders** are WGSL, passed straight to the host. With the crate's
  `naga-ir` feature a `ShaderSource::Naga` module is validated and written
  out as WGSL in the cart (this pulls naga in, about 800 KB).
- **Limits**: the adapter reports the host device's features and limits. Ask
  for more and the device request goes to the host, arriving a frame or more
  later (`GpuRequest`).
- Not available (they panic with a message): ray tracing, mesh shaders,
  external textures, `clear_texture`, non-fill polygon modes, binding arrays,
  indirect draw counts. `ClampToBorder` samples as clamp-to-edge.

`wasmcart-wgpu/src/ffi.rs` is generated from the pinned emdawnwebgpu release by
`scripts/gen-webgpu-ffi.mjs` (webgpu.h types plus compile-time checks of every
struct offset against the table the host's glue reads cart memory with).

## Examples

```sh
cd examples/hello    && cargo build --release --target wasm32-unknown-unknown
cd examples/hello_gl && cargo build --release --target wasm32-unknown-unknown
cd examples/wgpu_triangle && cargo build --release --target wasm32-unknown-unknown
cd examples/wgpu_scene && cargo build --release --target wasm32-unknown-unknown
```

- **`examples/hello`**: 2D. An animated gradient plus a d-pad-movable square.
- **`examples/hello_gl`**: GL. A spinning RGB triangle over a dark clear.
- **`examples/wgpu_triangle`**: WebGPU through wgpu. The smallest wgpu cart.
- **`examples/wgpu_scene`**: WebGPU through wgpu. A spinning textured cube with
  depth, a uniform buffer, a render bundle and a compute pass that regenerates
  its texture each frame, plus a buffer readback, an error scope and shader
  compilation info (results that arrive between frames).

`template/` is a copyable starter: copy it, rename the package, run
`./build.sh`. `template-wgpu/` is the same for a WebGPU cart drawing with wgpu.

## Testing

**"It built" and "it ran 30 frames" are not evidence.** A cart that loads,
returns frames, and renders nothing looks identical to a working one from the
outside. Take the screenshot and look at it.

```sh
./scripts/test.sh
```

That builds the examples, checks their exports and that every import is in an
allowed module (`env`, `gl`, `wasi*`), packs them, runs them, and fails if a
frame is one flat colour. The GL example goes through `scripts/glcheck.mjs`,
which drives a real headless `webgl-node` context and reads the result back
with `glReadPixels`, because the terminal player has no context to give a GL
cart. The wgpu examples are checked against the WebGPU tier's import and export
lists (`scripts/wgpuimports.mjs`), then run on the Node host's WebGPU, on a core
device and in compatibility mode, by `scripts/wgpucheck.mjs`: pixels at known
points, motion between frames, and the log lines that prove the async results
came back. Its controls must fail too: the triangle's checks against the
scene's picture, and a host with WebGPU turned off, which must refuse the cart.

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

`WcInfo` is 18 `u32` fields and the order is load-bearing. The v3 tail
(`pointer_ptr` at 56, `keys_ptr` at 60, `gpu_api` at 64) is what hand-written
bindings get wrong. The struct sizes are asserted at compile time against the
host's `src/abi.js`, so a drifted binding is a build error rather than a cart
that renders garbage:

```
WcPad      20 bytes (u32 buttons, six i16 axes incl. triggers)
WcTime     24 bytes (20 of fields, padded to align 8)
WcHostInfo 20 bytes
WcPointer   8 bytes
WcInfo     72 bytes (wheel_ptr at 68, 0 = unused)
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
