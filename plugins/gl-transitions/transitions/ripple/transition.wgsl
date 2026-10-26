// Author: gre
// License: MIT
// Description: Ripples spread from the center while crossfading
// Tags: ripple, wave, blend
// @param Frequency of the ripples
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
    var dir: vec2<f32>;
    var dist: f32;
    var offset: vec2<f32>;

    uv_5 = uv_4;
    let _e6 = uv_5;
    dir = (_e6 - vec2(0.5f));
    let _e11 = dir;
    dist = length(_e11);
    let _e14 = dir;
    let _e15 = u.progress;
    let _e16 = dist;
    let _e18 = p.amplitude;
    let _e20 = u.progress;
    let _e21 = p.speed;
    let _e31 = u.progress;
    offset = (((_e14 * (sin((((_e15 * _e16) * _e18) - (_e20 * _e21))) + 0.5f)) / vec2(30f)) * _e31);
    let _e34 = uv_5;
    let _e35 = offset;
    let _e37 = getFromColor((_e34 + _e35));
    let _e38 = uv_5;
    let _e39 = getToColor(_e38);
    let _e42 = u.progress;
    return mix(_e37, _e39, vec4(smoothstep(0.2f, 1f, _e42)));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
