// Author: gre
// License: MIT
// Description: A soft-edged circle opens from the center to reveal the incoming image
// Tags: circle, iris, wipe
// @param Softness of the circle's edge
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

const SQRT_2_: f32 = 1.4142135f;

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var local: f32;
    var x: f32;
    var m: f32;
    var local_1: f32;

    uv_5 = uv_4;
    let _e8 = (p.opening > 0.5);
    if _e8 {
        let _e9 = u.progress;
        local = _e9;
    } else {
        let _e11 = u.progress;
        local = (1f - _e11);
    }
    let _e14 = local;
    x = _e14;
    let _e16 = p.smoothness;
    let _e19 = uv_5;
    let _e22 = x;
    let _e24 = p.smoothness;
    m = smoothstep(-(_e16), 0f, ((SQRT_2_ * distance(center, _e19)) - (_e22 * (1f + _e24))));
    let _e30 = uv_5;
    let _e31 = getFromColor(_e30);
    let _e32 = uv_5;
    let _e33 = getToColor(_e32);
    let _e34 = (p.opening > 0.5);
    if _e34 {
        let _e36 = m;
        local_1 = (1f - _e36);
    } else {
        let _e38 = m;
        local_1 = _e38;
    }
    let _e40 = local_1;
    return mix(_e31, _e33, vec4(_e40));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
