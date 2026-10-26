// Author: gre
// License: MIT
// Description: Both images pixelate, crossfade, then sharpen again
// Tags: pixelate, blend
// forked from https://gist.github.com/benraziel/c528607361d90a072e98
//
// minimum number of squares (when the effect is at its higher level)
// @param Number of squares at the strongest pixelation
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
    var d: f32;
    var local: f32;
    var dist: f32;
    var squareSize: vec2<f32>;
    var local_1: vec2<f32>;
    var glt_p: vec2<f32>;

    uv_5 = uv_4;
    let _e6 = u.progress;
    let _e8 = u.progress;
    d = min(_e6, (1f - _e8));
    let _e12 = i32(p.steps);
    if (_e12 > 0i) {
        let _e15 = d;
        let _e16 = i32(p.steps);
        let _e20 = i32(p.steps);
        local = (ceil((_e15 * f32(_e16))) / f32(_e20));
    } else {
        let _e23 = d;
        local = _e23;
    }
    let _e25 = local;
    dist = _e25;
    let _e28 = dist;
    let _e30 = vec2<i32>(p.squares_min);
    squareSize = (vec2((2f * _e28)) / vec2<f32>(_e30));
    let _e35 = dist;
    if (_e35 > 0f) {
        let _e38 = uv_5;
        let _e39 = squareSize;
        let _e45 = squareSize;
        local_1 = ((floor((_e38 / _e39)) + vec2(0.5f)) * _e45);
    } else {
        let _e47 = uv_5;
        local_1 = _e47;
    }
    let _e49 = local_1;
    glt_p = _e49;
    let _e51 = glt_p;
    let _e52 = getFromColor(_e51);
    let _e53 = glt_p;
    let _e54 = getToColor(_e53);
    let _e55 = u.progress;
    return mix(_e52, _e54, vec4(_e55));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
