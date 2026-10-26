// Author: gre
// License: MIT
// Description: The image breaks into a fly's-eye grid of lenses with color separation
// Tags: lens, distort, chromatic
//
// Converted from the GL Transitions GLSL to WGSL for the ve engine with naga
// (snapcut-pro scripts/import_gl_transitions.py). Original header above.

// Engine colors are linear light, premultiplied; GL Transitions work on straight sRGB.
fn glt_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let x = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    return select(1.055 * pow(x, vec3<f32>(1.0 / 2.4)) - 0.055, x * 12.92, x <= vec3<f32>(0.0031308));
}
fn glt_to_linear(c: vec3<f32>) -> vec3<f32> {
    let x = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    return select(pow((x + 0.055) / 1.055, vec3<f32>(2.4)), x / 12.92, x <= vec3<f32>(0.04045));
}
fn glt_dec(c: vec4<f32>) -> vec4<f32> {
    let a = clamp(c.a, 0.0, 1.0);
    return vec4<f32>(glt_to_srgb(c.rgb / max(a, 1e-6)), a);
}
fn glt_enc(c: vec4<f32>) -> vec4<f32> {
    let a = clamp(c.a, 0.0, 1.0);
    return vec4<f32>(glt_to_linear(c.rgb) * a, a);
}
// GL Transitions put uv (0, 0) at the bottom left, the engine at the top left.
fn glt_flip(uv: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(uv.x, 1.0 - uv.y);
}
fn getFromColor(uv: vec2<f32>) -> vec4<f32> {
    return glt_dec(outgoing(glt_flip(uv)));
}
fn getToColor(uv: vec2<f32>) -> vec4<f32> {
    return glt_dec(incoming(glt_flip(uv)));
}

fn glt_main(glt_p: vec2<f32>) -> vec4<f32> {
    var p_1: vec2<f32>;
    var inv: f32;
    var disp: vec2<f32>;
    var texTo: vec4<f32>;
    var texFrom: vec4<f32>;

    p_1 = glt_p;
    let _e8 = u.progress;
    inv = (1f - _e8);
    let _e11 = p.size;
    let _e12 = p.zoom;
    let _e13 = p_1;
    let _e17 = p.zoom;
    let _e18 = p_1;
    disp = (_e11 * vec2<f32>(cos((_e12 * _e13.x)), sin((_e17 * _e18.y))));
    let _e25 = p_1;
    let _e26 = inv;
    let _e27 = disp;
    let _e30 = getToColor((_e25 + (_e26 * _e27)));
    texTo = _e30;
    let _e32 = p_1;
    let _e33 = u.progress;
    let _e34 = disp;
    let _e37 = p.color_separation;
    let _e41 = getFromColor((_e32 + ((_e33 * _e34) * (1f - _e37))));
    let _e43 = p_1;
    let _e44 = u.progress;
    let _e45 = disp;
    let _e48 = getFromColor((_e43 + (_e44 * _e45)));
    let _e50 = p_1;
    let _e51 = u.progress;
    let _e52 = disp;
    let _e55 = p.color_separation;
    let _e59 = getFromColor((_e50 + ((_e51 * _e52) * (1f + _e55))));
    texFrom = vec4<f32>(_e41.x, _e48.y, _e59.z, 1f);
    let _e64 = texTo;
    let _e65 = u.progress;
    let _e67 = texFrom;
    let _e68 = inv;
    return ((_e64 * _e65) + (_e67 * _e68));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
