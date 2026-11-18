//! wgpu-types values to webgpu.h values.

use crate::ffi::*;
use wgpu::wgt;

pub fn texture_format(f: wgt::TextureFormat) -> WGPUTextureFormat {
    use wgt::TextureFormat as F;
    match f {
        F::R8Unorm => WGPUTextureFormat_R8Unorm,
        F::R8Snorm => WGPUTextureFormat_R8Snorm,
        F::R8Uint => WGPUTextureFormat_R8Uint,
        F::R8Sint => WGPUTextureFormat_R8Sint,
        F::R16Unorm => WGPUTextureFormat_R16Unorm,
        F::R16Snorm => WGPUTextureFormat_R16Snorm,
        F::R16Uint => WGPUTextureFormat_R16Uint,
        F::R16Sint => WGPUTextureFormat_R16Sint,
        F::R16Float => WGPUTextureFormat_R16Float,
        F::Rg8Unorm => WGPUTextureFormat_RG8Unorm,
        F::Rg8Snorm => WGPUTextureFormat_RG8Snorm,
        F::Rg8Uint => WGPUTextureFormat_RG8Uint,
        F::Rg8Sint => WGPUTextureFormat_RG8Sint,
        F::R32Uint => WGPUTextureFormat_R32Uint,
        F::R32Sint => WGPUTextureFormat_R32Sint,
        F::R32Float => WGPUTextureFormat_R32Float,
        F::Rg16Unorm => WGPUTextureFormat_RG16Unorm,
        F::Rg16Snorm => WGPUTextureFormat_RG16Snorm,
        F::Rg16Uint => WGPUTextureFormat_RG16Uint,
        F::Rg16Sint => WGPUTextureFormat_RG16Sint,
        F::Rg16Float => WGPUTextureFormat_RG16Float,
        F::Rgba8Unorm => WGPUTextureFormat_RGBA8Unorm,
        F::Rgba8UnormSrgb => WGPUTextureFormat_RGBA8UnormSrgb,
        F::Rgba8Snorm => WGPUTextureFormat_RGBA8Snorm,
        F::Rgba8Uint => WGPUTextureFormat_RGBA8Uint,
        F::Rgba8Sint => WGPUTextureFormat_RGBA8Sint,
        F::Bgra8Unorm => WGPUTextureFormat_BGRA8Unorm,
        F::Bgra8UnormSrgb => WGPUTextureFormat_BGRA8UnormSrgb,
        F::Rgb9e5Ufloat => WGPUTextureFormat_RGB9E5Ufloat,
        F::Rgb10a2Uint => WGPUTextureFormat_RGB10A2Uint,
        F::Rgb10a2Unorm => WGPUTextureFormat_RGB10A2Unorm,
        F::Rg11b10Ufloat => WGPUTextureFormat_RG11B10Ufloat,
        F::Rg32Uint => WGPUTextureFormat_RG32Uint,
        F::Rg32Sint => WGPUTextureFormat_RG32Sint,
        F::Rg32Float => WGPUTextureFormat_RG32Float,
        F::Rgba16Unorm => WGPUTextureFormat_RGBA16Unorm,
        F::Rgba16Snorm => WGPUTextureFormat_RGBA16Snorm,
        F::Rgba16Uint => WGPUTextureFormat_RGBA16Uint,
        F::Rgba16Sint => WGPUTextureFormat_RGBA16Sint,
        F::Rgba16Float => WGPUTextureFormat_RGBA16Float,
        F::Rgba32Uint => WGPUTextureFormat_RGBA32Uint,
        F::Rgba32Sint => WGPUTextureFormat_RGBA32Sint,
        F::Rgba32Float => WGPUTextureFormat_RGBA32Float,
        F::Stencil8 => WGPUTextureFormat_Stencil8,
        F::Depth16Unorm => WGPUTextureFormat_Depth16Unorm,
        F::Depth24Plus => WGPUTextureFormat_Depth24Plus,
        F::Depth24PlusStencil8 => WGPUTextureFormat_Depth24PlusStencil8,
        F::Depth32Float => WGPUTextureFormat_Depth32Float,
        F::Depth32FloatStencil8 => WGPUTextureFormat_Depth32FloatStencil8,
        F::Bc1RgbaUnorm => WGPUTextureFormat_BC1RGBAUnorm,
        F::Bc1RgbaUnormSrgb => WGPUTextureFormat_BC1RGBAUnormSrgb,
        F::Bc2RgbaUnorm => WGPUTextureFormat_BC2RGBAUnorm,
        F::Bc2RgbaUnormSrgb => WGPUTextureFormat_BC2RGBAUnormSrgb,
        F::Bc3RgbaUnorm => WGPUTextureFormat_BC3RGBAUnorm,
        F::Bc3RgbaUnormSrgb => WGPUTextureFormat_BC3RGBAUnormSrgb,
        F::Bc4RUnorm => WGPUTextureFormat_BC4RUnorm,
        F::Bc4RSnorm => WGPUTextureFormat_BC4RSnorm,
        F::Bc5RgUnorm => WGPUTextureFormat_BC5RGUnorm,
        F::Bc5RgSnorm => WGPUTextureFormat_BC5RGSnorm,
        F::Bc6hRgbUfloat => WGPUTextureFormat_BC6HRGBUfloat,
        F::Bc6hRgbFloat => WGPUTextureFormat_BC6HRGBFloat,
        F::Bc7RgbaUnorm => WGPUTextureFormat_BC7RGBAUnorm,
        F::Bc7RgbaUnormSrgb => WGPUTextureFormat_BC7RGBAUnormSrgb,
        F::Etc2Rgb8Unorm => WGPUTextureFormat_ETC2RGB8Unorm,
        F::Etc2Rgb8UnormSrgb => WGPUTextureFormat_ETC2RGB8UnormSrgb,
        F::Etc2Rgb8A1Unorm => WGPUTextureFormat_ETC2RGB8A1Unorm,
        F::Etc2Rgb8A1UnormSrgb => WGPUTextureFormat_ETC2RGB8A1UnormSrgb,
        F::Etc2Rgba8Unorm => WGPUTextureFormat_ETC2RGBA8Unorm,
        F::Etc2Rgba8UnormSrgb => WGPUTextureFormat_ETC2RGBA8UnormSrgb,
        F::EacR11Unorm => WGPUTextureFormat_EACR11Unorm,
        F::EacR11Snorm => WGPUTextureFormat_EACR11Snorm,
        F::EacRg11Unorm => WGPUTextureFormat_EACRG11Unorm,
        F::EacRg11Snorm => WGPUTextureFormat_EACRG11Snorm,
        F::Astc { block, channel } => {
            use wgt::AstcBlock as B;
            let unorm = match block {
                B::B4x4 => WGPUTextureFormat_ASTC4x4Unorm,
                B::B5x4 => WGPUTextureFormat_ASTC5x4Unorm,
                B::B5x5 => WGPUTextureFormat_ASTC5x5Unorm,
                B::B6x5 => WGPUTextureFormat_ASTC6x5Unorm,
                B::B6x6 => WGPUTextureFormat_ASTC6x6Unorm,
                B::B8x5 => WGPUTextureFormat_ASTC8x5Unorm,
                B::B8x6 => WGPUTextureFormat_ASTC8x6Unorm,
                B::B8x8 => WGPUTextureFormat_ASTC8x8Unorm,
                B::B10x5 => WGPUTextureFormat_ASTC10x5Unorm,
                B::B10x6 => WGPUTextureFormat_ASTC10x6Unorm,
                B::B10x8 => WGPUTextureFormat_ASTC10x8Unorm,
                B::B10x10 => WGPUTextureFormat_ASTC10x10Unorm,
                B::B12x10 => WGPUTextureFormat_ASTC12x10Unorm,
                B::B12x12 => WGPUTextureFormat_ASTC12x12Unorm,
            };
            match channel {
                // Each Srgb value follows its Unorm one in webgpu.h.
                wgt::AstcChannel::Unorm => unorm,
                wgt::AstcChannel::UnormSrgb => unorm + 1,
                wgt::AstcChannel::Hdr => panic!("{f:?} has no WebGPU equivalent"),
            }
        }
        _ => panic!("{f:?} has no WebGPU equivalent"),
    }
}

pub fn vertex_format(f: wgt::VertexFormat) -> WGPUVertexFormat {
    use wgt::VertexFormat as V;
    match f {
        V::Uint8 => WGPUVertexFormat_Uint8,
        V::Uint8x2 => WGPUVertexFormat_Uint8x2,
        V::Uint8x4 => WGPUVertexFormat_Uint8x4,
        V::Sint8 => WGPUVertexFormat_Sint8,
        V::Sint8x2 => WGPUVertexFormat_Sint8x2,
        V::Sint8x4 => WGPUVertexFormat_Sint8x4,
        V::Unorm8 => WGPUVertexFormat_Unorm8,
        V::Unorm8x2 => WGPUVertexFormat_Unorm8x2,
        V::Unorm8x4 => WGPUVertexFormat_Unorm8x4,
        V::Snorm8 => WGPUVertexFormat_Snorm8,
        V::Snorm8x2 => WGPUVertexFormat_Snorm8x2,
        V::Snorm8x4 => WGPUVertexFormat_Snorm8x4,
        V::Uint16 => WGPUVertexFormat_Uint16,
        V::Uint16x2 => WGPUVertexFormat_Uint16x2,
        V::Uint16x4 => WGPUVertexFormat_Uint16x4,
        V::Sint16 => WGPUVertexFormat_Sint16,
        V::Sint16x2 => WGPUVertexFormat_Sint16x2,
        V::Sint16x4 => WGPUVertexFormat_Sint16x4,
        V::Unorm16 => WGPUVertexFormat_Unorm16,
        V::Unorm16x2 => WGPUVertexFormat_Unorm16x2,
        V::Unorm16x4 => WGPUVertexFormat_Unorm16x4,
        V::Snorm16 => WGPUVertexFormat_Snorm16,
        V::Snorm16x2 => WGPUVertexFormat_Snorm16x2,
        V::Snorm16x4 => WGPUVertexFormat_Snorm16x4,
        V::Float16 => WGPUVertexFormat_Float16,
        V::Float16x2 => WGPUVertexFormat_Float16x2,
        V::Float16x4 => WGPUVertexFormat_Float16x4,
        V::Float32 => WGPUVertexFormat_Float32,
        V::Float32x2 => WGPUVertexFormat_Float32x2,
        V::Float32x3 => WGPUVertexFormat_Float32x3,
        V::Float32x4 => WGPUVertexFormat_Float32x4,
        V::Uint32 => WGPUVertexFormat_Uint32,
        V::Uint32x2 => WGPUVertexFormat_Uint32x2,
        V::Uint32x3 => WGPUVertexFormat_Uint32x3,
        V::Uint32x4 => WGPUVertexFormat_Uint32x4,
        V::Sint32 => WGPUVertexFormat_Sint32,
        V::Sint32x2 => WGPUVertexFormat_Sint32x2,
        V::Sint32x3 => WGPUVertexFormat_Sint32x3,
        V::Sint32x4 => WGPUVertexFormat_Sint32x4,
        V::Unorm10_10_10_2 => WGPUVertexFormat_Unorm10_10_10_2,
        V::Unorm8x4Bgra => WGPUVertexFormat_Unorm8x4BGRA,
        _ => panic!("{f:?} has no WebGPU equivalent (64-bit vertex formats are native-only)"),
    }
}

pub fn texture_view_dimension(d: wgt::TextureViewDimension) -> WGPUTextureViewDimension {
    use wgt::TextureViewDimension as D;
    match d {
        D::D1 => WGPUTextureViewDimension_1D,
        D::D2 => WGPUTextureViewDimension_2D,
        D::D2Array => WGPUTextureViewDimension_2DArray,
        D::Cube => WGPUTextureViewDimension_Cube,
        D::CubeArray => WGPUTextureViewDimension_CubeArray,
        D::D3 => WGPUTextureViewDimension_3D,
    }
}

pub fn texture_dimension(d: wgt::TextureDimension) -> WGPUTextureDimension {
    match d {
        wgt::TextureDimension::D1 => WGPUTextureDimension_1D,
        wgt::TextureDimension::D2 => WGPUTextureDimension_2D,
        wgt::TextureDimension::D3 => WGPUTextureDimension_3D,
    }
}

pub fn sample_type(t: wgt::TextureSampleType) -> WGPUTextureSampleType {
    use wgt::TextureSampleType as T;
    match t {
        T::Float { filterable: true } => WGPUTextureSampleType_Float,
        T::Float { filterable: false } => WGPUTextureSampleType_UnfilterableFloat,
        T::Sint => WGPUTextureSampleType_Sint,
        T::Uint => WGPUTextureSampleType_Uint,
        T::Depth => WGPUTextureSampleType_Depth,
    }
}

pub fn compare(c: wgt::CompareFunction) -> WGPUCompareFunction {
    use wgt::CompareFunction as C;
    match c {
        C::Never => WGPUCompareFunction_Never,
        C::Less => WGPUCompareFunction_Less,
        C::Equal => WGPUCompareFunction_Equal,
        C::LessEqual => WGPUCompareFunction_LessEqual,
        C::Greater => WGPUCompareFunction_Greater,
        C::NotEqual => WGPUCompareFunction_NotEqual,
        C::GreaterEqual => WGPUCompareFunction_GreaterEqual,
        C::Always => WGPUCompareFunction_Always,
    }
}

pub fn stencil_op(o: wgt::StencilOperation) -> WGPUStencilOperation {
    use wgt::StencilOperation as S;
    match o {
        S::Keep => WGPUStencilOperation_Keep,
        S::Zero => WGPUStencilOperation_Zero,
        S::Replace => WGPUStencilOperation_Replace,
        S::Invert => WGPUStencilOperation_Invert,
        S::IncrementClamp => WGPUStencilOperation_IncrementClamp,
        S::DecrementClamp => WGPUStencilOperation_DecrementClamp,
        S::IncrementWrap => WGPUStencilOperation_IncrementWrap,
        S::DecrementWrap => WGPUStencilOperation_DecrementWrap,
    }
}

pub fn stencil_face(s: &wgt::StencilFaceState) -> WGPUStencilFaceState {
    WGPUStencilFaceState {
        compare: compare(s.compare),
        failOp: stencil_op(s.fail_op),
        depthFailOp: stencil_op(s.depth_fail_op),
        passOp: stencil_op(s.pass_op),
    }
}

pub fn blend_factor(f: wgt::BlendFactor) -> WGPUBlendFactor {
    use wgt::BlendFactor as B;
    match f {
        B::Zero => WGPUBlendFactor_Zero,
        B::One => WGPUBlendFactor_One,
        B::Src => WGPUBlendFactor_Src,
        B::OneMinusSrc => WGPUBlendFactor_OneMinusSrc,
        B::SrcAlpha => WGPUBlendFactor_SrcAlpha,
        B::OneMinusSrcAlpha => WGPUBlendFactor_OneMinusSrcAlpha,
        B::Dst => WGPUBlendFactor_Dst,
        B::OneMinusDst => WGPUBlendFactor_OneMinusDst,
        B::DstAlpha => WGPUBlendFactor_DstAlpha,
        B::OneMinusDstAlpha => WGPUBlendFactor_OneMinusDstAlpha,
        B::SrcAlphaSaturated => WGPUBlendFactor_SrcAlphaSaturated,
        B::Constant => WGPUBlendFactor_Constant,
        B::OneMinusConstant => WGPUBlendFactor_OneMinusConstant,
        B::Src1 => WGPUBlendFactor_Src1,
        B::OneMinusSrc1 => WGPUBlendFactor_OneMinusSrc1,
        B::Src1Alpha => WGPUBlendFactor_Src1Alpha,
        B::OneMinusSrc1Alpha => WGPUBlendFactor_OneMinusSrc1Alpha,
    }
}

pub fn blend_component(c: &wgt::BlendComponent) -> WGPUBlendComponent {
    use wgt::BlendOperation as O;
    WGPUBlendComponent {
        operation: match c.operation {
            O::Add => WGPUBlendOperation_Add,
            O::Subtract => WGPUBlendOperation_Subtract,
            O::ReverseSubtract => WGPUBlendOperation_ReverseSubtract,
            O::Min => WGPUBlendOperation_Min,
            O::Max => WGPUBlendOperation_Max,
        },
        srcFactor: blend_factor(c.src_factor),
        dstFactor: blend_factor(c.dst_factor),
    }
}

pub fn index_format(f: wgt::IndexFormat) -> WGPUIndexFormat {
    match f {
        wgt::IndexFormat::Uint16 => WGPUIndexFormat_Uint16,
        wgt::IndexFormat::Uint32 => WGPUIndexFormat_Uint32,
    }
}

pub fn topology(t: wgt::PrimitiveTopology) -> WGPUPrimitiveTopology {
    use wgt::PrimitiveTopology as P;
    match t {
        P::PointList => WGPUPrimitiveTopology_PointList,
        P::LineList => WGPUPrimitiveTopology_LineList,
        P::LineStrip => WGPUPrimitiveTopology_LineStrip,
        P::TriangleList => WGPUPrimitiveTopology_TriangleList,
        P::TriangleStrip => WGPUPrimitiveTopology_TriangleStrip,
    }
}

pub fn address_mode(m: wgt::AddressMode) -> WGPUAddressMode {
    match m {
        wgt::AddressMode::ClampToEdge => WGPUAddressMode_ClampToEdge,
        wgt::AddressMode::Repeat => WGPUAddressMode_Repeat,
        wgt::AddressMode::MirrorRepeat => WGPUAddressMode_MirrorRepeat,
        // WebGPU has no border colour; edge clamping is the closest.
        wgt::AddressMode::ClampToBorder => WGPUAddressMode_ClampToEdge,
    }
}

pub fn filter(m: wgt::FilterMode) -> WGPUFilterMode {
    match m {
        wgt::FilterMode::Nearest => WGPUFilterMode_Nearest,
        wgt::FilterMode::Linear => WGPUFilterMode_Linear,
    }
}

pub fn mipmap_filter(m: wgt::MipmapFilterMode) -> WGPUMipmapFilterMode {
    match m {
        wgt::MipmapFilterMode::Nearest => WGPUMipmapFilterMode_Nearest,
        wgt::MipmapFilterMode::Linear => WGPUMipmapFilterMode_Linear,
    }
}

pub fn aspect(a: wgt::TextureAspect) -> WGPUTextureAspect {
    match a {
        wgt::TextureAspect::All => WGPUTextureAspect_All,
        wgt::TextureAspect::StencilOnly => WGPUTextureAspect_StencilOnly,
        wgt::TextureAspect::DepthOnly => WGPUTextureAspect_DepthOnly,
        _ => panic!("multi-plane textures are not supported on WebGPU"),
    }
}

pub fn color(c: wgt::Color) -> WGPUColor {
    WGPUColor { r: c.r, g: c.g, b: c.b, a: c.a }
}

pub fn extent(e: wgt::Extent3d) -> WGPUExtent3D {
    WGPUExtent3D { width: e.width, height: e.height, depthOrArrayLayers: e.depth_or_array_layers }
}

pub fn origin(o: wgt::Origin3d) -> WGPUOrigin3D {
    WGPUOrigin3D { x: o.x, y: o.y, z: o.z }
}

pub fn bool(b: bool) -> WGPUBool {
    b as WGPUBool
}

pub fn optional_bool(b: Option<bool>) -> WGPUOptionalBool {
    match b {
        None => WGPUOptionalBool_Undefined,
        Some(true) => WGPUOptionalBool_True,
        Some(false) => WGPUOptionalBool_False,
    }
}

/// wgpu feature flags and the WebGPU features they correspond to.
pub const FEATURES: &[(wgt::Features, WGPUFeatureName)] = &[
    (wgt::Features::DEPTH_CLIP_CONTROL, WGPUFeatureName_DepthClipControl),
    (wgt::Features::DEPTH32FLOAT_STENCIL8, WGPUFeatureName_Depth32FloatStencil8),
    (wgt::Features::TEXTURE_COMPRESSION_BC, WGPUFeatureName_TextureCompressionBC),
    (wgt::Features::TEXTURE_COMPRESSION_BC_SLICED_3D, WGPUFeatureName_TextureCompressionBCSliced3D),
    (wgt::Features::TEXTURE_COMPRESSION_ETC2, WGPUFeatureName_TextureCompressionETC2),
    (wgt::Features::TEXTURE_COMPRESSION_ASTC, WGPUFeatureName_TextureCompressionASTC),
    (wgt::Features::TEXTURE_COMPRESSION_ASTC_SLICED_3D, WGPUFeatureName_TextureCompressionASTCSliced3D),
    (wgt::Features::TIMESTAMP_QUERY, WGPUFeatureName_TimestampQuery),
    (wgt::Features::INDIRECT_FIRST_INSTANCE, WGPUFeatureName_IndirectFirstInstance),
    (wgt::Features::SHADER_F16, WGPUFeatureName_ShaderF16),
    (wgt::Features::RG11B10UFLOAT_RENDERABLE, WGPUFeatureName_RG11B10UfloatRenderable),
    (wgt::Features::BGRA8UNORM_STORAGE, WGPUFeatureName_BGRA8UnormStorage),
    (wgt::Features::FLOAT32_FILTERABLE, WGPUFeatureName_Float32Filterable),
    (wgt::Features::FLOAT32_BLENDABLE, WGPUFeatureName_Float32Blendable),
    (wgt::Features::DUAL_SOURCE_BLENDING, WGPUFeatureName_DualSourceBlending),
    (wgt::Features::CLIP_DISTANCES, WGPUFeatureName_ClipDistances),
];

pub fn features_from(names: &[WGPUFeatureName]) -> wgt::Features {
    let mut out = wgt::Features::empty();
    for (flag, name) in FEATURES {
        if names.contains(name) {
            out |= *flag;
        }
    }
    out
}

pub fn limits_from(l: &WGPULimits) -> wgt::Limits {
    wgt::Limits {
        max_texture_dimension_1d: l.maxTextureDimension1D,
        max_texture_dimension_2d: l.maxTextureDimension2D,
        max_texture_dimension_3d: l.maxTextureDimension3D,
        max_texture_array_layers: l.maxTextureArrayLayers,
        max_bind_groups: l.maxBindGroups,
        max_bind_groups_plus_vertex_buffers: l.maxBindGroupsPlusVertexBuffers,
        max_bindings_per_bind_group: l.maxBindingsPerBindGroup,
        max_dynamic_uniform_buffers_per_pipeline_layout: l.maxDynamicUniformBuffersPerPipelineLayout,
        max_dynamic_storage_buffers_per_pipeline_layout: l.maxDynamicStorageBuffersPerPipelineLayout,
        max_sampled_textures_per_shader_stage: l.maxSampledTexturesPerShaderStage,
        max_samplers_per_shader_stage: l.maxSamplersPerShaderStage,
        max_storage_buffers_per_shader_stage: l.maxStorageBuffersPerShaderStage,
        max_storage_textures_per_shader_stage: l.maxStorageTexturesPerShaderStage,
        max_uniform_buffers_per_shader_stage: l.maxUniformBuffersPerShaderStage,
        max_binding_array_elements_per_shader_stage: 0,
        max_binding_array_sampler_elements_per_shader_stage: 0,
        max_binding_array_acceleration_structure_elements_per_shader_stage: 0,
        max_uniform_buffer_binding_size: l.maxUniformBufferBindingSize,
        max_storage_buffer_binding_size: l.maxStorageBufferBindingSize,
        max_vertex_buffers: l.maxVertexBuffers,
        max_buffer_size: l.maxBufferSize,
        max_vertex_attributes: l.maxVertexAttributes,
        max_vertex_buffer_array_stride: l.maxVertexBufferArrayStride,
        max_inter_stage_shader_variables: l.maxInterStageShaderVariables,
        min_uniform_buffer_offset_alignment: l.minUniformBufferOffsetAlignment,
        min_storage_buffer_offset_alignment: l.minStorageBufferOffsetAlignment,
        max_color_attachments: l.maxColorAttachments,
        max_color_attachment_bytes_per_sample: l.maxColorAttachmentBytesPerSample,
        max_compute_workgroup_storage_size: l.maxComputeWorkgroupStorageSize,
        max_compute_invocations_per_workgroup: l.maxComputeInvocationsPerWorkgroup,
        max_compute_workgroup_size_x: l.maxComputeWorkgroupSizeX,
        max_compute_workgroup_size_y: l.maxComputeWorkgroupSizeY,
        max_compute_workgroup_size_z: l.maxComputeWorkgroupSizeZ,
        max_compute_workgroups_per_dimension: l.maxComputeWorkgroupsPerDimension,
        // Immediates (push constants) are not part of WebGPU yet.
        max_immediate_size: 0,
        ..wgt::Limits::downlevel_webgl2_defaults()
    }
}

/// A webgpu.h limits struct asking for `l` (what wgpu's Limits name).
pub fn limits_to(l: &wgt::Limits) -> WGPULimits {
    WGPULimits {
        nextInChain: core::ptr::null_mut(),
        maxTextureDimension1D: l.max_texture_dimension_1d,
        maxTextureDimension2D: l.max_texture_dimension_2d,
        maxTextureDimension3D: l.max_texture_dimension_3d,
        maxTextureArrayLayers: l.max_texture_array_layers,
        maxBindGroups: l.max_bind_groups,
        maxBindGroupsPlusVertexBuffers: l.max_bind_groups_plus_vertex_buffers,
        maxBindingsPerBindGroup: l.max_bindings_per_bind_group,
        maxDynamicUniformBuffersPerPipelineLayout: l.max_dynamic_uniform_buffers_per_pipeline_layout,
        maxDynamicStorageBuffersPerPipelineLayout: l.max_dynamic_storage_buffers_per_pipeline_layout,
        maxSampledTexturesPerShaderStage: l.max_sampled_textures_per_shader_stage,
        maxSamplersPerShaderStage: l.max_samplers_per_shader_stage,
        maxStorageBuffersPerShaderStage: l.max_storage_buffers_per_shader_stage,
        maxStorageTexturesPerShaderStage: l.max_storage_textures_per_shader_stage,
        maxUniformBuffersPerShaderStage: l.max_uniform_buffers_per_shader_stage,
        maxUniformBufferBindingSize: l.max_uniform_buffer_binding_size,
        maxStorageBufferBindingSize: l.max_storage_buffer_binding_size,
        minUniformBufferOffsetAlignment: l.min_uniform_buffer_offset_alignment,
        minStorageBufferOffsetAlignment: l.min_storage_buffer_offset_alignment,
        maxVertexBuffers: l.max_vertex_buffers,
        maxBufferSize: l.max_buffer_size,
        maxVertexAttributes: l.max_vertex_attributes,
        maxVertexBufferArrayStride: l.max_vertex_buffer_array_stride,
        maxInterStageShaderVariables: l.max_inter_stage_shader_variables,
        maxColorAttachments: l.max_color_attachments,
        maxColorAttachmentBytesPerSample: l.max_color_attachment_bytes_per_sample,
        maxComputeWorkgroupStorageSize: l.max_compute_workgroup_storage_size,
        maxComputeInvocationsPerWorkgroup: l.max_compute_invocations_per_workgroup,
        maxComputeWorkgroupSizeX: l.max_compute_workgroup_size_x,
        maxComputeWorkgroupSizeY: l.max_compute_workgroup_size_y,
        maxComputeWorkgroupSizeZ: l.max_compute_workgroup_size_z,
        maxComputeWorkgroupsPerDimension: l.max_compute_workgroups_per_dimension,
        maxImmediateSize: 0,
    }
}

/// Whether `have` meets every limit in `want` (alignments: lower is better).
pub fn limits_satisfy(have: &wgt::Limits, want: &wgt::Limits) -> bool {
    let h = limits_to(have);
    let w = limits_to(want);
    macro_rules! hi { ($($f:ident),*) => { true $(&& h.$f >= w.$f)* } }
    macro_rules! lo { ($($f:ident),*) => { true $(&& h.$f <= w.$f)* } }
    hi!(maxTextureDimension1D, maxTextureDimension2D, maxTextureDimension3D, maxTextureArrayLayers,
        maxBindGroups, maxBindGroupsPlusVertexBuffers, maxBindingsPerBindGroup,
        maxDynamicUniformBuffersPerPipelineLayout, maxDynamicStorageBuffersPerPipelineLayout,
        maxSampledTexturesPerShaderStage, maxSamplersPerShaderStage, maxStorageBuffersPerShaderStage,
        maxStorageTexturesPerShaderStage, maxUniformBuffersPerShaderStage, maxUniformBufferBindingSize,
        maxStorageBufferBindingSize, maxVertexBuffers, maxBufferSize, maxVertexAttributes,
        maxVertexBufferArrayStride, maxInterStageShaderVariables, maxColorAttachments,
        maxColorAttachmentBytesPerSample, maxComputeWorkgroupStorageSize,
        maxComputeInvocationsPerWorkgroup, maxComputeWorkgroupSizeX, maxComputeWorkgroupSizeY,
        maxComputeWorkgroupSizeZ, maxComputeWorkgroupsPerDimension)
        && lo!(minUniformBufferOffsetAlignment, minStorageBufferOffsetAlignment)
}
