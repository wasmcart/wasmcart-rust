//! A [`wgpu`] backend for wasmcart carts.
//!
//! wasmcart's WebGPU tier gives a cart the WebGPU functions of Dawn's
//! emdawnwebgpu port as wasm imports (wasmcart SPEC.md, "WebGPU"). This crate
//! implements wgpu's `custom` backend traits over those imports, so a Rust
//! cart, or an engine built on wgpu, renders through the host's WebGPU with
//! the ordinary wgpu API. The cart needs no wasm-bindgen, no web-sys and no
//! JavaScript of its own: its imports are the frozen WebGPU set plus
//! wasmcart's own, and it runs on every host with WebGPU (browser, Node,
//! native).
//!
//! # Getting a device
//!
//! The host made a device before `wc_init`; [`Gpu::new`] wraps it, with the
//! cart's canvas configured, synchronously:
//!
//! ```ignore
//! static mut GPU: Option<wasmcart_wgpu::Gpu> = None;
//!
//! #[no_mangle]
//! pub extern "C" fn wc_init() {
//!     let gpu = wasmcart_wgpu::Gpu::new(640, 480).expect("no WebGPU");
//!     // gpu.device, gpu.queue, gpu.surface, gpu.config: plain wgpu objects
//!     unsafe { GPU = Some(gpu) };
//! }
//!
//! #[no_mangle]
//! pub extern "C" fn wc_render() {
//!     let gpu = unsafe { GPU.as_ref().unwrap() };
//!     let Some(frame) = gpu.frame() else { return };
//!     // ... encode passes into frame.view, submit ...
//!     gpu.present(frame); // the host presents after wc_render; this only ends the frame
//! }
//! ```
//!
//! The usual wgpu route works too, and resolves at once: [`instance()`], then
//! `request_adapter` and `request_device`, polled once with [`now`]. A device
//! request asking for more features or limits than the host's device has is
//! sent to the host and resolves between frames: poll it each frame with
//! [`now`] (or [`GpuRequest`]).
//!
//! # Rules the host imposes
//!
//! - Nothing blocks. Buffer mapping, error scopes, work-done and compilation
//!   info complete between frames, on the host's event loop; `Device::poll`
//!   returns at once. Check results on a later frame.
//! - Do not present: the host presents the canvas after `wc_render` returns.
//!   `Queue::present` (and `Gpu::present`) only release the frame here.
//! - WebGPU is used from the cart's main thread only.
//! - The cart's canvas is the one surface; [`Gpu::new`] sizes it.
//!
//! # Shaders
//!
//! WGSL goes straight to the host's WebGPU implementation, so the cart needs
//! no shader compiler. With the `naga-ir` feature a `ShaderSource::Naga`
//! module is validated and written out as WGSL first (that pulls naga into
//! the cart).

#![no_std]
// webgpu.h constant names are matched as is.
#![allow(non_upper_case_globals)]
extern crate alloc;

pub mod ffi;

mod alloc_exports;
mod backend;
mod conv;
mod emwgpu;

use alloc::string::String;
use core::future::Future;
use core::pin::{pin, Pin};
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

pub use backend::{WcAdapter, WcDevice, WcInstance, WcSurface};
pub use emwgpu::{pending_results, Handle};
pub use ffi::EMDAWNWEBGPU_RELEASE;
pub use wgpu;

#[link(wasm_import_module = "env")]
extern "C" {
    #[link_name = "wc_log"]
    fn wc_log_raw(ptr: *const u8, len: u32);
}

/// Write a line to the host's log (wasmcart's `wc_log`).
pub fn log(msg: &str) {
    unsafe { wc_log_raw(msg.as_ptr(), msg.len() as u32) }
}

pub(crate) fn log_error(error_type: u32, msg: &str) {
    let kind = match error_type {
        ffi::WGPUErrorType_Validation => "validation",
        ffi::WGPUErrorType_OutOfMemory => "out of memory",
        ffi::WGPUErrorType_Internal => "internal",
        _ => "unknown",
    };
    let mut s = String::from("wasmcart-wgpu: uncaptured WebGPU error (");
    s.push_str(kind);
    s.push_str("): ");
    s.push_str(msg);
    log(&s);
}

/// The sRGB transfer function (linear 0..1 to encoded 0..1).
pub fn srgb_encode(x: f64) -> f64 {
    if x <= 0.0031308 { 12.92 * x } else { 1.055 * libm_pow(x, 1.0 / 2.4) - 0.055 }
}

// no_std has no f64::powf; exp/ln series are enough for colours in 0..1.
fn libm_pow(x: f64, y: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    exp(y * ln(x))
}
fn ln(x: f64) -> f64 {
    // ln(x) = 2 atanh((x-1)/(x+1)), after scaling x into [0.5, 2).
    let mut x = x;
    let mut k = 0.0;
    while x > 2.0 { x /= 2.0; k += 1.0; }
    while x < 0.5 { x *= 2.0; k -= 1.0; }
    let t = (x - 1.0) / (x + 1.0);
    let t2 = t * t;
    let mut term = t;
    let mut sum = 0.0;
    let mut n = 1.0;
    for _ in 0..40 { sum += term / n; term *= t2; n += 2.0; }
    2.0 * sum + k * core::f64::consts::LN_2
}
fn exp(x: f64) -> f64 {
    let mut term = 1.0;
    let mut sum = 1.0;
    for i in 1..60 { term *= x / i as f64; sum += term; }
    sum
}

/// Whether `device` is a core WebGPU device (false: compatibility mode,
/// the wasmcart tier's floor; see wasmcart SPEC.md, "Feature level").
pub fn is_core(device: &wgpu::Device) -> bool {
    device.as_custom::<WcDevice>().map(|d| d.is_core()).unwrap_or(false)
}

/// A wgpu instance backed by the host's WebGPU.
pub fn instance() -> wgpu::Instance {
    wgpu::Instance::from_custom(WcInstance::create())
}

/// The window handle of the cart's canvas, for code that creates surfaces
/// through `Instance::create_surface` with something that has a window
/// handle. Every handle maps to the same canvas.
#[derive(Clone, Copy, Debug, Default)]
pub struct CanvasWindow;

impl wgpu::rwh::HasWindowHandle for CanvasWindow {
    fn window_handle(&self) -> Result<wgpu::rwh::WindowHandle<'_>, wgpu::rwh::HandleError> {
        let raw = wgpu::rwh::RawWindowHandle::Web(wgpu::rwh::WebWindowHandle::new(1));
        Ok(unsafe { wgpu::rwh::WindowHandle::borrow_raw(raw) })
    }
}

impl wgpu::rwh::HasDisplayHandle for CanvasWindow {
    fn display_handle(&self) -> Result<wgpu::rwh::DisplayHandle<'_>, wgpu::rwh::HandleError> {
        Ok(wgpu::rwh::DisplayHandle::web())
    }
}

/// The surface for the cart's canvas.
pub fn canvas_surface(instance: &wgpu::Instance) -> wgpu::Surface<'static> {
    let target = wgpu::SurfaceTargetUnsafe::RawHandle {
        raw_display_handle: None,
        raw_window_handle: wgpu::rwh::RawWindowHandle::Web(wgpu::rwh::WebWindowHandle::new(1)),
    };
    // SAFETY: the handle is not dereferenced; the backend always uses "#canvas".
    unsafe { instance.create_surface_unsafe(target) }.expect("the cart canvas")
}

fn noop_waker() -> Waker {
    const VTABLE: RawWakerVTable = RawWakerVTable::new(|_| RAW, |_| {}, |_| {}, |_| {});
    const RAW: RawWaker = RawWaker::new(core::ptr::null(), &VTABLE);
    unsafe { Waker::from_raw(RAW) }
}

/// Poll a future once, without waiting. `Some` if it has finished.
///
/// Requests the host's device satisfies, and anything else already complete,
/// finish on the first poll. A result the host delivers between frames
/// finishes on a poll in a later frame.
pub fn now<F: Future>(fut: F) -> Option<F::Output> {
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    match pin!(fut).poll(&mut cx) {
        Poll::Ready(v) => Some(v),
        Poll::Pending => None,
    }
}

/// Poll a pinned future once (for futures kept across frames).
pub fn poll_once<F: Future + ?Sized>(fut: Pin<&mut F>) -> Poll<F::Output> {
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    fut.poll(&mut cx)
}

/// Everything a cart draws with: device, queue and its configured canvas.
#[derive(Debug)]
pub struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    /// [`Frame::view`] is an sRGB view: the GPU encodes linear colour on
    /// write. False on a compatibility-mode device.
    pub srgb: bool,
}

/// One frame's canvas texture and a view of it.
pub struct Frame {
    pub texture: wgpu::SurfaceTexture,
    pub view: wgpu::TextureView,
}

impl Gpu {
    /// The host's device and the cart's canvas, configured `width` x
    /// `height` in the canvas's preferred format. Synchronous; call it from
    /// `wc_init` (or later). `None` if the host has no WebGPU device. The
    /// device has the host device's limits (`device.limits()`), which on a
    /// compatibility-mode host are below `wgpu::Limits::default()`.
    pub fn new(width: u32, height: u32) -> Option<Gpu> {
        // The device's own limits: a compatibility-mode device (the tier's
        // floor) has lower ones than wgpu's defaults, which are core WebGPU's.
        let instance = instance();
        let adapter = now(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?.ok()?;
        let desc = wgpu::DeviceDescriptor { required_limits: adapter.limits(), ..Default::default() };
        let (device, queue) = now(adapter.request_device(&desc))?.ok()?;
        Some(Self::finish(instance, adapter, device, queue, width, height))
    }

    /// As [`Gpu::new`], asking for `desc`'s features and limits. `None` if
    /// the host's device does not have them; use [`GpuRequest`] to ask the
    /// host for a device that does.
    pub fn with_descriptor(width: u32, height: u32, desc: &wgpu::DeviceDescriptor<'_>) -> Option<Gpu> {
        let instance = instance();
        let adapter = now(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?.ok()?;
        let (device, queue) = now(adapter.request_device(desc))?.ok()?;
        Some(Self::finish(instance, adapter, device, queue, width, height))
    }

    fn finish(
        instance: wgpu::Instance,
        adapter: wgpu::Adapter,
        device: wgpu::Device,
        queue: wgpu::Queue,
        width: u32,
        height: u32,
    ) -> Gpu {
        let surface = canvas_surface(&instance);
        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats[0];
        // An sRGB view of the canvas needs view-format reinterpretation,
        // which compatibility mode (the tier's floor) does not allow.
        let srgb = is_core(&device);
        let view_formats = if srgb { alloc::vec![format.add_srgb_suffix()] } else { alloc::vec![] };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats,
            color_space: Default::default(),
        };
        surface.configure(&device, &config);
        Gpu { instance, adapter, device, queue, surface, config, srgb }
    }

    /// The format of [`Frame::view`]: the canvas format's sRGB form on a
    /// core device; on a compatibility-mode device the canvas format itself,
    /// and a shader writing linear colour must encode sRGB itself (see
    /// [`Gpu::srgb`]). Use it for render pipeline targets.
    pub fn view_format(&self) -> wgpu::TextureFormat {
        if self.srgb {
            self.config.format.add_srgb_suffix()
        } else {
            self.config.format
        }
    }

    /// A linear clear colour as it must be given for [`Frame::view`]:
    /// unchanged on an sRGB view, sRGB-encoded otherwise, so the canvas shows
    /// the same colour on core and compatibility devices.
    pub fn clear_color(&self, linear: wgpu::Color) -> wgpu::Color {
        if self.srgb {
            return linear;
        }
        wgpu::Color { r: srgb_encode(linear.r), g: srgb_encode(linear.g), b: srgb_encode(linear.b), a: linear.a }
    }

    /// Pipeline constants for shaders that take an `override SRGB_ENCODE: bool`:
    /// true when [`Frame::view`] is not an sRGB view and the shader must encode.
    pub fn srgb_constants(&self) -> &'static [(&'static str, f64)] {
        if self.srgb { &[("SRGB_ENCODE", 0.0)] } else { &[("SRGB_ENCODE", 1.0)] }
    }

    /// Resize the canvas.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    /// End the frame. The host presents the canvas after `wc_render`
    /// returns; this only releases the texture (dropping the frame does too).
    pub fn present(&self, frame: Frame) {
        drop(frame.view);
        self.queue.present(frame.texture);
    }

    /// This frame's canvas texture, with an sRGB view of it. `None` if the
    /// host has no texture for this frame.
    pub fn frame(&self) -> Option<Frame> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                let view = texture.texture.create_view(&wgpu::TextureViewDescriptor {
                    format: Some(self.view_format()),
                    ..Default::default()
                });
                Some(Frame { texture, view })
            }
            _ => None,
        }
    }
}

type DeviceFuture = Pin<alloc::boxed::Box<dyn Future<Output = Result<(wgpu::Device, wgpu::Queue), wgpu::RequestDeviceError>>>>;

/// A device request the host answers between frames (more features or
/// limits than its own device has). Poll it each frame.
pub struct GpuRequest {
    instance: Option<wgpu::Instance>,
    adapter: Option<wgpu::Adapter>,
    fut: DeviceFuture,
    width: u32,
    height: u32,
}

impl GpuRequest {
    /// Ask for a device made for `desc`; the canvas is configured at
    /// `width` x `height` once it arrives.
    pub fn new(width: u32, height: u32, desc: &wgpu::DeviceDescriptor<'_>) -> Option<GpuRequest> {
        let instance = instance();
        let adapter = now(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?.ok()?;
        let fut: DeviceFuture = alloc::boxed::Box::pin(adapter.request_device(desc));
        Some(GpuRequest { instance: Some(instance), adapter: Some(adapter), fut, width, height })
    }

    /// `None` while waiting; then the result, once.
    pub fn poll(&mut self) -> Option<Result<Gpu, wgpu::RequestDeviceError>> {
        match poll_once(self.fut.as_mut()) {
            Poll::Pending => None,
            Poll::Ready(Err(e)) => Some(Err(e)),
            Poll::Ready(Ok((device, queue))) => {
                let instance = self.instance.take()?;
                let adapter = self.adapter.take()?;
                Some(Ok(Gpu::finish(instance, adapter, device, queue, self.width, self.height)))
            }
        }
    }
}
