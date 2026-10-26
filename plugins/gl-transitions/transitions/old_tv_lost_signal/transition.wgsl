// Author: mernking (gitlab.com/Godswork)
// License: MIT
// Description: A crossfade with drifting tracking lines and tearing, like a TV losing its signal
// Tags: retro, glitch, scanlines
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

fn hash(glt_p: vec2<f32>) -> f32 {
    var p_1: vec2<f32>;

    p_1 = glt_p;
    let _e4 = p_1;
    return fract((sin(dot(_e4, vec2<f32>(127.1f, 311.7f))) * 43758.547f));
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var p_2: f32;
    var strength: f32;
    var tv: vec2<f32>;
    var fromColor: vec4<f32>;
    var toColor: vec4<f32>;
    var color: vec4<f32>;
    var lineY: f32;
    var noise: f32;
    var line: f32;
    var drift: f32;
    var shiftedFrom: vec4<f32>;
    var shiftedTo: vec4<f32>;
    var lineColor: vec4<f32>;
    var scan: f32;

    uv_5 = uv_4;
    let _e4 = u.progress;
    p_2 = _e4;
    let _e6 = p_2;
    strength = sin((_e6 * 3.1415927f));
    let _e11 = uv_5;
    tv = _e11;
    let _e13 = tv;
    let _e14 = getFromColor(_e13);
    fromColor = _e14;
    let _e16 = tv;
    let _e17 = getToColor(_e16);
    toColor = _e17;
    let _e19 = fromColor;
    let _e20 = toColor;
    let _e21 = p_2;
    color = mix(_e19, _e20, vec4(_e21));
    let _e25 = tv;
    lineY = floor((_e25.y * 120f));
    let _e31 = lineY;
    let _e32 = p_2;
    let _e36 = hash(vec2<f32>(_e31, (_e32 * 20f)));
    noise = _e36;
    let _e39 = noise;
    line = step(0.92f, _e39);
    let _e42 = tv;
    let _e46 = p_2;
    let _e53 = strength;
    drift = ((sin(((_e42.y * 30f) + (_e46 * 10f))) * 0.02f) * _e53);
    let _e56 = tv;
    let _e57 = drift;
    let _e61 = getFromColor((_e56 + vec2<f32>(_e57, 0f)));
    shiftedFrom = _e61;
    let _e63 = tv;
    let _e64 = drift;
    let _e68 = getToColor((_e63 + vec2<f32>(_e64, 0f)));
    shiftedTo = _e68;
    let _e70 = shiftedFrom;
    let _e71 = shiftedTo;
    let _e72 = p_2;
    lineColor = mix(_e70, _e71, vec4(_e72));
    let _e76 = color;
    let _e77 = lineColor;
    let _e78 = line;
    let _e79 = strength;
    color = mix(_e76, _e77, vec4((_e78 * _e79)));
    let _e83 = tv;
    scan = (sin((_e83.y * 900f)) * 0.03f);
    let _e91 = color;
    let _e93 = color;
    let _e95 = scan;
    let _e96 = strength;
    let _e99 = (_e93.xyz - vec3((_e95 * _e96)));
    color.x = _e99.x;
    color.y = _e99.y;
    color.z = _e99.z;
    let _e106 = color;
    return _e106;
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
