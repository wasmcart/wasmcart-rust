//! wgpu_scene: a spinning textured cube with a depth buffer, drawn through a
//! render bundle, over a texture a compute pass regenerates every frame.
//!
//! It touches most of the wgpu surface a real renderer uses:
//! - a compute pipeline writing a storage buffer, copied into a texture
//!   (copy_buffer_to_texture) each frame;
//! - vertex and index buffers created mapped (mapped_at_creation);
//! - a uniform buffer rewritten each frame (queue.write_buffer);
//! - a sampler, an explicit bind group layout and pipeline layout;
//! - a Depth24Plus attachment with depth testing and back-face culling;
//! - a render bundle executed inside the pass;
//! - pipeline-overridable constants, error scopes, shader compilation info,
//!   and a buffer read back with map_async: results that arrive between
//!   frames, logged with wc_log.
//!
//! Log lines (the gate reads them): "scene: readback ok ..." once the
//! compute result is mapped and matches, "scene: errors none" once the error
//! scope around setup pops clean, "scene: shader messages N", "scene: caps ..."
//! (the limits and downlevel flags wgpu code reads: honest in compatibility
//! mode), "scene: core request ..." (the result of asking for a core device
//! with wasmcart_wgpu::request_core).

use std::sync::atomic::{AtomicU32, Ordering};

use wasmcart::*;
use wasmcart_wgpu::{wgpu, Gpu};

const W: u32 = 640;
const H: u32 = 480;
const TEX: u32 = 64;

wc_cart!(W, H, gpu_api: WC_GPU_API_WEBGPU);

const SHADER: &str = r#"
override SRGB_ENCODE: bool = false;

struct Uniforms {
    mvp: mat4x4f,
    time: f32,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var tex: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VOut {
    @builtin(position) pos: vec4f,
    @location(0) uv: vec2f,
    @location(1) shade: f32,
};

@vertex
fn vs(@location(0) pos: vec3f, @location(1) uv: vec2f, @location(2) normal: vec3f) -> VOut {
    var o: VOut;
    o.pos = u.mvp * vec4f(pos, 1.0);
    o.uv = uv;
    // Light fixed in model space, so faces keep distinct shades as it turns.
    o.shade = 0.45 + 0.55 * max(dot(normal, normalize(vec3f(0.4, 0.7, 0.6))), 0.0);
    return o;
}

fn encode(c: vec3f) -> vec3f {
    if (!SRGB_ENCODE) { return c; }
    return select(1.055 * pow(c, vec3f(1.0 / 2.4)) - 0.055, 12.92 * c, c <= vec3f(0.0031308));
}

@fragment
fn fs(v: VOut) -> @location(0) vec4f {
    let c = textureSample(tex, samp, v.uv).rgb * v.shade;
    return vec4f(encode(c), 1.0);
}
"#;

const COMPUTE: &str = r#"
struct Params { time: f32, size: u32 };
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> pixels: array<u32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3u) {
    if (id.x >= params.size || id.y >= params.size) { return; }
    let cell = ((id.x / 8u) + (id.y / 8u)) % 2u;
    let wave = 0.5 + 0.5 * sin(f32(id.x) * 0.2 + params.time * 3.0);
    var c: vec4f;
    if (cell == 0u) {
        c = vec4f(1.0, 0.55 * wave, 0.1, 1.0);
    } else {
        c = vec4f(0.1, 0.35 + 0.5 * wave, 1.0, 1.0);
    }
    pixels[id.y * params.size + id.x] = pack4x8unorm(c);
}
"#;

/// Clear colour, linear: a dark grey-green no cube face can produce.
pub const CLEAR: wgpu::Color = wgpu::Color { r: 0.02, g: 0.05, b: 0.03, a: 1.0 };

struct State {
    gpu: Gpu,
    depth: wgpu::TextureView,
    uniforms: wgpu::Buffer,
    params: wgpu::Buffer,
    pixels: wgpu::Buffer,
    texture: wgpu::Texture,
    compute: wgpu::ComputePipeline,
    compute_group: wgpu::BindGroup,
    bundle: wgpu::RenderBundle,
    readback: wgpu::Buffer,
    frame: u32,
}

static mut STATE: Option<State> = None;
/// 0 = not started, 1 = map requested, 2 = done.
static READBACK: AtomicU32 = AtomicU32::new(0);

fn log(s: &str) {
    wc_log(s);
}

// Column-major 4x4 matrices, enough for one camera and one model.
type M4 = [f32; 16];

fn mul(a: &M4, b: &M4) -> M4 {
    let mut r = [0.0; 16];
    for c in 0..4 {
        for row in 0..4 {
            r[c * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[c * 4 + k]).sum();
        }
    }
    r
}

fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> M4 {
    // WebGPU depth range 0..1.
    let f = 1.0 / (fovy / 2.0).tan();
    let mut m = [0.0; 16];
    m[0] = f / aspect;
    m[5] = f;
    m[10] = far / (near - far);
    m[11] = -1.0;
    m[14] = near * far / (near - far);
    m
}

fn rot_y(a: f32) -> M4 {
    let (s, c) = a.sin_cos();
    [c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, 0.0, 0.0, 0.0, 1.0]
}

fn rot_x(a: f32) -> M4 {
    let (s, c) = a.sin_cos();
    [1.0, 0.0, 0.0, 0.0, 0.0, c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0]
}

fn translate(x: f32, y: f32, z: f32) -> M4 {
    [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, y, z, 1.0]
}

/// 24 vertices (4 per face): position, uv, normal.
fn cube() -> (Vec<f32>, Vec<u16>) {
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
    ];
    let mut v = Vec::new();
    let mut idx = Vec::new();
    for (f, (n, r, up)) in faces.iter().enumerate() {
        for (cu, cv) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            for k in 0..3 {
                v.push(n[k] + r[k] * cu + up[k] * cv);
            }
            v.push((cu + 1.0) / 2.0);
            v.push(1.0 - (cv + 1.0) / 2.0);
            v.extend_from_slice(n);
        }
        let b = (f * 4) as u16;
        idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
    }
    (v, idx)
}

fn bytes<T>(s: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
}

/// A buffer filled at creation through a mapping (exercises mapped_at_creation).
fn buffer_init(device: &wgpu::Device, label: &str, data: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
    let size = (data.len() as u64 + 3) & !3;
    let buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage, mapped_at_creation: true });
    buf.slice(..).get_mapped_range_mut().expect("mapped at creation").slice(..data.len()).copy_from_slice(data);
    buf.unmap();
    buf
}

#[no_mangle]
pub extern "C" fn wc_init() {
    std::panic::set_hook(Box::new(|info| wc_log(&info.to_string())));
    let gpu = Gpu::new(W, H).expect("this host has no WebGPU device");
    let device = &gpu.device;
    log(&format!(
        "scene: device core={} srgb={} adapter={}",
        wasmcart_wgpu::is_core(device),
        gpu.srgb,
        gpu.adapter.get_info().name
    ));

    let l = gpu.adapter.limits();
    let dl = gpu.adapter.get_downlevel_capabilities().flags;
    log(&format!(
        "scene: caps storage_buffers={} storage_textures={} invocations={} cube_array={} vertex_storage={} independent_blend={} view_formats={} format={:?}",
        l.max_storage_buffers_per_shader_stage,
        l.max_storage_textures_per_shader_stage,
        l.max_compute_invocations_per_workgroup,
        dl.contains(wgpu::DownlevelFlags::CUBE_ARRAY_TEXTURES),
        dl.contains(wgpu::DownlevelFlags::VERTEX_STORAGE),
        dl.contains(wgpu::DownlevelFlags::INDEPENDENT_BLEND),
        dl.contains(wgpu::DownlevelFlags::VIEW_FORMATS),
        gpu.config.format,
    ));
    // Ask for a core device. On a core host its own device answers at once;
    // on a compatibility-mode host a new device arrives between frames where
    // the GPU has core, and the request fails where it does not.
    let core = wasmcart_wgpu::request_core(&gpu.adapter, &wgpu::DeviceDescriptor::default());
    PENDING.with(|p| p.borrow_mut().push(Box::pin(async move {
        match core.await {
            Ok((d, _q)) => log(&format!("scene: core request ok core={}", wasmcart_wgpu::is_core(&d))),
            Err(e) => log(&format!("scene: core request failed: {e}")),
        }
    })));

    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("scene"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let cshader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("pattern"),
        source: wgpu::ShaderSource::Wgsl(COMPUTE.into()),
    });

    // The compilation info arrives between frames.
    let info = shader.get_compilation_info();
    PENDING.with(|p| p.borrow_mut().push(Box::pin(async move {
        let info = info.await;
        log(&format!("scene: shader messages {}", info.messages.len()));
    })));

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pattern"),
        size: wgpu::Extent3d { width: TEX, height: TEX, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let tex_view = texture.create_view(&Default::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("nearest"),
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });

    let depth = device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24Plus,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default());

    let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("uniforms"),
        size: 80,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let params = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("params"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let pixels = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("pixels"),
        size: (TEX * TEX * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: 64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let (verts, idx) = cube();
    let vbuf = buffer_init(device, "cube vertices", bytes(&verts), wgpu::BufferUsages::VERTEX);
    let ibuf = buffer_init(device, "cube indices", bytes(&idx), wgpu::BufferUsages::INDEX);

    // Compute: auto layout, bind group from the pipeline.
    let compute = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("pattern"),
        layout: None,
        module: &cshader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let compute_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("pattern"),
        layout: &compute.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: pixels.as_entire_binding() },
        ],
    });

    // Render: explicit layouts.
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("scene"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("scene"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&tex_view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("scene"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let stride = (8 * 4) as wgpu::BufferAddress;
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("scene"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: stride,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x3],
            })],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            targets: &[Some(gpu.view_format().into())],
            compilation_options: wgpu::PipelineCompilationOptions {
                constants: gpu.srgb_constants(),
                ..Default::default()
            },
        }),
        primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth24Plus,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });

    // The cube draw, recorded once as a render bundle.
    let mut be = device.create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
        label: Some("cube"),
        color_formats: &[Some(gpu.view_format())],
        depth_stencil: Some(wgpu::RenderBundleDepthStencil {
            format: wgpu::TextureFormat::Depth24Plus,
            depth_read_only: false,
            stencil_read_only: true,
        }),
        sample_count: 1,
        multiview: None,
    });
    be.set_pipeline(&pipeline);
    be.set_bind_group(0, &group, &[]);
    be.set_vertex_buffer(0, vbuf.slice(..));
    be.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint16);
    be.draw_indexed(0..idx.len() as u32, 0, 0..1);
    let bundle = be.finish(&wgpu::RenderBundleDescriptor { label: Some("cube") });

    let errors = scope.pop();
    PENDING.with(|p| p.borrow_mut().push(Box::pin(async move {
        match errors.await {
            None => log("scene: errors none"),
            Some(e) => log(&format!("scene: errors {e}")),
        }
    })));

    log("scene: ready");
    unsafe {
        STATE = Some(State {
            gpu,
            depth,
            uniforms,
            params,
            pixels,
            texture,
            compute,
            compute_group,
            bundle,
            readback,
            frame: 0,
        })
    };
}

type Task = std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>;
thread_local! {
    /// Futures waiting on results the host delivers between frames, polled
    /// once per frame (nothing may block inside wc_render).
    static PENDING: std::cell::RefCell<Vec<Task>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn poll_pending() {
    let tasks: Vec<Task> = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
    let mut left = Vec::new();
    for mut t in tasks {
        if wasmcart_wgpu::poll_once(t.as_mut()).is_pending() {
            left.push(t);
        }
    }
    PENDING.with(|p| p.borrow_mut().extend(left));
}

#[no_mangle]
pub extern "C" fn wc_render() {
    poll_pending();
    #[allow(static_mut_refs)]
    let Some(s) = (unsafe { STATE.as_mut() }) else { return };
    s.frame += 1;
    let t = s.frame as f32 / 60.0;
    let gpu = &s.gpu;

    // Uniforms: model-view-projection and time.
    let model = mul(&rot_y(t * 0.9), &rot_x(0.5 + t * 0.4));
    let view = translate(0.0, 0.0, -4.2);
    let proj = perspective(0.9, W as f32 / H as f32, 0.1, 50.0);
    let mvp = mul(&proj, &mul(&view, &model));
    let mut u = [0f32; 20];
    u[..16].copy_from_slice(&mvp);
    u[16] = t;
    gpu.queue.write_buffer(&s.uniforms, 0, bytes(&u));
    let p: [u32; 4] = [t.to_bits(), TEX, 0, 0];
    gpu.queue.write_buffer(&s.params, 0, bytes(&p));

    let Some(frame) = gpu.frame() else { return };
    let mut enc = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
    {
        let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("pattern"), timestamp_writes: None });
        cp.set_pipeline(&s.compute);
        cp.set_bind_group(0, &s.compute_group, &[]);
        cp.dispatch_workgroups(TEX / 8, TEX / 8, 1);
    }
    enc.copy_buffer_to_texture(
        wgpu::TexelCopyBufferInfo {
            buffer: &s.pixels,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(TEX * 4), rows_per_image: Some(TEX) },
        },
        s.texture.as_image_copy(),
        wgpu::Extent3d { width: TEX, height: TEX, depth_or_array_layers: 1 },
    );
    let start_readback = s.frame == 2 && READBACK.load(Ordering::SeqCst) == 0;
    if start_readback {
        enc.copy_buffer_to_buffer(&s.pixels, 0, &s.readback, 0, 64);
    }
    {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &frame.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(gpu.clear_color(CLEAR)), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &s.depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.execute_bundles([&s.bundle]);
    }
    gpu.queue.submit([enc.finish()]);

    if start_readback {
        READBACK.store(1, Ordering::SeqCst);
        let buf = s.readback.clone();
        s.readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            if r.is_err() {
                log("scene: readback map failed");
                return;
            }
            let data = buf.slice(..).get_mapped_range().expect("mapped for reading");
            let px: Vec<u32> = data.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
            drop(data);
            buf.unmap();
            // Row 0, x 0..7 is a cell-0 square: red 255, alpha 255, blue 0.1*255.
            let ok = px[..8].iter().all(|p| p & 0xff == 255 && p >> 24 == 255 && (p >> 16) & 0xff == 26);
            log(&format!("scene: readback {} {:08x} {:08x}", if ok { "ok" } else { "BAD" }, px[0], px[7]));
            READBACK.store(2, Ordering::SeqCst);
        });
    }
    gpu.present(frame);
}
