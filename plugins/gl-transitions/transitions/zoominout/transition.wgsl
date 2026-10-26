// Author: OllyOllyOlly
// License: MIT
// Description: Zooms into the outgoing image until it blurs into a flat color, then out to the incoming one
// Tags: zoom, blur
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

fn zoom(uv_4: vec2<f32>, amount: f32) -> vec2<f32> {
    var uv_5: vec2<f32>;
    var amount_1: f32;

    uv_5 = uv_4;
    amount_1 = amount;
    let _e7 = uv_5;
    let _e12 = amount_1;
    return (vec2(0.5f) + ((_e7 - vec2(0.5f)) * (1f - _e12)));
}

fn glt_main(uv_6: vec2<f32>) -> vec4<f32> {
    var uv_7: vec2<f32>;
    var zoomFrom: f32;
    var zoomTo: f32;
    var crossfade: f32;

    uv_7 = uv_6;
    let _e6 = u.progress;
    zoomFrom = smoothstep(0f, 1f, (_e6 * 2f));
    let _e14 = u.progress;
    zoomTo = smoothstep(0f, 1f, ((1f - _e14) * 2f));
    let _e22 = u.progress;
    crossfade = smoothstep(0.4f, 0.6f, _e22);
    let _e25 = uv_7;
    let _e26 = zoomFrom;
    let _e27 = zoom(_e25, _e26);
    let _e28 = getFromColor(_e27);
    let _e29 = uv_7;
    let _e30 = zoomTo;
    let _e31 = zoom(_e29, _e30);
    let _e32 = getToColor(_e31);
    let _e33 = crossfade;
    return mix(_e28, _e32, vec4(_e33));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
