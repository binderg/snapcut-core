// Author: Rich Harris
// License: MIT
// Description: The incoming image appears through blobs of Perlin noise
// Tags: noise, dissolve
//
// @param Scale of the noise; higher values make smaller blobs
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

fn random(co: vec2<f32>) -> f32 {
    var co_1: vec2<f32>;
    var a: f32;
    var b: f32 = 78.233f;
    var c: f32 = 43758.547f;
    var dt: f32;
    var sn: f32;

    co_1 = co;
    let _e7 = p.seed;
    a = _e7;
    let _e13 = co_1;
    let _e15 = a;
    let _e16 = b;
    dt = dot(_e13.xy, vec2<f32>(_e15, _e16));
    let _e20 = dt;
    sn = (_e20 - (floor((_e20 / 3.14f)) * 3.14f));
    let _e27 = sn;
    let _e29 = c;
    return fract((sin(_e27) * _e29));
}

fn noise(st: vec2<f32>) -> f32 {
    var st_1: vec2<f32>;
    var i: vec2<f32>;
    var f: vec2<f32>;
    var a_1: f32;
    var b_1: f32;
    var c_1: f32;
    var d: f32;
    var glt_u: vec2<f32>;

    st_1 = st;
    let _e7 = st_1;
    i = floor(_e7);
    let _e10 = st_1;
    f = fract(_e10);
    let _e13 = i;
    let _e14 = random(_e13);
    a_1 = _e14;
    let _e16 = i;
    let _e21 = random((_e16 + vec2<f32>(1f, 0f)));
    b_1 = _e21;
    let _e23 = i;
    let _e28 = random((_e23 + vec2<f32>(0f, 1f)));
    c_1 = _e28;
    let _e30 = i;
    let _e35 = random((_e30 + vec2<f32>(1f, 1f)));
    d = _e35;
    let _e37 = f;
    let _e38 = f;
    let _e42 = f;
    glt_u = ((_e37 * _e38) * (vec2(3f) - (2f * _e42)));
    let _e48 = a_1;
    let _e49 = b_1;
    let _e50 = glt_u;
    let _e53 = c_1;
    let _e54 = a_1;
    let _e56 = glt_u;
    let _e60 = glt_u;
    let _e65 = d;
    let _e66 = b_1;
    let _e68 = glt_u;
    let _e71 = glt_u;
    return ((mix(_e48, _e49, _e50.x) + (((_e53 - _e54) * _e56.y) * (1f - _e60.x))) + (((_e65 - _e66) * _e68.x) * _e71.y));
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var from_: vec4<f32>;
    var to: vec4<f32>;
    var n: f32;
    var glt_p: f32;
    var lower: f32;
    var higher: f32;
    var q: f32;

    uv_5 = uv_4;
    let _e7 = uv_5;
    let _e8 = getFromColor(_e7);
    from_ = _e8;
    let _e10 = uv_5;
    let _e11 = getToColor(_e10);
    to = _e11;
    let _e13 = uv_5;
    let _e14 = p.scale;
    let _e16 = noise((_e13 * _e14));
    n = _e16;
    let _e18 = p.smoothness;
    let _e21 = p.smoothness;
    let _e23 = u.progress;
    glt_p = mix(-(_e18), (1f + _e21), _e23);
    let _e26 = glt_p;
    let _e27 = p.smoothness;
    lower = (_e26 - _e27);
    let _e30 = glt_p;
    let _e31 = p.smoothness;
    higher = (_e30 + _e31);
    let _e34 = lower;
    let _e35 = higher;
    let _e36 = n;
    q = smoothstep(_e34, _e35, _e36);
    let _e39 = from_;
    let _e40 = to;
    let _e42 = q;
    return mix(_e39, _e40, vec4((1f - _e42)));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
