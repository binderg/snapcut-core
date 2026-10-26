// Author: martiniti
// License: MIT
// Description: A rectangle grows from the center to cover the outgoing image, then shrinks to reveal the incoming one
// Tags: box, shape, iris
//
// @color Color of the rectangle
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
    var s: f32;
    var sq: vec2<f32>;
    var bl: vec2<f32>;
    var dist: f32;
    var tr: vec2<f32>;
    var local: vec4<f32>;

    p_1 = glt_p;
    let _e6 = u.progress;
    s = pow((2f * abs((_e6 - 0.5f))), 3f);
    let _e14 = p_1;
    sq = (_e14.xy / vec2(1f));
    let _e23 = u.progress;
    let _e28 = sq;
    bl = step(vec2(abs((1f - (2f * _e23)))), (_e28 + vec2(0.25f)));
    let _e34 = bl;
    let _e36 = bl;
    dist = (_e34.x * _e36.y);
    let _e42 = u.progress;
    let _e48 = sq;
    tr = step(vec2(abs((1f - (2f * _e42)))), (vec2(1.25f) - _e48));
    let _e53 = dist;
    let _e55 = tr;
    let _e58 = tr;
    dist = (_e53 * ((1f * _e55.x) * _e58.y));
    let _e62 = u.progress;
    if (_e62 < 0.5f) {
        let _e65 = p_1;
        let _e66 = getFromColor(_e65);
        local = _e66;
    } else {
        let _e67 = p_1;
        let _e68 = getToColor(_e67);
        local = _e68;
    }
    let _e70 = local;
    let _e71 = p.bgcolor;
    let _e72 = s;
    let _e73 = dist;
    return mix(_e70, _e71, vec4(step(_e72, _e73)));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
