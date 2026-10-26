// Author: Thibaut Foussard
// based on Directional transition by Gaëtan Renaudeau
// https://gl-transitions.com/editor/Directional
// License: MIT
// Description: Like Directional, with both images scaled down while they slide
// Tags: slide, push, zoom, directional
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

fn parabola(x: f32) -> f32 {
    var x_1: f32;
    var y: f32;

    x_1 = x;
    let _e6 = x_1;
    y = pow(sin((_e6 * 3.1415927f)), 1f);
    let _e15 = y;
    return _e15;
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var easedProgress: f32;
    var glt_p: vec2<f32>;
    var f: vec2<f32>;
    var s: f32;
    var mixer: f32;
    var col: vec4<f32>;
    var border: f32;

    uv_5 = uv_4;
    let _e6 = u.progress;
    easedProgress = pow(sin(((_e6 * 3.1415927f) / 2f)), 3f);
    let _e17 = uv_5;
    let _e18 = easedProgress;
    let _e19 = p.direction;
    glt_p = (_e17 + (_e18 * sign(_e19)));
    let _e24 = glt_p;
    f = fract(_e24);
    let _e30 = p.scale;
    let _e33 = u.progress;
    let _e34 = parabola(_e33);
    s = (1f - ((1f - (1f / _e30)) * _e34));
    let _e38 = f;
    let _e42 = s;
    f = (((_e38 - vec2(0.5f)) * _e42) + vec2(0.5f));
    let _e48 = glt_p;
    let _e51 = glt_p;
    let _e57 = glt_p;
    let _e61 = glt_p;
    mixer = (((step(0f, _e48.y) * step(_e51.y, 1f)) * step(0f, _e57.x)) * step(_e61.x, 1f));
    let _e67 = f;
    let _e68 = getToColor(_e67);
    let _e69 = f;
    let _e70 = getFromColor(_e69);
    let _e71 = mixer;
    col = mix(_e68, _e70, vec4(_e71));
    let _e76 = f;
    let _e81 = f;
    let _e87 = f;
    let _e93 = f;
    border = (((step(0f, _e76.x) * step(0f, (1f - _e81.x))) * step(0f, _e87.y)) * step(0f, (1f - _e93.y)));
    let _e99 = col;
    let _e100 = border;
    col = (_e99 * _e100);
    let _e102 = col;
    return _e102;
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
