// Author: Adrian Purser
// License: MIT
// Description: The outgoing image drops out of the frame with a few bounces, revealing the incoming one
// Tags: bounce, slide, vertical, shadow
//
// @color Color of the shadow cast on the incoming image
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

const PI: f32 = 3.1415927f;

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var time: f32;
    var stime: f32;
    var phase: f32;
    var y: f32;
    var d: f32;

    uv_5 = uv_4;
    let _e8 = u.progress;
    time = _e8;
    let _e10 = time;
    stime = sin(((_e10 * PI) / 2f));
    let _e16 = time;
    let _e18 = p.bounces;
    phase = ((_e16 * PI) * _e18);
    let _e21 = phase;
    let _e25 = stime;
    y = (abs(cos(_e21)) * (1f - _e25));
    let _e29 = uv_5;
    let _e31 = y;
    d = (_e29.y - _e31);
    let _e34 = uv_5;
    let _e35 = getToColor(_e34);
    let _e36 = p.shadow_colour;
    let _e37 = d;
    let _e38 = p.shadow_height;
    let _e41 = d;
    let _e42 = p.shadow_height;
    let _e44 = p.shadow_colour;
    let _e48 = p.shadow_colour;
    let _e55 = u.progress;
    let _e62 = uv_5;
    let _e64 = uv_5;
    let _e67 = y;
    let _e71 = getFromColor(vec2<f32>(_e62.x, (_e64.y + (1f - _e67))));
    let _e72 = d;
    return mix(mix(_e35, _e36, vec4((step(_e37, _e38) * (1f - mix((((_e41 / _e42) * _e44.w) + (1f - _e48.w)), 1f, smoothstep(0.95f, 1f, _e55)))))), _e71, vec4(step(_e72, 0f)));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
