// Author: pschroen
// License: MIT
// Description: A diagonal wipe with a warped, smeared edge
// Tags: wipe, warp, directional
//
// @param Softness of the edge
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
    let _e35 = p.smoothness;
    let _e38 = v;
    let _e40 = uv_5;
    let _e43 = v;
    let _e45 = uv_5;
    let _e49 = d;
    let _e52 = u.progress;
    let _e54 = p.smoothness;
    m = (1f - smoothstep(-(_e35), 0f, (((_e38.x * _e40.x) + (_e43.y * _e45.y)) - ((_e49 - 0.5f) + (_e52 * (1f + _e54))))));
    let _e62 = uv_5;
    let _e67 = m;
    let _e73 = getFromColor((((_e62 - vec2(0.5f)) * (1f - _e67)) + vec2(0.5f)));
    let _e74 = uv_5;
    let _e78 = m;
    let _e83 = getToColor((((_e74 - vec2(0.5f)) * _e78) + vec2(0.5f)));
    let _e84 = m;
    return mix(_e73, _e83, vec4(_e84));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
