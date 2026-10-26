// Author: gre
// License: MIT
// Description: A crossfade through grayscale
// Tags: fade, grayscale, color
//
// @range(0, 1) How long the grayscale phase lasts
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

fn grayscale(color: vec3<f32>) -> vec3<f32> {
    var color_1: vec3<f32>;

    color_1 = color;
    let _e6 = color_1;
    let _e10 = color_1;
    let _e15 = color_1;
    return vec3((((0.2126f * _e6.x) + (0.7152f * _e10.y)) + (0.0722f * _e15.z)));
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var fc: vec4<f32>;
    var tc: vec4<f32>;

    uv_5 = uv_4;
    let _e5 = uv_5;
    let _e6 = getFromColor(_e5);
    fc = _e6;
    let _e8 = uv_5;
    let _e9 = getToColor(_e8);
    tc = _e9;
    let _e11 = fc;
    let _e13 = grayscale(_e11.xyz);
    let _e19 = fc;
    let _e21 = p.intensity;
    let _e24 = u.progress;
    let _e28 = tc;
    let _e30 = grayscale(_e28.xyz);
    let _e36 = tc;
    let _e37 = p.intensity;
    let _e39 = u.progress;
    let _e43 = u.progress;
    return mix(mix(vec4<f32>(_e13.x, _e13.y, _e13.z, 1f), _e19, vec4(smoothstep((1f - _e21), 0f, _e24))), mix(vec4<f32>(_e30.x, _e30.y, _e30.z, 1f), _e36, vec4(smoothstep(_e37, 1f, _e39))), vec4(_e43));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
