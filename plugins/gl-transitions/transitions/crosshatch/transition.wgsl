// License: MIT
// Author: pthrasher
// Description: The incoming image appears through a crosshatch of lines spreading from the center
// Tags: pattern, lines, noise
// adapted by gre from https://gist.github.com/pthrasher/04fd9a7de4012cbb03f6
//
// @param Point the lines spread from
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

fn rand(co: vec2<f32>) -> f32 {
    var co_1: vec2<f32>;

    co_1 = co;
    let _e7 = co_1;
    return fract((sin(dot(_e7.xy, vec2<f32>(12.9898f, 78.233f))) * 43758.547f));
}

fn glt_main(glt_p: vec2<f32>) -> vec4<f32> {
    var p_1: vec2<f32>;
    var dist: f32;
    var r: f32;

    p_1 = glt_p;
    let _e7 = p.center;
    let _e8 = p_1;
    let _e10 = p.threshold;
    dist = (distance(_e7, _e8) / _e10);
    let _e13 = u.progress;
    let _e14 = p_1;
    let _e18 = rand(vec2<f32>(_e14.y, 0f));
    let _e20 = p_1;
    let _e23 = rand(vec2<f32>(0f, _e20.x));
    r = (_e13 - min(_e18, _e23));
    let _e27 = p_1;
    let _e28 = getFromColor(_e27);
    let _e29 = p_1;
    let _e30 = getToColor(_e29);
    let _e32 = dist;
    let _e33 = r;
    let _e37 = p.fade_edge;
    let _e40 = u.progress;
    let _e44 = p.fade_edge;
    let _e45 = u.progress;
    return mix(_e28, _e30, vec4(mix(0f, mix(step(_e32, _e33), 1f, smoothstep((1f - _e37), 1f, _e40)), smoothstep(0f, _e44, _e45))));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
