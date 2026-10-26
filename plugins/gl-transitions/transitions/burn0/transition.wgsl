// Author: liubailin2020@gmail.com
// License: MIT
// Description: The outgoing image burns away in noisy, glowing patches
// Tags: burn, fire, noise
//
// @color Color of the burning edges
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

fn random(st: vec2<f32>) -> f32 {
    var st_1: vec2<f32>;

    st_1 = st;
    let _e5 = st_1;
    return fract((sin(dot(_e5.xy, vec2<f32>(12.9898f, 78.233f))) * 43758.547f));
}

fn noise(st_2: vec2<f32>) -> f32 {
    var st_3: vec2<f32>;
    var i: vec2<f32>;
    var f: vec2<f32>;
    var a: f32;
    var b: f32;
    var c: f32;
    var d: f32;
    var glt_u: vec2<f32>;

    st_3 = st_2;
    let _e5 = st_3;
    i = floor(_e5);
    let _e8 = st_3;
    f = fract(_e8);
    let _e11 = i;
    let _e12 = random(_e11);
    a = _e12;
    let _e14 = i;
    let _e19 = random((_e14 + vec2<f32>(1f, 0f)));
    b = _e19;
    let _e21 = i;
    let _e26 = random((_e21 + vec2<f32>(0f, 1f)));
    c = _e26;
    let _e28 = i;
    let _e33 = random((_e28 + vec2<f32>(1f, 1f)));
    d = _e33;
    let _e35 = f;
    let _e36 = f;
    let _e40 = f;
    glt_u = ((_e35 * _e36) * (vec2(3f) - (2f * _e40)));
    let _e46 = a;
    let _e47 = b;
    let _e48 = glt_u;
    let _e51 = c;
    let _e52 = a;
    let _e54 = glt_u;
    let _e58 = glt_u;
    let _e63 = d;
    let _e64 = b;
    let _e66 = glt_u;
    let _e69 = glt_u;
    return ((mix(_e46, _e47, _e48.x) + (((_e51 - _e52) * _e54.y) * (1f - _e58.x))) + (((_e63 - _e64) * _e66.x) * _e69.y));
}

fn fbm(st_4: vec2<f32>) -> f32 {
    var st_5: vec2<f32>;
    var value: f32 = 0f;
    var amplitude: f32 = 0.5f;
    var i_1: i32 = 0i;

    st_5 = st_4;
    loop {
        let _e11 = i_1;
        if !((_e11 < 4i)) {
            break;
        }
        {
            let _e18 = value;
            let _e19 = amplitude;
            let _e20 = st_5;
            let _e21 = noise(_e20);
            value = (_e18 + (_e19 * _e21));
            let _e24 = st_5;
            st_5 = (_e24 * 2f);
            let _e27 = amplitude;
            amplitude = (_e27 * 0.5f);
        }
        continuing {
            let _e15 = i_1;
            i_1 = (_e15 + 1i);
        }
    }
    let _e30 = value;
    return _e30;
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var from_: vec4<f32>;
    var to: vec4<f32>;
    var n: f32;
    var l: f32;
    var edge: f32;

    uv_5 = uv_4;
    let _e5 = u.progress;
    if (_e5 <= 0f) {
        let _e8 = uv_5;
        let _e9 = getFromColor(_e8);
        return _e9;
    }
    let _e10 = u.progress;
    if (_e10 >= 1f) {
        let _e13 = uv_5;
        let _e14 = getToColor(_e13);
        return _e14;
    }
    let _e15 = uv_5;
    let _e16 = getFromColor(_e15);
    from_ = _e16;
    let _e18 = uv_5;
    let _e19 = getToColor(_e18);
    to = _e19;
    let _e21 = uv_5;
    let _e24 = fbm((_e21 * 4f));
    n = _e24;
    let _e26 = u.progress;
    let _e27 = u.progress;
    let _e30 = n;
    l = smoothstep(_e26, (_e27 + 0.05f), _e30);
    let _e34 = l;
    let _e36 = l;
    edge = (((1f - _e34) * _e36) * 5f);
    let _e41 = to;
    let _e42 = from_;
    let _e43 = l;
    let _e46 = p.burn_color;
    let _e52 = edge;
    return (mix(_e41, _e42, vec4(_e43)) + (vec4<f32>(_e46.x, _e46.y, _e46.z, 0f) * _e52));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
