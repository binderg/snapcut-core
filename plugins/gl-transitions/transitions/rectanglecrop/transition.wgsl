// License: MIT
// Author: martiniti
// Description: The outgoing image shrinks into a small rectangle over a background color, then the incoming one grows from it
// Tags: box, iris, zoom
//
// @color Background color
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
    var s: f32;
    var q: vec2<f32>;
    var bl: vec2<f32>;
    var tr: vec2<f32>;
    var dist: f32;
    var local: vec4<f32>;

    uv_5 = uv_4;
    let _e6 = u.progress;
    s = pow((2f * abs((_e6 - 0.5f))), 3f);
    let _e14 = uv_5;
    q = (_e14.xy / vec2(1f));
    let _e23 = u.progress;
    let _e30 = q;
    bl = step(vec2((1f - (2f * abs((_e23 - 0.5f))))), (_e30 + vec2(0.25f)));
    let _e38 = u.progress;
    let _e46 = q;
    tr = step(vec2((1f - (2f * abs((_e38 - 0.5f))))), (vec2(1.25f) - _e46));
    let _e52 = bl;
    let _e54 = bl;
    let _e57 = tr;
    let _e60 = tr;
    dist = length((1f - (((_e52.x * _e54.y) * _e57.x) * _e60.y)));
    let _e66 = u.progress;
    if (_e66 < 0.5f) {
        let _e69 = uv_5;
        let _e70 = getFromColor(_e69);
        local = _e70;
    } else {
        let _e71 = uv_5;
        let _e72 = getToColor(_e71);
        local = _e72;
    }
    let _e74 = local;
    let _e75 = p.bgcolor;
    let _e76 = s;
    let _e77 = dist;
    return mix(_e74, _e75, vec4(step(_e76, _e77)));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
