//! The cart side of the emdawnwebgpu port, in Rust.
//!
//! wasmcart's WebGPU tier is the import set of Dawn's emdawnwebgpu port. That
//! port has two halves: JavaScript (the host supplies it) and C++
//! (`webgpu.cpp`, compiled into the cart). This module is the second half,
//! reduced to what a wgpu backend needs:
//!
//! - Every WebGPU object is a small heap block in cart memory. The JavaScript
//!   half keys its table of real GPU objects by the block's address, and asks
//!   the cart to make blocks through the `emwgpuCreate*` exports; the cart
//!   counts references and tells the host when one dies (`emwgpuDelete`).
//! - Asynchronous results (adapter and device requests, buffer mapping, error
//!   scopes, work done, compilation info) arrive through the `emwgpuOn*`
//!   exports, keyed by a future id the cart chose. The host calls them between
//!   frames; every callback here is "spontaneous": it runs when the result
//!   arrives. Nothing ever blocks.
//!
//! The exports are listed in wasmcart's `wgpu-wasi/exports.txt`; their
//! signatures follow `webgpu.cpp` of the pinned release. WebGPU is used from
//! the cart's main thread only (wasmcart SPEC.md, "WebGPU / Threads"), so the
//! state below is single-threaded.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use core::cell::{Cell, RefCell};
use core::ffi::{c_char, c_void};
use core::ptr::{null_mut, NonNull};

use crate::ffi::*;

// ---------------------------------------------------------------------------
// Imports the C++ half declares (library_webgpu.js), beyond webgpu.h's.
// ---------------------------------------------------------------------------
#[link(wasm_import_module = "env")]
extern "C" {
    pub fn emwgpuDelete(ptr: *mut c_void);
    pub fn emwgpuGetPreferredFormat() -> WGPUTextureFormat;
    pub fn emwgpuDeviceCreateBuffer(device: WGPUDevice, desc: *const WGPUBufferDescriptor, buffer: WGPUBuffer) -> i32;
    pub fn emwgpuDeviceCreateShaderModule(device: WGPUDevice, desc: *const WGPUShaderModuleDescriptor, shader: WGPUShaderModule);
    pub fn emwgpuDeviceDestroy(device: WGPUDevice);
    pub fn emwgpuBufferDestroy(buffer: WGPUBuffer);
    pub fn emwgpuBufferGetConstMappedRange(buffer: WGPUBuffer, offset: usize, size: usize) -> *const c_void;
    pub fn emwgpuBufferGetMappedRange(buffer: WGPUBuffer, offset: usize, size: usize) -> *mut c_void;
    pub fn emwgpuBufferUnmap(buffer: WGPUBuffer);
    pub fn emwgpuAdapterRequestDevice(
        adapter: WGPUAdapter,
        future: u64,
        lost_future: u64,
        device: WGPUDevice,
        queue: WGPUQueue,
        desc: *const WGPUDeviceDescriptor,
    );
    pub fn emwgpuBufferMapAsync(buffer: WGPUBuffer, future: u64, mode: WGPUMapMode, offset: usize, size: usize);
    pub fn emwgpuDevicePopErrorScope(device: WGPUDevice, future: u64);
    pub fn emwgpuInstanceRequestAdapter(
        instance: WGPUInstance,
        future: u64,
        options: *const WGPURequestAdapterOptions,
        adapter: WGPUAdapter,
    );
    pub fn emwgpuQueueOnSubmittedWorkDone(queue: WGPUQueue, future: u64);
    pub fn emwgpuShaderModuleGetCompilationInfo(shader: WGPUShaderModule, future: u64, info: *mut WGPUCompilationInfo);
    pub fn emscripten_webgpu_get_device() -> WGPUDevice;
}

// ---------------------------------------------------------------------------
// Main-thread global state.
// ---------------------------------------------------------------------------

/// State touched only from the thread that makes WebGPU calls.
pub(crate) struct MainThread<T>(pub(crate) T);
// SAFETY: wasmcart allows WebGPU calls from the cart's main thread only, and
// the host delivers every callback there; nothing here is reached from a
// worker.
unsafe impl<T> Sync for MainThread<T> {}

/// The payload an `emwgpuOn*` export delivers to the waiting callback.
pub(crate) enum Event<'a> {
    RequestAdapter { ok: bool, adapter: *mut Obj, message: &'a str },
    RequestDevice { ok: bool, device: *mut Obj, message: &'a str },
    DeviceLost { reason: WGPUDeviceLostReason, message: &'a str },
    MapAsync { status: WGPUMapAsyncStatus },
    PopErrorScope { status: WGPUPopErrorScopeStatus, error_type: WGPUErrorType, message: &'a str },
    WorkDone,
    CompilationInfo { info: *mut WGPUCompilationInfo },
}

type Waiter = Box<dyn FnOnce(Event<'_>)>;

static EVENTS: MainThread<RefCell<BTreeMap<u64, Waiter>>> = MainThread(RefCell::new(BTreeMap::new()));
static NEXT_FUTURE: MainThread<Cell<u64>> = MainThread(Cell::new(1));

/// Reserve a future id and the callback its result goes to.
pub(crate) fn track(f: impl FnOnce(Event<'_>) + 'static) -> u64 {
    let id = NEXT_FUTURE.0.get();
    NEXT_FUTURE.0.set(id + 1);
    EVENTS.0.borrow_mut().insert(id, Box::new(f));
    id
}

fn complete(id: f64, ev: Event<'_>) {
    // Take the waiter out before calling it: it may start more work.
    let waiter = EVENTS.0.borrow_mut().remove(&(id as u64));
    if let Some(w) = waiter {
        w(ev);
    }
}

/// Number of results still outstanding (diagnostics).
pub fn pending_results() -> usize {
    EVENTS.0.borrow().len()
}

unsafe fn c_str<'a>(p: *const c_char) -> &'a str {
    if p.is_null() {
        return "";
    }
    let mut n = 0;
    while *p.add(n) != 0 {
        n += 1;
    }
    core::str::from_utf8(core::slice::from_raw_parts(p as *const u8, n)).unwrap_or("(message is not UTF-8)")
}

// ---------------------------------------------------------------------------
// Objects.
// ---------------------------------------------------------------------------

/// What an uncaptured error on a device the cart requested is handed to.
pub(crate) type ErrorSink = Box<dyn Fn(WGPUErrorType, &str)>;

/// The block behind every WebGPU handle. Its address is the handle.
pub(crate) struct Obj {
    refs: Cell<u32>,
    /// A device: its queue, released with it.
    queue: Cell<*mut Obj>,
    /// A device the cart requested (not the host's): destroyed when the cart
    /// drops its last reference, as webgpu.cpp does.
    destroy_on_release: Cell<bool>,
    /// A requested device: where uncaptured errors go.
    on_error: RefCell<Option<ErrorSink>>,
}

pub(crate) fn new_obj() -> *mut Obj {
    Box::into_raw(Box::new(Obj {
        refs: Cell::new(1),
        queue: Cell::new(null_mut()),
        destroy_on_release: Cell::new(false),
        on_error: RefCell::new(None),
    }))
}

pub(crate) unsafe fn add_ref(p: *mut Obj) {
    let o = &*p;
    o.refs.set(o.refs.get() + 1);
}

pub(crate) unsafe fn release(p: *mut Obj) {
    let o = &*p;
    let n = o.refs.get() - 1;
    o.refs.set(n);
    if n != 0 {
        return;
    }
    if o.destroy_on_release.get() {
        emwgpuDeviceDestroy(p as WGPUDevice);
    }
    let queue = o.queue.get();
    drop(Box::from_raw(p));
    // After the free, as webgpu.cpp does: the host drops its object for it.
    emwgpuDelete(p as *mut c_void);
    if !queue.is_null() {
        release(queue);
    }
}

/// Free a block the host never attached an object to (a failed async
/// creation), without telling the host.
unsafe fn discard(p: *mut Obj) {
    if !p.is_null() {
        drop(Box::from_raw(p));
    }
}

pub(crate) unsafe fn set_queue(device: *mut Obj, queue: *mut Obj) {
    (*device).queue.set(queue);
}
pub(crate) unsafe fn queue_of(device: *mut Obj) -> *mut Obj {
    (*device).queue.get()
}
pub(crate) unsafe fn set_destroy_on_release(device: *mut Obj) {
    (*device).destroy_on_release.set(true);
}
pub(crate) unsafe fn set_error_sink(device: *mut Obj, sink: Option<ErrorSink>) {
    *(*device).on_error.borrow_mut() = sink;
}

/// One counted reference to a WebGPU object. Dropping it releases the
/// reference; cloning takes another.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Handle(NonNull<Obj>);

impl Handle {
    /// Take ownership of one reference the caller holds (null: `None`).
    pub(crate) unsafe fn from_owned<T>(p: *mut T) -> Option<Self> {
        NonNull::new(p as *mut Obj).map(Handle)
    }
    /// The handle as the webgpu.h pointer type the imports take.
    #[inline]
    pub fn raw<T>(&self) -> *mut T {
        self.0.as_ptr() as *mut T
    }
    pub(crate) fn obj(&self) -> *mut Obj {
        self.0.as_ptr()
    }
}

impl Clone for Handle {
    fn clone(&self) -> Self {
        unsafe { add_ref(self.0.as_ptr()) };
        Handle(self.0)
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { release(self.0.as_ptr()) }
    }
}

impl core::fmt::Debug for Obj {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Obj(refs={})", self.refs.get())
    }
}

// ---------------------------------------------------------------------------
// Exports the host glue calls (wgpu-wasi/exports.txt).
// ---------------------------------------------------------------------------

macro_rules! create_exports {
    ($($name:ident),* $(,)?) => {$(
        #[no_mangle]
        pub extern "C" fn $name(_source: *const c_void) -> *mut c_void {
            new_obj() as *mut c_void
        }
    )*};
}
create_exports!(
    emwgpuCreateAdapter,
    emwgpuCreateBindGroup,
    emwgpuCreateBindGroupLayout,
    emwgpuCreateCommandBuffer,
    emwgpuCreateCommandEncoder,
    emwgpuCreateComputePassEncoder,
    emwgpuCreateComputePipeline,
    emwgpuCreateExternalTexture,
    emwgpuCreatePipelineLayout,
    emwgpuCreateQuerySet,
    emwgpuCreateQueue,
    emwgpuCreateRenderBundle,
    emwgpuCreateRenderBundleEncoder,
    emwgpuCreateRenderPassEncoder,
    emwgpuCreateRenderPipeline,
    emwgpuCreateSampler,
    emwgpuCreateShaderModule,
    emwgpuCreateSurface,
    emwgpuCreateTexture,
    emwgpuCreateTextureView,
    emwgpuImportBuffer,
);

/// A device the host made (the host's own, from `emscripten_webgpu_get_device`):
/// its queue comes with it and is released with it.
#[no_mangle]
pub extern "C" fn emwgpuCreateDevice(_source: *const c_void, queue: *mut c_void) -> *mut c_void {
    let d = new_obj();
    unsafe { set_queue(d, queue as *mut Obj) };
    d as *mut c_void
}

/// The glue keeps the host device alive with this when it first hands it out.
#[no_mangle]
pub unsafe extern "C" fn wgpuDeviceAddRef(device: *mut c_void) {
    add_ref(device as *mut Obj);
}

#[no_mangle]
pub unsafe extern "C" fn emwgpuOnCompilationInfoCompleted(
    future: f64,
    _status: u32,
    info: *mut WGPUCompilationInfo,
) {
    complete(future, Event::CompilationInfo { info });
}

#[no_mangle]
pub unsafe extern "C" fn emwgpuOnCreateComputePipelineCompleted(
    future: f64,
    status: u32,
    pipeline: *mut c_void,
    _message: *const c_char,
) {
    // Async pipeline creation is not used by the wgpu backend (wgpu has no
    // async pipeline API); a failed one's block is freed as webgpu.cpp does.
    if status != WGPUCreatePipelineAsyncStatus_Success {
        discard(pipeline as *mut Obj);
    }
    let _ = future;
}

#[no_mangle]
pub unsafe extern "C" fn emwgpuOnCreateRenderPipelineCompleted(
    future: f64,
    status: u32,
    pipeline: *mut c_void,
    message: *const c_char,
) {
    emwgpuOnCreateComputePipelineCompleted(future, status, pipeline, message)
}

#[no_mangle]
pub unsafe extern "C" fn emwgpuOnDeviceLostCompleted(future: f64, reason: u32, message: *const c_char) {
    complete(future, Event::DeviceLost { reason, message: c_str(message) });
}

#[no_mangle]
pub unsafe extern "C" fn emwgpuOnMapAsyncCompleted(future: f64, status: u32, message: *const c_char) {
    let _ = message;
    complete(future, Event::MapAsync { status });
}

#[no_mangle]
pub unsafe extern "C" fn emwgpuOnPopErrorScopeCompleted(
    future: f64,
    status: u32,
    error_type: u32,
    message: *const c_char,
) {
    complete(future, Event::PopErrorScope { status, error_type, message: c_str(message) });
}

#[no_mangle]
pub unsafe extern "C" fn emwgpuOnRequestAdapterCompleted(
    future: f64,
    status: u32,
    adapter: *mut c_void,
    message: *const c_char,
) {
    let ok = status == WGPURequestAdapterStatus_Success;
    if !ok {
        discard(adapter as *mut Obj);
    }
    let adapter = if ok { adapter as *mut Obj } else { null_mut() };
    complete(future, Event::RequestAdapter { ok, adapter, message: c_str(message) });
}

#[no_mangle]
pub unsafe extern "C" fn emwgpuOnRequestDeviceCompleted(
    future: f64,
    status: u32,
    device: *mut c_void,
    message: *const c_char,
) {
    let ok = status == WGPURequestDeviceStatus_Success;
    complete(future, Event::RequestDevice { ok, device: device as *mut Obj, message: c_str(message) });
}

#[no_mangle]
pub unsafe extern "C" fn emwgpuOnWorkDoneCompleted(future: f64, _status: u32) {
    complete(future, Event::WorkDone);
}

#[no_mangle]
pub unsafe extern "C" fn emwgpuOnUncapturedError(device: *mut c_void, error_type: u32, message: *const c_char) {
    let msg = c_str(message);
    let o = &*(device as *mut Obj);
    let sink = o.on_error.borrow();
    match &*sink {
        Some(f) => f(error_type, msg),
        None => crate::log_error(error_type, msg),
    }
}

/// Copy a host string (a webgpu.h string view) into an owned String.
pub(crate) unsafe fn string_view(v: &WGPUStringView) -> String {
    if v.data.is_null() {
        return String::new();
    }
    let len = if v.length == WGPU_STRLEN {
        let mut n = 0;
        while *v.data.add(n) != 0 {
            n += 1;
        }
        n
    } else {
        v.length
    };
    String::from_utf8_lossy(core::slice::from_raw_parts(v.data as *const u8, len)).into_owned()
}
