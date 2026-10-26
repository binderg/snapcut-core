// Author: gre
// License: MIT
// Description: A soft-edged diagonal wipe
// Tags: wipe, directional
//
// @param Direction of the wipe
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

const center: vec2<f32> = vec2<f32>(0.5f, 0.5f);

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var v: vec2<f32>;
    var d: f32;
    var m: f32;

    uv_5 = uv_4;
    let _e7 = p.direction;
    v = normalize(_e7);
    let _e10 = v;
    let _e11 = v;
    let _e14 = v;
    v = (_e10 / vec2((abs(_e11.x) + abs(_e14.y))));
    let _e20 = v;
    let _e26 = v;
    d = ((_e20.x * 0.5f) + (_e26.y * 0.5f));
    let _e35 = u.progress;
    let _e40 = p.smoothness;
    let _e43 = v;
    let _e45 = uv_5;
    let _e48 = v;
    let _e50 = uv_5;
    let _e54 = d;
    let _e57 = u.progress;
    let _e59 = p.smoothness;
    m = ((1f - step(_e35, 0f)) * (1f - smoothstep(-(_e40), 0f, (((_e43.x * _e45.x) + (_e48.y * _e50.y)) - ((_e54 - 0.5f) + (_e57 * (1f + _e59)))))));
    let _e68 = uv_5;
    let _e69 = getFromColor(_e68);
    let _e70 = uv_5;
    let _e71 = getToColor(_e70);
    let _e72 = m;
    return mix(_e69, _e71, vec4(_e72));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
