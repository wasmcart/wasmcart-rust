//! A wasmcart cart, in Rust.
//!
//! Build and run:
//!   ./build.sh
//!   npx wasmcart my-cart.wasc

#![no_std]

use wasmcart::*;

const W: u32 = 320;
const H: u32 = 240;

// Declares the framebuffer, audio ring, pad/time/pointer/key regions, the live
// WcInfo, `wc_get_info`, and a panic handler that reports through `wc_log`.
// Everything below reaches them through the accessors it generates: fb(),
// pads(), time(), audio(), host_info(), pointers(), keys(), save(), info().
wc_cart!(W, H);

/// Cart state. A cart is single-threaded and lives inside one wasm instance,
/// so a plain mutable static is the natural place for it; keep the `unsafe`
/// blocks small and never hand out a `&mut` to one.
static mut PLAYER: (i32, i32) = (W as i32 / 2, H as i32 / 2);

/// Called once, after the host has filled in `host_info()`.
#[no_mangle]
pub extern "C" fn wc_init() {
    wc_log("my-cart: init");

    // Optional: adopt the host's preferred resolution. This only succeeds if
    // the framebuffer can back it, so give `wc_cart!` a `max_width`/
    // `max_height` big enough first.
    // let hi = host_info();
    // set_size(hi.preferred_width, hi.preferred_height);
}

/// Called once per frame. Draw into `fb()`.
#[no_mangle]
pub extern "C" fn wc_render() {
    let pad = pads()[0];

    let (mut px, mut py) = unsafe { PLAYER };
    if pad.down(WC_BTN_LEFT) {
        px -= 2;
    }
    if pad.down(WC_BTN_RIGHT) {
        px += 2;
    }
    if pad.down(WC_BTN_UP) {
        py -= 2;
    }
    if pad.down(WC_BTN_DOWN) {
        py += 2;
    }
    px = px.clamp(0, W as i32 - 8);
    py = py.clamp(0, H as i32 - 8);
    unsafe { PLAYER = (px, py) };

    // Pixels are 0xAARRGGBB as a u32.
    let buf = fb();
    buf.fill(0xFF10_1830);
    for y in py..py + 8 {
        for x in px..px + 8 {
            buf[(y as u32 * W + x as u32) as usize] = 0xFFFF_D040;
        }
    }
}
