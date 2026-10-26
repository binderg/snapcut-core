// Author: mandubian
// License: MIT
// Description: Wavy, color-separated ripples scramble the outgoing image into the incoming one
// Tags: wave, distort, chromatic
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

fn compute(glt_p: vec2<f32>, progress: f32, center: vec2<f32>) -> f32 {
    var p_1: vec2<f32>;
    var progress_1: f32;
    var center_1: vec2<f32>;
    var o: vec2<f32>;
    var h: vec2<f32> = vec2<f32>(1f, 0f);
    var theta: f32;
    var s: f32;
    var s2_: f32;

    p_1 = glt_p;
    progress_1 = progress;
    center_1 = center;
    let _e12 = p_1;
    let _e13 = progress_1;
    let _e14 = p.amplitude;
    let _e18 = center_1;
    o = ((_e12 * sin((_e13 * _e14))) - _e18);
    let _e25 = o;
    let _e26 = h;
    let _e29 = p.waves;
    theta = (acos(dot(_e25, _e26)) * _e29);
    let _e33 = theta;
    s = sin((((2f * _e33) - PI) / 24f));
    let _e40 = s;
    let _e41 = s;
    s2_ = (_e40 * _e41);
    let _e44 = theta;
    let _e49 = theta;
    let _e54 = s2_;
    let _e55 = s2_;
    let _e57 = s;
    return (((exp(cos(_e44)) - (2f * cos((4f * _e49)))) + ((_e54 * _e55) * _e57)) / 10f);
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var p_2: vec2<f32>;
    var inv: f32;
    var disp: f32;
    var texTo: vec4<f32>;
    var texFrom: vec4<f32>;

    uv_5 = uv_4;
    let _e8 = u.progress;
    if (_e8 <= 0f) {
        let _e11 = uv_5;
        let _e12 = getFromColor(_e11);
        return _e12;
    }
    let _e13 = u.progress;
    if (_e13 >= 1f) {
        let _e16 = uv_5;
        let _e17 = getToColor(_e16);
        return _e17;
    }
    let _e18 = uv_5;
    p_2 = _e18;
    let _e21 = u.progress;
    inv = (1f - _e21);
    let _e24 = p_2;
    let _e25 = u.progress;
    let _e29 = compute(_e24, _e25, vec2<f32>(0.5f, 0.5f));
    disp = _e29;
    let _e31 = p_2;
    let _e32 = inv;
    let _e33 = disp;
    let _e37 = getToColor((_e31 + vec2((_e32 * _e33))));
    texTo = _e37;
    let _e39 = p_2;
    let _e40 = u.progress;
    let _e41 = disp;
    let _e44 = p.color_separation;
    let _e49 = getFromColor((_e39 + vec2(((_e40 * _e41) * (1f - _e44)))));
    let _e51 = p_2;
    let _e52 = u.progress;
    let _e53 = disp;
    let _e57 = getFromColor((_e51 + vec2((_e52 * _e53))));
    let _e59 = p_2;
    let _e60 = u.progress;
    let _e61 = disp;
    let _e64 = p.color_separation;
    let _e69 = getFromColor((_e59 + vec2(((_e60 * _e61) * (1f + _e64)))));
    texFrom = vec4<f32>(_e49.x, _e57.y, _e69.z, 1f);
    let _e74 = texTo;
    let _e75 = u.progress;
    let _e77 = texFrom;
    let _e78 = inv;
    return ((_e74 * _e75) + (_e77 * _e78));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
