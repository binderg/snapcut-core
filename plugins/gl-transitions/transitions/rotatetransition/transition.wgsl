// Author: haiyoucuv
// License: MIT
// Description: The images spin around the center while crossfading
// Tags: rotate, spin, blend
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

fn rotate2D(uv_4: vec2<f32>, angle: f32) -> vec2<f32> {
    var uv_5: vec2<f32>;
    var angle_1: f32;

    uv_5 = uv_4;
    angle_1 = angle;
    let _e6 = uv_5;
    let _e7 = angle_1;
    let _e9 = angle_1;
    let _e12 = angle_1;
    let _e14 = angle_1;
    return (_e6 * mat2x2<f32>(vec2<f32>(cos(_e7), -(sin(_e9))), vec2<f32>(sin(_e12), cos(_e14))));
}

fn glt_main(uv_6: vec2<f32>) -> vec4<f32> {
    var uv_7: vec2<f32>;
    var glt_p: vec2<f32>;

    uv_7 = uv_6;
    let _e4 = uv_7;
    let _e8 = u.progress;
    let _e13 = rotate2D((_e4 - vec2(0.5f)), ((_e8 * 3.1415925f) * 2f));
    glt_p = fract((_e13 + vec2(0.5f)));
    let _e19 = glt_p;
    let _e20 = getFromColor(_e19);
    let _e21 = glt_p;
    let _e22 = getToColor(_e21);
    let _e23 = u.progress;
    return mix(_e20, _e22, vec4(_e23));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
