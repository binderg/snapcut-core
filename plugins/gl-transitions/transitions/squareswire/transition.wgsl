// Author: gre
// License: MIT
// Description: Squares grow along a diagonal sweep to reveal the incoming image
// Tags: squares, grid, wipe, directional
//
// @param Number of squares (columns, rows)
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

fn glt_main(glt_p: vec2<f32>) -> vec4<f32> {
    var p_1: vec2<f32>;
    var v: vec2<f32>;
    var d: f32;
    var offset: f32;
    var pr: f32;
    var squarep: vec2<f32>;
    var squaremin: vec2<f32>;
    var squaremax: vec2<f32>;
    var a: f32;

    p_1 = glt_p;
    let _e8 = p.direction;
    v = normalize(_e8);
    let _e11 = v;
    let _e12 = v;
    let _e15 = v;
    v = (_e11 / vec2((abs(_e12.x) + abs(_e15.y))));
    let _e21 = v;
    let _e27 = v;
    d = ((_e21.x * 0.5f) + (_e27.y * 0.5f));
    let _e35 = p.smoothness;
    offset = _e35;
    let _e37 = offset;
    let _e40 = v;
    let _e42 = p_1;
    let _e45 = v;
    let _e47 = p_1;
    let _e51 = d;
    let _e54 = u.progress;
    let _e56 = offset;
    pr = smoothstep(-(_e37), 0f, (((_e40.x * _e42.x) + (_e45.y * _e47.y)) - ((_e51 - 0.5f) + (_e54 * (1f + _e56)))));
    let _e63 = p_1;
    let _e64 = vec2<i32>(p.squares);
    squarep = fract((_e63 * vec2<f32>(_e64)));
    let _e69 = pr;
    squaremin = vec2((_e69 / 2f));
    let _e75 = pr;
    squaremax = vec2((1f - (_e75 / 2f)));
    let _e82 = u.progress;
    let _e86 = squaremin;
    let _e88 = squarep;
    let _e92 = squaremin;
    let _e94 = squarep;
    let _e98 = squarep;
    let _e100 = squaremax;
    let _e104 = squarep;
    let _e106 = squaremax;
    a = (((((1f - step(_e82, 0f)) * step(_e86.x, _e88.x)) * step(_e92.y, _e94.y)) * step(_e98.x, _e100.x)) * step(_e104.y, _e106.y));
    let _e111 = p_1;
    let _e112 = getFromColor(_e111);
    let _e113 = p_1;
    let _e114 = getToColor(_e113);
    let _e115 = a;
    return mix(_e112, _e114, vec4(_e115));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
