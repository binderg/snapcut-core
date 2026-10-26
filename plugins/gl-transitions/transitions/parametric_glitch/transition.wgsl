// Author: Yoni Maltsman @friendlyspinach
// License: MIT
// Description: A crossfade with glitchy horizontal bands and color shifts
// Tags: glitch, blend
//
// @param Horizontal amplitude of the glitch
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
    var from_: vec4<f32>;
    var to: vec4<f32>;
    var r: f32;
    var g: f32;
    var b: f32;
    var sphere: f32;
    var spiralX: f32;
    var spiralY: f32;
    var st: vec2<f32>;
    var diff: vec2<f32>;

    uv_5 = uv_4;
    let _e6 = uv_5;
    let _e7 = getFromColor(_e6);
    from_ = _e7;
    let _e9 = uv_5;
    let _e10 = getToColor(_e9);
    to = _e10;
    let _e12 = from_;
    r = _e12.x;
    let _e15 = from_;
    g = _e15.y;
    let _e18 = from_;
    b = _e18.z;
    let _e21 = r;
    let _e22 = r;
    let _e24 = g;
    let _e25 = g;
    let _e28 = b;
    let _e29 = b;
    sphere = ((((_e21 * _e22) + (_e24 * _e25)) + (_e28 * _e29)) - 1f);
    let _e35 = sphere;
    let _e36 = uv_5;
    let _e38 = u.progress;
    spiralX = cos((_e35 - (_e36.x / (_e38 + 0.01f))));
    let _e45 = sphere;
    let _e46 = uv_5;
    let _e48 = u.progress;
    spiralY = sin((_e45 - (_e46.y / (_e48 + 0.01f))));
    let _e55 = uv_5;
    st = _e55;
    let _e58 = p.ampx;
    let _e59 = st;
    let _e62 = spiralX;
    st.x = fract(((_e58 * _e59.x) * _e62));
    let _e66 = p.ampy;
    let _e67 = st;
    let _e70 = spiralY;
    st.y = fract(((_e66 * _e67.y) * _e70));
    let _e73 = uv_5;
    let _e74 = st;
    diff = (_e73 - _e74);
    let _e77 = uv_5;
    let _e78 = u.progress;
    let _e79 = diff;
    let _e82 = getFromColor((_e77 + (_e78 * _e79)));
    from_ = _e82;
    let _e83 = from_;
    let _e84 = to;
    let _e85 = u.progress;
    return mix(_e83, _e84, vec4(_e85));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
