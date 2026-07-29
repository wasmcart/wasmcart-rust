//! hello_gl: a GL cart. Draws a spinning RGB triangle over a dark blue clear.
//!
//! The triangle rotates so a screenshot proves the frame loop is live, and the
//! clear colour differs from every vertex colour so a screenshot also proves
//! the draw call landed rather than just the clear.
//!
//! Build:
//!   cargo build --release --target wasm32-unknown-unknown
//!   wasmcart pack --wasm target/wasm32-unknown-unknown/release/hello_gl.wasm \
//!     --name hello_gl -o hello_gl.wasc
//!   wasmcart hello_gl.wasc --gl
//!
//! The bundled terminal player cannot show a GL cart; run it with a real
//! window (`--gl`) or through a host that supplies a GL context.

#![no_std]

use wasmcart::gl::*;
use wasmcart::*;

const W: u32 = 640;
const H: u32 = 480;

// gpu_api MUST agree with the import section: the host detects GL from the
// wasm imports before instantiation and treats gpu_api as authoritative after
// wc_get_info. Declaring one without the other is the standard GL cart bug.
wc_cart!(W, H, gpu_api: WC_GPU_API_WEBGL2);

const VS_SRC: &[u8] = shader_source!(
    "#version 300 es\n",
    "layout(location=0) in vec2 a_pos;\n",
    "layout(location=1) in vec3 a_col;\n",
    "uniform float u_angle;\n",
    "out vec3 v_col;\n",
    "void main() {\n",
    "  float c = cos(u_angle), s = sin(u_angle);\n",
    "  vec2 p = vec2(a_pos.x * c - a_pos.y * s, a_pos.x * s + a_pos.y * c);\n",
    "  gl_Position = vec4(p, 0.0, 1.0);\n",
    "  v_col = a_col;\n",
    "}\n",
);

const FS_SRC: &[u8] = shader_source!(
    "#version 300 es\n",
    "precision mediump float;\n",
    "in vec3 v_col;\n",
    "out vec4 o_col;\n",
    "void main() { o_col = vec4(v_col, 1.0); }\n",
);

// x, y, r, g, b per vertex.
#[rustfmt::skip]
static VERTS: [f32; 15] = [
     0.0,  0.75,  1.0, 0.2, 0.2,
    -0.75, -0.6,  0.2, 1.0, 0.2,
     0.75, -0.6,  0.2, 0.2, 1.0,
];

static mut PROGRAM: GLuint = 0;
static mut VBO: GLuint = 0;
static mut U_ANGLE: GLint = -1;

#[no_mangle]
pub extern "C" fn wc_init() {
    wc_log("hello_gl: rust GL cart init");
    unsafe {
        let prog = link_program(VS_SRC, FS_SRC);
        PROGRAM = prog;
        U_ANGLE = glGetUniformLocation(prog, b"u_angle\0".as_ptr());

        // A VAO is mandatory in ES 3.0 core: without one bound, every draw is
        // an invalid operation and the screen shows only the clear colour.
        let mut vao: GLuint = 0;
        glGenVertexArrays(1, &mut vao);
        glBindVertexArray(vao);

        let mut vbo: GLuint = 0;
        glGenBuffers(1, &mut vbo);
        VBO = vbo;
        glBindBuffer(GL_ARRAY_BUFFER, vbo);
        glBufferData(
            GL_ARRAY_BUFFER,
            core::mem::size_of_val(&VERTS) as GLsizeiptr,
            core::ptr::addr_of!(VERTS) as *const core::ffi::c_void,
            GL_STATIC_DRAW,
        );

        let stride = (5 * core::mem::size_of::<f32>()) as GLsizei;
        glEnableVertexAttribArray(0);
        glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, stride, core::ptr::null());
        glEnableVertexAttribArray(1);
        glVertexAttribPointer(
            1,
            3,
            GL_FLOAT,
            GL_FALSE,
            stride,
            (2 * core::mem::size_of::<f32>()) as *const core::ffi::c_void,
        );
    }
}

#[no_mangle]
pub extern "C" fn wc_render() {
    let angle = time().frame as f32 * 0.05;
    unsafe {
        glViewport(0, 0, W as GLsizei, H as GLsizei);
        glClearColor(0.05, 0.07, 0.15, 1.0);
        glClear(GL_COLOR_BUFFER_BIT);
        glUseProgram(PROGRAM);
        if U_ANGLE >= 0 {
            glUniform1f(U_ANGLE, angle);
        }
        glDrawArrays(GL_TRIANGLES, 0, 3);
    }
}
