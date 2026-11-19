//! What a compatibility-mode device lacks, filled in where it can be exactly.
//!
//! WebGPU compatibility mode (the wasmcart tier's floor; SPEC.md, "Feature
//! level") has no cube-array views and only `@interpolate(flat, either)`.
//! Engines written for core WebGPU use both unconditionally (Bevy's point
//! light shadows are a `texture_depth_cube_array`, and its mesh shaders pass
//! the instance index `@interpolate(flat)`), so on a compat device the
//! backend translates:
//!
//! - A cube array holding ONE cube is a cube: its views and bind group
//!   layout entries become `cube`, and in WGSL `texture_cube_array<T>` /
//!   `texture_depth_cube_array` become `texture_cube<T>` / `texture_depth_cube`,
//!   the array index argument of every sampling call on such a texture is
//!   dropped (with one cube it can only be 0) and `textureNumLayers` of it
//!   is `1u`. A cube array of more than one cube cannot be expressed and
//!   still fails validation, as it should.
//! - `@interpolate(flat)` (whose default sampling, `first`, compat lacks)
//!   becomes `@interpolate(flat, either)`: the value is the same in every
//!   vertex of a primitive for the inputs engines mark flat (indices).

use alloc::string::String;
use alloc::vec::Vec;

/// The WGSL `code` as a compatibility-mode device accepts it, or `None` if
/// it needs no change.
pub fn translate_wgsl(code: &str) -> Option<String> {
    let has_cube_array = code.contains("cube_array");
    let has_flat = code.contains("flat");
    if !has_cube_array && !has_flat {
        return None;
    }
    let mut out = String::from(code);
    if has_flat {
        out = fix_flat(&out);
    }
    if has_cube_array {
        out = fix_cube_arrays(&out);
    }
    if out == code {
        None
    } else {
        Some(out)
    }
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// `@interpolate(flat)` -> `@interpolate(flat, either)`.
fn fix_flat(code: &str) -> String {
    let b = code.as_bytes();
    let mut out = String::with_capacity(code.len() + 64);
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'@' && code[i..].starts_with("@interpolate") {
            let mut j = i + "@interpolate".len();
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < b.len() && b[j] == b'(' {
                if let Some(close) = code[j..].find(')') {
                    let inner = code[j + 1..j + close].trim();
                    if inner == "flat" {
                        out.push_str("@interpolate(flat, either)");
                        i = j + close + 1;
                        continue;
                    }
                }
            }
        }
        // copy one UTF-8 character
        let ch = code[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Names declared with a cube-array texture type: `name: texture_cube_array<..>`
/// or `name: texture_depth_cube_array` (module vars and function parameters).
fn cube_array_names(code: &str) -> Vec<String> {
    let mut names = Vec::new();
    let b = code.as_bytes();
    for pat in ["texture_depth_cube_array", "texture_cube_array"] {
        let mut from = 0;
        while let Some(p) = code[from..].find(pat) {
            let at = from + p;
            from = at + pat.len();
            // walk back over whitespace and ':' to the declared name
            let mut j = at;
            while j > 0 && b[j - 1].is_ascii_whitespace() {
                j -= 1;
            }
            if j == 0 || b[j - 1] != b':' {
                continue;
            }
            j -= 1;
            while j > 0 && b[j - 1].is_ascii_whitespace() {
                j -= 1;
            }
            let end = j;
            while j > 0 && is_ident(b[j - 1]) {
                j -= 1;
            }
            if j < end {
                let n = String::from(&code[j..end]);
                if !names.contains(&n) {
                    names.push(n);
                }
            }
        }
    }
    names
}

/// Builtins taking a cube-array texture and an array index right after the
/// coordinates (texture, sampler, coords, index, ...).
const INDEXED: &[&str] = &[
    "textureSample",
    "textureSampleBias",
    "textureSampleLevel",
    "textureSampleGrad",
    "textureSampleCompare",
    "textureSampleCompareLevel",
    "textureGather",
    "textureGatherCompare",
];

/// Split a call's argument list (text between the parentheses) at top-level
/// commas, keeping each argument's text.
fn split_args(s: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                args.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    if !s[start..].trim().is_empty() || !args.is_empty() {
        args.push(&s[start..]);
    }
    args
}

/// Index of the ')' closing the '(' at `open`.
fn matching_paren(code: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in code[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + i);
                }
            }
            _ => {}
        }
    }
    None
}

fn fix_cube_arrays(code: &str) -> String {
    let names = cube_array_names(code);
    let mut s = String::from(code);
    if !names.is_empty() {
        s = rewrite_calls(&s, &names);
    }
    s.replace("texture_depth_cube_array", "texture_depth_cube")
        .replace("texture_cube_array", "texture_cube")
}

fn rewrite_calls(code: &str, names: &[String]) -> String {
    let b = code.as_bytes();
    let mut out = String::with_capacity(code.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b't' && (i == 0 || !is_ident(b[i - 1])) && code[i..].starts_with("texture") {
            let mut j = i;
            while j < b.len() && is_ident(b[j]) {
                j += 1;
            }
            let func = &code[i..j];
            let mut k = j;
            while k < b.len() && b[k].is_ascii_whitespace() {
                k += 1;
            }
            let indexed = INDEXED.contains(&func);
            if (indexed || func == "textureNumLayers") && k < b.len() && b[k] == b'(' {
                if let Some(close) = matching_paren(code, k) {
                    let inner = &code[k + 1..close];
                    let args = split_args(inner);
                    // the texture is the first argument, or the second for
                    // textureGather's component form
                    let tex_at = args.iter().position(|a| names.iter().any(|n| a.trim() == n));
                    if let Some(t) = tex_at {
                        if func == "textureNumLayers" {
                            out.push_str("1u");
                            i = close + 1;
                            continue;
                        }
                        let idx = t + 3;
                        if t <= 1 && idx < args.len() {
                            out.push_str(func);
                            out.push('(');
                            let kept: Vec<String> = args
                                .iter()
                                .enumerate()
                                .filter(|(n, _)| *n != idx)
                                .map(|(_, a)| rewrite_calls(a, names))
                                .collect();
                            out.push_str(&kept.join(","));
                            out.push(')');
                            i = close + 1;
                            continue;
                        }
                    }
                }
            }
            out.push_str(func);
            i = j;
            continue;
        }
        let ch = code[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_gets_either() {
        let s = "struct V { @location(3) @interpolate(flat) id: u32, @location(4) @interpolate(flat, either) b: u32 }";
        let t = translate_wgsl(s).unwrap();
        assert_eq!(t, "struct V { @location(3) @interpolate(flat, either) id: u32, @location(4) @interpolate(flat, either) b: u32 }");
    }

    #[test]
    fn cube_array_becomes_cube() {
        let s = "@group(0) @binding(2) var point_shadow_textures: texture_depth_cube_array;\n\
                 fn f(v: vec3f, i: u32, d: f32) -> f32 { return textureSampleCompareLevel(point_shadow_textures, samp, v * vec3(1.0, 1.0, -1.0), i32(i), d); }\n\
                 fn g() -> u32 { return textureNumLayers(point_shadow_textures) + textureNumLayers(other); }\n\
                 fn h(t: texture_cube_array<f32>, s: sampler) -> vec4f { return textureSampleLevel(t, s, vec3f(0.0), 0, 0.0); }\n\
                 fn k() -> f32 { return textureSample(plain, samp, vec2f(0.5)).r; }";
        let t = translate_wgsl(s).unwrap();
        assert!(t.contains("var point_shadow_textures: texture_depth_cube;"));
        assert!(t.contains("textureSampleCompareLevel(point_shadow_textures, samp, v * vec3(1.0, 1.0, -1.0), d)"));
        assert!(t.contains("return 1u + textureNumLayers(other);"));
        assert!(t.contains("fn h(t: texture_cube<f32>, s: sampler)"));
        assert!(t.contains("textureSampleLevel(t, s, vec3f(0.0), 0.0)"));
        assert!(t.contains("textureSample(plain, samp, vec2f(0.5))"));
    }

    #[test]
    fn untouched_without_either() {
        assert!(translate_wgsl("fn main() {}").is_none());
    }
}
