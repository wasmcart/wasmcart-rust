//! wgpu's `custom` backend traits, implemented over the webgpu.h imports a
//! wasmcart host provides.
//!
//! Each wgpu object wraps one counted [`Handle`]. Descriptors are converted to
//! webgpu.h structs on the stack (or in short-lived Vecs) and passed to the
//! host, which reads them during the call. The mapping mirrors wgpu's own
//! browser backend (`wgpu/src/backend/webgpu.rs`), with webgpu.h structs in
//! place of web-sys objects.

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use core::ffi::c_void;
use core::future::Future;
use core::ops::Range;
use core::pin::Pin;
use core::ptr::{null, null_mut};
use core::task::{Context, Poll, Waker};

use wgpu::custom::*;
use wgpu::wgt;

use crate::conv;
use crate::emwgpu::{self, Event, Handle};
use crate::ffi::*;

// ---------------------------------------------------------------------------
// Small helpers.
// ---------------------------------------------------------------------------

const NULL_CHAIN: *mut WGPUChainedStruct = null_mut();

fn sv(s: &str) -> WGPUStringView {
    WGPUStringView { data: s.as_ptr() as *const _, length: s.len() }
}

fn osv(s: Option<&str>) -> WGPUStringView {
    match s {
        Some(s) => sv(s),
        None => WGPUStringView { data: null(), length: WGPU_STRLEN },
    }
}

/// Handles made by the host for a call that returns one (refs already 1).
fn owned<T>(p: *mut T, what: &str) -> Handle {
    match unsafe { Handle::from_owned(p) } {
        Some(h) => h,
        None => panic!("wasmcart-wgpu: the host returned no object for {what}"),
    }
}

macro_rules! send_sync {
    ($($t:ty),* $(,)?) => {$(
        // SAFETY: WebGPU objects are used from the cart's main thread only
        // (wasmcart SPEC.md, "WebGPU / Threads"); wgpu asks for Send + Sync
        // only so they can sit in its shared structures.
        unsafe impl Send for $t {}
        unsafe impl Sync for $t {}
    )*};
}

/// A result that a host callback fills in later, and the future that waits
/// for it. Polling it never blocks; it is ready once the host has delivered
/// the result between frames.
pub(crate) struct Slot<T> {
    inner: Rc<RefCell<(Option<T>, Option<Waker>)>>,
}
impl<T> Clone for Slot<T> {
    fn clone(&self) -> Self {
        Slot { inner: self.inner.clone() }
    }
}
impl<T> Slot<T> {
    pub(crate) fn new() -> Self {
        Slot { inner: Rc::new(RefCell::new((None, None))) }
    }
    pub(crate) fn set(&self, v: T) {
        let waker = {
            let mut s = self.inner.borrow_mut();
            s.0 = Some(v);
            s.1.take()
        };
        if let Some(w) = waker {
            w.wake();
        }
    }
}
impl<T> Future for Slot<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        let mut s = self.inner.borrow_mut();
        match s.0.take() {
            Some(v) => Poll::Ready(v),
            None => {
                s.1 = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}
// SAFETY: see send_sync! above; slots are filled by host callbacks on the
// main thread.
unsafe impl<T> Send for Slot<T> {}
unsafe impl<T> Sync for Slot<T> {}

/// A future that is already resolved but carries a !Send payload.
struct Ready<T>(Option<T>);
impl<T> Future for Ready<T> {
    type Output = T;
    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<T> {
        // SAFETY: Ready<T> is Unpin-agnostic; we never move out of a pinned field.
        let this = unsafe { self.as_mut().get_unchecked_mut() };
        Poll::Ready(this.0.take().expect("Ready polled after completion"))
    }
}
unsafe impl<T> Send for Ready<T> {}
unsafe impl<T> Sync for Ready<T> {}

fn ready<T>(v: T) -> Ready<T> {
    Ready(Some(v))
}

// ---------------------------------------------------------------------------
// Instance.
// ---------------------------------------------------------------------------

/// The wgpu instance for a wasmcart cart. Make one with [`crate::instance`].
#[derive(Debug)]
pub struct WcInstance {
    pub(crate) inner: Handle,
}

impl WcInstance {
    pub(crate) fn create() -> Self {
        // The instance lives entirely on the cart side (webgpu.cpp's
        // wgpuCreateInstance makes no JS object): just a block to key by.
        let p = emwgpu::new_obj();
        WcInstance { inner: unsafe { Handle::from_owned(p).unwrap() } }
    }

    /// The host's device, if this host has one to hand out
    /// (`emscripten_webgpu_get_device`, valid from `wc_init` on).
    fn host_device() -> Option<Handle> {
        let d = unsafe { emwgpu::emscripten_webgpu_get_device() };
        unsafe { Handle::from_owned(d) }
    }
}

impl InstanceInterface for WcInstance {
    fn new(_desc: wgpu::InstanceDescriptor) -> Self {
        Self::create()
    }

    unsafe fn create_surface(
        &self,
        _target: wgpu::SurfaceTargetUnsafe,
    ) -> Result<DispatchSurface, wgpu::CreateSurfaceError> {
        // A wasmcart cart has exactly one surface: the host's "#canvas".
        // Whatever window handle the caller has, this is the one it renders to.
        static SELECTOR: &[u8] = b"#canvas\0";
        let mut src = WGPUEmscriptenSurfaceSourceCanvasHTMLSelector {
            chain: WGPUChainedStruct { next: null_mut(), sType: WGPUSType_EmscriptenSurfaceSourceCanvasHTMLSelector },
            selector: WGPUStringView { data: SELECTOR.as_ptr() as *const _, length: SELECTOR.len() - 1 },
        };
        let mut desc = WGPUSurfaceDescriptor { nextInChain: &mut src.chain, label: osv(None) };
        let s = wgpuInstanceCreateSurface(self.inner.raw(), &mut desc);
        let h = match Handle::from_owned(s) {
            Some(h) => h,
            None => panic!("wasmcart-wgpu: the host has no WebGPU canvas (\"#canvas\") for this cart"),
        };
        Ok(DispatchSurface::custom(WcSurface { inner: h, configured: Cell::new(false), emulated: RefCell::new(None) }))
    }

    fn request_adapter(&self, _options: &wgpu::RequestAdapterOptions<'_, '_>) -> Pin<Box<dyn RequestAdapterFuture>> {
        // The host chooses the adapter. Hand back one that answers from the
        // host's device right away, so a cart can set up in wc_init without
        // waiting a frame; asking it for more than that device has goes to
        // the host asynchronously (WcAdapter::request_device).
        let result = match Self::host_device() {
            Some(device) => Ok(DispatchAdapter::custom(WcAdapter::new(self.inner.clone(), device))),
            None => Err(wgt::RequestAdapterError::NotFound {
                active_backends: wgt::Backends::BROWSER_WEBGPU,
                requested_backends: wgt::Backends::BROWSER_WEBGPU,
                supported_backends: wgt::Backends::BROWSER_WEBGPU,
                no_fallback_backends: wgt::Backends::empty(),
                no_adapter_backends: wgt::Backends::BROWSER_WEBGPU,
                incompatible_surface_backends: wgt::Backends::empty(),
            }),
        };
        Box::pin(ready(result))
    }

    fn poll_all_devices(&self, _force_wait: bool) -> bool {
        // Results arrive between frames on the host's event loop.
        true
    }

    fn wgsl_language_features(&self) -> wgpu::WgslLanguageFeatures {
        let mut sf = WGPUSupportedWGSLLanguageFeatures { featureCount: 0, features: null() };
        unsafe { wgpuInstanceGetWGSLLanguageFeatures(self.inner.raw(), &mut sf) };
        let names: &[WGPUWGSLLanguageFeatureName] = if sf.features.is_null() {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(sf.features, sf.featureCount) }
        };
        let mut out = wgpu::WgslLanguageFeatures::empty();
        for &n in names {
            out |= match n {
                WGPUWGSLLanguageFeatureName_ReadonlyAndReadwriteStorageTextures => {
                    wgpu::WgslLanguageFeatures::ReadOnlyAndReadWriteStorageTextures
                }
                WGPUWGSLLanguageFeatureName_Packed4x8IntegerDotProduct => {
                    wgpu::WgslLanguageFeatures::Packed4x8IntegerDotProduct
                }
                WGPUWGSLLanguageFeatureName_UnrestrictedPointerParameters => {
                    wgpu::WgslLanguageFeatures::UnrestrictedPointerParameters
                }
                WGPUWGSLLanguageFeatureName_PointerCompositeAccess => {
                    wgpu::WgslLanguageFeatures::PointerCompositeAccess
                }
                _ => wgpu::WgslLanguageFeatures::empty(),
            };
        }
        if !sf.features.is_null() {
            unsafe { crate::alloc_exports::host_free(sf.features as *mut u8) };
        }
        out
    }

    fn enumerate_adapters(&self, _backends: wgpu::Backends) -> Pin<Box<dyn EnumerateAdapterFuture>> {
        let list = match Self::host_device() {
            Some(device) => vec![DispatchAdapter::custom(WcAdapter::new(self.inner.clone(), device))],
            None => Vec::new(),
        };
        Box::pin(ready(list))
    }
}

// ---------------------------------------------------------------------------
// Adapter.
// ---------------------------------------------------------------------------

/// The host's adapter, seen through its device.
///
/// webgpu.h has no synchronous adapter, and a wasmcart host already made
/// its device (with the default limits of its adapter's feature level). The
/// adapter reports that device's features, limits and adapter info, and
/// `request_device` returns it at once when the request fits; otherwise it
/// asks the host for a device made for the request, which arrives between
/// frames.
#[derive(Debug)]
pub struct WcAdapter {
    instance: Handle,
    device: Handle,
    caps: Caps,
    /// Requests through this adapter also require `core-features-and-limits`.
    want_core: bool,
}

impl WcAdapter {
    fn new(instance: Handle, device: Handle) -> Self {
        let caps = device_caps(&device);
        WcAdapter { instance, device, caps, want_core: false }
    }

    /// An adapter describing `device` (a device the host made on request).
    pub(crate) fn for_device(instance: &WcInstance, device: &WcDevice) -> Self {
        WcAdapter::new(instance.inner.clone(), device.inner.clone())
    }

    /// The same adapter, asking for a core device in every request.
    pub(crate) fn wanting_core(&self) -> Self {
        WcAdapter { instance: self.instance.clone(), device: self.device.clone(), caps: self.caps.clone(), want_core: true }
    }

    /// The host's device is a core device.
    pub(crate) fn is_core(&self) -> bool {
        self.caps.core
    }
}

/// What a device can do, in wgpu's terms.
#[derive(Debug, Clone)]
pub(crate) struct Caps {
    features: wgt::Features,
    /// The device's limits exactly as WebGPU reports them; requests are
    /// checked against these.
    raw_limits: wgt::Limits,
    /// The limits wgpu code sees. On a core device the same as
    /// `raw_limits`. In compatibility mode WebGPU has per-stage storage
    /// limits wgpu cannot express (`maxStorageBuffersInVertexStage` is 0 by
    /// default), and wgpu's single per-stage limit means "in every stage", so
    /// it is the smallest of them, as wgpu's own GLES backend reports it.
    limits: wgt::Limits,
    downlevel: wgt::DownlevelCapabilities,
    core: bool,
}

fn device_caps(device: &Handle) -> Caps {
    let core = unsafe { wgpuDeviceHasFeature(device.raw(), WGPUFeatureName_CoreFeaturesAndLimits) != 0 };
    let mut compat = WGPUCompatibilityModeLimits {
        chain: WGPUChainedStruct { next: null_mut(), sType: WGPUSType_CompatibilityModeLimits },
        maxStorageBuffersInVertexStage: 0,
        maxStorageTexturesInVertexStage: 0,
        maxStorageBuffersInFragmentStage: 0,
        maxStorageTexturesInFragmentStage: 0,
    };
    let mut l: WGPULimits = unsafe { core::mem::zeroed() };
    l.nextInChain = &mut compat.chain;
    unsafe { wgpuDeviceGetLimits(device.raw(), &mut l) };
    l.nextInChain = null_mut();
    let raw_limits = conv::limits_from(&l);
    let mut limits = raw_limits.clone();
    let mut downlevel = wgt::DownlevelCapabilities::default();
    if !core {
        compat_caps(&l, &compat, &mut limits, &mut downlevel.flags);
    }
    Caps { features: device_features(device), raw_limits, limits, downlevel, core }
}

/// A compatibility-mode device described honestly (wasmcart SPEC.md,
/// "Feature level"), so wgpu code that reads limits and downlevel flags
/// takes its fallback paths instead of failing validation.
fn compat_caps(
    l: &WGPULimits,
    c: &WGPUCompatibilityModeLimits,
    limits: &mut wgt::Limits,
    flags: &mut wgt::DownlevelFlags,
) {
    use wgt::DownlevelFlags as F;
    limits.max_storage_buffers_per_shader_stage = l
        .maxStorageBuffersPerShaderStage
        .min(c.maxStorageBuffersInVertexStage)
        .min(c.maxStorageBuffersInFragmentStage);
    limits.max_storage_textures_per_shader_stage = l
        .maxStorageTexturesPerShaderStage
        .min(c.maxStorageTexturesInVertexStage)
        .min(c.maxStorageTexturesInFragmentStage);
    // No cube-array views, one blend state for every colour attachment, no
    // sample_mask / sample_index, no view-format reinterpretation (textures
    // or the canvas), depthBiasClamp must be 0.
    flags.remove(
        F::CUBE_ARRAY_TEXTURES
            | F::INDEPENDENT_BLEND
            | F::MULTISAMPLED_SHADING
            | F::VIEW_FORMATS
            | F::SURFACE_VIEW_FORMATS
            | F::DEPTH_BIAS_CLAMP,
    );
    if c.maxStorageBuffersInVertexStage == 0 && c.maxStorageTexturesInVertexStage == 0 {
        flags.remove(F::VERTEX_STORAGE);
    }
    if c.maxStorageBuffersInFragmentStage == 0 && c.maxStorageTexturesInFragmentStage == 0 {
        flags.remove(F::FRAGMENT_STORAGE | F::FRAGMENT_WRITABLE_STORAGE);
    }
}

fn device_features(device: &Handle) -> wgt::Features {
    let mut sf = WGPUSupportedFeatures { featureCount: 0, features: null() };
    unsafe { wgpuDeviceGetFeatures(device.raw(), &mut sf) };
    if sf.features.is_null() {
        return wgt::Features::empty();
    }
    let names = unsafe { core::slice::from_raw_parts(sf.features, sf.featureCount) };
    let f = conv::features_from(names);
    unsafe { crate::alloc_exports::host_free(sf.features as *mut u8) };
    f
}

fn device_adapter_info(device: &Handle) -> wgt::AdapterInfo {
    let mut info: WGPUAdapterInfo = unsafe { core::mem::zeroed() };
    unsafe { wgpuDeviceGetAdapterInfo(device.raw(), &mut info) };
    let (vendor, architecture, dev, description) = unsafe {
        (
            emwgpu::string_view(&info.vendor),
            emwgpu::string_view(&info.architecture),
            emwgpu::string_view(&info.device),
            emwgpu::string_view(&info.description),
        )
    };
    // All four strings share one block, freed through the first.
    if !info.vendor.data.is_null() {
        unsafe { crate::alloc_exports::host_free(info.vendor.data as *mut u8) };
    }
    let name = if !description.is_empty() {
        description.clone()
    } else if !dev.is_empty() {
        dev.clone()
    } else {
        String::from("WebGPU (wasmcart host)")
    };
    let mut driver = vendor;
    if !architecture.is_empty() {
        if !driver.is_empty() {
            driver.push(' ');
        }
        driver.push_str(&architecture);
    }
    wgt::AdapterInfo {
        name,
        vendor: info.vendorID,
        device: info.deviceID,
        device_type: match info.adapterType {
            WGPUAdapterType_DiscreteGPU => wgt::DeviceType::DiscreteGpu,
            WGPUAdapterType_IntegratedGPU => wgt::DeviceType::IntegratedGpu,
            WGPUAdapterType_CPU => wgt::DeviceType::Cpu,
            _ => wgt::DeviceType::Other,
        },
        device_pci_bus_id: String::new(),
        driver,
        driver_info: description,
        backend: wgt::Backend::BrowserWebGpu,
        subgroup_min_size: info.subgroupMinSize,
        subgroup_max_size: info.subgroupMaxSize,
        transient_saves_memory: None,
        limit_bucket: None,
    }
}

/// `lost` is where set_device_lost_callback puts the cart's callback; for a
/// requested device the device-lost future fires it.
fn make_device_pair(
    device: Handle,
    lost: Rc<RefCell<Option<BoxDeviceLostCallback>>>,
) -> (DispatchDevice, DispatchQueue) {
    let queue = unsafe {
        let q = emwgpu::queue_of(device.obj());
        emwgpu::add_ref(q);
        Handle::from_owned(q).expect("device without a queue")
    };
    let caps = device_caps(&device);
    let dev = WcDevice {
        inner: device,
        caps,
        error_scopes: Cell::new(0),
        lost,
    };
    (DispatchDevice::custom(dev), DispatchQueue::custom(WcQueue { inner: queue }))
}

impl AdapterInterface for WcAdapter {
    fn request_device(&self, desc: &wgpu::DeviceDescriptor<'_>) -> Pin<Box<dyn RequestDeviceFuture>> {
        if (self.caps.core || !self.want_core)
            && self.caps.features.contains(desc.required_features)
            && conv::limits_satisfy(&self.caps.raw_limits, &desc.required_limits)
        {
            return Box::pin(ready(Ok(make_device_pair(self.device.clone(), Rc::new(RefCell::new(None))))));
        }
        Box::pin(request_device_from_host(&self.instance, desc, self.want_core))
    }

    fn is_surface_supported(&self, _surface: &DispatchSurface) -> bool {
        true
    }

    fn features(&self) -> wgpu::Features {
        self.caps.features
    }

    fn limits(&self) -> wgpu::Limits {
        self.caps.limits.clone()
    }

    fn downlevel_capabilities(&self) -> wgpu::DownlevelCapabilities {
        self.caps.downlevel.clone()
    }

    fn get_info(&self) -> wgpu::AdapterInfo {
        device_adapter_info(&self.device)
    }

    fn get_texture_format_features(&self, format: wgpu::TextureFormat) -> wgpu::TextureFormatFeatures {
        format.guaranteed_format_features(self.caps.features)
    }

    fn get_presentation_timestamp(&self) -> wgpu::PresentationTimestamp {
        wgpu::PresentationTimestamp::INVALID_TIMESTAMP
    }

    fn cooperative_matrix_properties(&self) -> Vec<wgt::CooperativeMatrixProperties> {
        Vec::new()
    }
}

/// Ask the host for a real adapter, then a device made for `desc`. Both
/// arrive between frames. `core` also requires `core-features-and-limits`
/// (wgpu has no Feature for it): a core device where the GPU has one, or a
/// failed request.
pub(crate) fn request_device_from_host(
    instance: &Handle,
    desc: &wgpu::DeviceDescriptor<'_>,
    core: bool,
) -> Slot<Result<(DispatchDevice, DispatchQueue), wgpu::RequestDeviceError>> {
    let slot = Slot::new();
    let mut features: Vec<WGPUFeatureName> = conv::FEATURES
        .iter()
        .filter(|(f, _)| desc.required_features.contains(*f))
        .map(|(_, n)| *n)
        .collect();
    if core {
        features.push(WGPUFeatureName_CoreFeaturesAndLimits);
    }
    let limits = conv::limits_to(&desc.required_limits);
    let label: Option<String> = desc.label.map(|s| s.to_string());
    let lost: Rc<RefCell<Option<BoxDeviceLostCallback>>> = Rc::new(RefCell::new(None));

    let adapter_obj = emwgpu::new_obj();
    let s = slot.clone();
    let fid = emwgpu::track(move |ev| {
        let Event::RequestAdapter { ok, adapter, message } = ev else { return };
        if !ok {
            s.set(Err(wgpu::RequestDeviceError::from_message(format_msg("no WebGPU adapter", message))));
            return;
        }
        let adapter = unsafe { Handle::from_owned(adapter).unwrap() };
        let queue = emwgpu::new_obj();
        let device = emwgpu::new_obj();
        unsafe {
            emwgpu::set_queue(device, queue);
            emwgpu::set_destroy_on_release(device);
        }
        let lost_slot = lost.clone();
        let lost_fid = emwgpu::track(move |ev| {
            if let Event::DeviceLost { reason, message } = ev {
                if let Some(cb) = lost_slot.borrow_mut().take() {
                    let r = if reason == WGPUDeviceLostReason_Destroyed {
                        wgpu::DeviceLostReason::Destroyed
                    } else {
                        wgpu::DeviceLostReason::Unknown
                    };
                    cb(r, message.to_string());
                }
            }
        });
        let s2 = s.clone();
        let lost2 = lost.clone();
        let fid = emwgpu::track(move |ev| {
            let Event::RequestDevice { ok, device, message } = ev else { return };
            if !ok {
                unsafe {
                    // The host never attached objects to these blocks.
                    let q = emwgpu::queue_of(device);
                    drop(Box::from_raw(q));
                    drop(Box::from_raw(device));
                }
                s2.set(Err(wgpu::RequestDeviceError::from_message(format_msg("device request failed", message))));
                return;
            }
            let device = unsafe { Handle::from_owned(device).unwrap() };
            let (d, q) = make_device_pair(device, lost2.clone());
            s2.set(Ok((d, q)));
        });
        let label_sv = osv(label.as_deref());
        let desc = WGPUDeviceDescriptor {
            nextInChain: NULL_CHAIN,
            label: label_sv,
            requiredFeatureCount: features.len(),
            requiredFeatures: features.as_ptr(),
            requiredLimits: &limits,
            defaultQueue: WGPUQueueDescriptor { nextInChain: NULL_CHAIN, label: osv(None) },
            deviceLostCallbackInfo: unsafe { core::mem::zeroed() },
            uncapturedErrorCallbackInfo: unsafe { core::mem::zeroed() },
        };
        unsafe {
            emwgpu::emwgpuAdapterRequestDevice(adapter.raw(), fid, lost_fid, device as _, queue as _, &desc);
        }
        drop(adapter);
    });
    let opts = WGPURequestAdapterOptions {
        nextInChain: NULL_CHAIN,
        featureLevel: WGPUFeatureLevel_Core,
        powerPreference: WGPUPowerPreference_Undefined,
        forceFallbackAdapter: 0,
        backendType: WGPUBackendType_Undefined,
        compatibleSurface: null_mut(),
    };
    unsafe { emwgpu::emwgpuInstanceRequestAdapter(instance.raw(), fid, &opts, adapter_obj as _) };
    slot
}

fn format_msg(what: &str, message: &str) -> String {
    if message.is_empty() {
        what.to_string()
    } else {
        let mut s = String::from(what);
        s.push_str(": ");
        s.push_str(message);
        s
    }
}

// ---------------------------------------------------------------------------
// Device.
// ---------------------------------------------------------------------------

/// A device: the host's, or one requested for more features or limits.
pub struct WcDevice {
    pub(crate) inner: Handle,
    caps: Caps,
    error_scopes: Cell<u32>,
    lost: Rc<RefCell<Option<BoxDeviceLostCallback>>>,
}

impl core::fmt::Debug for WcDevice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WcDevice").field("inner", &self.inner).finish()
    }
}

impl WcDevice {
    fn raw(&self) -> WGPUDevice {
        self.inner.raw()
    }

    /// A cube-array binding is a cube on a compatibility-mode device, which
    /// has no cube arrays (see crate::compat).
    fn compat_view_dimension(&self, d: WGPUTextureViewDimension) -> WGPUTextureViewDimension {
        if !self.caps.core && d == WGPUTextureViewDimension_CubeArray {
            WGPUTextureViewDimension_Cube
        } else {
            d
        }
    }
}

fn constants(c: &[(&str, f64)]) -> Vec<WGPUConstantEntry> {
    c.iter()
        .map(|(k, v)| WGPUConstantEntry { nextInChain: NULL_CHAIN, key: sv(k), value: *v })
        .collect()
}

fn shader_module_from_wgsl(device: &WcDevice, label: Option<&str>, code: &str) -> WcShaderModule {
    let translated = if device.caps.core { None } else { crate::compat::translate_wgsl(code) };
    let code = translated.as_deref().unwrap_or(code);
    let mut src = WGPUShaderSourceWGSL {
        chain: WGPUChainedStruct { next: null_mut(), sType: WGPUSType_ShaderSourceWGSL },
        code: sv(code),
    };
    let desc = WGPUShaderModuleDescriptor { nextInChain: &mut src.chain, label: osv(label) };
    let obj = emwgpu::new_obj();
    unsafe { emwgpu::emwgpuDeviceCreateShaderModule(device.raw(), &desc, obj as _) };
    WcShaderModule { inner: unsafe { Handle::from_owned(obj).unwrap() }, front_end_errors: None }
}

#[cfg(feature = "naga-ir")]
fn naga_to_wgsl(module: &naga::Module) -> Result<String, String> {
    use naga::valid::{Capabilities, ValidationFlags, Validator};
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(module)
        .map_err(|e| alloc::format!("{e:?}"))?;
    naga::back::wgsl::write_string(module, &info, naga::back::wgsl::WriterFlags::empty())
        .map_err(|e| alloc::format!("{e:?}"))
}

impl DeviceInterface for WcDevice {
    fn features(&self) -> wgpu::Features {
        self.caps.features
    }

    fn limits(&self) -> wgpu::Limits {
        self.caps.limits.clone()
    }

    fn adapter_info(&self) -> wgpu::AdapterInfo {
        device_adapter_info(&self.inner)
    }

    fn create_shader_module(
        &self,
        desc: wgpu::ShaderModuleDescriptor<'_>,
        _checks: wgpu::ShaderRuntimeChecks,
    ) -> DispatchShaderModule {
        let module = match desc.source {
            wgpu::ShaderSource::Wgsl(ref code) => shader_module_from_wgsl(self, desc.label, code),
            #[cfg(feature = "naga-ir")]
            wgpu::ShaderSource::Naga(ref module) => match naga_to_wgsl(module) {
                Ok(code) => shader_module_from_wgsl(self, desc.label, &code),
                Err(e) => {
                    let mut m = shader_module_from_wgsl(self, desc.label, "");
                    m.front_end_errors = Some(e);
                    m
                }
            },
            _ => panic!(
                "wasmcart-wgpu: this shader source needs translating to WGSL; pass WGSL \
                 (or a naga module, with the crate's `naga-ir` feature)"
            ),
        };
        DispatchShaderModule::custom(module)
    }

    unsafe fn create_shader_module_passthrough(
        &self,
        desc: &wgpu::ShaderModuleDescriptorPassthrough<'_>,
    ) -> DispatchShaderModule {
        match desc.wgsl.as_deref() {
            Some(code) => DispatchShaderModule::custom(shader_module_from_wgsl(self, desc.label, code)),
            None => panic!("wasmcart-wgpu: a passthrough shader needs WGSL source on WebGPU"),
        }
    }

    fn create_bind_group_layout(&self, desc: &wgpu::BindGroupLayoutDescriptor<'_>) -> DispatchBindGroupLayout {
        let entries: Vec<WGPUBindGroupLayoutEntry> = desc
            .entries
            .iter()
            .map(|e| {
                let mut out: WGPUBindGroupLayoutEntry = unsafe { core::mem::zeroed() };
                out.binding = e.binding;
                out.visibility = (e.visibility.bits() & 0x7) as u64;
                out.bindingArraySize = e.count.map(|c| c.get()).unwrap_or(0);
                match e.ty {
                    wgt::BindingType::Buffer { ty, has_dynamic_offset, min_binding_size } => {
                        out.buffer.r#type = match ty {
                            wgt::BufferBindingType::Uniform => WGPUBufferBindingType_Uniform,
                            wgt::BufferBindingType::Storage { read_only: false } => WGPUBufferBindingType_Storage,
                            wgt::BufferBindingType::Storage { read_only: true } => {
                                WGPUBufferBindingType_ReadOnlyStorage
                            }
                        };
                        out.buffer.hasDynamicOffset = conv::bool(has_dynamic_offset);
                        out.buffer.minBindingSize = min_binding_size.map(|s| s.get()).unwrap_or(0);
                    }
                    wgt::BindingType::Sampler(ty) => {
                        out.sampler.r#type = match ty {
                            wgt::SamplerBindingType::Filtering => WGPUSamplerBindingType_Filtering,
                            wgt::SamplerBindingType::NonFiltering => WGPUSamplerBindingType_NonFiltering,
                            wgt::SamplerBindingType::Comparison => WGPUSamplerBindingType_Comparison,
                        };
                    }
                    wgt::BindingType::Texture { sample_type, view_dimension, multisampled } => {
                        out.texture.sampleType = conv::sample_type(sample_type);
                        out.texture.viewDimension = self.compat_view_dimension(conv::texture_view_dimension(view_dimension));
                        out.texture.multisampled = conv::bool(multisampled);
                    }
                    wgt::BindingType::StorageTexture { access, format, view_dimension } => {
                        out.storageTexture.access = match access {
                            wgt::StorageTextureAccess::WriteOnly => WGPUStorageTextureAccess_WriteOnly,
                            wgt::StorageTextureAccess::ReadOnly => WGPUStorageTextureAccess_ReadOnly,
                            wgt::StorageTextureAccess::ReadWrite => WGPUStorageTextureAccess_ReadWrite,
                            wgt::StorageTextureAccess::Atomic => {
                                panic!("wasmcart-wgpu: atomic storage textures are not in WebGPU")
                            }
                        };
                        out.storageTexture.format = conv::texture_format(format);
                        out.storageTexture.viewDimension = conv::texture_view_dimension(view_dimension);
                    }
                    _ => panic!("wasmcart-wgpu: binding type {:?} is not supported on WebGPU", e.ty),
                }
                out
            })
            .collect();
        let d = WGPUBindGroupLayoutDescriptor {
            nextInChain: NULL_CHAIN,
            label: osv(desc.label),
            entryCount: entries.len(),
            entries: entries.as_ptr(),
        };
        let p = unsafe { wgpuDeviceCreateBindGroupLayout(self.raw(), &d) };
        DispatchBindGroupLayout::custom(WcBindGroupLayout { inner: owned(p, "createBindGroupLayout") })
    }

    fn create_bind_group(&self, desc: &wgpu::BindGroupDescriptor<'_>) -> DispatchBindGroup {
        let entries: Vec<WGPUBindGroupEntry> = desc
            .entries
            .iter()
            .map(|e| {
                let mut out = WGPUBindGroupEntry {
                    nextInChain: NULL_CHAIN,
                    binding: e.binding,
                    buffer: null_mut(),
                    offset: 0,
                    size: WGPU_WHOLE_SIZE,
                    sampler: null_mut(),
                    textureView: null_mut(),
                };
                match &e.resource {
                    wgpu::BindingResource::Buffer(b) => {
                        out.buffer = custom_buffer(b.buffer).inner.raw();
                        out.offset = b.offset;
                        out.size = b.size.map(|s| s.get()).unwrap_or(WGPU_WHOLE_SIZE);
                    }
                    wgpu::BindingResource::Sampler(s) => {
                        out.sampler = s.as_custom::<WcSampler>().expect("not a wasmcart sampler").inner.raw();
                    }
                    wgpu::BindingResource::TextureView(v) => {
                        out.textureView = custom_view(v).inner.raw();
                    }
                    _ => panic!("wasmcart-wgpu: binding arrays and acceleration structures are not in WebGPU"),
                }
                out
            })
            .collect();
        let layout = desc.layout.as_custom::<WcBindGroupLayout>().expect("not a wasmcart bind group layout");
        let d = WGPUBindGroupDescriptor {
            nextInChain: NULL_CHAIN,
            label: osv(desc.label),
            layout: layout.inner.raw(),
            entryCount: entries.len(),
            entries: entries.as_ptr(),
        };
        let p = unsafe { wgpuDeviceCreateBindGroup(self.raw(), &d) };
        DispatchBindGroup::custom(WcBindGroup { inner: owned(p, "createBindGroup") })
    }

    fn create_pipeline_layout(&self, desc: &wgpu::PipelineLayoutDescriptor<'_>) -> DispatchPipelineLayout {
        let layouts: Vec<WGPUBindGroupLayout> = desc
            .bind_group_layouts
            .iter()
            .map(|l| match l {
                Some(l) => l.as_custom::<WcBindGroupLayout>().expect("not a wasmcart layout").inner.raw(),
                None => null_mut(),
            })
            .collect();
        let d = WGPUPipelineLayoutDescriptor {
            nextInChain: NULL_CHAIN,
            label: osv(desc.label),
            bindGroupLayoutCount: layouts.len(),
            bindGroupLayouts: layouts.as_ptr(),
            immediateSize: desc.immediate_size,
        };
        let p = unsafe { wgpuDeviceCreatePipelineLayout(self.raw(), &d) };
        DispatchPipelineLayout::custom(WcPipelineLayout { inner: owned(p, "createPipelineLayout") })
    }

    fn create_render_pipeline(&self, desc: &wgpu::RenderPipelineDescriptor<'_>) -> DispatchRenderPipeline {
        let vmod = custom_module(desc.vertex.module);
        let vconsts = constants(desc.vertex.compilation_options.constants);
        let attrs: Vec<Vec<WGPUVertexAttribute>> = desc
            .vertex
            .buffers
            .iter()
            .map(|b| match b {
                Some(b) => b
                    .attributes
                    .iter()
                    .map(|a| WGPUVertexAttribute {
                        nextInChain: NULL_CHAIN,
                        format: conv::vertex_format(a.format),
                        offset: a.offset,
                        shaderLocation: a.shader_location,
                    })
                    .collect(),
                None => Vec::new(),
            })
            .collect();
        let buffers: Vec<WGPUVertexBufferLayout> = desc
            .vertex
            .buffers
            .iter()
            .zip(attrs.iter())
            .map(|(b, a)| match b {
                Some(b) => WGPUVertexBufferLayout {
                    nextInChain: NULL_CHAIN,
                    stepMode: match b.step_mode {
                        wgt::VertexStepMode::Vertex => WGPUVertexStepMode_Vertex,
                        wgt::VertexStepMode::Instance => WGPUVertexStepMode_Instance,
                    },
                    arrayStride: b.array_stride,
                    attributeCount: a.len(),
                    attributes: a.as_ptr(),
                },
                // A hole: no step mode and no attributes.
                None => WGPUVertexBufferLayout {
                    nextInChain: NULL_CHAIN,
                    stepMode: WGPUVertexStepMode_Undefined,
                    arrayStride: 0,
                    attributeCount: 0,
                    attributes: null(),
                },
            })
            .collect();

        let depth_stencil = desc.depth_stencil.as_ref().map(|ds| WGPUDepthStencilState {
            nextInChain: NULL_CHAIN,
            format: conv::texture_format(ds.format),
            // The glue reads this as a plain bool, so Undefined must not reach it.
            depthWriteEnabled: conv::optional_bool(Some(ds.depth_write_enabled.unwrap_or(false))),
            depthCompare: ds.depth_compare.map(conv::compare).unwrap_or(WGPUCompareFunction_Undefined),
            stencilFront: conv::stencil_face(&ds.stencil.front),
            stencilBack: conv::stencil_face(&ds.stencil.back),
            stencilReadMask: ds.stencil.read_mask,
            stencilWriteMask: ds.stencil.write_mask,
            depthBias: ds.bias.constant,
            depthBiasSlopeScale: ds.bias.slope_scale,
            // compatibility mode requires 0 (no DEPTH_BIAS_CLAMP downlevel flag)
            depthBiasClamp: if self.caps.core { ds.bias.clamp } else { 0.0 },
        });

        let blends: Vec<Option<WGPUBlendState>> = desc
            .fragment
            .as_ref()
            .map(|f| {
                f.targets
                    .iter()
                    .map(|t| {
                        t.as_ref().and_then(|t| t.blend).map(|b| WGPUBlendState {
                            color: conv::blend_component(&b.color),
                            alpha: conv::blend_component(&b.alpha),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let targets: Vec<WGPUColorTargetState> = desc
            .fragment
            .as_ref()
            .map(|f| {
                f.targets
                    .iter()
                    .zip(blends.iter())
                    .map(|(t, b)| match t {
                        Some(t) => WGPUColorTargetState {
                            nextInChain: NULL_CHAIN,
                            format: conv::texture_format(t.format),
                            blend: b.as_ref().map(|b| b as *const _).unwrap_or(null()),
                            writeMask: t.write_mask.bits() as u64,
                        },
                        None => WGPUColorTargetState {
                            nextInChain: NULL_CHAIN,
                            format: WGPUTextureFormat_Undefined,
                            blend: null(),
                            writeMask: 0,
                        },
                    })
                    .collect()
            })
            .unwrap_or_default();
        let fconsts = desc
            .fragment
            .as_ref()
            .map(|f| constants(f.compilation_options.constants))
            .unwrap_or_default();
        let fragment = desc.fragment.as_ref().map(|f| WGPUFragmentState {
            nextInChain: NULL_CHAIN,
            module: custom_module(f.module).inner.raw(),
            entryPoint: osv(f.entry_point),
            constantCount: fconsts.len(),
            constants: fconsts.as_ptr(),
            targetCount: targets.len(),
            targets: targets.as_ptr(),
        });

        let p = &desc.primitive;
        if p.polygon_mode != wgt::PolygonMode::Fill {
            panic!("wasmcart-wgpu: polygon mode {:?} is not in WebGPU", p.polygon_mode);
        }
        let d = WGPURenderPipelineDescriptor {
            nextInChain: NULL_CHAIN,
            label: osv(desc.label),
            layout: desc
                .layout
                .map(|l| l.as_custom::<WcPipelineLayout>().expect("not a wasmcart layout").inner.raw())
                .unwrap_or(null_mut()),
            vertex: WGPUVertexState {
                nextInChain: NULL_CHAIN,
                module: vmod.inner.raw(),
                entryPoint: osv(desc.vertex.entry_point),
                constantCount: vconsts.len(),
                constants: vconsts.as_ptr(),
                bufferCount: buffers.len(),
                buffers: buffers.as_ptr(),
            },
            primitive: WGPUPrimitiveState {
                nextInChain: NULL_CHAIN,
                topology: conv::topology(p.topology),
                stripIndexFormat: p.strip_index_format.map(conv::index_format).unwrap_or(WGPUIndexFormat_Undefined),
                frontFace: match p.front_face {
                    wgt::FrontFace::Ccw => WGPUFrontFace_CCW,
                    wgt::FrontFace::Cw => WGPUFrontFace_CW,
                },
                cullMode: match p.cull_mode {
                    None => WGPUCullMode_None,
                    Some(wgt::Face::Front) => WGPUCullMode_Front,
                    Some(wgt::Face::Back) => WGPUCullMode_Back,
                },
                unclippedDepth: conv::bool(p.unclipped_depth),
            },
            depthStencil: depth_stencil.as_ref().map(|d| d as *const _).unwrap_or(null()),
            multisample: WGPUMultisampleState {
                nextInChain: NULL_CHAIN,
                count: desc.multisample.count,
                mask: desc.multisample.mask as u32,
                alphaToCoverageEnabled: conv::bool(desc.multisample.alpha_to_coverage_enabled),
            },
            fragment: fragment.as_ref().map(|f| f as *const _).unwrap_or(null()),
        };
        let p = unsafe { wgpuDeviceCreateRenderPipeline(self.raw(), &d) };
        DispatchRenderPipeline::custom(WcRenderPipeline { inner: owned(p, "createRenderPipeline") })
    }

    fn create_mesh_pipeline(&self, _desc: &wgpu::MeshPipelineDescriptor<'_>) -> DispatchRenderPipeline {
        panic!("wasmcart-wgpu: mesh shaders are not in WebGPU")
    }

    fn create_compute_pipeline(&self, desc: &wgpu::ComputePipelineDescriptor<'_>) -> DispatchComputePipeline {
        let consts = constants(desc.compilation_options.constants);
        let d = WGPUComputePipelineDescriptor {
            nextInChain: NULL_CHAIN,
            label: osv(desc.label),
            layout: desc
                .layout
                .map(|l| l.as_custom::<WcPipelineLayout>().expect("not a wasmcart layout").inner.raw())
                .unwrap_or(null_mut()),
            compute: WGPUComputeState {
                nextInChain: NULL_CHAIN,
                module: custom_module(desc.module).inner.raw(),
                entryPoint: osv(desc.entry_point),
                constantCount: consts.len(),
                constants: consts.as_ptr(),
            },
        };
        let p = unsafe { wgpuDeviceCreateComputePipeline(self.raw(), &d) };
        DispatchComputePipeline::custom(WcComputePipeline { inner: owned(p, "createComputePipeline") })
    }

    unsafe fn create_pipeline_cache(&self, _desc: &wgpu::PipelineCacheDescriptor<'_>) -> DispatchPipelineCache {
        DispatchPipelineCache::custom(WcPipelineCache)
    }

    fn create_buffer(&self, desc: &wgpu::BufferDescriptor<'_>) -> DispatchBuffer {
        let d = WGPUBufferDescriptor {
            nextInChain: NULL_CHAIN,
            label: osv(desc.label),
            usage: desc.usage.bits() as u64 & 0x3FF,
            size: desc.size,
            mappedAtCreation: conv::bool(desc.mapped_at_creation),
        };
        let obj = emwgpu::new_obj();
        let ok = unsafe { emwgpu::emwgpuDeviceCreateBuffer(self.raw(), &d, obj as _) };
        if ok == 0 {
            unsafe { drop(Box::from_raw(obj)) };
            panic!("wasmcart-wgpu: createBuffer of {} bytes failed (out of memory)", desc.size);
        }
        DispatchBuffer::custom(WcBuffer {
            inner: unsafe { Handle::from_owned(obj).unwrap() },
            map: Rc::new(Cell::new(if desc.mapped_at_creation { MapState::Write } else { MapState::Unmapped })),
        })
    }

    fn create_texture(&self, desc: &wgpu::TextureDescriptor<'_>) -> DispatchTexture {
        let pending = PendingTexture {
            device: self.inner.clone(),
            label: desc.label.map(String::from),
            usage: desc.usage.bits() as u64 & 0x1F,
            dimension: conv::texture_dimension(desc.dimension),
            size: conv::extent(desc.size),
            format: conv::texture_format(desc.format),
            mip_level_count: desc.mip_level_count,
            sample_count: desc.sample_count,
            view_formats: desc.view_formats.iter().map(|f| conv::texture_format(*f)).collect(),
        };
        // A compatibility-mode texture is bound with one view dimension,
        // fixed when it is made (textureBindingViewDimension). WebGPU's
        // default for a 2D texture of several layers is 2d-array, so one that
        // will be bound as a cube must say so. wgpu's descriptor does not
        // carry it, so a 6-layer 2D texture is made when its first view is
        // (that view's dimension tells), or, used before any view, as a cube.
        let deferred = !self.caps.core
            && desc.dimension == wgt::TextureDimension::D2
            && desc.size.depth_or_array_layers == 6
            && desc.usage.contains(wgt::TextureUsages::TEXTURE_BINDING);
        let tex = WcTexture { inner: core::cell::OnceCell::new(), pending: RefCell::new(None), layers: desc.size.depth_or_array_layers, core: self.caps.core };
        if deferred {
            *tex.pending.borrow_mut() = Some(pending);
        } else {
            let _ = tex.inner.set(pending.create(None));
        }
        DispatchTexture::custom(tex)
    }

    fn create_external_texture(
        &self,
        _desc: &wgpu::ExternalTextureDescriptor<'_>,
        _planes: &[&wgpu::TextureView],
    ) -> DispatchExternalTexture {
        panic!("wasmcart-wgpu: external textures are not supported")
    }

    fn create_blas(
        &self,
        _desc: &wgpu::CreateBlasDescriptor<'_>,
        _sizes: wgpu::BlasGeometrySizeDescriptors,
    ) -> (Option<u64>, DispatchBlas) {
        panic!("wasmcart-wgpu: ray tracing is not in WebGPU")
    }

    fn create_tlas(&self, _desc: &wgpu::CreateTlasDescriptor<'_>) -> DispatchTlas {
        panic!("wasmcart-wgpu: ray tracing is not in WebGPU")
    }

    fn create_sampler(&self, desc: &wgpu::SamplerDescriptor<'_>) -> DispatchSampler {
        let d = WGPUSamplerDescriptor {
            nextInChain: NULL_CHAIN,
            label: osv(desc.label),
            addressModeU: conv::address_mode(desc.address_mode_u),
            addressModeV: conv::address_mode(desc.address_mode_v),
            addressModeW: conv::address_mode(desc.address_mode_w),
            magFilter: conv::filter(desc.mag_filter),
            minFilter: conv::filter(desc.min_filter),
            mipmapFilter: conv::mipmap_filter(desc.mipmap_filter),
            lodMinClamp: desc.lod_min_clamp,
            lodMaxClamp: desc.lod_max_clamp,
            compare: desc.compare.map(conv::compare).unwrap_or(WGPUCompareFunction_Undefined),
            maxAnisotropy: desc.anisotropy_clamp,
        };
        let p = unsafe { wgpuDeviceCreateSampler(self.raw(), &d) };
        DispatchSampler::custom(WcSampler { inner: owned(p, "createSampler") })
    }

    fn create_query_set(&self, desc: &wgpu::QuerySetDescriptor<'_>) -> DispatchQuerySet {
        let d = WGPUQuerySetDescriptor {
            nextInChain: NULL_CHAIN,
            label: osv(desc.label),
            r#type: match desc.ty {
                wgt::QueryType::Occlusion => WGPUQueryType_Occlusion,
                wgt::QueryType::Timestamp => WGPUQueryType_Timestamp,
                _ => panic!("wasmcart-wgpu: pipeline statistics queries are not in WebGPU"),
            },
            count: desc.count,
        };
        let p = unsafe { wgpuDeviceCreateQuerySet(self.raw(), &d) };
        DispatchQuerySet::custom(WcQuerySet { inner: owned(p, "createQuerySet") })
    }

    fn create_command_encoder(&self, desc: &wgpu::CommandEncoderDescriptor<'_>) -> DispatchCommandEncoder {
        let d = WGPUCommandEncoderDescriptor { nextInChain: NULL_CHAIN, label: osv(desc.label) };
        let p = unsafe { wgpuDeviceCreateCommandEncoder(self.raw(), &d) };
        DispatchCommandEncoder::custom(WcCommandEncoder { inner: owned(p, "createCommandEncoder") })
    }

    fn create_render_bundle_encoder(
        &self,
        desc: &wgpu::RenderBundleEncoderDescriptor<'_>,
    ) -> DispatchRenderBundleEncoder {
        let formats: Vec<WGPUTextureFormat> = desc
            .color_formats
            .iter()
            .map(|f| f.map(conv::texture_format).unwrap_or(WGPUTextureFormat_Undefined))
            .collect();
        let d = WGPURenderBundleEncoderDescriptor {
            nextInChain: NULL_CHAIN,
            label: osv(desc.label),
            colorFormatCount: formats.len(),
            colorFormats: formats.as_ptr(),
            depthStencilFormat: desc
                .depth_stencil
                .map(|d| conv::texture_format(d.format))
                .unwrap_or(WGPUTextureFormat_Undefined),
            sampleCount: desc.sample_count,
            depthReadOnly: conv::bool(desc.depth_stencil.map(|d| d.depth_read_only).unwrap_or(false)),
            stencilReadOnly: conv::bool(desc.depth_stencil.map(|d| d.stencil_read_only).unwrap_or(false)),
        };
        let p = unsafe { wgpuDeviceCreateRenderBundleEncoder(self.raw(), &d) };
        DispatchRenderBundleEncoder::custom(WcRenderBundleEncoder { inner: owned(p, "createRenderBundleEncoder") })
    }

    fn set_device_lost_callback(&self, callback: BoxDeviceLostCallback) {
        // Only a device the cart requested reports loss to it; the host's own
        // device stops the cart instead (wasmcart SPEC.md, "The host owns the GPU").
        *self.lost.borrow_mut() = Some(callback);
    }

    fn on_uncaptured_error(&self, handler: Arc<dyn wgpu::UncapturedErrorHandler>) {
        // Delivered for a device the cart requested. On the host's device the
        // host reports uncaptured errors itself (its log or console).
        let sink: emwgpu::ErrorSink = Box::new(move |ty, msg| handler(make_error(ty, msg)));
        unsafe { emwgpu::set_error_sink(self.inner.obj(), Some(sink)) };
    }

    fn push_error_scope(&self, filter: wgpu::ErrorFilter) -> u32 {
        let index = self.error_scopes.get();
        self.error_scopes.set(index + 1);
        let f = match filter {
            wgpu::ErrorFilter::Validation => WGPUErrorFilter_Validation,
            wgpu::ErrorFilter::OutOfMemory => WGPUErrorFilter_OutOfMemory,
            wgpu::ErrorFilter::Internal => WGPUErrorFilter_Internal,
        };
        unsafe { wgpuDevicePushErrorScope(self.raw(), f) };
        index
    }

    fn pop_error_scope(&self, index: u32) -> Pin<Box<dyn PopErrorScopeFuture>> {
        let count = self.error_scopes.get();
        assert!(count > 0 && index + 1 == count, "error scopes must be popped in reverse order");
        self.error_scopes.set(index);
        let slot: Slot<Option<wgpu::Error>> = Slot::new();
        let s = slot.clone();
        let fid = emwgpu::track(move |ev| {
            let Event::PopErrorScope { status, error_type, message } = ev else { return };
            if status != WGPUPopErrorScopeStatus_Success || error_type == WGPUErrorType_NoError {
                s.set(None);
            } else {
                s.set(Some(make_error(error_type, message)));
            }
        });
        unsafe { emwgpu::emwgpuDevicePopErrorScope(self.raw(), fid) };
        Box::pin(slot)
    }

    unsafe fn start_graphics_debugger_capture(&self) {}
    unsafe fn stop_graphics_debugger_capture(&self) {}

    fn poll(&self, _poll_type: wgt::PollType<u64>) -> Result<wgpu::PollStatus, wgpu::PollError> {
        // Nothing can wait inside a frame: results arrive between frames.
        Ok(wgpu::PollStatus::QueueEmpty)
    }

    fn get_internal_counters(&self) -> wgpu::InternalCounters {
        wgpu::InternalCounters::default()
    }

    fn generate_allocator_report(&self) -> Option<wgpu::AllocatorReport> {
        None
    }

    fn destroy(&self) {
        unsafe { emwgpu::emwgpuDeviceDestroy(self.raw()) };
    }
}

fn make_error(ty: WGPUErrorType, msg: &str) -> wgpu::Error {
    let source: Box<dyn core::error::Error + Send + Sync> = Box::from(String::from(msg));
    match ty {
        WGPUErrorType_OutOfMemory => wgpu::Error::OutOfMemory { source },
        WGPUErrorType_Validation => wgpu::Error::Validation { source, description: msg.to_string() },
        _ => wgpu::Error::Internal { source, description: msg.to_string() },
    }
}

fn custom_buffer(b: &wgpu::Buffer) -> &WcBuffer {
    b.as_custom::<WcBuffer>().expect("not a wasmcart buffer")
}
fn custom_view(v: &wgpu::TextureView) -> &WcTextureView {
    v.as_custom::<WcTextureView>().expect("not a wasmcart texture view")
}
fn custom_module(m: &wgpu::ShaderModule) -> &WcShaderModule {
    m.as_custom::<WcShaderModule>().expect("not a wasmcart shader module")
}
fn custom_texture(t: &wgpu::Texture) -> &WcTexture {
    t.as_custom::<WcTexture>().expect("not a wasmcart texture")
}
fn dbuf(b: &DispatchBuffer) -> WGPUBuffer {
    b.as_custom::<WcBuffer>().expect("not a wasmcart buffer").inner.raw()
}

fn texel_texture(t: &wgpu::TexelCopyTextureInfo<'_>) -> WGPUTexelCopyTextureInfo {
    WGPUTexelCopyTextureInfo {
        texture: custom_texture(t.texture).handle(None).raw(),
        mipLevel: t.mip_level,
        origin: conv::origin(t.origin),
        aspect: conv::aspect(t.aspect),
    }
}

fn texel_layout(l: &wgt::TexelCopyBufferLayout) -> WGPUTexelCopyBufferLayout {
    WGPUTexelCopyBufferLayout {
        offset: l.offset,
        bytesPerRow: l.bytes_per_row.unwrap_or(WGPU_COPY_STRIDE_UNDEFINED),
        rowsPerImage: l.rows_per_image.unwrap_or(WGPU_COPY_STRIDE_UNDEFINED),
    }
}

fn texel_buffer(b: &wgpu::TexelCopyBufferInfo<'_>) -> WGPUTexelCopyBufferInfo {
    WGPUTexelCopyBufferInfo { layout: texel_layout(&b.layout), buffer: custom_buffer(b.buffer).inner.raw() }
}

// ---------------------------------------------------------------------------
// Queue.
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct WcQueue {
    inner: Handle,
}

static SUBMISSIONS: emwgpu::MainThread<Cell<u64>> = emwgpu::MainThread(Cell::new(0));

impl QueueInterface for WcQueue {
    fn write_buffer(&self, buffer: &DispatchBuffer, offset: wgpu::BufferAddress, data: &[u8]) {
        unsafe { wgpuQueueWriteBuffer(self.inner.raw(), dbuf(buffer), offset, data.as_ptr() as *const c_void, data.len()) };
    }

    fn create_staging_buffer(&self, size: wgpu::BufferSize) -> Option<DispatchQueueWriteBuffer> {
        Some(DispatchQueueWriteBuffer::custom(WcQueueWriteBuffer { data: vec![0; size.get() as usize].into_boxed_slice() }))
    }

    fn validate_write_buffer(
        &self,
        _buffer: &DispatchBuffer,
        offset: wgpu::BufferAddress,
        size: wgpu::BufferSize,
    ) -> Option<()> {
        // The host validates the write itself and reports through error
        // scopes; only reject what could not be expressed at all.
        if offset % wgt::COPY_BUFFER_ALIGNMENT != 0 || size.get() % wgt::COPY_BUFFER_ALIGNMENT != 0 {
            return None;
        }
        Some(())
    }

    fn write_staging_buffer(
        &self,
        buffer: &DispatchBuffer,
        offset: wgpu::BufferAddress,
        staging_buffer: &DispatchQueueWriteBuffer,
    ) {
        let s = staging_buffer.as_custom::<WcQueueWriteBuffer>().expect("not a wasmcart staging buffer");
        self.write_buffer(buffer, offset, &s.data);
    }

    fn write_texture(
        &self,
        texture: wgpu::TexelCopyTextureInfo<'_>,
        data: &[u8],
        data_layout: wgpu::TexelCopyBufferLayout,
        size: wgpu::Extent3d,
    ) {
        let dst = texel_texture(&texture);
        let layout = texel_layout(&data_layout);
        let ext = conv::extent(size);
        unsafe {
            wgpuQueueWriteTexture(self.inner.raw(), &dst, data.as_ptr() as *const c_void, data.len(), &layout, &ext)
        };
    }

    fn submit(&self, command_buffers: &mut dyn Iterator<Item = DispatchCommandBuffer>) -> u64 {
        let held: Vec<DispatchCommandBuffer> = command_buffers.collect();
        let raw: Vec<WGPUCommandBuffer> = held
            .iter()
            .map(|c| c.as_custom::<WcCommandBuffer>().expect("not a wasmcart command buffer").inner.raw())
            .collect();
        unsafe { wgpuQueueSubmit(self.inner.raw(), raw.len(), raw.as_ptr()) };
        let n = SUBMISSIONS.0.get() + 1;
        SUBMISSIONS.0.set(n);
        n
    }

    fn get_timestamp_period(&self) -> f32 {
        1.0
    }

    fn on_submitted_work_done(&self, callback: BoxSubmittedWorkDoneCallback) {
        let fid = emwgpu::track(move |_| callback());
        unsafe { emwgpu::emwgpuQueueOnSubmittedWorkDone(self.inner.raw(), fid) };
    }

    fn compact_blas(&self, _blas: &DispatchBlas) -> (Option<u64>, DispatchBlas) {
        panic!("wasmcart-wgpu: ray tracing is not in WebGPU")
    }

    fn present(&self, detail: &DispatchSurfaceOutputDetail) {
        // The host presents "#canvas" after wc_render returns; a cart must
        // not call wgpuSurfacePresent (wasmcart SPEC.md, "Presenting"). An
        // emulated sRGB canvas is copied to the real one here.
        if let Some(d) = detail.as_custom::<WcSurfaceOutputDetail>() {
            d.finish();
        }
    }
}

// ---------------------------------------------------------------------------
// Simple objects.
// ---------------------------------------------------------------------------

macro_rules! simple {
    ($($name:ident: $iface:ident),* $(,)?) => {$(
        #[derive(Debug)]
        pub struct $name { pub(crate) inner: Handle }
        impl $iface for $name {}
    )*};
}
simple!(
    WcBindGroupLayout: BindGroupLayoutInterface,
    WcBindGroup: BindGroupInterface,
    WcTextureView: TextureViewInterface,
    WcSampler: SamplerInterface,
    WcPipelineLayout: PipelineLayoutInterface,
    WcCommandBuffer: CommandBufferInterface,
    WcRenderBundle: RenderBundleInterface,
);

#[derive(Debug)]
pub struct WcPipelineCache;
impl PipelineCacheInterface for WcPipelineCache {
    fn get_data(&self) -> Option<Vec<u8>> {
        None
    }
}

#[derive(Debug)]
pub struct WcRenderPipeline {
    inner: Handle,
}
impl RenderPipelineInterface for WcRenderPipeline {
    fn get_bind_group_layout(&self, index: u32) -> DispatchBindGroupLayout {
        let p = unsafe { wgpuRenderPipelineGetBindGroupLayout(self.inner.raw(), index) };
        DispatchBindGroupLayout::custom(WcBindGroupLayout { inner: owned(p, "getBindGroupLayout") })
    }
}

#[derive(Debug)]
pub struct WcComputePipeline {
    inner: Handle,
}
impl ComputePipelineInterface for WcComputePipeline {
    fn get_bind_group_layout(&self, index: u32) -> DispatchBindGroupLayout {
        let p = unsafe { wgpuComputePipelineGetBindGroupLayout(self.inner.raw(), index) };
        DispatchBindGroupLayout::custom(WcBindGroupLayout { inner: owned(p, "getBindGroupLayout") })
    }
}

#[derive(Debug)]
pub struct WcQuerySet {
    inner: Handle,
}
impl QuerySetInterface for WcQuerySet {
    fn destroy(&self) {
        unsafe { wgpuQuerySetDestroy(self.inner.raw()) };
    }
}

/// A shader module. WGSL goes to the host as is; a naga module is written
/// out as WGSL first (feature `naga-ir`).
#[derive(Debug)]
pub struct WcShaderModule {
    inner: Handle,
    /// Translation failed before the host saw the shader.
    front_end_errors: Option<String>,
}
impl ShaderModuleInterface for WcShaderModule {
    fn get_compilation_info(&self) -> Pin<Box<dyn ShaderCompilationInfoFuture>> {
        if let Some(e) = &self.front_end_errors {
            return Box::pin(ready(wgpu::CompilationInfo {
                messages: vec![wgpu::CompilationMessage {
                    message: e.clone(),
                    message_type: wgpu::CompilationMessageType::Error,
                    location: None,
                }],
            }));
        }
        let slot: Slot<wgpu::CompilationInfo> = Slot::new();
        let s = slot.clone();
        // The host fills this struct; its arrays come from wc_alloc and are
        // freed here once read.
        let info: *mut WGPUCompilationInfo =
            Box::into_raw(Box::new(WGPUCompilationInfo { nextInChain: NULL_CHAIN, messageCount: 0, messages: null() }));
        let fid = emwgpu::track(move |ev| {
            let Event::CompilationInfo { info } = ev else { return };
            let mut messages = Vec::new();
            unsafe {
                let i = &*info;
                for k in 0..i.messageCount {
                    let m = &*i.messages.add(k);
                    messages.push(wgpu::CompilationMessage {
                        message: emwgpu::string_view(&m.message),
                        message_type: match m.r#type {
                            WGPUCompilationMessageType_Error => wgpu::CompilationMessageType::Error,
                            WGPUCompilationMessageType_Warning => wgpu::CompilationMessageType::Warning,
                            _ => wgpu::CompilationMessageType::Info,
                        },
                        location: if m.lineNum == 0 {
                            None
                        } else {
                            Some(wgpu::SourceLocation {
                                line_number: m.lineNum as u32,
                                line_position: m.linePos as u32,
                                offset: m.offset as u32,
                                length: m.length as u32,
                            })
                        },
                    });
                }
                if i.messageCount > 0 {
                    let first = &*i.messages;
                    crate::alloc_exports::host_free(first.message.data as *mut u8);
                    crate::alloc_exports::host_free(first.nextInChain as *mut u8);
                    crate::alloc_exports::host_free(i.messages as *mut u8);
                }
                drop(Box::from_raw(info));
            }
            s.set(wgpu::CompilationInfo { messages });
        });
        unsafe { emwgpu::emwgpuShaderModuleGetCompilationInfo(self.inner.raw(), fid, info) };
        Box::pin(slot)
    }
}

// ---------------------------------------------------------------------------
// Buffers.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MapState {
    Unmapped,
    Pending,
    Read,
    Write,
}

#[derive(Debug)]
pub struct WcBuffer {
    pub(crate) inner: Handle,
    map: Rc<Cell<MapState>>,
}

impl BufferInterface for WcBuffer {
    fn map_async(&self, mode: wgpu::MapMode, range: Range<wgpu::BufferAddress>, callback: BufferMapCallback) {
        if self.map.get() != MapState::Unmapped {
            callback(Err(wgpu::BufferAsyncError));
            return;
        }
        self.map.set(MapState::Pending);
        let state = self.map.clone();
        let want = match mode {
            wgpu::MapMode::Read => MapState::Read,
            wgpu::MapMode::Write => MapState::Write,
        };
        let fid = emwgpu::track(move |ev| {
            let Event::MapAsync { status, .. } = ev else { return };
            if status == WGPUMapAsyncStatus_Success {
                state.set(want);
                callback(Ok(()));
            } else {
                if state.get() == MapState::Pending {
                    state.set(MapState::Unmapped);
                }
                callback(Err(wgpu::BufferAsyncError));
            }
        });
        let m = match mode {
            wgpu::MapMode::Read => WGPUMapMode_Read,
            wgpu::MapMode::Write => WGPUMapMode_Write,
        };
        unsafe {
            emwgpu::emwgpuBufferMapAsync(
                self.inner.raw(),
                fid,
                m,
                range.start as usize,
                (range.end - range.start) as usize,
            )
        };
    }

    fn get_mapped_range(&self, sub_range: Range<wgpu::BufferAddress>) -> Result<DispatchBufferMappedRange, wgpu::MapRangeError> {
        let offset = sub_range.start as usize;
        let size = (sub_range.end - sub_range.start) as usize;
        // The host copies the GPU mapping into cart memory (wc_alloc); a
        // write mapping is copied back at unmap, and the host frees the copy.
        let ptr = match self.map.get() {
            MapState::Write => unsafe { emwgpu::emwgpuBufferGetMappedRange(self.inner.raw(), offset, size) as *mut u8 },
            MapState::Read => unsafe {
                emwgpu::emwgpuBufferGetConstMappedRange(self.inner.raw(), offset, size) as *mut u8
            },
            _ => null_mut(),
        };
        if ptr.is_null() {
            panic!("wasmcart-wgpu: getMappedRange({offset}, {size}) failed: the buffer is not mapped there");
        }
        Ok(DispatchBufferMappedRange::custom(WcBufferMappedRange { ptr, len: size }))
    }

    fn unmap(&self) {
        unsafe { emwgpu::emwgpuBufferUnmap(self.inner.raw()) };
        self.map.set(MapState::Unmapped);
    }

    fn destroy(&self) {
        unsafe { emwgpu::emwgpuBufferDestroy(self.inner.raw()) };
        self.map.set(MapState::Unmapped);
    }
}

/// A mapped range: the host's copy of it in cart memory, valid until unmap.
#[derive(Debug)]
pub struct WcBufferMappedRange {
    ptr: *mut u8,
    len: usize,
}
impl BufferMappedRangeInterface for WcBufferMappedRange {
    fn len(&self) -> usize {
        self.len
    }
    unsafe fn read_slice(&self) -> &[u8] {
        core::slice::from_raw_parts(self.ptr, self.len)
    }
    unsafe fn write_slice(&mut self) -> wgpu::WriteOnly<'_, [u8]> {
        wgpu::WriteOnly::from_mut(core::slice::from_raw_parts_mut(self.ptr, self.len))
    }
}

#[derive(Debug)]
pub struct WcQueueWriteBuffer {
    data: Box<[u8]>,
}
impl QueueWriteBufferInterface for WcQueueWriteBuffer {
    fn len(&self) -> usize {
        self.data.len()
    }
    unsafe fn write_slice(&mut self) -> wgpu::WriteOnly<'_, [u8]> {
        wgpu::WriteOnly::from_mut(&mut self.data[..])
    }
}

// ---------------------------------------------------------------------------
// Textures.
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct WcTexture {
    inner: core::cell::OnceCell<Handle>,
    /// A texture whose creation waits for its first view (see create_texture).
    pending: RefCell<Option<PendingTexture>>,
    layers: u32,
    core: bool,
}

/// A texture descriptor kept until the texture is made.
struct PendingTexture {
    device: Handle,
    label: Option<String>,
    usage: u64,
    dimension: WGPUTextureDimension,
    size: WGPUExtent3D,
    format: WGPUTextureFormat,
    mip_level_count: u32,
    sample_count: u32,
    view_formats: Vec<WGPUTextureFormat>,
}

impl core::fmt::Debug for PendingTexture {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PendingTexture").field("label", &self.label).finish()
    }
}

impl PendingTexture {
    fn create(&self, binding: Option<WGPUTextureViewDimension>) -> Handle {
        let mut chain = WGPUTextureBindingViewDimension {
            chain: WGPUChainedStruct { next: null_mut(), sType: WGPUSType_TextureBindingViewDimension },
            textureBindingViewDimension: binding.unwrap_or(WGPUTextureViewDimension_Undefined),
        };
        let d = WGPUTextureDescriptor {
            nextInChain: if binding.is_some() { &mut chain.chain } else { NULL_CHAIN },
            label: osv(self.label.as_deref()),
            usage: self.usage,
            dimension: self.dimension,
            size: self.size,
            format: self.format,
            mipLevelCount: self.mip_level_count,
            sampleCount: self.sample_count,
            viewFormatCount: self.view_formats.len(),
            viewFormats: self.view_formats.as_ptr(),
        };
        owned(unsafe { wgpuDeviceCreateTexture(self.device.raw(), &d) }, "createTexture")
    }
}

impl WcTexture {
    fn from_handle(inner: Handle) -> Self {
        let cell = core::cell::OnceCell::new();
        let _ = cell.set(inner);
        WcTexture { inner: cell, pending: RefCell::new(None), layers: 1, core: true }
    }

    /// The texture, made now if it was waiting; `view` is the dimension of
    /// the view that needs it, if a view does.
    fn handle(&self, view: Option<WGPUTextureViewDimension>) -> &Handle {
        self.inner.get_or_init(|| {
            let p = self.pending.borrow_mut().take().expect("texture neither made nor pending");
            let binding = match view {
                Some(WGPUTextureViewDimension_2DArray) => WGPUTextureViewDimension_2DArray,
                Some(WGPUTextureViewDimension_2D) => WGPUTextureViewDimension_2DArray,
                _ => WGPUTextureViewDimension_Cube,
            };
            p.create(Some(binding))
        })
    }
}

impl TextureInterface for WcTexture {
    fn create_view(&self, desc: &wgpu::TextureViewDescriptor<'_>) -> DispatchTextureView {
        let mut dimension = desc.dimension.map(conv::texture_view_dimension).unwrap_or(WGPUTextureViewDimension_Undefined);
        // Compatibility mode: a cube array of one cube is a cube (crate::compat).
        if !self.core
            && dimension == WGPUTextureViewDimension_CubeArray
            && desc.base_array_layer == 0
            && desc.array_layer_count.unwrap_or(self.layers) == 6
        {
            dimension = WGPUTextureViewDimension_Cube;
        }
        // A single layer of a multi-layer texture (a render target) does not
        // say how the texture is bound.
        let hint = match dimension {
            WGPUTextureViewDimension_2D if desc.array_layer_count != Some(self.layers) => None,
            WGPUTextureViewDimension_Undefined => None,
            d => Some(d),
        };
        let raw = self.handle(hint).raw();
        let d = WGPUTextureViewDescriptor {
            nextInChain: NULL_CHAIN,
            label: osv(desc.label),
            format: desc.format.map(conv::texture_format).unwrap_or(WGPUTextureFormat_Undefined),
            dimension,
            baseMipLevel: desc.base_mip_level,
            mipLevelCount: desc.mip_level_count.unwrap_or(WGPU_MIP_LEVEL_COUNT_UNDEFINED),
            baseArrayLayer: desc.base_array_layer,
            arrayLayerCount: desc.array_layer_count.unwrap_or(WGPU_ARRAY_LAYER_COUNT_UNDEFINED),
            aspect: conv::aspect(desc.aspect),
            usage: desc.usage.map(|u| u.bits() as u64 & 0x1F).unwrap_or(0),
        };
        let p = unsafe { wgpuTextureCreateView(raw, &d) };
        DispatchTextureView::custom(WcTextureView { inner: owned(p, "createView") })
    }

    fn destroy(&self) {
        match self.inner.get() {
            Some(h) => unsafe { wgpuTextureDestroy(h.raw()) },
            None => drop(self.pending.borrow_mut().take()),
        }
    }
}

// ---------------------------------------------------------------------------
// Command encoding.
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct WcCommandEncoder {
    inner: Handle,
}

fn timestamp_writes(q: &wgpu::QuerySet, begin: Option<u32>, end: Option<u32>) -> WGPUPassTimestampWrites {
    WGPUPassTimestampWrites {
        nextInChain: NULL_CHAIN,
        querySet: q.as_custom::<WcQuerySet>().expect("not a wasmcart query set").inner.raw(),
        beginningOfPassWriteIndex: begin.unwrap_or(WGPU_QUERY_SET_INDEX_UNDEFINED),
        endOfPassWriteIndex: end.unwrap_or(WGPU_QUERY_SET_INDEX_UNDEFINED),
    }
}

impl CommandEncoderInterface for WcCommandEncoder {
    fn copy_buffer_to_buffer(
        &self,
        source: &DispatchBuffer,
        source_offset: wgpu::BufferAddress,
        destination: &DispatchBuffer,
        destination_offset: wgpu::BufferAddress,
        copy_size: Option<wgpu::BufferAddress>,
    ) {
        unsafe {
            wgpuCommandEncoderCopyBufferToBuffer(
                self.inner.raw(),
                dbuf(source),
                source_offset,
                dbuf(destination),
                destination_offset,
                copy_size.unwrap_or(WGPU_WHOLE_SIZE),
            )
        };
    }

    fn copy_buffer_to_texture(
        &self,
        source: wgpu::TexelCopyBufferInfo<'_>,
        destination: wgpu::TexelCopyTextureInfo<'_>,
        copy_size: wgpu::Extent3d,
    ) {
        let (s, d, e) = (texel_buffer(&source), texel_texture(&destination), conv::extent(copy_size));
        unsafe { wgpuCommandEncoderCopyBufferToTexture(self.inner.raw(), &s, &d, &e) };
    }

    fn copy_texture_to_buffer(
        &self,
        source: wgpu::TexelCopyTextureInfo<'_>,
        destination: wgpu::TexelCopyBufferInfo<'_>,
        copy_size: wgpu::Extent3d,
    ) {
        let (s, d, e) = (texel_texture(&source), texel_buffer(&destination), conv::extent(copy_size));
        unsafe { wgpuCommandEncoderCopyTextureToBuffer(self.inner.raw(), &s, &d, &e) };
    }

    fn copy_texture_to_texture(
        &self,
        source: wgpu::TexelCopyTextureInfo<'_>,
        destination: wgpu::TexelCopyTextureInfo<'_>,
        copy_size: wgpu::Extent3d,
    ) {
        let (s, d, e) = (texel_texture(&source), texel_texture(&destination), conv::extent(copy_size));
        unsafe { wgpuCommandEncoderCopyTextureToTexture(self.inner.raw(), &s, &d, &e) };
    }

    fn begin_compute_pass(&self, desc: &wgpu::ComputePassDescriptor<'_>) -> DispatchComputePass {
        let tw = desc.timestamp_writes.as_ref().map(|t| timestamp_writes(t.query_set, t.beginning_of_pass_write_index, t.end_of_pass_write_index));
        let d = WGPUComputePassDescriptor {
            nextInChain: NULL_CHAIN,
            label: osv(desc.label),
            timestampWrites: tw.as_ref().map(|t| t as *const _).unwrap_or(null()),
        };
        let p = unsafe { wgpuCommandEncoderBeginComputePass(self.inner.raw(), &d) };
        DispatchComputePass::custom(WcComputePass { inner: owned(p, "beginComputePass") })
    }

    fn begin_render_pass(&self, desc: &wgpu::RenderPassDescriptor<'_>) -> DispatchRenderPass {
        let colors: Vec<WGPURenderPassColorAttachment> = desc
            .color_attachments
            .iter()
            .map(|a| match a {
                Some(a) => {
                    let (load, clear) = match a.ops.load {
                        wgpu::LoadOp::Clear(c) => (WGPULoadOp_Clear, c),
                        // WebGPU has no don't-care load: clear to black.
                        wgpu::LoadOp::DontCare(_) => (WGPULoadOp_Clear, wgpu::Color::BLACK),
                        wgpu::LoadOp::Load => (WGPULoadOp_Load, wgpu::Color::TRANSPARENT),
                    };
                    WGPURenderPassColorAttachment {
                        nextInChain: NULL_CHAIN,
                        view: custom_view(a.view).inner.raw(),
                        depthSlice: a.depth_slice.unwrap_or(WGPU_DEPTH_SLICE_UNDEFINED),
                        resolveTarget: a.resolve_target.map(|r| custom_view(r).inner.raw()).unwrap_or(null_mut()),
                        loadOp: load,
                        storeOp: store_op(a.ops.store),
                        clearValue: conv::color(clear),
                    }
                }
                None => WGPURenderPassColorAttachment {
                    nextInChain: NULL_CHAIN,
                    view: null_mut(),
                    depthSlice: WGPU_DEPTH_SLICE_UNDEFINED,
                    resolveTarget: null_mut(),
                    loadOp: WGPULoadOp_Undefined,
                    storeOp: WGPUStoreOp_Undefined,
                    clearValue: WGPUColor { r: 0.0, g: 0.0, b: 0.0, a: 0.0 },
                },
            })
            .collect();
        let ds = desc.depth_stencil_attachment.as_ref().map(|d| {
            let mut out = WGPURenderPassDepthStencilAttachment {
                nextInChain: NULL_CHAIN,
                view: custom_view(d.view).inner.raw(),
                depthLoadOp: WGPULoadOp_Undefined,
                depthStoreOp: WGPUStoreOp_Undefined,
                // Never NaN: the glue hands it to WebGPU as a float.
                depthClearValue: 0.0,
                depthReadOnly: conv::bool(d.depth_ops.is_none()),
                stencilLoadOp: WGPULoadOp_Undefined,
                stencilStoreOp: WGPUStoreOp_Undefined,
                stencilClearValue: 0,
                stencilReadOnly: conv::bool(d.stencil_ops.is_none()),
            };
            if let Some(ops) = &d.depth_ops {
                let (l, v) = match ops.load {
                    wgpu::LoadOp::Clear(v) => (WGPULoadOp_Clear, v),
                    wgpu::LoadOp::DontCare(_) => (WGPULoadOp_Clear, 1.0),
                    wgpu::LoadOp::Load => (WGPULoadOp_Load, 0.0),
                };
                out.depthLoadOp = l;
                out.depthClearValue = v;
                out.depthStoreOp = store_op(ops.store);
            }
            if let Some(ops) = &d.stencil_ops {
                let (l, v) = match ops.load {
                    wgpu::LoadOp::Clear(v) => (WGPULoadOp_Clear, v),
                    wgpu::LoadOp::DontCare(_) => (WGPULoadOp_Clear, 0),
                    wgpu::LoadOp::Load => (WGPULoadOp_Load, 0),
                };
                out.stencilLoadOp = l;
                out.stencilClearValue = v;
                out.stencilStoreOp = store_op(ops.store);
            }
            out
        });
        let tw = desc.timestamp_writes.as_ref().map(|t| timestamp_writes(t.query_set, t.beginning_of_pass_write_index, t.end_of_pass_write_index));
        let d = WGPURenderPassDescriptor {
            nextInChain: NULL_CHAIN,
            label: osv(desc.label),
            colorAttachmentCount: colors.len(),
            colorAttachments: colors.as_ptr(),
            depthStencilAttachment: ds.as_ref().map(|d| d as *const _).unwrap_or(null()),
            occlusionQuerySet: desc
                .occlusion_query_set
                .map(|q| q.as_custom::<WcQuerySet>().expect("not a wasmcart query set").inner.raw())
                .unwrap_or(null_mut()),
            timestampWrites: tw.as_ref().map(|t| t as *const _).unwrap_or(null()),
        };
        let p = unsafe { wgpuCommandEncoderBeginRenderPass(self.inner.raw(), &d) };
        DispatchRenderPass::custom(WcRenderPass { inner: owned(p, "beginRenderPass") })
    }

    fn finish(&mut self) -> DispatchCommandBuffer {
        let d = WGPUCommandBufferDescriptor { nextInChain: NULL_CHAIN, label: osv(None) };
        let p = unsafe { wgpuCommandEncoderFinish(self.inner.raw(), &d) };
        DispatchCommandBuffer::custom(WcCommandBuffer { inner: owned(p, "finish") })
    }

    fn clear_texture(&self, _texture: &DispatchTexture, _range: &wgpu::ImageSubresourceRange) {
        panic!("wasmcart-wgpu: clear_texture is not in WebGPU (clear with a render pass)")
    }

    fn clear_buffer(&self, buffer: &DispatchBuffer, offset: wgpu::BufferAddress, size: Option<wgpu::BufferAddress>) {
        unsafe { wgpuCommandEncoderClearBuffer(self.inner.raw(), dbuf(buffer), offset, size.unwrap_or(WGPU_WHOLE_SIZE)) };
    }

    fn insert_debug_marker(&self, label: &str) {
        let s = sv(label);
        unsafe { wgpuCommandEncoderInsertDebugMarker(self.inner.raw(), &s) };
    }

    fn push_debug_group(&self, label: &str) {
        let s = sv(label);
        unsafe { wgpuCommandEncoderPushDebugGroup(self.inner.raw(), &s) };
    }

    fn pop_debug_group(&self) {
        unsafe { wgpuCommandEncoderPopDebugGroup(self.inner.raw()) };
    }

    fn write_timestamp(&self, query_set: &DispatchQuerySet, query_index: u32) {
        let q = query_set.as_custom::<WcQuerySet>().expect("not a wasmcart query set");
        unsafe { wgpuCommandEncoderWriteTimestamp(self.inner.raw(), q.inner.raw(), query_index) };
    }

    fn resolve_query_set(
        &self,
        query_set: &DispatchQuerySet,
        first_query: u32,
        query_count: u32,
        destination: &DispatchBuffer,
        destination_offset: wgpu::BufferAddress,
    ) {
        let q = query_set.as_custom::<WcQuerySet>().expect("not a wasmcart query set");
        unsafe {
            wgpuCommandEncoderResolveQuerySet(
                self.inner.raw(),
                q.inner.raw(),
                first_query,
                query_count,
                dbuf(destination),
                destination_offset,
            )
        };
    }

    fn mark_acceleration_structures_built<'a>(
        &self,
        _blas: &mut dyn Iterator<Item = &'a wgpu::Blas>,
        _tlas: &mut dyn Iterator<Item = &'a wgpu::Tlas>,
    ) {
        panic!("wasmcart-wgpu: ray tracing is not in WebGPU")
    }

    fn build_acceleration_structures<'a>(
        &self,
        _blas: &mut dyn Iterator<Item = &'a wgpu::BlasBuildEntry<'a>>,
        _tlas: &mut dyn Iterator<Item = &'a wgpu::Tlas>,
    ) {
        panic!("wasmcart-wgpu: ray tracing is not in WebGPU")
    }

    fn transition_resources<'a>(
        &mut self,
        _buffers: &mut dyn Iterator<Item = wgt::BufferTransition<&'a DispatchBuffer>>,
        _textures: &mut dyn Iterator<Item = wgt::TextureTransition<&'a DispatchTexture>>,
    ) {
        // WebGPU tracks resource state itself.
    }
}

fn store_op(s: wgpu::StoreOp) -> WGPUStoreOp {
    match s {
        wgpu::StoreOp::Store => WGPUStoreOp_Store,
        wgpu::StoreOp::Discard => WGPUStoreOp_Discard,
    }
}

fn dyn_offsets(o: &[wgpu::DynamicOffset]) -> (usize, *const u32) {
    if o.is_empty() {
        (0, null())
    } else {
        (o.len(), o.as_ptr())
    }
}

#[derive(Debug)]
pub struct WcComputePass {
    inner: Handle,
}
impl ComputePassInterface for WcComputePass {
    fn set_pipeline(&mut self, pipeline: &DispatchComputePipeline) {
        let p = pipeline.as_custom::<WcComputePipeline>().expect("not a wasmcart compute pipeline");
        unsafe { wgpuComputePassEncoderSetPipeline(self.inner.raw(), p.inner.raw()) };
    }
    fn set_bind_group(&mut self, index: u32, bind_group: Option<&DispatchBindGroup>, offsets: &[wgpu::DynamicOffset]) {
        let g = bind_group
            .map(|g| g.as_custom::<WcBindGroup>().expect("not a wasmcart bind group").inner.raw())
            .unwrap_or(null_mut());
        let (n, p) = dyn_offsets(offsets);
        unsafe { wgpuComputePassEncoderSetBindGroup(self.inner.raw(), index, g, n, p) };
    }
    fn set_immediates(&mut self, offset: u32, data: &[u8]) {
        unsafe { wgpuComputePassEncoderSetImmediates(self.inner.raw(), offset, data.as_ptr() as *const c_void, data.len()) };
    }
    fn insert_debug_marker(&mut self, label: &str) {
        let s = sv(label);
        unsafe { wgpuComputePassEncoderInsertDebugMarker(self.inner.raw(), &s) };
    }
    fn push_debug_group(&mut self, group_label: &str) {
        let s = sv(group_label);
        unsafe { wgpuComputePassEncoderPushDebugGroup(self.inner.raw(), &s) };
    }
    fn pop_debug_group(&mut self) {
        unsafe { wgpuComputePassEncoderPopDebugGroup(self.inner.raw()) };
    }
    fn write_timestamp(&mut self, query_set: &DispatchQuerySet, query_index: u32) {
        let q = query_set.as_custom::<WcQuerySet>().expect("not a wasmcart query set");
        unsafe { wgpuComputePassEncoderWriteTimestamp(self.inner.raw(), q.inner.raw(), query_index) };
    }
    fn begin_pipeline_statistics_query(&mut self, _query_set: &DispatchQuerySet, _query_index: u32) {}
    fn end_pipeline_statistics_query(&mut self) {}
    fn dispatch_workgroups(&mut self, x: u32, y: u32, z: u32) {
        unsafe { wgpuComputePassEncoderDispatchWorkgroups(self.inner.raw(), x, y, z) };
    }
    fn dispatch_workgroups_indirect(&mut self, indirect_buffer: &DispatchBuffer, indirect_offset: wgpu::BufferAddress) {
        unsafe { wgpuComputePassEncoderDispatchWorkgroupsIndirect(self.inner.raw(), dbuf(indirect_buffer), indirect_offset) };
    }
    fn transition_resources<'a>(
        &mut self,
        _buffers: &mut dyn Iterator<Item = wgt::BufferTransition<&'a DispatchBuffer>>,
        _textures: &mut dyn Iterator<Item = wgt::TextureTransition<&'a DispatchTextureView>>,
    ) {
    }
}
impl Drop for WcComputePass {
    fn drop(&mut self) {
        unsafe { wgpuComputePassEncoderEnd(self.inner.raw()) };
    }
}

#[derive(Debug)]
pub struct WcRenderPass {
    inner: Handle,
}
impl RenderPassInterface for WcRenderPass {
    fn set_pipeline(&mut self, pipeline: &DispatchRenderPipeline) {
        let p = pipeline.as_custom::<WcRenderPipeline>().expect("not a wasmcart render pipeline");
        unsafe { wgpuRenderPassEncoderSetPipeline(self.inner.raw(), p.inner.raw()) };
    }
    fn set_bind_group(&mut self, index: u32, bind_group: Option<&DispatchBindGroup>, offsets: &[wgpu::DynamicOffset]) {
        let g = bind_group
            .map(|g| g.as_custom::<WcBindGroup>().expect("not a wasmcart bind group").inner.raw())
            .unwrap_or(null_mut());
        let (n, p) = dyn_offsets(offsets);
        unsafe { wgpuRenderPassEncoderSetBindGroup(self.inner.raw(), index, g, n, p) };
    }
    fn set_index_buffer(
        &mut self,
        buffer: &DispatchBuffer,
        index_format: wgpu::IndexFormat,
        offset: wgpu::BufferAddress,
        size: Option<wgpu::BufferSize>,
    ) {
        unsafe {
            wgpuRenderPassEncoderSetIndexBuffer(
                self.inner.raw(),
                dbuf(buffer),
                conv::index_format(index_format),
                offset,
                size.map(|s| s.get()).unwrap_or(WGPU_WHOLE_SIZE),
            )
        };
    }
    fn set_vertex_buffer(
        &mut self,
        slot: u32,
        buffer: Option<&DispatchBuffer>,
        offset: wgpu::BufferAddress,
        size: Option<wgpu::BufferSize>,
    ) {
        let b = buffer.map(dbuf).unwrap_or(null_mut());
        unsafe {
            wgpuRenderPassEncoderSetVertexBuffer(
                self.inner.raw(),
                slot,
                b,
                offset,
                size.map(|s| s.get()).unwrap_or(WGPU_WHOLE_SIZE),
            )
        };
    }
    fn set_immediates(&mut self, offset: u32, data: &[u8]) {
        unsafe { wgpuRenderPassEncoderSetImmediates(self.inner.raw(), offset, data.as_ptr() as *const c_void, data.len()) };
    }
    fn set_blend_constant(&mut self, color: wgpu::Color) {
        let c = conv::color(color);
        unsafe { wgpuRenderPassEncoderSetBlendConstant(self.inner.raw(), &c) };
    }
    fn set_scissor_rect(&mut self, x: u32, y: u32, width: u32, height: u32) {
        unsafe { wgpuRenderPassEncoderSetScissorRect(self.inner.raw(), x, y, width, height) };
    }
    fn set_viewport(&mut self, x: f32, y: f32, width: f32, height: f32, min_depth: f32, max_depth: f32) {
        unsafe { wgpuRenderPassEncoderSetViewport(self.inner.raw(), x, y, width, height, min_depth, max_depth) };
    }
    fn set_stencil_reference(&mut self, reference: u32) {
        unsafe { wgpuRenderPassEncoderSetStencilReference(self.inner.raw(), reference) };
    }
    fn draw(&mut self, vertices: Range<u32>, instances: Range<u32>) {
        unsafe {
            wgpuRenderPassEncoderDraw(
                self.inner.raw(),
                vertices.end - vertices.start,
                instances.end - instances.start,
                vertices.start,
                instances.start,
            )
        };
    }
    fn draw_indexed(&mut self, indices: Range<u32>, base_vertex: i32, instances: Range<u32>) {
        unsafe {
            wgpuRenderPassEncoderDrawIndexed(
                self.inner.raw(),
                indices.end - indices.start,
                instances.end - instances.start,
                indices.start,
                base_vertex,
                instances.start,
            )
        };
    }
    fn draw_mesh_tasks(&mut self, _x: u32, _y: u32, _z: u32) {
        panic!("wasmcart-wgpu: mesh shaders are not in WebGPU")
    }
    fn draw_indirect(&mut self, indirect_buffer: &DispatchBuffer, indirect_offset: wgpu::BufferAddress) {
        unsafe { wgpuRenderPassEncoderDrawIndirect(self.inner.raw(), dbuf(indirect_buffer), indirect_offset) };
    }
    fn draw_indexed_indirect(&mut self, indirect_buffer: &DispatchBuffer, indirect_offset: wgpu::BufferAddress) {
        unsafe { wgpuRenderPassEncoderDrawIndexedIndirect(self.inner.raw(), dbuf(indirect_buffer), indirect_offset) };
    }
    fn draw_mesh_tasks_indirect(&mut self, _b: &DispatchBuffer, _o: wgpu::BufferAddress) {
        panic!("wasmcart-wgpu: mesh shaders are not in WebGPU")
    }
    fn multi_draw_indirect(&mut self, indirect_buffer: &DispatchBuffer, indirect_offset: wgpu::BufferAddress, count: u32) {
        // Without the multi-draw feature, one indirect draw per command.
        for i in 0..count as u64 {
            self.draw_indirect(indirect_buffer, indirect_offset + i * 16);
        }
    }
    fn multi_draw_indexed_indirect(
        &mut self,
        indirect_buffer: &DispatchBuffer,
        indirect_offset: wgpu::BufferAddress,
        count: u32,
    ) {
        for i in 0..count as u64 {
            self.draw_indexed_indirect(indirect_buffer, indirect_offset + i * 20);
        }
    }
    fn multi_draw_indirect_count(
        &mut self,
        _b: &DispatchBuffer,
        _o: wgpu::BufferAddress,
        _cb: &DispatchBuffer,
        _co: wgpu::BufferAddress,
        _max: u32,
    ) {
        panic!("wasmcart-wgpu: indirect draw counts are not in WebGPU")
    }
    fn multi_draw_mesh_tasks_indirect(&mut self, _b: &DispatchBuffer, _o: wgpu::BufferAddress, _c: u32) {
        panic!("wasmcart-wgpu: mesh shaders are not in WebGPU")
    }
    fn multi_draw_indexed_indirect_count(
        &mut self,
        _b: &DispatchBuffer,
        _o: wgpu::BufferAddress,
        _cb: &DispatchBuffer,
        _co: wgpu::BufferAddress,
        _max: u32,
    ) {
        panic!("wasmcart-wgpu: indirect draw counts are not in WebGPU")
    }
    fn multi_draw_mesh_tasks_indirect_count(
        &mut self,
        _b: &DispatchBuffer,
        _o: wgpu::BufferAddress,
        _cb: &DispatchBuffer,
        _co: wgpu::BufferAddress,
        _max: u32,
    ) {
        panic!("wasmcart-wgpu: mesh shaders are not in WebGPU")
    }
    fn insert_debug_marker(&mut self, label: &str) {
        let s = sv(label);
        unsafe { wgpuRenderPassEncoderInsertDebugMarker(self.inner.raw(), &s) };
    }
    fn push_debug_group(&mut self, group_label: &str) {
        let s = sv(group_label);
        unsafe { wgpuRenderPassEncoderPushDebugGroup(self.inner.raw(), &s) };
    }
    fn pop_debug_group(&mut self) {
        unsafe { wgpuRenderPassEncoderPopDebugGroup(self.inner.raw()) };
    }
    fn write_timestamp(&mut self, query_set: &DispatchQuerySet, query_index: u32) {
        let q = query_set.as_custom::<WcQuerySet>().expect("not a wasmcart query set");
        unsafe { wgpuRenderPassEncoderWriteTimestamp(self.inner.raw(), q.inner.raw(), query_index) };
    }
    fn begin_occlusion_query(&mut self, query_index: u32) {
        unsafe { wgpuRenderPassEncoderBeginOcclusionQuery(self.inner.raw(), query_index) };
    }
    fn end_occlusion_query(&mut self) {
        unsafe { wgpuRenderPassEncoderEndOcclusionQuery(self.inner.raw()) };
    }
    fn begin_pipeline_statistics_query(&mut self, _query_set: &DispatchQuerySet, _query_index: u32) {}
    fn end_pipeline_statistics_query(&mut self) {}
    fn execute_bundles(&mut self, render_bundles: &mut dyn Iterator<Item = &DispatchRenderBundle>) {
        let raw: Vec<WGPURenderBundle> = render_bundles
            .map(|b| b.as_custom::<WcRenderBundle>().expect("not a wasmcart render bundle").inner.raw())
            .collect();
        unsafe { wgpuRenderPassEncoderExecuteBundles(self.inner.raw(), raw.len(), raw.as_ptr()) };
    }
}
impl Drop for WcRenderPass {
    fn drop(&mut self) {
        unsafe { wgpuRenderPassEncoderEnd(self.inner.raw()) };
    }
}

#[derive(Debug)]
pub struct WcRenderBundleEncoder {
    inner: Handle,
}
impl RenderBundleEncoderInterface for WcRenderBundleEncoder {
    fn set_pipeline(&mut self, pipeline: &DispatchRenderPipeline) {
        let p = pipeline.as_custom::<WcRenderPipeline>().expect("not a wasmcart render pipeline");
        unsafe { wgpuRenderBundleEncoderSetPipeline(self.inner.raw(), p.inner.raw()) };
    }
    fn set_bind_group(&mut self, index: u32, bind_group: Option<&DispatchBindGroup>, offsets: &[wgpu::DynamicOffset]) {
        let g = bind_group
            .map(|g| g.as_custom::<WcBindGroup>().expect("not a wasmcart bind group").inner.raw())
            .unwrap_or(null_mut());
        let (n, p) = dyn_offsets(offsets);
        unsafe { wgpuRenderBundleEncoderSetBindGroup(self.inner.raw(), index, g, n, p) };
    }
    fn set_index_buffer(
        &mut self,
        buffer: &DispatchBuffer,
        index_format: wgpu::IndexFormat,
        offset: wgpu::BufferAddress,
        size: Option<wgpu::BufferSize>,
    ) {
        unsafe {
            wgpuRenderBundleEncoderSetIndexBuffer(
                self.inner.raw(),
                dbuf(buffer),
                conv::index_format(index_format),
                offset,
                size.map(|s| s.get()).unwrap_or(WGPU_WHOLE_SIZE),
            )
        };
    }
    fn set_vertex_buffer(
        &mut self,
        slot: u32,
        buffer: Option<&DispatchBuffer>,
        offset: wgpu::BufferAddress,
        size: Option<wgpu::BufferSize>,
    ) {
        let b = buffer.map(dbuf).unwrap_or(null_mut());
        unsafe {
            wgpuRenderBundleEncoderSetVertexBuffer(
                self.inner.raw(),
                slot,
                b,
                offset,
                size.map(|s| s.get()).unwrap_or(WGPU_WHOLE_SIZE),
            )
        };
    }
    fn set_immediates(&mut self, offset: u32, data: &[u8]) {
        unsafe {
            wgpuRenderBundleEncoderSetImmediates(self.inner.raw(), offset, data.as_ptr() as *const c_void, data.len())
        };
    }
    fn draw(&mut self, vertices: Range<u32>, instances: Range<u32>) {
        unsafe {
            wgpuRenderBundleEncoderDraw(
                self.inner.raw(),
                vertices.end - vertices.start,
                instances.end - instances.start,
                vertices.start,
                instances.start,
            )
        };
    }
    fn draw_indexed(&mut self, indices: Range<u32>, base_vertex: i32, instances: Range<u32>) {
        unsafe {
            wgpuRenderBundleEncoderDrawIndexed(
                self.inner.raw(),
                indices.end - indices.start,
                instances.end - instances.start,
                indices.start,
                base_vertex,
                instances.start,
            )
        };
    }
    fn draw_indirect(&mut self, indirect_buffer: &DispatchBuffer, indirect_offset: wgpu::BufferAddress) {
        unsafe { wgpuRenderBundleEncoderDrawIndirect(self.inner.raw(), dbuf(indirect_buffer), indirect_offset) };
    }
    fn draw_indexed_indirect(&mut self, indirect_buffer: &DispatchBuffer, indirect_offset: wgpu::BufferAddress) {
        unsafe { wgpuRenderBundleEncoderDrawIndexedIndirect(self.inner.raw(), dbuf(indirect_buffer), indirect_offset) };
    }
    fn finish(self, desc: &wgpu::RenderBundleDescriptor<'_>) -> DispatchRenderBundle
    where
        Self: Sized,
    {
        let d = WGPURenderBundleDescriptor { nextInChain: NULL_CHAIN, label: osv(desc.label) };
        let p = unsafe { wgpuRenderBundleEncoderFinish(self.inner.raw(), &d) };
        DispatchRenderBundle::custom(WcRenderBundle { inner: owned(p, "finish") })
    }
    fn finish_boxed(self: Box<Self>, desc: &wgpu::RenderBundleDescriptor<'_>) -> DispatchRenderBundle {
        (*self).finish(desc)
    }
}

// ---------------------------------------------------------------------------
// Surface.
// ---------------------------------------------------------------------------

/// The cart's canvas ("#canvas"), a texture the host owns and presents.
///
/// On a compatibility-mode device a canvas has no sRGB view (no view-format
/// reinterpretation, and no `bgra8unorm-srgb` at all), so the surface also
/// offers `Rgba8UnormSrgb`: configured with it, the cart draws into an
/// `Rgba8UnormSrgb` texture of the canvas size and, when the frame is
/// presented (or dropped), it is copied byte for byte into an `Rgba8Unorm`
/// canvas. The bytes already hold sRGB-encoded colour, so the canvas shows
/// what a core device's sRGB view would.
#[derive(Debug)]
pub struct WcSurface {
    inner: Handle,
    configured: Cell<bool>,
    emulated: RefCell<Option<SrgbCanvas>>,
}

/// The texture an emulated sRGB canvas is drawn into, and where it goes.
#[derive(Debug, Clone)]
struct SrgbCanvas {
    device: Handle,
    queue: Handle,
    texture: Handle,
    width: u32,
    height: u32,
}

fn adapter_is_core(adapter: &DispatchAdapter) -> bool {
    adapter.as_custom::<WcAdapter>().map(|a| a.is_core()).unwrap_or(true)
}

impl SurfaceInterface for WcSurface {
    fn get_capabilities(&self, adapter: &DispatchAdapter) -> wgpu::SurfaceCapabilities {
        let preferred = match unsafe { emwgpu::emwgpuGetPreferredFormat() } {
            WGPUTextureFormat_RGBA8Unorm => wgpu::TextureFormat::Rgba8Unorm,
            _ => wgpu::TextureFormat::Bgra8Unorm,
        };
        let other = if preferred == wgpu::TextureFormat::Rgba8Unorm {
            wgpu::TextureFormat::Bgra8Unorm
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let mut formats = vec![preferred, other, wgpu::TextureFormat::Rgba16Float];
        if !adapter_is_core(adapter) {
            formats.push(wgpu::TextureFormat::Rgba8UnormSrgb);
        }
        wgpu::SurfaceCapabilities {
            format_capabilities: formats
                .iter()
                .map(|&format| wgt::SurfaceFormatCapabilities {
                    format,
                    color_spaces: wgt::SurfaceColorSpaces::SRGB,
                })
                .collect(),
            formats,
            present_modes: vec![wgpu::PresentMode::Fifo],
            alpha_modes: vec![wgpu::CompositeAlphaMode::Opaque, wgpu::CompositeAlphaMode::PreMultiplied],
            usages: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::TEXTURE_BINDING,
        }
    }

    fn configure(&self, device: &DispatchDevice, config: &wgpu::SurfaceConfiguration) {
        let dev = device.as_custom::<WcDevice>().expect("not a wasmcart device");
        // A canvas is never an sRGB format; an sRGB surface is drawn into a
        // texture of that format and copied (see WcSurface).
        let emulate = config.format.is_srgb();
        let (format, usage, view_formats): (WGPUTextureFormat, u64, Vec<WGPUTextureFormat>) = if emulate {
            (
                conv::texture_format(config.format.remove_srgb_suffix()),
                wgpu::TextureUsages::COPY_DST.bits() as u64,
                Vec::new(),
            )
        } else {
            (
                conv::texture_format(config.format),
                config.usage.bits() as u64 & 0x1F,
                config.view_formats.iter().map(|f| conv::texture_format(*f)).collect(),
            )
        };
        let c = WGPUSurfaceConfiguration {
            nextInChain: NULL_CHAIN,
            device: dev.raw(),
            format,
            usage,
            width: config.width,
            height: config.height,
            viewFormatCount: view_formats.len(),
            viewFormats: view_formats.as_ptr(),
            alphaMode: match config.alpha_mode {
                wgpu::CompositeAlphaMode::PreMultiplied => WGPUCompositeAlphaMode_Premultiplied,
                _ => WGPUCompositeAlphaMode_Opaque,
            },
            presentMode: WGPUPresentMode_Fifo,
        };
        unsafe { wgpuSurfaceConfigure(self.inner.raw(), &c) };
        *self.emulated.borrow_mut() = if emulate {
            let d = WGPUTextureDescriptor {
                nextInChain: NULL_CHAIN,
                label: osv(Some("wasmcart-wgpu sRGB canvas")),
                usage: (config.usage | wgpu::TextureUsages::COPY_SRC).bits() as u64 & 0x1F,
                dimension: WGPUTextureDimension_2D,
                size: WGPUExtent3D { width: config.width, height: config.height, depthOrArrayLayers: 1 },
                format: conv::texture_format(config.format),
                mipLevelCount: 1,
                sampleCount: 1,
                viewFormatCount: 0,
                viewFormats: null(),
            };
            let texture = owned(unsafe { wgpuDeviceCreateTexture(dev.raw(), &d) }, "createTexture (sRGB canvas)");
            let queue = unsafe {
                let q = emwgpu::queue_of(dev.inner.obj());
                emwgpu::add_ref(q);
                Handle::from_owned(q).expect("device without a queue")
            };
            Some(SrgbCanvas { device: dev.inner.clone(), queue, texture, width: config.width, height: config.height })
        } else {
            None
        };
        self.configured.set(true);
    }

    fn get_current_texture(&self) -> (Option<DispatchTexture>, wgpu::SurfaceStatus, DispatchSurfaceOutputDetail) {
        let none = || DispatchSurfaceOutputDetail::custom(WcSurfaceOutputDetail { pending: RefCell::new(None) });
        if !self.configured.get() {
            return (None, wgpu::SurfaceStatus::Outdated, none());
        }
        let mut st = WGPUSurfaceTexture { nextInChain: NULL_CHAIN, texture: null_mut(), status: 0 };
        unsafe { wgpuSurfaceGetCurrentTexture(self.inner.raw(), &mut st) };
        match unsafe { Handle::from_owned(st.texture) } {
            Some(h) if st.status == WGPUSurfaceGetCurrentTextureStatus_SuccessOptimal
                || st.status == WGPUSurfaceGetCurrentTextureStatus_SuccessSuboptimal =>
            {
                match self.emulated.borrow().clone() {
                    Some(e) => {
                        let tex = WcTexture::from_handle(e.texture.clone());
                        let detail = WcSurfaceOutputDetail { pending: RefCell::new(Some((e, h))) };
                        (Some(DispatchTexture::custom(tex)), wgpu::SurfaceStatus::Good, DispatchSurfaceOutputDetail::custom(detail))
                    }
                    None => (Some(DispatchTexture::custom(WcTexture::from_handle(h))), wgpu::SurfaceStatus::Good, none()),
                }
            }
            _ => (None, wgpu::SurfaceStatus::Lost, none()),
        }
    }
}

/// A frame of an emulated sRGB canvas still to be copied to the canvas.
#[derive(Debug)]
pub struct WcSurfaceOutputDetail {
    pending: RefCell<Option<(SrgbCanvas, Handle)>>,
}

impl WcSurfaceOutputDetail {
    /// Copy the frame to the canvas (once). The host presents the canvas
    /// after wc_render returns.
    fn finish(&self) {
        let Some((e, canvas)) = self.pending.borrow_mut().take() else { return };
        let tex = |t: &Handle| WGPUTexelCopyTextureInfo {
            texture: t.raw(),
            mipLevel: 0,
            origin: WGPUOrigin3D { x: 0, y: 0, z: 0 },
            aspect: WGPUTextureAspect_All,
        };
        let (src, dst) = (tex(&e.texture), tex(&canvas));
        let size = WGPUExtent3D { width: e.width, height: e.height, depthOrArrayLayers: 1 };
        unsafe {
            let d = WGPUCommandEncoderDescriptor { nextInChain: NULL_CHAIN, label: osv(None) };
            let enc = owned(wgpuDeviceCreateCommandEncoder(e.device.raw(), &d), "createCommandEncoder");
            wgpuCommandEncoderCopyTextureToTexture(enc.raw(), &src, &dst, &size);
            let d = WGPUCommandBufferDescriptor { nextInChain: NULL_CHAIN, label: osv(None) };
            let cb = owned(wgpuCommandEncoderFinish(enc.raw(), &d), "finish");
            let raw: [WGPUCommandBuffer; 1] = [cb.raw()];
            wgpuQueueSubmit(e.queue.raw(), 1, raw.as_ptr());
        }
    }
}

impl SurfaceOutputDetailInterface for WcSurfaceOutputDetail {
    fn texture_discard(&self) {
        // A cart may drop its frame instead of presenting it; the host
        // presents the canvas either way, so it gets the frame either way.
        self.finish();
    }
    fn texture_release(&self) {}
}

send_sync!(
    WcInstance,
    WcAdapter,
    WcDevice,
    WcQueue,
    WcShaderModule,
    WcBindGroupLayout,
    WcBindGroup,
    WcTextureView,
    WcSampler,
    WcBuffer,
    WcTexture,
    WcQuerySet,
    WcPipelineLayout,
    WcRenderPipeline,
    WcComputePipeline,
    WcPipelineCache,
    WcCommandEncoder,
    WcComputePass,
    WcRenderPass,
    WcCommandBuffer,
    WcRenderBundleEncoder,
    WcRenderBundle,
    WcSurface,
    WcSurfaceOutputDetail,
    WcQueueWriteBuffer,
    WcBufferMappedRange,
);

impl WcDevice {
    /// A core device (`core-features-and-limits`), not compatibility mode.
    pub fn is_core(&self) -> bool {
        self.caps.core
    }
}
