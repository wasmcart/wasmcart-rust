//! The `wc_cart!` macro: buffer declarations + `wc_get_info` + panic handler.
//!
//! This is the Rust analogue of `WC_CART_BUFFERS` / `WC_FILL_INFO` in
//! `wc_cart.h`, except it also emits `wc_get_info` itself. The C header cannot
//! do that because each cart fills the struct slightly differently; here the
//! variations are named options with defaults, carried by [`CartConfig`].
//!
//! Everything the macro declares lives in a private `__wc_cart` module, so a
//! cart author never writes `unsafe` around a mutable static and never holds a
//! `&mut` to one (a hard error on recent editions). Access goes through the
//! generated `fb()` / `pads()` / `time()` / ... accessors, which build slices
//! from raw pointers.

/// Cart configuration, consumed by [`wc_cart!`](crate::wc_cart) at compile time.
///
/// Written as a struct with a `const` default rather than as macro keyword
/// arguments so that the options are documented, type-checked, and usable in
/// `const` context. You normally never name this type: `wc_cart!` builds it
/// from the options you pass.
#[derive(Clone, Copy, Debug)]
pub struct CartConfig {
    /// Initial reported width.
    pub width: u32,
    /// Initial reported height.
    pub height: u32,
    /// Framebuffer capacity width. Defaults to `width`. Larger than `width`
    /// leaves room for a cart that adopts the host's preferred size in
    /// `wc_init`; the host rejects a resize the framebuffer cannot back.
    pub max_width: u32,
    /// Framebuffer capacity height. Defaults to `height`.
    pub max_height: u32,
    /// Audio ring capacity in stereo frames. 0 means the cart emits no audio.
    pub audio_cap: u32,
    /// Requested ring sample rate. 0 lets the host decide (typically 48000).
    pub audio_sample_rate: u32,
    /// Extra `WC_FLAG_*` bits. `WC_FLAG_AUDIO_F32` is always added.
    pub flags: u32,
    /// `WC_GPU_API_*`. Set `WC_GPU_API_WEBGL2` for a GL cart.
    pub gpu_api: u32,
    /// Bytes of save data exposed to the host. 0 means no save support.
    pub save_size: u32,
    /// Emit a `#[panic_handler]` that reports through `wc_log`. Set false if
    /// the cart (or another crate in the binary) provides its own.
    pub panic_handler: bool,
}

impl CartConfig {
    /// Defaults for a `width` x `height` 2D cart.
    pub const fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            max_width: width,
            max_height: height,
            audio_cap: 4096,
            audio_sample_rate: 0,
            flags: 0,
            gpu_api: crate::WC_GPU_API_NONE,
            save_size: 0,
            panic_handler: true,
        }
    }
}

/// Declare a cart: buffers, the live `WcInfo`, `wc_get_info`, and a panic
/// handler that logs the panic message through `wc_log` before trapping.
///
/// # Forms
///
/// ```ignore
/// wc_cart!(320, 240);                                   // 2D framebuffer cart
/// wc_cart!(640, 480, gpu_api: WC_GPU_API_WEBGL2);       // GL cart
/// wc_cart!(320, 240, audio_cap: 2048, flags: WC_FLAG_POINTER);
/// wc_cart!(320, 240, max_width: 1920, max_height: 1080); // room to resize
/// ```
///
/// Options are the fields of [`CartConfig`]; anything you omit takes its
/// default. Audio is `f32` (the crate always sets `WC_FLAG_AUDIO_F32`),
/// matching what `wc_cart.h` does by default.
///
/// # Generated items
///
/// - `wc_get_info()`: the `#[no_mangle]` export, returning the live struct
/// - `fb() -> &'static mut [u32]`: framebuffer, `width * height` long
/// - `fb_all() -> &'static mut [u32]`: the whole `max_width * max_height` buffer
/// - `audio() -> &'static mut [f32]`: interleaved stereo ring, `audio_cap * 2` long
/// - `audio_cursor() -> &'static mut u32`: the cart's write cursor
/// - `pads() -> &'static [WcPad; 4]`
/// - `time() -> &'static WcTime`
/// - `host_info() -> &'static WcHostInfo`
/// - `pointers() -> &'static [WcPointer; 10]`
/// - `keys() -> &'static [u8; 32]`
/// - `save() -> &'static mut [u8]`
/// - `info() -> &'static mut WcInfo`
/// - `set_size(w, h) -> bool`: resize; false (and no change) if the
///   framebuffer cannot back it, which is also what the host would decide
///
/// You still write `wc_init` and `wc_render`:
///
/// ```ignore
/// #[no_mangle] pub extern "C" fn wc_init() {}
/// #[no_mangle] pub extern "C" fn wc_render() {}
/// ```
///
/// Invoke `wc_cart!` exactly once per cart. A second invocation is a duplicate
/// definition error at compile time, not a confusing runtime failure.
#[macro_export]
macro_rules! wc_cart {
    ($w:expr, $h:expr) => { $crate::wc_cart!($w, $h,); };
    ($w:expr, $h:expr, $($opts:tt)*) => {
        // Options are forwarded as raw token trees, NOT as pre-parsed
        // `$key:ident : $val:expr` pairs. Capturing them as `expr` first would
        // turn `false` into an opaque fragment that the muncher below can no
        // longer match against the literal, which is exactly the
        // `panic_handler: false` case it needs to see.
        $crate::__wc_split!([$w, $h] [] [panic_handler] $($opts)*);
    };
}

// Split the option list: everything except `panic_handler` accumulates into the
// struct-literal fields, `panic_handler` selects the emit mode.
#[doc(hidden)]
#[macro_export]
macro_rules! __wc_split {
    ([$w:expr, $h:expr] [$($f:tt)*] [$ph:ident] panic_handler : false, $($rest:tt)*) => {
        $crate::__wc_split!([$w, $h] [$($f)* panic_handler: false,] [no_panic_handler] $($rest)*);
    };
    ([$w:expr, $h:expr] [$($f:tt)*] [$ph:ident] panic_handler : true, $($rest:tt)*) => {
        $crate::__wc_split!([$w, $h] [$($f)* panic_handler: true,] [panic_handler] $($rest)*);
    };
    // Anything else spelled for panic_handler would have to be decided at
    // runtime, which a #[panic_handler] item cannot be. Reject it loudly rather
    // than silently emitting the handler anyway.
    ([$w:expr, $h:expr] [$($f:tt)*] [$ph:ident] panic_handler : $other:expr, $($rest:tt)*) => {
        ::core::compile_error!(
            "wc_cart!: panic_handler must be the literal `true` or `false`"
        );
    };
    ([$w:expr, $h:expr] [$($f:tt)*] [$ph:ident] $key:ident : $val:expr, $($rest:tt)*) => {
        $crate::__wc_split!([$w, $h] [$($f)* $key: $val,] [$ph] $($rest)*);
    };
    // Final option with no trailing comma.
    ([$w:expr, $h:expr] [$($f:tt)*] [$ph:ident] panic_handler : false) => {
        $crate::__wc_split!([$w, $h] [$($f)* panic_handler: false,] [no_panic_handler]);
    };
    ([$w:expr, $h:expr] [$($f:tt)*] [$ph:ident] panic_handler : true) => {
        $crate::__wc_split!([$w, $h] [$($f)* panic_handler: true,] [panic_handler]);
    };
    ([$w:expr, $h:expr] [$($f:tt)*] [$ph:ident] panic_handler : $other:expr) => {
        ::core::compile_error!(
            "wc_cart!: panic_handler must be the literal `true` or `false`"
        );
    };
    ([$w:expr, $h:expr] [$($f:tt)*] [$ph:ident] $key:ident : $val:expr) => {
        $crate::__wc_split!([$w, $h] [$($f)* $key: $val,] [$ph]);
    };
    ([$w:expr, $h:expr] [$($f:tt)*] [$ph:ident]) => {
        $crate::wc_cart_config!(
            $crate::CartConfig { $($f)* ..$crate::CartConfig::new($w, $h) },
            $ph
        );
    };
}

/// Declare a cart from an explicit [`CartConfig`] const expression.
///
/// [`wc_cart!`](crate::wc_cart) is the ergonomic front end for this; reach for
/// this form when the config is computed, e.g. shared between two builds.
#[macro_export]
macro_rules! wc_cart_config {
    ($cfg:expr) => { $crate::wc_cart_config!($cfg, panic_handler); };
    ($cfg:expr, $ph:ident) => {
        const __WC_CFG: $crate::CartConfig = $cfg;

        #[doc(hidden)]
        #[allow(non_snake_case, dead_code)]
        mod __wc_cart {
            use super::__WC_CFG as CFG;
            use $crate::{WcHostInfo, WcInfo, WcPad, WcPointer, WcTime};

            pub const FB_CAP: usize = (CFG.max_width as usize) * (CFG.max_height as usize);
            pub const AUDIO_SAMPLES: usize = (CFG.audio_cap as usize) * 2;
            pub const SAVE_BYTES: usize = CFG.save_size as usize;

            // The framebuffer must be able to back the initially reported size,
            // or the very first frame the host reads runs off the end of it.
            const _: () = assert!(
                CFG.width <= CFG.max_width && CFG.height <= CFG.max_height,
                "wc_cart!: width/height exceed max_width/max_height"
            );

            pub static mut FB: [u32; FB_CAP] = [0; FB_CAP];
            pub static mut AUDIO: [f32; AUDIO_SAMPLES] = [0.0; AUDIO_SAMPLES];
            pub static mut AUDIO_CURSOR: u32 = 0;
            pub static mut PADS: [WcPad; 4] = [WcPad {
                buttons: 0,
                left_x: 0,
                left_y: 0,
                right_x: 0,
                right_y: 0,
                left_trigger: 0,
                right_trigger: 0,
                connected: 0,
                _pad: [0; 3],
            }; 4];
            pub static mut TIME: WcTime = WcTime {
                time_ms: 0.0,
                delta_ms: 0.0,
                frame: 0,
            };
            pub static mut HOST_INFO: WcHostInfo = WcHostInfo {
                preferred_width: 0,
                preferred_height: 0,
                _reserved0: 0,
                audio_sample_rate: 0,
                flags: 0,
            };
            pub static mut POINTERS: [WcPointer; 10] = [WcPointer {
                x: 0,
                y: 0,
                buttons: 0,
                active: 0,
                _pad: [0; 2],
            }; 10];
            pub static mut KEYS: [u8; 32] = [0; 32];
            pub static mut SAVE: [u8; SAVE_BYTES] = [0; SAVE_BYTES];

            // Zero-initialized; wc_get_info fills it. It is filled there rather
            // than as a static initializer because the buffer addresses are not
            // const-evaluable, and because the host re-reads the struct after
            // wc_init, so it has to be a live struct either way.
            pub static mut INFO: WcInfo = WcInfo {
                version: $crate::WC_ABI_VERSION,
                width: CFG.width,
                height: CFG.height,
                fb_ptr: 0,
                audio_ptr: 0,
                audio_cap: CFG.audio_cap,
                audio_write_ptr: 0,
                input_ptr: 0,
                save_ptr: 0,
                save_size: CFG.save_size,
                time_ptr: 0,
                host_info_ptr: 0,
                flags: CFG.flags | $crate::WC_FLAG_AUDIO_F32,
                audio_sample_rate: CFG.audio_sample_rate,
                pointer_ptr: 0,
                keys_ptr: 0,
                gpu_api: CFG.gpu_api,
            };
        }

        /// The cart's live `WcInfo`. Change `width`/`height` through
        /// `set_size` rather than writing them here, so the framebuffer and
        /// the reported size cannot disagree.
        #[allow(dead_code)]
        #[inline]
        pub fn info() -> &'static mut $crate::WcInfo {
            unsafe { &mut *::core::ptr::addr_of_mut!(__wc_cart::INFO) }
        }

        /// The visible framebuffer: `width * height` pixels, `0xAABBGGRR`
        /// byte order in memory (a `u32` of `0xFFRRGGBB` on little-endian).
        #[allow(dead_code)]
        #[inline]
        pub fn fb() -> &'static mut [u32] {
            let n = {
                let i = info();
                (i.width as usize) * (i.height as usize)
            };
            unsafe {
                ::core::slice::from_raw_parts_mut(
                    ::core::ptr::addr_of_mut!(__wc_cart::FB) as *mut u32,
                    n,
                )
            }
        }

        /// The whole framebuffer allocation, `max_width * max_height` pixels.
        #[allow(dead_code)]
        #[inline]
        pub fn fb_all() -> &'static mut [u32] {
            unsafe {
                ::core::slice::from_raw_parts_mut(
                    ::core::ptr::addr_of_mut!(__wc_cart::FB) as *mut u32,
                    __wc_cart::FB_CAP,
                )
            }
        }

        /// Interleaved stereo audio ring, `audio_cap * 2` samples.
        #[allow(dead_code)]
        #[inline]
        pub fn audio() -> &'static mut [f32] {
            unsafe {
                ::core::slice::from_raw_parts_mut(
                    ::core::ptr::addr_of_mut!(__wc_cart::AUDIO) as *mut f32,
                    __wc_cart::AUDIO_SAMPLES,
                )
            }
        }

        /// The cart's audio write cursor, in stereo frames.
        #[allow(dead_code)]
        #[inline]
        pub fn audio_cursor() -> &'static mut u32 {
            unsafe { &mut *::core::ptr::addr_of_mut!(__wc_cart::AUDIO_CURSOR) }
        }

        /// Gamepad state, written by the host before each `wc_render`.
        #[allow(dead_code)]
        #[inline]
        pub fn pads() -> &'static [$crate::WcPad; 4] {
            unsafe { &*::core::ptr::addr_of!(__wc_cart::PADS) }
        }

        /// Frame timing, written by the host before each `wc_render`.
        #[allow(dead_code)]
        #[inline]
        pub fn time() -> &'static $crate::WcTime {
            unsafe { &*::core::ptr::addr_of!(__wc_cart::TIME) }
        }

        /// Host preferences, written once before `wc_init`.
        #[allow(dead_code)]
        #[inline]
        pub fn host_info() -> &'static $crate::WcHostInfo {
            unsafe { &*::core::ptr::addr_of!(__wc_cart::HOST_INFO) }
        }

        /// Pointer (mouse/touch) state. Requires `WC_FLAG_POINTER`.
        #[allow(dead_code)]
        #[inline]
        pub fn pointers() -> &'static [$crate::WcPointer; 10] {
            unsafe { &*::core::ptr::addr_of!(__wc_cart::POINTERS) }
        }

        /// Keyboard bitmask, indexed with `wasmcart::key_is_down`. Requires
        /// `WC_FLAG_KEYBOARD`.
        #[allow(dead_code)]
        #[inline]
        pub fn keys() -> &'static [u8; 32] {
            unsafe { &*::core::ptr::addr_of!(__wc_cart::KEYS) }
        }

        /// Save-data region, `save_size` bytes. Empty unless `save_size` was set.
        #[allow(dead_code)]
        #[inline]
        pub fn save() -> &'static mut [u8] {
            unsafe {
                ::core::slice::from_raw_parts_mut(
                    ::core::ptr::addr_of_mut!(__wc_cart::SAVE) as *mut u8,
                    __wc_cart::SAVE_BYTES,
                )
            }
        }

        /// Change the reported resolution. Returns false and changes nothing
        /// if the framebuffer cannot back `w * h`, which is the same call the
        /// host makes: a size it cannot back is refused rather than reported.
        ///
        /// Typically called from `wc_init` after reading `host_info()`.
        #[allow(dead_code)]
        #[inline]
        pub fn set_size(w: u32, h: u32) -> bool {
            if w == 0 || h == 0 {
                return false;
            }
            if (w as usize) * (h as usize) > __wc_cart::FB_CAP {
                return false;
            }
            let i = info();
            i.width = w;
            i.height = h;
            true
        }

        /// Returned to the host, which re-reads it after `wc_init`.
        #[no_mangle]
        pub extern "C" fn wc_get_info() -> *mut $crate::WcInfo {
            unsafe {
                let i = &mut *::core::ptr::addr_of_mut!(__wc_cart::INFO);
                i.fb_ptr = ::core::ptr::addr_of!(__wc_cart::FB) as u32;
                i.audio_ptr = if __wc_cart::AUDIO_SAMPLES > 0 {
                    ::core::ptr::addr_of!(__wc_cart::AUDIO) as u32
                } else {
                    0
                };
                i.audio_write_ptr = ::core::ptr::addr_of!(__wc_cart::AUDIO_CURSOR) as u32;
                i.input_ptr = ::core::ptr::addr_of!(__wc_cart::PADS) as u32;
                i.save_ptr = if __wc_cart::SAVE_BYTES > 0 {
                    ::core::ptr::addr_of!(__wc_cart::SAVE) as u32
                } else {
                    0
                };
                i.time_ptr = ::core::ptr::addr_of!(__wc_cart::TIME) as u32;
                i.host_info_ptr = ::core::ptr::addr_of!(__wc_cart::HOST_INFO) as u32;
                i.pointer_ptr = ::core::ptr::addr_of!(__wc_cart::POINTERS) as u32;
                i.keys_ptr = ::core::ptr::addr_of!(__wc_cart::KEYS) as u32;
                ::core::ptr::addr_of_mut!(__wc_cart::INFO)
            }
        }

        $crate::__wc_panic_handler!($ph);
    };
}

// A `#[panic_handler]` item cannot be emitted conditionally on a const, so the
// switch is a macro-level token rather than a config field.
#[doc(hidden)]
#[macro_export]
macro_rules! __wc_panic_handler {
    (no_panic_handler) => {};
    (panic_handler) => {
        // `target_arch` is fine to test here, but a `feature` test is NOT: cfgs
        // inside a macro are resolved in the CART's crate, so `feature = "std"`
        // would ask whether the cart has a feature by that name, not whether
        // this crate was built with std. The std case is instead handled by
        // `$crate::__private::WANT_PANIC_HANDLER` being false, which makes the
        // whole item disappear via the wrapper macro below.
        #[cfg(target_arch = "wasm32")]
        $crate::__wc_panic_handler_inner!();
    };
}

// std builds already have std's #[panic_handler]; a second one is a hard error.
// Two definitions of the same macro name, selected by cfg IN THIS CRATE, is the
// only way to make the item conditional on one of this crate's own features.
#[doc(hidden)]
#[cfg(not(feature = "std"))]
#[macro_export]
macro_rules! __wc_panic_handler_inner {
    () => {
        #[panic_handler]
        fn __wc_panic(info: &::core::panic::PanicInfo) -> ! {
            $crate::__private::report_panic(info)
        }
    };
}

#[doc(hidden)]
#[cfg(feature = "std")]
#[macro_export]
macro_rules! __wc_panic_handler_inner {
    () => {};
}
