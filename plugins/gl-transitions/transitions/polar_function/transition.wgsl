// Author: Fernando Kuteken
// License: MIT
// Description: A flower shape with several petals grows to reveal the incoming image
// Tags: shape, wipe, polar
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
    var angle: f32;
    var normalized: f32;
    var radius: f32;
    var difference: f32;

    uv_5 = uv_4;
    let _e5 = uv_5;
    let _e9 = uv_5;
    angle = (atan2((_e5.y - 0.5f), (_e9.x - 0.5f)) - 1.5707964f);
    let _e19 = angle;
    normalized = ((_e19 + 4.712389f) * 6.2831855f);
    let _e29 = i32(p.segments);
    let _e31 = angle;
    radius = ((cos((f32(_e29) * _e31)) + 4f) / 4f);
    let _e39 = uv_5;
    difference = length((_e39 - vec2<f32>(0.5f, 0.5f)));
    let _e46 = difference;
    let _e47 = radius;
    let _e48 = u.progress;
    if (_e46 > (_e47 * _e48)) {
        let _e51 = uv_5;
        let _e52 = getFromColor(_e51);
        return _e52;
    } else {
        let _e53 = uv_5;
        let _e54 = getToColor(_e53);
        return _e54;
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
