// Author: nwoeanhinnogaehr
// License: MIT
// Description: A crossfade through the HSV color space that cycles through hues
// Tags: fade, color, hue
// Ported from https://gist.github.com/nwoeanhinnogaehr/b185145363d65751009b
//
// HSV functions from http://lolengine.net/blog/2013/07/27/rgb-to-hsv-in-glsl
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

fn hsv2rgb(c: vec3<f32>) -> vec3<f32> {
    var c_1: vec3<f32>;
    var K: vec4<f32> = vec4<f32>(1f, 0.6666667f, 0.33333334f, 3f);
    var glt_p: vec3<f32>;

    c_1 = c;
    let _e14 = c_1;
    let _e16 = K;
    let _e22 = K;
    glt_p = abs(((fract((_e14.xxx + _e16.xyz)) * 6f) - _e22.www));
    let _e27 = c_1;
    let _e29 = K;
    let _e31 = glt_p;
    let _e32 = K;
    let _e40 = c_1;
    return (_e27.z * mix(_e29.xxx, clamp((_e31 - _e32.xxx), vec3(0f), vec3(1f)), vec3(_e40.y)));
}

fn rgb2hsv(c_2: vec3<f32>) -> vec3<f32> {
    var c_3: vec3<f32>;
    var K_1: vec4<f32> = vec4<f32>(0f, -0.33333334f, 0.6666667f, -1f);
    var p_1: vec4<f32>;
    var q: vec4<f32>;
    var d: f32;

    c_3 = c_2;
    let _e16 = c_3;
    let _e17 = _e16.zy;
    let _e18 = K_1;
    let _e19 = _e18.wz;
    let _e25 = c_3;
    let _e26 = _e25.yz;
    let _e27 = K_1;
    let _e28 = _e27.xy;
    let _e34 = c_3;
    let _e36 = c_3;
    p_1 = mix(vec4<f32>(_e17.x, _e17.y, _e19.x, _e19.y), vec4<f32>(_e26.x, _e26.y, _e28.x, _e28.y), vec4(step(_e34.z, _e36.y)));
    let _e42 = p_1;
    let _e43 = _e42.xyw;
    let _e44 = c_3;
    let _e50 = c_3;
    let _e52 = p_1;
    let _e53 = _e52.yzx;
    let _e58 = p_1;
    let _e60 = c_3;
    q = mix(vec4<f32>(_e43.x, _e43.y, _e43.z, _e44.x), vec4<f32>(_e50.x, _e53.x, _e53.y, _e53.z), vec4(step(_e58.x, _e60.x)));
    let _e66 = q;
    let _e68 = q;
    let _e70 = q;
    d = (_e66.x - min(_e68.w, _e70.y));
    let _e75 = q;
    let _e77 = q;
    let _e79 = q;
    let _e83 = d;
    let _e90 = d;
    let _e91 = q;
    let _e96 = q;
    return vec3<f32>(abs((_e75.z + ((_e77.w - _e79.y) / ((6f * _e83) + 0.001f)))), (_e90 / (_e91.x + 0.001f)), _e96.x);
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var a: vec3<f32>;
    var b: vec3<f32>;
    var m: vec3<f32>;

    uv_5 = uv_4;
    let _e4 = uv_5;
    let _e5 = getFromColor(_e4);
    let _e7 = rgb2hsv(_e5.xyz);
    a = _e7;
    let _e9 = uv_5;
    let _e10 = getToColor(_e9);
    let _e12 = rgb2hsv(_e10.xyz);
    b = _e12;
    let _e14 = a;
    let _e15 = b;
    let _e16 = u.progress;
    m = mix(_e14, _e15, vec3(_e16));
    let _e20 = m;
    let _e21 = hsv2rgb(_e20);
    return vec4<f32>(_e21.x, _e21.y, _e21.z, 1f);
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
