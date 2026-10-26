// License: MIT
// Author: fkuteken
// Description: The outgoing image closes into a circle over a background color, then the incoming one opens from it
// Tags: circle, iris, shape
// ported by gre from https://gist.github.com/fkuteken/f63e3009c1143950dee9063c3b83fb88
//
// @color Background color
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
    var ratio2_: vec2<f32>;
    var s: f32;
    var dist: f32;
    var local: vec4<f32>;

    p_1 = glt_p;
    let _e7 = (u.resolution.x / u.resolution.y);
    ratio2_ = vec2<f32>(1f, (1f / _e7));
    let _e12 = u.progress;
    s = pow((2f * abs((_e12 - 0.5f))), 3f);
    let _e20 = p_1;
    let _e25 = ratio2_;
    dist = length(((vec2<f32>(_e20) - vec2(0.5f)) * _e25));
    let _e29 = u.progress;
    if (_e29 < 0.5f) {
        let _e32 = p_1;
        let _e33 = getFromColor(_e32);
        local = _e33;
    } else {
        let _e34 = p_1;
        let _e35 = getToColor(_e34);
        local = _e35;
    }
    let _e37 = local;
    let _e38 = p.bgcolor;
    let _e39 = s;
    let _e40 = dist;
    return mix(_e37, _e38, vec4(step(_e39, _e40)));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
