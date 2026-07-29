//! OpenGL ES 3.0 / WebGL2 bindings, imported from the wasm module `gl`.
//!
//! Set `gpu_api: WC_GPU_API_WEBGL2` in [`wc_cart!`](crate::wc_cart) to declare
//! a GL cart. The host detects GL from the import section before instantiation
//! and confirms it against `gpu_api`, so both must agree.
//!
//! These are raw `extern "C"` declarations: `unsafe` to call, C signatures,
//! `GLenum` constants rather than Rust enums. That is deliberate. A cart doing
//! GL is porting or writing GL code, and a safe wrapper here would be a second
//! API to learn that still could not make `glDrawArrays` safe.
//!
//! Only functions that are actually called end up in the wasm import section,
//! so a cart that draws a triangle imports a handful of symbols, not all of
//! these. A 2D cart that never touches this module imports none.
//!
//! # Shader versions
//!
//! Shaders must be `#version 300 es` (recommended) or `#version 100`. Desktop
//! GL version strings are not supported and the host does not translate them.

#![allow(non_camel_case_types, non_snake_case, clippy::missing_safety_doc)]

pub type GLenum = u32;
pub type GLboolean = u8;
pub type GLbitfield = u32;
pub type GLint = i32;
pub type GLuint = u32;
pub type GLsizei = i32;
pub type GLfloat = f32;
/// wasm32: 4 bytes, matching the C header's `signed long int`.
pub type GLsizeiptr = i32;
pub type GLintptr = i32;

// ─── Constants ───────────────────────────────────────────────────────────

pub const GL_FALSE: GLboolean = 0;
pub const GL_TRUE: GLboolean = 1;
pub const GL_NONE: GLenum = 0;

// Clear bits
pub const GL_DEPTH_BUFFER_BIT: GLbitfield = 0x0000_0100;
pub const GL_STENCIL_BUFFER_BIT: GLbitfield = 0x0000_0400;
pub const GL_COLOR_BUFFER_BIT: GLbitfield = 0x0000_4000;

// Primitives
pub const GL_POINTS: GLenum = 0x0000;
pub const GL_LINES: GLenum = 0x0001;
pub const GL_LINE_LOOP: GLenum = 0x0002;
pub const GL_LINE_STRIP: GLenum = 0x0003;
pub const GL_TRIANGLES: GLenum = 0x0004;
pub const GL_TRIANGLE_STRIP: GLenum = 0x0005;
pub const GL_TRIANGLE_FAN: GLenum = 0x0006;

// Data types
pub const GL_BYTE: GLenum = 0x1400;
pub const GL_UNSIGNED_BYTE: GLenum = 0x1401;
pub const GL_SHORT: GLenum = 0x1402;
pub const GL_UNSIGNED_SHORT: GLenum = 0x1403;
pub const GL_INT: GLenum = 0x1404;
pub const GL_UNSIGNED_INT: GLenum = 0x1405;
pub const GL_FLOAT: GLenum = 0x1406;
pub const GL_HALF_FLOAT: GLenum = 0x140B;

// Enable / Disable
pub const GL_BLEND: GLenum = 0x0BE2;
pub const GL_CULL_FACE: GLenum = 0x0B44;
pub const GL_DEPTH_TEST: GLenum = 0x0B71;
pub const GL_DITHER: GLenum = 0x0BD0;
pub const GL_POLYGON_OFFSET_FILL: GLenum = 0x8037;
pub const GL_SCISSOR_TEST: GLenum = 0x0C11;
pub const GL_STENCIL_TEST: GLenum = 0x0B90;

// Blend factors
pub const GL_ZERO: GLenum = 0;
pub const GL_ONE: GLenum = 1;
pub const GL_SRC_COLOR: GLenum = 0x0300;
pub const GL_ONE_MINUS_SRC_COLOR: GLenum = 0x0301;
pub const GL_SRC_ALPHA: GLenum = 0x0302;
pub const GL_ONE_MINUS_SRC_ALPHA: GLenum = 0x0303;
pub const GL_DST_ALPHA: GLenum = 0x0304;
pub const GL_ONE_MINUS_DST_ALPHA: GLenum = 0x0305;
pub const GL_DST_COLOR: GLenum = 0x0306;
pub const GL_ONE_MINUS_DST_COLOR: GLenum = 0x0307;

// Blend equations
pub const GL_FUNC_ADD: GLenum = 0x8006;
pub const GL_FUNC_SUBTRACT: GLenum = 0x800A;
pub const GL_FUNC_REVERSE_SUBTRACT: GLenum = 0x800B;

// Depth / stencil
pub const GL_NEVER: GLenum = 0x0200;
pub const GL_LESS: GLenum = 0x0201;
pub const GL_EQUAL: GLenum = 0x0202;
pub const GL_LEQUAL: GLenum = 0x0203;
pub const GL_GREATER: GLenum = 0x0204;
pub const GL_NOTEQUAL: GLenum = 0x0205;
pub const GL_GEQUAL: GLenum = 0x0206;
pub const GL_ALWAYS: GLenum = 0x0207;
pub const GL_KEEP: GLenum = 0x1E00;
pub const GL_REPLACE: GLenum = 0x1E01;
pub const GL_INCR: GLenum = 0x1E02;
pub const GL_DECR: GLenum = 0x1E03;
pub const GL_INCR_WRAP: GLenum = 0x8507;
pub const GL_DECR_WRAP: GLenum = 0x8508;
pub const GL_INVERT: GLenum = 0x150A;

// Face culling
pub const GL_FRONT: GLenum = 0x0404;
pub const GL_BACK: GLenum = 0x0405;
pub const GL_FRONT_AND_BACK: GLenum = 0x0408;
pub const GL_CW: GLenum = 0x0900;
pub const GL_CCW: GLenum = 0x0901;

// Buffer targets / usage
pub const GL_ARRAY_BUFFER: GLenum = 0x8892;
pub const GL_ELEMENT_ARRAY_BUFFER: GLenum = 0x8893;
pub const GL_STATIC_DRAW: GLenum = 0x88E4;
pub const GL_DYNAMIC_DRAW: GLenum = 0x88E8;
pub const GL_STREAM_DRAW: GLenum = 0x88E0;

// Textures
pub const GL_TEXTURE_2D: GLenum = 0x0DE1;
pub const GL_TEXTURE_CUBE_MAP: GLenum = 0x8513;
pub const GL_TEXTURE_MIN_FILTER: GLenum = 0x2801;
pub const GL_TEXTURE_MAG_FILTER: GLenum = 0x2800;
pub const GL_TEXTURE_WRAP_S: GLenum = 0x2802;
pub const GL_TEXTURE_WRAP_T: GLenum = 0x2803;
pub const GL_NEAREST: GLenum = 0x2600;
pub const GL_LINEAR: GLenum = 0x2601;
pub const GL_NEAREST_MIPMAP_NEAREST: GLenum = 0x2700;
pub const GL_LINEAR_MIPMAP_NEAREST: GLenum = 0x2701;
pub const GL_NEAREST_MIPMAP_LINEAR: GLenum = 0x2702;
pub const GL_LINEAR_MIPMAP_LINEAR: GLenum = 0x2703;
pub const GL_CLAMP_TO_EDGE: GLenum = 0x812F;
pub const GL_REPEAT: GLenum = 0x2901;
pub const GL_MIRRORED_REPEAT: GLenum = 0x8370;
pub const GL_TEXTURE0: GLenum = 0x84C0;

// Pixel formats
pub const GL_ALPHA: GLenum = 0x1906;
pub const GL_RGB: GLenum = 0x1907;
pub const GL_RGBA: GLenum = 0x1908;
pub const GL_LUMINANCE: GLenum = 0x1909;
pub const GL_LUMINANCE_ALPHA: GLenum = 0x190A;
pub const GL_RED: GLenum = 0x1903;
pub const GL_RG: GLenum = 0x8227;
pub const GL_R8: GLenum = 0x8229;
pub const GL_RG8: GLenum = 0x822B;
pub const GL_RGB8: GLenum = 0x8051;
pub const GL_RGBA8: GLenum = 0x8058;

// Shaders / programs
pub const GL_VERTEX_SHADER: GLenum = 0x8B31;
pub const GL_FRAGMENT_SHADER: GLenum = 0x8B30;
pub const GL_COMPILE_STATUS: GLenum = 0x8B81;
pub const GL_LINK_STATUS: GLenum = 0x8B82;
pub const GL_VALIDATE_STATUS: GLenum = 0x8B83;
pub const GL_INFO_LOG_LENGTH: GLenum = 0x8B84;

// Framebuffers
pub const GL_FRAMEBUFFER: GLenum = 0x8D40;
pub const GL_READ_FRAMEBUFFER: GLenum = 0x8CA8;
pub const GL_DRAW_FRAMEBUFFER: GLenum = 0x8CA9;
pub const GL_RENDERBUFFER: GLenum = 0x8D41;
pub const GL_COLOR_ATTACHMENT0: GLenum = 0x8CE0;
pub const GL_DEPTH_ATTACHMENT: GLenum = 0x8D00;
pub const GL_STENCIL_ATTACHMENT: GLenum = 0x8D20;
pub const GL_DEPTH_STENCIL_ATTACHMENT: GLenum = 0x821A;
pub const GL_FRAMEBUFFER_COMPLETE: GLenum = 0x8CD5;
pub const GL_DEPTH_COMPONENT16: GLenum = 0x81A5;
pub const GL_DEPTH_COMPONENT24: GLenum = 0x81A6;
pub const GL_DEPTH24_STENCIL8: GLenum = 0x88F0;

// Strings / errors
pub const GL_VENDOR: GLenum = 0x1F00;
pub const GL_RENDERER: GLenum = 0x1F01;
pub const GL_VERSION: GLenum = 0x1F02;
pub const GL_NO_ERROR: GLenum = 0;

// ─── Imports ─────────────────────────────────────────────────────────────

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "gl")]
extern "C" {
    // State
    pub fn glEnable(cap: GLenum);
    pub fn glDisable(cap: GLenum);
    pub fn glGetError() -> GLenum;
    pub fn glFinish();
    pub fn glFlush();
    pub fn glHint(target: GLenum, mode: GLenum);
    pub fn glPixelStorei(pname: GLenum, param: GLint);
    pub fn glGetIntegerv(pname: GLenum, data: *mut GLint);
    pub fn glGetString(name: GLenum) -> *const u8;

    // Viewport / clear
    pub fn glViewport(x: GLint, y: GLint, w: GLsizei, h: GLsizei);
    pub fn glScissor(x: GLint, y: GLint, w: GLsizei, h: GLsizei);
    pub fn glClear(mask: GLbitfield);
    pub fn glClearColor(r: GLfloat, g: GLfloat, b: GLfloat, a: GLfloat);
    pub fn glClearDepthf(d: GLfloat);
    pub fn glClearStencil(s: GLint);

    // Blending
    pub fn glBlendFunc(sfactor: GLenum, dfactor: GLenum);
    pub fn glBlendFuncSeparate(srcRGB: GLenum, dstRGB: GLenum, srcA: GLenum, dstA: GLenum);
    pub fn glBlendEquation(mode: GLenum);
    pub fn glBlendEquationSeparate(modeRGB: GLenum, modeAlpha: GLenum);
    pub fn glBlendColor(r: GLfloat, g: GLfloat, b: GLfloat, a: GLfloat);
    pub fn glColorMask(r: GLboolean, g: GLboolean, b: GLboolean, a: GLboolean);

    // Depth / stencil
    pub fn glDepthFunc(func: GLenum);
    pub fn glDepthMask(flag: GLboolean);
    pub fn glDepthRangef(n: GLfloat, f: GLfloat);
    pub fn glStencilFunc(func: GLenum, reference: GLint, mask: GLuint);
    pub fn glStencilFuncSeparate(face: GLenum, func: GLenum, reference: GLint, mask: GLuint);
    pub fn glStencilOp(sfail: GLenum, zfail: GLenum, zpass: GLenum);
    pub fn glStencilOpSeparate(face: GLenum, sfail: GLenum, dpfail: GLenum, dppass: GLenum);
    pub fn glStencilMask(mask: GLuint);
    pub fn glStencilMaskSeparate(face: GLenum, mask: GLuint);

    // Culling
    pub fn glCullFace(mode: GLenum);
    pub fn glFrontFace(mode: GLenum);
    pub fn glPolygonOffset(factor: GLfloat, units: GLfloat);
    pub fn glLineWidth(width: GLfloat);

    // Buffers
    pub fn glGenBuffers(n: GLsizei, buffers: *mut GLuint);
    pub fn glDeleteBuffers(n: GLsizei, buffers: *const GLuint);
    pub fn glBindBuffer(target: GLenum, buffer: GLuint);
    pub fn glBufferData(
        target: GLenum,
        size: GLsizeiptr,
        data: *const core::ffi::c_void,
        usage: GLenum,
    );
    pub fn glBufferSubData(
        target: GLenum,
        offset: GLintptr,
        size: GLsizeiptr,
        data: *const core::ffi::c_void,
    );

    // Textures
    pub fn glGenTextures(n: GLsizei, textures: *mut GLuint);
    pub fn glDeleteTextures(n: GLsizei, textures: *const GLuint);
    pub fn glBindTexture(target: GLenum, texture: GLuint);
    pub fn glActiveTexture(texture: GLenum);
    #[allow(clippy::too_many_arguments)]
    pub fn glTexImage2D(
        target: GLenum,
        level: GLint,
        internalformat: GLint,
        width: GLsizei,
        height: GLsizei,
        border: GLint,
        format: GLenum,
        ty: GLenum,
        pixels: *const core::ffi::c_void,
    );
    #[allow(clippy::too_many_arguments)]
    pub fn glTexSubImage2D(
        target: GLenum,
        level: GLint,
        xoffset: GLint,
        yoffset: GLint,
        width: GLsizei,
        height: GLsizei,
        format: GLenum,
        ty: GLenum,
        pixels: *const core::ffi::c_void,
    );
    pub fn glTexParameteri(target: GLenum, pname: GLenum, param: GLint);
    pub fn glTexParameterf(target: GLenum, pname: GLenum, param: GLfloat);
    pub fn glGenerateMipmap(target: GLenum);
    #[allow(clippy::too_many_arguments)]
    pub fn glCompressedTexImage2D(
        target: GLenum,
        level: GLint,
        internalformat: GLenum,
        width: GLsizei,
        height: GLsizei,
        border: GLint,
        imageSize: GLsizei,
        data: *const core::ffi::c_void,
    );

    // Shaders
    pub fn glCreateShader(ty: GLenum) -> GLuint;
    pub fn glDeleteShader(shader: GLuint);
    pub fn glShaderSource(
        shader: GLuint,
        count: GLsizei,
        string: *const *const u8,
        length: *const GLint,
    );
    pub fn glCompileShader(shader: GLuint);
    pub fn glGetShaderiv(shader: GLuint, pname: GLenum, params: *mut GLint);
    pub fn glGetShaderInfoLog(
        shader: GLuint,
        bufSize: GLsizei,
        length: *mut GLsizei,
        infoLog: *mut u8,
    );

    // Programs
    pub fn glCreateProgram() -> GLuint;
    pub fn glDeleteProgram(program: GLuint);
    pub fn glAttachShader(program: GLuint, shader: GLuint);
    pub fn glDetachShader(program: GLuint, shader: GLuint);
    pub fn glLinkProgram(program: GLuint);
    pub fn glUseProgram(program: GLuint);
    pub fn glGetProgramiv(program: GLuint, pname: GLenum, params: *mut GLint);
    pub fn glGetProgramInfoLog(
        program: GLuint,
        bufSize: GLsizei,
        length: *mut GLsizei,
        infoLog: *mut u8,
    );
    pub fn glValidateProgram(program: GLuint);
    pub fn glBindAttribLocation(program: GLuint, index: GLuint, name: *const u8);
    pub fn glGetAttribLocation(program: GLuint, name: *const u8) -> GLint;
    pub fn glGetUniformLocation(program: GLuint, name: *const u8) -> GLint;
    #[allow(clippy::too_many_arguments)]
    pub fn glGetActiveAttrib(
        program: GLuint,
        index: GLuint,
        bufSize: GLsizei,
        length: *mut GLsizei,
        size: *mut GLint,
        ty: *mut GLenum,
        name: *mut u8,
    );
    #[allow(clippy::too_many_arguments)]
    pub fn glGetActiveUniform(
        program: GLuint,
        index: GLuint,
        bufSize: GLsizei,
        length: *mut GLsizei,
        size: *mut GLint,
        ty: *mut GLenum,
        name: *mut u8,
    );

    // Uniforms
    pub fn glUniform1i(location: GLint, v0: GLint);
    pub fn glUniform2i(location: GLint, v0: GLint, v1: GLint);
    pub fn glUniform3i(location: GLint, v0: GLint, v1: GLint, v2: GLint);
    pub fn glUniform4i(location: GLint, v0: GLint, v1: GLint, v2: GLint, v3: GLint);
    pub fn glUniform1f(location: GLint, v0: GLfloat);
    pub fn glUniform2f(location: GLint, v0: GLfloat, v1: GLfloat);
    pub fn glUniform3f(location: GLint, v0: GLfloat, v1: GLfloat, v2: GLfloat);
    pub fn glUniform4f(location: GLint, v0: GLfloat, v1: GLfloat, v2: GLfloat, v3: GLfloat);
    pub fn glUniform1iv(location: GLint, count: GLsizei, value: *const GLint);
    pub fn glUniform2iv(location: GLint, count: GLsizei, value: *const GLint);
    pub fn glUniform3iv(location: GLint, count: GLsizei, value: *const GLint);
    pub fn glUniform4iv(location: GLint, count: GLsizei, value: *const GLint);
    pub fn glUniform1fv(location: GLint, count: GLsizei, value: *const GLfloat);
    pub fn glUniform2fv(location: GLint, count: GLsizei, value: *const GLfloat);
    pub fn glUniform3fv(location: GLint, count: GLsizei, value: *const GLfloat);
    pub fn glUniform4fv(location: GLint, count: GLsizei, value: *const GLfloat);
    pub fn glUniformMatrix2fv(
        location: GLint,
        count: GLsizei,
        transpose: GLboolean,
        value: *const GLfloat,
    );
    pub fn glUniformMatrix3fv(
        location: GLint,
        count: GLsizei,
        transpose: GLboolean,
        value: *const GLfloat,
    );
    pub fn glUniformMatrix4fv(
        location: GLint,
        count: GLsizei,
        transpose: GLboolean,
        value: *const GLfloat,
    );

    // Vertex attribs
    pub fn glEnableVertexAttribArray(index: GLuint);
    pub fn glDisableVertexAttribArray(index: GLuint);
    pub fn glVertexAttribPointer(
        index: GLuint,
        size: GLint,
        ty: GLenum,
        normalized: GLboolean,
        stride: GLsizei,
        pointer: *const core::ffi::c_void,
    );

    // Drawing
    pub fn glDrawArrays(mode: GLenum, first: GLint, count: GLsizei);
    pub fn glDrawElements(
        mode: GLenum,
        count: GLsizei,
        ty: GLenum,
        indices: *const core::ffi::c_void,
    );

    // FBOs
    pub fn glGenFramebuffers(n: GLsizei, framebuffers: *mut GLuint);
    pub fn glDeleteFramebuffers(n: GLsizei, framebuffers: *const GLuint);
    pub fn glBindFramebuffer(target: GLenum, framebuffer: GLuint);
    pub fn glCheckFramebufferStatus(target: GLenum) -> GLenum;
    pub fn glFramebufferTexture2D(
        target: GLenum,
        attachment: GLenum,
        textarget: GLenum,
        texture: GLuint,
        level: GLint,
    );
    pub fn glFramebufferRenderbuffer(
        target: GLenum,
        attachment: GLenum,
        renderbuffertarget: GLenum,
        renderbuffer: GLuint,
    );

    // RBOs
    pub fn glGenRenderbuffers(n: GLsizei, renderbuffers: *mut GLuint);
    pub fn glDeleteRenderbuffers(n: GLsizei, renderbuffers: *const GLuint);
    pub fn glBindRenderbuffer(target: GLenum, renderbuffer: GLuint);
    pub fn glRenderbufferStorage(
        target: GLenum,
        internalformat: GLenum,
        width: GLsizei,
        height: GLsizei,
    );

    // Readback
    pub fn glReadPixels(
        x: GLint,
        y: GLint,
        width: GLsizei,
        height: GLsizei,
        format: GLenum,
        ty: GLenum,
        pixels: *mut core::ffi::c_void,
    );

    // ES3 / VAO
    pub fn glGenVertexArrays(n: GLsizei, arrays: *mut GLuint);
    pub fn glDeleteVertexArrays(n: GLsizei, arrays: *const GLuint);
    pub fn glBindVertexArray(array: GLuint);
    pub fn glDrawArraysInstanced(
        mode: GLenum,
        first: GLint,
        count: GLsizei,
        instancecount: GLsizei,
    );
    pub fn glDrawElementsInstanced(
        mode: GLenum,
        count: GLsizei,
        ty: GLenum,
        indices: *const core::ffi::c_void,
        instancecount: GLsizei,
    );
    pub fn glVertexAttribDivisor(index: GLuint, divisor: GLuint);
    pub fn glDrawBuffers(n: GLsizei, bufs: *const GLenum);

    // UBOs
    pub fn glBindBufferBase(target: GLenum, index: GLuint, buffer: GLuint);
    pub fn glBindBufferRange(
        target: GLenum,
        index: GLuint,
        buffer: GLuint,
        offset: GLintptr,
        size: GLsizeiptr,
    );
    pub fn glGetUniformBlockIndex(program: GLuint, uniformBlockName: *const u8) -> GLuint;
    pub fn glUniformBlockBinding(
        program: GLuint,
        uniformBlockIndex: GLuint,
        uniformBlockBinding: GLuint,
    );
    pub fn glTexStorage2D(
        target: GLenum,
        levels: GLsizei,
        internalformat: GLenum,
        width: GLsizei,
        height: GLsizei,
    );
}

// ─── Helpers ─────────────────────────────────────────────────────────────

/// Compile a shader from a NUL-terminated source string.
///
/// The host reads the source as a C string when no explicit length array is
/// given, so the trailing NUL is required. Write sources as
/// `b"#version 300 es\n...\0"` or use [`shader_source!`](crate::shader_source).
///
/// Returns the shader name. Compile errors are reported by the host on its own
/// console; check [`glGetShaderiv`] with [`GL_COMPILE_STATUS`] if the cart
/// needs to know.
///
/// # Safety
/// `src` must be NUL-terminated.
#[cfg(target_arch = "wasm32")]
pub unsafe fn compile_shader(ty: GLenum, src: &[u8]) -> GLuint {
    let s = glCreateShader(ty);
    let ptr = src.as_ptr();
    glShaderSource(s, 1, &ptr as *const *const u8, core::ptr::null());
    glCompileShader(s);
    s
}

/// Link a vertex and fragment shader source pair into a program.
///
/// Both sources must be NUL-terminated. The shaders are deleted after linking;
/// the program is left bound to nothing (call [`glUseProgram`] yourself).
///
/// # Safety
/// Both sources must be NUL-terminated.
#[cfg(target_arch = "wasm32")]
pub unsafe fn link_program(vert_src: &[u8], frag_src: &[u8]) -> GLuint {
    let vs = compile_shader(GL_VERTEX_SHADER, vert_src);
    let fs = compile_shader(GL_FRAGMENT_SHADER, frag_src);
    let p = glCreateProgram();
    glAttachShader(p, vs);
    glAttachShader(p, fs);
    glLinkProgram(p);
    glDeleteShader(vs);
    glDeleteShader(fs);
    p
}

/// A NUL-terminated GLSL source literal, ready for [`compile_shader`].
///
/// ```ignore
/// const VS: &[u8] = shader_source!(
///     "#version 300 es\n",
///     "void main() { gl_Position = vec4(0.0); }\n",
/// );
/// ```
#[macro_export]
macro_rules! shader_source {
    ($($line:expr),+ $(,)?) => {
        ::core::concat!($($line,)+ "\0").as_bytes()
    };
}
