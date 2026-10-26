// Author: mikolalysenko
// License: MIT
// Description: A crossfade with a gentle wavy distortion
// Tags: wave, blend
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

fn offset(progress: f32, x: f32, theta: f32) -> vec2<f32> {
    var progress_1: f32;
    var x_1: f32;
    var theta_1: f32;
    var phase: f32;
    var shifty: f32;

    progress_1 = progress;
    x_1 = x;
    theta_1 = theta;
    let _e8 = progress_1;
    let _e9 = progress_1;
    let _e11 = progress_1;
    let _e13 = theta_1;
    phase = (((_e8 * _e9) + _e11) + _e13);
    let _e17 = progress_1;
    let _e20 = progress_1;
    let _e21 = x_1;
    shifty = ((0.03f * _e17) * cos((10f * (_e20 + _e21))));
    let _e28 = shifty;
    return vec2<f32>(0f, _e28);
}

fn glt_main(glt_p: vec2<f32>) -> vec4<f32> {
    var p_1: vec2<f32>;

    p_1 = glt_p;
    let _e4 = p_1;
    let _e5 = u.progress;
    let _e6 = p_1;
    let _e9 = offset(_e5, _e6.x, 0f);
    let _e11 = getFromColor((_e4 + _e9));
    let _e12 = p_1;
    let _e14 = u.progress;
    let _e16 = p_1;
    let _e19 = offset((1f - _e14), _e16.x, 3.14f);
    let _e21 = getToColor((_e12 + _e19));
    let _e22 = u.progress;
    return mix(_e11, _e21, vec4(_e22));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
