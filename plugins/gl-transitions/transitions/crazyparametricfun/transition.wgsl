// Author: mandubian
// License: MIT
// Description: The outgoing image is pulled along a parametric curve into a psychedelic swirl
// Tags: distort, psychedelic
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
    var glt_p: vec2<f32>;
    var dir: vec2<f32>;
    var dist: f32;
    var x: f32;
    var y: f32;
    var offset: vec2<f32>;

    uv_5 = uv_4;
    let _e8 = uv_5;
    glt_p = (_e8.xy / vec2(1f));
    let _e15 = glt_p;
    dir = (_e15 - vec2(0.5f));
    let _e20 = dir;
    dist = length(_e20);
    let _e23 = p.a;
    let _e24 = p.b;
    let _e26 = u.progress;
    let _e29 = p.b;
    let _e30 = u.progress;
    let _e31 = p.a;
    let _e32 = p.b;
    x = (((_e23 - _e24) * cos(_e26)) + (_e29 * cos((_e30 * ((_e31 / _e32) - 1f)))));
    let _e41 = p.a;
    let _e42 = p.b;
    let _e44 = u.progress;
    let _e47 = p.b;
    let _e48 = u.progress;
    let _e49 = p.a;
    let _e50 = p.b;
    y = (((_e41 - _e42) * sin(_e44)) - (_e47 * sin((_e48 * ((_e49 / _e50) - 1f)))));
    let _e59 = dir;
    let _e60 = u.progress;
    let _e61 = dist;
    let _e63 = p.amplitude;
    let _e65 = x;
    let _e68 = u.progress;
    let _e69 = dist;
    let _e71 = p.amplitude;
    let _e73 = y;
    let _e78 = p.smoothness;
    offset = ((_e59 * vec2<f32>(sin((((_e60 * _e61) * _e63) * _e65)), sin((((_e68 * _e69) * _e71) * _e73)))) / vec2(_e78));
    let _e82 = glt_p;
    let _e83 = offset;
    let _e85 = getFromColor((_e82 + _e83));
    let _e86 = glt_p;
    let _e87 = getToColor(_e86);
    let _e90 = u.progress;
    return mix(_e85, _e87, vec4(smoothstep(0.2f, 1f, _e90)));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
