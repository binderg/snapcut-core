// Author: Fernando Kuteken
// License: MIT
// Description: A crossfade through the multiplied blend of both images
// Tags: blend, fade
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

fn blend(a: vec4<f32>, b: vec4<f32>) -> vec4<f32> {
    var a_1: vec4<f32>;
    var b_1: vec4<f32>;

    a_1 = a;
    b_1 = b;
    let _e6 = a_1;
    let _e7 = b_1;
    return (_e6 * _e7);
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var blended: vec4<f32>;

    uv_5 = uv_4;
    let _e4 = uv_5;
    let _e5 = getFromColor(_e4);
    let _e6 = uv_5;
    let _e7 = getToColor(_e6);
    let _e8 = blend(_e5, _e7);
    blended = _e8;
    let _e10 = u.progress;
    if (_e10 < 0.5f) {
        let _e13 = uv_5;
        let _e14 = getFromColor(_e13);
        let _e15 = blended;
        let _e17 = u.progress;
        return mix(_e14, _e15, vec4((2f * _e17)));
    } else {
        let _e21 = blended;
        let _e22 = uv_5;
        let _e23 = getToColor(_e22);
        let _e25 = u.progress;
        return mix(_e21, _e23, vec4(((2f * _e25) - 1f)));
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
