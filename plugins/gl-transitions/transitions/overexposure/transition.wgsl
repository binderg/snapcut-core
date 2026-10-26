// Author: Ben Zhang
// License: MIT
// Description: A crossfade through a washed-out, overexposed white
// Tags: flash, light, fade
//
// @param Amount of overexposure
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

const PI: f32 = 3.1415927f;

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var from_: vec4<f32>;
    var to: vec4<f32>;
    var from_m: f32;
    var to_m: f32;

    uv_5 = uv_4;
    let _e6 = uv_5;
    let _e7 = getFromColor(_e6);
    from_ = _e7;
    let _e9 = uv_5;
    let _e10 = getToColor(_e9);
    to = _e10;
    let _e13 = u.progress;
    let _e15 = u.progress;
    let _e18 = p.strength;
    from_m = ((1f - _e13) + (sin((PI * _e15)) * _e18));
    let _e22 = u.progress;
    let _e23 = u.progress;
    let _e26 = p.strength;
    to_m = (_e22 + (sin((PI * _e23)) * _e26));
    let _e30 = from_;
    let _e32 = from_;
    let _e35 = from_m;
    let _e37 = to;
    let _e39 = to;
    let _e42 = to_m;
    let _e45 = from_;
    let _e47 = from_;
    let _e50 = from_m;
    let _e52 = to;
    let _e54 = to;
    let _e57 = to_m;
    let _e60 = from_;
    let _e62 = from_;
    let _e65 = from_m;
    let _e67 = to;
    let _e69 = to;
    let _e72 = to_m;
    let _e75 = from_;
    let _e77 = to;
    let _e79 = u.progress;
    return vec4<f32>((((_e30.x * _e32.w) * _e35) + ((_e37.x * _e39.w) * _e42)), (((_e45.y * _e47.w) * _e50) + ((_e52.y * _e54.w) * _e57)), (((_e60.z * _e62.w) * _e65) + ((_e67.z * _e69.w) * _e72)), mix(_e75.w, _e77.w, _e79));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
