// License: MIT
// Author: P-Seebauer
// Description: Pixels switch to the incoming image in order of how close their colors are
// Tags: blend, color
// ported by gre from https://gist.github.com/P-Seebauer/2a5fa2f77c883dd661f9
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
    var fTex: vec4<f32>;
    var tTex: vec4<f32>;
    var m: f32;

    p_1 = glt_p;
    let _e5 = p_1;
    let _e6 = getFromColor(_e5);
    fTex = _e6;
    let _e8 = p_1;
    let _e9 = getToColor(_e8);
    tTex = _e9;
    let _e11 = fTex;
    let _e12 = tTex;
    let _e14 = u.progress;
    m = step(distance(_e11, _e12), _e14);
    let _e17 = fTex;
    let _e18 = tTex;
    let _e19 = m;
    let _e22 = tTex;
    let _e23 = u.progress;
    let _e24 = p.power;
    return mix(mix(_e17, _e18, vec4(_e19)), _e22, vec4(pow(_e23, _e24)));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
