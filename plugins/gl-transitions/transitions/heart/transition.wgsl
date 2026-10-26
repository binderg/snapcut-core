// Author: gre
// License: MIT
// Description: A heart shape grows from the center to reveal the incoming image
// Tags: shape, wipe, heart
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

fn inHeart(glt_p: vec2<f32>, center: vec2<f32>, size: f32) -> f32 {
    var p_1: vec2<f32>;
    var center_1: vec2<f32>;
    var size_1: f32;
    var o: vec2<f32>;
    var a: f32;

    p_1 = glt_p;
    center_1 = center;
    size_1 = size;
    let _e8 = size_1;
    if (_e8 == 0f) {
        return 0f;
    }
    let _e12 = p_1;
    let _e13 = center_1;
    let _e16 = size_1;
    o = ((_e12 - _e13) / vec2((1.6f * _e16)));
    let _e21 = o;
    let _e23 = o;
    let _e26 = o;
    let _e28 = o;
    a = (((_e21.x * _e23.x) + (_e26.y * _e28.y)) - 0.3f);
    let _e35 = a;
    let _e36 = a;
    let _e38 = a;
    let _e40 = o;
    let _e42 = o;
    let _e45 = o;
    let _e48 = o;
    let _e51 = o;
    return step(((_e35 * _e36) * _e38), ((((_e40.x * _e42.x) * _e45.y) * _e48.y) * _e51.y));
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;

    uv_5 = uv_4;
    let _e4 = uv_5;
    let _e5 = getFromColor(_e4);
    let _e6 = uv_5;
    let _e7 = getToColor(_e6);
    let _e8 = uv_5;
    let _e12 = u.progress;
    let _e13 = inHeart(_e8, vec2<f32>(0.5f, 0.4f), _e12);
    return mix(_e5, _e7, vec4(_e13));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
