// Author: Sergey Kosarevsky
// License: MIT
// Description: Both images break into large blocks that blend, then sharpen into the incoming image
// Tags: pixelate, mosaic, blend
// Ported from https://gist.github.com/corporateshark/21d2fdd24c706952dc8c
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
    var T: f32;
    var half: f32 = 0.5f;
    var local: f32;
    var size: f32;
    var D: f32;
    var UV: vec2<f32>;
    var coord: vec2<f32>;
    var C0_: vec4<f32>;
    var C1_: vec4<f32>;

    uv_5 = uv_4;
    let _e5 = u.progress;
    T = _e5;
    let _e9 = T;
    let _e10 = half;
    if (_e9 < _e10) {
        let _e13 = p.pixel_size;
        let _e14 = T;
        let _e15 = half;
        local = mix(1f, _e13, (_e14 / _e15));
    } else {
        let _e18 = p.pixel_size;
        let _e20 = T;
        let _e21 = half;
        let _e23 = half;
        local = mix(_e18, 1f, ((_e20 - _e21) / _e23));
    }
    let _e27 = local;
    size = _e27;
    let _e29 = size;
    D = (_e29 * 0.005f);
    let _e33 = uv_5;
    let _e37 = D;
    UV = ((_e33 - vec2(0.5f)) / vec2(_e37));
    let _e41 = D;
    let _e42 = UV;
    coord = clamp(((_e41 * ceil((_e42 - vec2(0.5f)))) + vec2(0.5f)), vec2(0f), vec2(1f));
    let _e57 = coord;
    let _e58 = getFromColor(_e57);
    C0_ = _e58;
    let _e60 = coord;
    let _e61 = getToColor(_e60);
    C1_ = _e61;
    let _e63 = C0_;
    let _e64 = C1_;
    let _e65 = T;
    return mix(_e63, _e64, vec4(_e65));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
