// Author: @Flexi23
// License: MIT
// Description: A cannabis-leaf shape grows from the center to reveal the incoming image
// Tags: shape, wipe
//
// inspired by http://www.wolframalpha.com/input/?i=cannabis+curve
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
    var leaf_uv: vec2<f32>;
    var r: f32 = 0.18f;
    var o: f32;

    uv_5 = uv_4;
    let _e4 = u.progress;
    if (_e4 == 0f) {
        {
            let _e7 = uv_5;
            let _e8 = getFromColor(_e7);
            return _e8;
        }
    }
    let _e9 = uv_5;
    let _e16 = u.progress;
    leaf_uv = (((_e9 - vec2(0.5f)) / vec2(10f)) / vec2(pow(_e16, 3.5f)));
    let _e23 = leaf_uv;
    leaf_uv.y = (_e23.y + 0.35f);
    let _e29 = leaf_uv;
    let _e31 = leaf_uv;
    o = atan2(_e29.y, _e31.x);
    let _e35 = uv_5;
    let _e36 = getFromColor(_e35);
    let _e37 = uv_5;
    let _e38 = getToColor(_e37);
    let _e41 = leaf_uv;
    let _e44 = r;
    let _e46 = o;
    let _e53 = o;
    let _e62 = o;
    let _e71 = o;
    return mix(_e36, _e38, vec4((1f - step(((1f - length(_e41)) + ((((_e44 * (1f + sin(_e46))) * (1f + (0.9f * cos((8f * _e53))))) * (1f + (0.1f * cos((24f * _e62))))) * (0.9f + (0.05f * cos((200f * _e71)))))), 1f))));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
