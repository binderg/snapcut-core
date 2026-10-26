// Author: Jake Nelson
// License: MIT
// Description: A hard-edged wipe from top to bottom
// Tags: wipe, vertical
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
    var a: vec4<f32>;
    var b: vec4<f32>;

    uv_5 = uv_4;
    let _e4 = uv_5;
    glt_p = (_e4.xy / vec2(1f));
    let _e11 = glt_p;
    let _e12 = getFromColor(_e11);
    a = _e12;
    let _e14 = glt_p;
    let _e15 = getToColor(_e14);
    b = _e15;
    let _e17 = a;
    let _e18 = b;
    let _e20 = glt_p;
    let _e23 = u.progress;
    return mix(_e17, _e18, vec4(step((1f - _e20.y), _e23)));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
