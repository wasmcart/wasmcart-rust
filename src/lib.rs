//! # wasmcart
//!
//! Rust bindings for the [wasmcart](https://github.com/monteslu/wasmcart) cart ABI (v3).
//!
//! A wasmcart cart is a freestanding `wasm32-unknown-unknown` module that exports
//! `memory`, `wc_get_info`, `wc_render` (and, optionally but conventionally,
//! `wc_init`). There is no runtime to embed and no JS glue: this crate is a
//! binding, not an engine.
//!
//! ## Quick start
//!
//! ```ignore
//! #![no_std]
//! use wasmcart::*;
//!
//! wc_cart!(320, 240);
//!
//! #[no_mangle]
//! pub extern "C" fn wc_init() {}
//!
//! #[no_mangle]
//! pub extern "C" fn wc_render() {
//!     for px in fb() {
//!         *px = 0xFF3050C0;
//!     }
//! }
//! ```
//!
//! `wc_cart!` declares every buffer the ABI needs plus the live `wc_info_t`
//! struct, and emits `wc_get_info`. It also installs a panic handler that
//! reports the panic through `wc_log` before parking, so a panicking cart says
//! why instead of freezing silently.
//!
//! ## no_std
//!
//! This crate is `#![no_std]` and has no dependencies. The optional `std`
//! feature exists only to let a cart that wants `Vec`/`String` link against
//! `std`; it is NOT enabled by default, and enabling it pulls in machinery that
//! makes the cart substantially larger.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::missing_safety_doc)]

pub mod gl;

mod cart;
mod panic;

pub use cart::CartConfig;

// Re-exported so the macros can name them without the cart author importing
// anything beyond `wasmcart::*`.
#[doc(hidden)]
pub mod __private {
    pub use crate::panic::report_panic;
}

/// ABI version implemented by this crate.
pub const WC_ABI_VERSION: u32 = 3;

// ─── Buttons (wc_pad_t.buttons bitmask) ──────────────────────────────────

pub const WC_BTN_A: u16 = 1 << 0;
pub const WC_BTN_B: u16 = 1 << 1;
pub const WC_BTN_X: u16 = 1 << 2;
pub const WC_BTN_Y: u16 = 1 << 3;
pub const WC_BTN_L: u16 = 1 << 4;
pub const WC_BTN_R: u16 = 1 << 5;
pub const WC_BTN_START: u16 = 1 << 6;
pub const WC_BTN_SELECT: u16 = 1 << 7;
pub const WC_BTN_UP: u16 = 1 << 8;
pub const WC_BTN_DOWN: u16 = 1 << 9;
pub const WC_BTN_LEFT: u16 = 1 << 10;
pub const WC_BTN_RIGHT: u16 = 1 << 11;
pub const WC_BTN_L3: u16 = 1 << 12;
pub const WC_BTN_R3: u16 = 1 << 13;

// ─── Cart info flags (WcInfo.flags) ──────────────────────────────────────

/// Audio ring buffer holds `f32` samples rather than `i16`.
pub const WC_FLAG_AUDIO_F32: u32 = 1 << 0;
/// Cart wants WebSocket imports.
pub const WC_FLAG_NET_WS: u32 = 1 << 1;
/// Cart wants data-channel imports.
pub const WC_FLAG_NET_DC: u32 = 1 << 2;
/// Cart wants pointer (mouse/touch) input.
pub const WC_FLAG_POINTER: u32 = 1 << 3;
/// Cart wants raw keyboard input.
pub const WC_FLAG_KEYBOARD: u32 = 1 << 4;
/// Cart exports `wc_debug_state()`. Opt-in, default off.
pub const WC_FLAG_DEBUG: u32 = 1 << 5;
/// Cart honors deterministic mode. Opt-in, default off.
pub const WC_FLAG_DETERMINISTIC: u32 = 1 << 6;

/// Host-info flag: this run is a deterministic replay.
pub const WC_HOST_FLAG_DETERMINISTIC: u32 = 1 << 0;

// ─── gpu_api values (WcInfo.gpu_api) ─────────────────────────────────────

/// 2D framebuffer only.
pub const WC_GPU_API_NONE: u32 = 0;
/// WebGL2 / OpenGL ES 3.0.
pub const WC_GPU_API_WEBGL2: u32 = 1;
/// Reserved.
pub const WC_GPU_API_WEBGPU: u32 = 2;
/// Reserved.
pub const WC_GPU_API_VULKAN: u32 = 3;

// ─── Region sizes ────────────────────────────────────────────────────────

/// Pads exposed through `input_ptr`.
pub const WC_MAX_PADS: usize = 4;
/// Pointers exposed through `pointer_ptr`.
pub const WC_MAX_POINTERS: usize = 10;
/// Bytes in the keyboard bitmask at `keys_ptr` (256 keys).
pub const WC_KEYS_STATE_SIZE: usize = 32;
/// Longest single rumble effect the host will honor, in milliseconds.
pub const WC_RUMBLE_MAX_MS: u32 = 5000;

// ─── Structs ─────────────────────────────────────────────────────────────

/// Gamepad state, 16 bytes. The host writes this before each `wc_render`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WcPad {
    pub buttons: u16,
    pub left_x: i16,
    pub left_y: i16,
    pub right_x: i16,
    pub right_y: i16,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub connected: u8,
    pub _pad: [u8; 3],
}

impl WcPad {
    /// True if any of `mask`'s buttons are held.
    #[inline]
    pub fn down(&self, mask: u16) -> bool {
        self.buttons & mask != 0
    }
    /// True if the host reports this pad as present.
    #[inline]
    pub fn is_connected(&self) -> bool {
        self.connected != 0
    }
}

/// Frame timing, 20 bytes and 8-byte aligned (two `f64` then a `u32`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WcTime {
    pub time_ms: f64,
    pub delta_ms: f64,
    pub frame: u32,
}

/// Written by the host before `wc_init`, read by the cart once at init.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WcHostInfo {
    /// Host's preferred width, 0 = no preference.
    pub preferred_width: u32,
    /// Host's preferred height, 0 = no preference.
    pub preferred_height: u32,
    /// Reserved (was `host_fps`; carts use [`WcTime::delta_ms`] instead).
    pub _reserved0: u32,
    /// Host audio rate, e.g. 48000.
    pub audio_sample_rate: u32,
    /// `WC_HOST_FLAG_*`.
    pub flags: u32,
}

/// Unified mouse/touch pointer, 8 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WcPointer {
    /// Cart-space X.
    pub x: i16,
    /// Cart-space Y.
    pub y: i16,
    /// bit0 = primary, bit1 = secondary, bit2 = middle.
    pub buttons: u8,
    /// 1 if this pointer exists.
    pub active: u8,
    pub _pad: [u8; 2],
}

impl WcPointer {
    #[inline]
    pub fn is_active(&self) -> bool {
        self.active != 0
    }
}

/// The struct `wc_get_info` returns a pointer to. All fields are `u32`;
/// field order is load-bearing and matches the C `wc_info_t` exactly.
///
/// The host re-reads this after `wc_init`, so it must stay live for the
/// lifetime of the cart. Never return a pointer to a copy.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WcInfo {
    /// ABI version. Always [`WC_ABI_VERSION`] for carts built with this crate.
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub fb_ptr: u32,
    pub audio_ptr: u32,
    /// Ring capacity in stereo frames.
    pub audio_cap: u32,
    /// Pointer to the cart's `u32` write cursor.
    pub audio_write_ptr: u32,
    pub input_ptr: u32,
    pub save_ptr: u32,
    pub save_size: u32,
    pub time_ptr: u32,
    pub host_info_ptr: u32,
    /// `WC_FLAG_*`.
    pub flags: u32,
    /// 0 = let the host decide (typically 48000).
    pub audio_sample_rate: u32,
    /// `WcPointer[10]`, 0 = unused.
    pub pointer_ptr: u32,
    /// `u8[32]` key bitmask, 0 = unused.
    pub keys_ptr: u32,
    /// `WC_GPU_API_*`.
    pub gpu_api: u32,
}

// ─── Keyboard ────────────────────────────────────────────────────────────

/// USB HID scancodes, as delivered in the `keys_ptr` bitmask.
pub mod key {
    pub const A: u8 = 0x04;
    pub const B: u8 = 0x05;
    pub const C: u8 = 0x06;
    pub const D: u8 = 0x07;
    pub const E: u8 = 0x08;
    pub const F: u8 = 0x09;
    pub const G: u8 = 0x0A;
    pub const H: u8 = 0x0B;
    pub const I: u8 = 0x0C;
    pub const J: u8 = 0x0D;
    pub const K: u8 = 0x0E;
    pub const L: u8 = 0x0F;
    pub const M: u8 = 0x10;
    pub const N: u8 = 0x11;
    pub const O: u8 = 0x12;
    pub const P: u8 = 0x13;
    pub const Q: u8 = 0x14;
    pub const R: u8 = 0x15;
    pub const S: u8 = 0x16;
    pub const T: u8 = 0x17;
    pub const U: u8 = 0x18;
    pub const V: u8 = 0x19;
    pub const W: u8 = 0x1A;
    pub const X: u8 = 0x1B;
    pub const Y: u8 = 0x1C;
    pub const Z: u8 = 0x1D;
    pub const NUM1: u8 = 0x1E;
    pub const NUM2: u8 = 0x1F;
    pub const NUM3: u8 = 0x20;
    pub const NUM4: u8 = 0x21;
    pub const NUM5: u8 = 0x22;
    pub const NUM6: u8 = 0x23;
    pub const NUM7: u8 = 0x24;
    pub const NUM8: u8 = 0x25;
    pub const NUM9: u8 = 0x26;
    pub const NUM0: u8 = 0x27;
    pub const ENTER: u8 = 0x28;
    pub const ESCAPE: u8 = 0x29;
    pub const BACKSPACE: u8 = 0x2A;
    pub const TAB: u8 = 0x2B;
    pub const SPACE: u8 = 0x2C;
    pub const MINUS: u8 = 0x2D;
    pub const EQUAL: u8 = 0x2E;
    pub const LBRACKET: u8 = 0x2F;
    pub const RBRACKET: u8 = 0x30;
    pub const BACKSLASH: u8 = 0x31;
    pub const SEMICOLON: u8 = 0x33;
    pub const QUOTE: u8 = 0x34;
    pub const GRAVE: u8 = 0x35;
    pub const COMMA: u8 = 0x36;
    pub const PERIOD: u8 = 0x37;
    pub const SLASH: u8 = 0x38;
    pub const CAPSLOCK: u8 = 0x39;
    pub const F1: u8 = 0x3A;
    pub const F2: u8 = 0x3B;
    pub const F3: u8 = 0x3C;
    pub const F4: u8 = 0x3D;
    pub const F5: u8 = 0x3E;
    pub const F6: u8 = 0x3F;
    pub const F7: u8 = 0x40;
    pub const F8: u8 = 0x41;
    pub const F9: u8 = 0x42;
    pub const F10: u8 = 0x43;
    pub const F11: u8 = 0x44;
    pub const F12: u8 = 0x45;
    pub const INSERT: u8 = 0x49;
    pub const HOME: u8 = 0x4A;
    pub const PAGEUP: u8 = 0x4B;
    pub const DELETE: u8 = 0x4C;
    pub const END: u8 = 0x4D;
    pub const PAGEDOWN: u8 = 0x4E;
    pub const RIGHT: u8 = 0x4F;
    pub const LEFT: u8 = 0x50;
    pub const DOWN: u8 = 0x51;
    pub const UP: u8 = 0x52;
    pub const NUMLOCK: u8 = 0x53;
    pub const KP_DIVIDE: u8 = 0x54;
    pub const KP_MULTIPLY: u8 = 0x55;
    pub const KP_MINUS: u8 = 0x56;
    pub const KP_PLUS: u8 = 0x57;
    pub const KP_ENTER: u8 = 0x58;
    pub const KP_1: u8 = 0x59;
    pub const KP_2: u8 = 0x5A;
    pub const KP_3: u8 = 0x5B;
    pub const KP_4: u8 = 0x5C;
    pub const KP_5: u8 = 0x5D;
    pub const KP_6: u8 = 0x5E;
    pub const KP_7: u8 = 0x5F;
    pub const KP_8: u8 = 0x60;
    pub const KP_9: u8 = 0x61;
    pub const KP_0: u8 = 0x62;
    pub const KP_PERIOD: u8 = 0x63;
    pub const LCTRL: u8 = 0xE0;
    pub const LSHIFT: u8 = 0xE1;
    pub const LALT: u8 = 0xE2;
    pub const LMETA: u8 = 0xE3;
    pub const RCTRL: u8 = 0xE4;
    pub const RSHIFT: u8 = 0xE5;
    pub const RALT: u8 = 0xE6;
    pub const RMETA: u8 = 0xE7;
}

/// Keyboard modifier bitmask values.
pub mod modifier {
    pub const SHIFT: u8 = 0x01;
    pub const CTRL: u8 = 0x02;
    pub const ALT: u8 = 0x04;
    pub const META: u8 = 0x08;
}

/// Test a keycode against a 32-byte key-state bitmask.
#[inline]
pub fn key_is_down(keys: &[u8; WC_KEYS_STATE_SIZE], keycode: u8) -> bool {
    keys[(keycode >> 3) as usize] & (1 << (keycode & 7)) != 0
}

// ─── Host imports ────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
mod raw {
    #[link(wasm_import_module = "env")]
    extern "C" {
        pub fn wc_log(ptr: *const u8, len: u32);
        pub fn wc_debug_mark(id: u32);
        pub fn wc_pad_name(pad_id: u32, buf: *mut u8, buf_len: u32) -> i32;
        pub fn wc_pad_has_rumble(pad_id: u32) -> u32;
        pub fn wc_pad_rumble(pad_id: u32, low: f32, high: f32, duration_ms: u32);
        pub fn wc_pad_rumble_stop(pad_id: u32);
        pub fn wc_asset_size(path: *const u8, path_len: u32) -> i32;
        pub fn wc_load_asset(path: *const u8, path_len: u32, dest: *mut u8, max_size: u32) -> i32;
    }
}

/// Write a message to the host's debug log.
///
/// This is the one import a cart built with this crate always has (the panic
/// handler calls it). On non-wasm targets it is a no-op so the crate still
/// builds for `cargo test` / rust-analyzer.
#[inline]
pub fn wc_log(msg: &str) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        raw::wc_log(msg.as_ptr(), msg.len() as u32);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = msg;
}

/// Stamp `{frame, id}` into a debug-capable host's event trace.
///
/// An uncalled import is not emitted into the wasm binary, so merely having
/// this in the crate costs a cart with no call sites nothing.
#[inline]
pub fn wc_debug_mark(id: u32) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        raw::wc_debug_mark(id);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = id;
}

/// Copy the host's name for `pad_id` into `buf`. Returns bytes written.
#[inline]
pub fn wc_pad_name(pad_id: u32, buf: &mut [u8]) -> usize {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let n = raw::wc_pad_name(pad_id, buf.as_mut_ptr(), buf.len() as u32);
        if n > 0 {
            n as usize
        } else {
            0
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (pad_id, buf);
        0
    }
}

/// True if the host reports rumble support for this pad. Capability is
/// per-device, so ask rather than assume.
#[inline]
pub fn wc_pad_has_rumble(pad_id: u32) -> bool {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        raw::wc_pad_has_rumble(pad_id) != 0
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = pad_id;
        false
    }
}

/// Start a rumble effect. `low`/`high` are clamped to 0..1 by the host, and
/// `duration_ms` is capped at [`WC_RUMBLE_MAX_MS`]. Re-arm each frame for
/// sustained rumble.
#[inline]
pub fn wc_pad_rumble(pad_id: u32, low: f32, high: f32, duration_ms: u32) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        raw::wc_pad_rumble(pad_id, low, high, duration_ms);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (pad_id, low, high, duration_ms);
}

/// Stop any rumble effect on this pad.
#[inline]
pub fn wc_pad_rumble_stop(pad_id: u32) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        raw::wc_pad_rumble_stop(pad_id);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = pad_id;
}

/// Size of an asset packed into the `.wasc` archive, or `None` if absent.
/// Bare `.wasm` carts have no archive and always get `None`.
#[inline]
pub fn wc_asset_size(path: &str) -> Option<usize> {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let n = raw::wc_asset_size(path.as_ptr(), path.len() as u32);
        if n >= 0 {
            Some(n as usize)
        } else {
            None
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = path;
        None
    }
}

/// Load an archive asset into `dest`. Returns bytes loaded, or `None` on error.
#[inline]
pub fn wc_load_asset(path: &str, dest: &mut [u8]) -> Option<usize> {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        let n = raw::wc_load_asset(
            path.as_ptr(),
            path.len() as u32,
            dest.as_mut_ptr(),
            dest.len() as u32,
        );
        if n >= 0 {
            Some(n as usize)
        } else {
            None
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (path, dest);
        None
    }
}

// ─── Compile-time ABI layout assertions ──────────────────────────────────
//
// These mirror src/abi.js in the host. If one of them ever fails, the binding
// has drifted from the ABI and every cart built with it renders garbage.

const _: () = {
    use core::mem::{align_of, size_of};
    assert!(size_of::<WcPad>() == 16);
    assert!(size_of::<WcTime>() == 24); // 20 bytes of fields, padded to align 8
    assert!(align_of::<WcTime>() == 8);
    assert!(size_of::<WcHostInfo>() == 20);
    assert!(size_of::<WcPointer>() == 8);
    assert!(size_of::<WcInfo>() == 68); // 17 u32 fields
};
