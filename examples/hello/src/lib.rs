//! hello: a 2D framebuffer cart.
//!
//! Draws an animated plasma-ish gradient and a movable cursor square, so the
//! screenshot is unmistakably NOT a flat fill. A blank or solid frame is the
//! standard silent failure for a cart, and a flat fill cannot tell you the
//! difference between "rendered" and "the host cleared it for me".
//!
//! Build:
//!   cargo build --release --target wasm32-unknown-unknown
//!   wasmcart pack --wasm target/wasm32-unknown-unknown/release/hello.wasm \
//!     --name hello -o hello.wasc
//!   wasmcart hello.wasc

#![no_std]

use wasmcart::*;

const W: u32 = 320;
const H: u32 = 240;

// Declares the framebuffer, audio ring, pad/time/pointer/key regions, the live
// WcInfo, wc_get_info, and a panic handler that reports through wc_log.
wc_cart!(W, H);

/// Player square position, in pixels.
static mut CURSOR: (i32, i32) = (W as i32 / 2, H as i32 / 2);

#[no_mangle]
pub extern "C" fn wc_init() {
    wc_log("hello: rust cart init");
}

#[no_mangle]
pub extern "C" fn wc_render() {
    let t = time().frame;

    // Gradient background. The frame counter shifts it, so consecutive
    // screenshots differ and a frozen cart is visible rather than plausible.
    let buf = fb();
    for y in 0..H {
        for x in 0..W {
            let r = ((x * 255 / W) as u32) & 0xFF;
            let g = ((y * 255 / H) as u32) & 0xFF;
            let b = (t.wrapping_mul(2) & 0xFF) as u32;
            buf[(y * W + x) as usize] = 0xFF00_0000 | (r << 16) | (g << 8) | b;
        }
    }

    // D-pad moves a white square.
    let pad = pads()[0];
    let (mut cx, mut cy) = unsafe { CURSOR };
    if pad.down(WC_BTN_LEFT) {
        cx -= 2;
    }
    if pad.down(WC_BTN_RIGHT) {
        cx += 2;
    }
    if pad.down(WC_BTN_UP) {
        cy -= 2;
    }
    if pad.down(WC_BTN_DOWN) {
        cy += 2;
    }
    cx = cx.clamp(0, W as i32 - 16);
    cy = cy.clamp(0, H as i32 - 16);
    unsafe { CURSOR = (cx, cy) };

    for y in cy..cy + 16 {
        for x in cx..cx + 16 {
            buf[(y as u32 * W + x as u32) as usize] = 0xFFFF_FFFF;
        }
    }
}
