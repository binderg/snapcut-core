// Author: hjm1fb
// License: MIT
// Description: The incoming image spreads over the outgoing one with a hot, glowing edge
// Tags: dissolve, burn, glow
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

fn hash(glt_p: vec2<f32>) -> vec2<f32> {
    var p_1: vec2<f32>;

    p_1 = glt_p;
    let _e9 = p_1;
    let _e14 = p_1;
    p_1 = vec2<f32>(dot(_e9, vec2<f32>(127.1f, 311.7f)), dot(_e14, vec2<f32>(269.5f, 183.3f)));
    let _e23 = p_1;
    return (vec2(-1f) + (2f * fract((sin(_e23) * 43758.547f))));
}

fn noise(p_2: vec2<f32>) -> f32 {
    var p_3: vec2<f32>;
    var K1_: f32 = 0.36602542f;
    var K2_: f32 = 0.21132487f;
    var i: vec2<f32>;
    var a: vec2<f32>;
    var m: f32;
    var o: vec2<f32>;
    var b: vec2<f32>;
    var c: vec2<f32>;
    var h: vec3<f32>;
    var n: vec3<f32>;

    p_3 = p_2;
    let _e13 = p_3;
    let _e14 = p_3;
    let _e16 = p_3;
    let _e19 = K1_;
    i = floor((_e13 + vec2(((_e14.x + _e16.y) * _e19))));
    let _e25 = p_3;
    let _e26 = i;
    let _e28 = i;
    let _e30 = i;
    let _e33 = K2_;
    a = ((_e25 - _e26) + vec2(((_e28.x + _e30.y) * _e33)));
    let _e38 = a;
    let _e40 = a;
    m = step(_e38.y, _e40.x);
    let _e44 = m;
    let _e46 = m;
    o = vec2<f32>(_e44, (1f - _e46));
    let _e50 = a;
    let _e51 = o;
    let _e53 = K2_;
    b = ((_e50 - _e51) + vec2(_e53));
    let _e57 = a;
    let _e62 = K2_;
    c = ((_e57 - vec2(1f)) + vec2((2f * _e62)));
    let _e68 = a;
    let _e69 = a;
    let _e71 = b;
    let _e72 = b;
    let _e74 = c;
    let _e75 = c;
    h = max((vec3(0.5f) - vec3<f32>(dot(_e68, _e69), dot(_e71, _e72), dot(_e74, _e75))), vec3(0f));
    let _e84 = h;
    let _e85 = h;
    let _e87 = h;
    let _e89 = h;
    let _e91 = a;
    let _e92 = i;
    let _e96 = hash((_e92 + vec2(0f)));
    let _e98 = b;
    let _e99 = i;
    let _e100 = o;
    let _e102 = hash((_e99 + _e100));
    let _e104 = c;
    let _e105 = i;
    let _e109 = hash((_e105 + vec2(1f)));
    n = ((((_e84 * _e85) * _e87) * _e89) * vec3<f32>(dot(_e91, _e96), dot(_e98, _e102), dot(_e104, _e109)));
    let _e114 = n;
    return dot(_e114, vec3(70f));
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var from_: vec4<f32>;
    var to: vec4<f32>;
    var outColor: vec4<f32>;
    var burn: f32;
    var show: f32;
    var factor: f32;
    var burnColor: vec3<f32>;
    var finalRGB: vec3<f32>;

    uv_5 = uv_4;
    let _e9 = uv_5;
    let _e10 = getFromColor(_e9);
    from_ = _e10;
    let _e12 = uv_5;
    let _e13 = getToColor(_e12);
    to = _e13;
    let _e20 = from_;
    let _e24 = from_;
    let _e29 = from_;
    burn = (0.5f + (0.5f * (((0.299f * _e20.x) + (0.587f * _e24.y)) + (0.114f * _e29.z))));
    let _e35 = burn;
    let _e36 = u.progress;
    show = (_e35 - _e36);
    let _e39 = show;
    if (_e39 < 0.001f) {
        {
            let _e42 = to;
            outColor = _e42;
        }
    } else {
        {
            let _e45 = p.u_line_width;
            let _e46 = show;
            factor = (1f - smoothstep(0f, _e45, _e46));
            let _e50 = p.u_spread_clr;
            let _e51 = p.u_hot_clr;
            let _e52 = factor;
            burnColor = mix(_e50, _e51, vec3(_e52));
            let _e56 = burnColor;
            let _e57 = p.u_pow;
            let _e60 = p.u_intensity;
            burnColor = (pow(_e56, vec3(_e57)) * _e60);
            let _e62 = from_;
            let _e64 = burnColor;
            let _e65 = factor;
            let _e67 = u.progress;
            finalRGB = mix(_e62.xyz, _e64, vec3((_e65 * step(0.0001f, _e67))));
            let _e73 = finalRGB;
            let _e74 = from_;
            let _e76 = (_e73 * _e74.w);
            let _e77 = from_;
            outColor = vec4<f32>(_e76.x, _e76.y, _e76.z, _e77.w);
        }
    }
    let _e83 = outColor;
    return _e83;
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
