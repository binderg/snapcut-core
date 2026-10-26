// Author: gre
// License: MIT
// Description: The outgoing image is squeezed into a line with color separation, revealing the incoming one
// Tags: squeeze, chromatic, vertical
//
// @param Amount of color separation
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
    var y: f32;
    var fp: vec2<f32>;
    var off: vec2<f32>;
    var c: vec4<f32>;
    var cn: vec4<f32>;
    var cp: vec4<f32>;

    uv_5 = uv_4;
    let _e6 = uv_5;
    let _e11 = u.progress;
    y = (0.5f + ((_e6.y - 0.5f) / (1f - _e11)));
    let _e16 = y;
    let _e19 = y;
    if ((_e16 < 0f) || (_e19 > 1f)) {
        {
            let _e23 = uv_5;
            let _e24 = getToColor(_e23);
            return _e24;
        }
    } else {
        {
            let _e25 = uv_5;
            let _e27 = y;
            fp = vec2<f32>(_e25.x, _e27);
            let _e30 = u.progress;
            let _e32 = p.color_separation;
            off = (_e30 * vec2<f32>(0f, _e32));
            let _e36 = fp;
            let _e37 = getFromColor(_e36);
            c = _e37;
            let _e39 = fp;
            let _e40 = off;
            let _e42 = getFromColor((_e39 - _e40));
            cn = _e42;
            let _e44 = fp;
            let _e45 = off;
            let _e47 = getFromColor((_e44 + _e45));
            cp = _e47;
            let _e49 = cn;
            let _e51 = c;
            let _e53 = cp;
            let _e55 = c;
            return vec4<f32>(_e49.x, _e51.y, _e53.z, _e55.w);
        }
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
