// Author: martiniti
// License: MIT
// Description: The incoming image opens from a line in the middle toward the top and bottom edges
// Tags: wipe, split
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

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var regress: f32;
    var s: f32;

    uv_5 = uv_4;
    let _e5 = u.progress;
    regress = (1f - _e5);
    let _e9 = uv_5;
    let _e13 = regress;
    let _e20 = regress;
    s = ((2f - abs(((_e9.y - 0.5f) / (_e13 - 1f)))) - (2f * _e20));
    let _e24 = uv_5;
    let _e25 = getFromColor(_e24);
    let _e26 = uv_5;
    let _e27 = getToColor(_e26);
    let _e30 = s;
    return mix(_e25, _e27, vec4(smoothstep(0f, 0.5f, _e30)));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
