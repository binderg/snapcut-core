// Author: paniq
// License: MIT
// Description: A crossfade in which each image is displaced by the other's colors, like a morph
// Tags: morph, distort, blend
// @param Strength of the displacement
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
    var ca: vec4<f32>;
    var cb: vec4<f32>;
    var oa: vec2<f32>;
    var ob: vec2<f32>;
    var oc: vec2<f32>;
    var w0_: f32;
    var w1_: f32;

    p_1 = glt_p;
    let _e5 = p_1;
    let _e6 = getFromColor(_e5);
    ca = _e6;
    let _e8 = p_1;
    let _e9 = getToColor(_e8);
    cb = _e9;
    let _e11 = ca;
    let _e13 = ca;
    oa = ((((_e11.xy + vec2(_e13.z)) * 0.5f) * 2f) - vec2(1f));
    let _e25 = cb;
    let _e27 = cb;
    ob = ((((_e25.xy + vec2(_e27.z)) * 0.5f) * 2f) - vec2(1f));
    let _e39 = oa;
    let _e40 = ob;
    let _e44 = p.strength;
    oc = (mix(_e39, _e40, vec2(0.5f)) * _e44);
    let _e47 = u.progress;
    w0_ = _e47;
    let _e50 = w0_;
    w1_ = (1f - _e50);
    let _e53 = p_1;
    let _e54 = oc;
    let _e55 = w0_;
    let _e58 = getFromColor((_e53 + (_e54 * _e55)));
    let _e59 = p_1;
    let _e60 = oc;
    let _e61 = w1_;
    let _e64 = getToColor((_e59 - (_e60 * _e61)));
    let _e65 = u.progress;
    return mix(_e58, _e64, vec4(_e65));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
