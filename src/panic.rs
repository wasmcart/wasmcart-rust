//! Panic reporting.
//!
//! A cart that panics in a `loop {}` handler just freezes, and from the host's
//! side that is indistinguishable from a slow frame. The host captures `wc_log`
//! into its debug trace, so routing the panic message there first turns a
//! mystery hang into a line of text.
//!
//! The formatting is done into a fixed stack buffer with no allocator, so this
//! works in `#![no_std]` with nothing linked in.

use core::fmt::{self, Write};

/// Bytes of panic message kept. Longer messages are truncated.
const BUF_LEN: usize = 512;

struct FixedWriter {
    buf: [u8; BUF_LEN],
    len: usize,
}

impl FixedWriter {
    const fn new() -> Self {
        Self {
            buf: [0; BUF_LEN],
            len: 0,
        }
    }
    fn as_str(&self) -> &str {
        // Only whole UTF-8 chars are ever pushed (see write_str), so this is
        // valid; fall back to the empty string rather than risk a panic inside
        // the panic handler.
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl Write for FixedWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            let need = c.len_utf8();
            if self.len + need > BUF_LEN {
                return Ok(()); // silently truncate; never fail out of a panic
            }
            c.encode_utf8(&mut self.buf[self.len..]);
            self.len += need;
        }
        Ok(())
    }
}

/// Format a `PanicInfo` into a fixed buffer and hand it to `wc_log`, then trap.
///
/// Called by the panic handler that [`crate::wc_cart!`] installs. Exposed so a
/// cart that writes its own `#[panic_handler]` can still get the message out.
///
/// The trap is deliberate, and it is where this diverges from the obvious
/// `loop {}`. A spin loop inside `wc_render` never returns, so the host has no
/// frame, no error, and no way to tell a panicking cart from a slow one: the
/// player simply stops printing and has to be killed. `unreachable` unwinds
/// out to the host as a RuntimeError it can report, and by then the reason is
/// already in the debug log. Both were tried; the trap is the one that leaves
/// a diagnosable failure instead of a hang.
pub fn report_panic(info: &core::panic::PanicInfo) -> ! {
    let mut w = FixedWriter::new();
    let _ = write!(w, "[rust panic] {info}");
    crate::wc_log(w.as_str());
    // Nothing sensible to resume into: cart state is whatever the panic left
    // behind, so hand control back to the host rather than pretending.
    #[cfg(target_arch = "wasm32")]
    core::arch::wasm32::unreachable();
    #[cfg(not(target_arch = "wasm32"))]
    loop {
        core::hint::spin_loop();
    }
}
