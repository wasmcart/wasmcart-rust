//! `wc_alloc` / `wc_free`: how the host's WebGPU glue puts adapter info,
//! feature lists, mapped buffer ranges and callback strings into cart memory
//! (wasmcart SPEC.md, "Cart memory the host writes"). Every WebGPU cart needs
//! them.
//!
//! With the default `alloc-exports` feature this crate exports both, over
//! Rust's global allocator (the same scheme as wasmcart's `wasmcart-alloc`
//! crate: blocks carry their size and alignment in front of the pointer,
//! because `dealloc` needs the layout and `wc_free` gets only the pointer).
//! A cart that exports its own pair turns the feature off.

#[cfg(feature = "alloc-exports")]
mod exports {
    use alloc::alloc::{alloc, dealloc, Layout};
    use core::ptr::null_mut;

    /// Allocate `size` bytes aligned to `align` (a power of two) for the host.
    #[no_mangle]
    pub unsafe extern "C" fn wc_alloc(size: usize, align: usize) -> *mut u8 {
        if !align.is_power_of_two() {
            return null_mut();
        }
        let align = align.max(8);
        let Some(total) = size.checked_add(align) else { return null_mut() };
        let Ok(layout) = Layout::from_size_align(total, align) else { return null_mut() };
        let base = alloc(layout);
        if base.is_null() {
            return null_mut();
        }
        let p = base.add(align);
        (p as *mut u32).sub(2).write(total as u32);
        (p as *mut u32).sub(1).write(align as u32);
        p
    }

    /// Release a block from `wc_alloc`. Null is ignored.
    #[no_mangle]
    pub unsafe extern "C" fn wc_free(p: *mut u8) {
        if p.is_null() {
            return;
        }
        let total = (p as *const u32).sub(2).read() as usize;
        let align = (p as *const u32).sub(1).read() as usize;
        dealloc(p.sub(align), Layout::from_size_align_unchecked(total, align));
    }
}

#[cfg(feature = "alloc-exports")]
pub(crate) use exports::wc_free;

#[cfg(not(feature = "alloc-exports"))]
extern "C" {
    // The cart's own export, resolved at link time.
    fn wc_free(p: *mut u8);
}

/// Free a block the host allocated in cart memory and handed over (feature
/// lists, adapter info strings, compilation messages).
pub(crate) unsafe fn host_free(p: *mut u8) {
    wc_free(p)
}
