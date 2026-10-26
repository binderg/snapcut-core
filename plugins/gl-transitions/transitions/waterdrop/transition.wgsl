// Author: Paweł Płóciennik
// License: MIT
// Description: Ripples spread from the center like a drop falling into water, crossfading to the incoming image
// Tags: ripple, water, blend
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

fn glt_main(glt_p: vec2<f32>) -> vec4<f32> {
    var p_1: vec2<f32>;
    var dir: vec2<f32>;
    var dist: f32;
    var offset: vec2<f32>;

    p_1 = glt_p;
    let _e6 = p_1;
    dir = (_e6 - vec2(0.5f));
    let _e11 = dir;
    dist = length(_e11);
    let _e14 = dist;
    let _e15 = u.progress;
    if (_e14 > _e15) {
        {
            let _e17 = p_1;
            let _e18 = getFromColor(_e17);
            let _e19 = p_1;
            let _e20 = getToColor(_e19);
            let _e21 = u.progress;
            return mix(_e18, _e20, vec4(_e21));
        }
    } else {
        {
            let _e24 = dir;
            let _e25 = dist;
            let _e26 = p.amplitude;
            let _e28 = u.progress;
            let _e29 = p.speed;
            offset = (_e24 * sin(((_e25 * _e26) - (_e28 * _e29))));
            let _e35 = p_1;
            let _e36 = offset;
            let _e38 = getFromColor((_e35 + _e36));
            let _e39 = p_1;
            let _e40 = getToColor(_e39);
            let _e41 = u.progress;
            return mix(_e38, _e40, vec4(_e41));
        }
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
