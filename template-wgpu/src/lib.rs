//! A wasmcart WebGPU cart, in Rust, drawing with wgpu.
//!
//! Build and run:
//!   ./build.sh
//!   npx wasmcart my_wgpu_cart.wasc
//!
//! On a compatibility-mode host (the WebGPU tier's floor) the canvas has no
//! sRGB view: use gpu.view_format(), gpu.clear_color() and the SRGB_ENCODE
//! constant below, as here, and the picture is the same everywhere.

use wasmcart::*;
use wasmcart_wgpu::{wgpu, Gpu};

const W: u32 = 640;
const H: u32 = 480;

// gpu_api must agree with the imports: this cart imports WebGPU functions.
wc_cart!(W, H, gpu_api: WC_GPU_API_WEBGPU);

const SHADER: &str = r#"
// True when the canvas view is not sRGB (a compatibility-mode device): the
// shader encodes its linear colour itself (Gpu::srgb_constants).
override SRGB_ENCODE: bool = false;

fn encode(c: vec3f) -> vec3f {
    if (!SRGB_ENCODE) { return c; }
    return select(1.055 * pow(c, vec3f(1.0 / 2.4)) - 0.055, 12.92 * c, c <= vec3f(0.0031308));
}

struct Out {
    @builtin(position) pos: vec4f,
    @location(0) color: vec3f,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> Out {
    var p = array<vec2f, 3>(vec2f(0.0, 0.75), vec2f(-0.75, -0.6), vec2f(0.75, -0.6));
    var c = array<vec3f, 3>(vec3f(1.0, 0.05, 0.05), vec3f(0.05, 1.0, 0.05), vec3f(0.05, 0.05, 1.0));
    var o: Out;
    o.pos = vec4f(p[i], 0.0, 1.0);
    o.color = c[i];
    return o;
}

@fragment
fn fs(v: Out) -> @location(0) vec4f {
    return vec4f(encode(v.color), 1.0);
}
"#;

/// The clear colour, linear.
pub const CLEAR: wgpu::Color = wgpu::Color { r: 0.0, g: 0.0, b: 0.125, a: 1.0 };

struct State {
    gpu: Gpu,
    pipeline: wgpu::RenderPipeline,
}

static mut STATE: Option<State> = None;

#[no_mangle]
pub extern "C" fn wc_init() {
    std::panic::set_hook(Box::new(|info| wc_log(&info.to_string())));
    let gpu = Gpu::new(W, H).expect("this host has no WebGPU device");
    let module = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("triangle"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let pipeline = gpu.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("triangle"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            targets: &[Some(gpu.view_format().into())],
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: gpu.srgb_constants(),
                ..Default::default()
            },
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    wc_log("my-wgpu-cart: ready");
    unsafe { STATE = Some(State { gpu, pipeline }) };
}

#[no_mangle]
pub extern "C" fn wc_render() {
    #[allow(static_mut_refs)]
    let Some(s) = (unsafe { STATE.as_ref() }) else { return };
    let Some(frame) = s.gpu.frame() else { return };
    let mut enc = s.gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("triangle"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &frame.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(s.gpu.clear_color(CLEAR)), store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&s.pipeline);
        pass.draw(0..3, 0..1);
    }
    s.gpu.queue.submit([enc.finish()]);
    s.gpu.present(frame);
}
