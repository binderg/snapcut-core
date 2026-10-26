// Author: Travis Fischer
// License: MIT
// Description: A crossfade in which a displacement map (an extra grayscale texture) warps both images
// Tags: displace, distort, blend
//
// Adapted from a Codrops article by Robin Delaporte
// https://tympanus.net/Development/DistortionHoverEffect
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
fn glt_img_displacementMap(uv: vec2<f32>) -> vec4<f32> {
    return displacement_map(glt_flip(uv));
}

fn glt_main(uv_6: vec2<f32>) -> vec4<f32> {
    var uv_7: vec2<f32>;
    var displacement: f32;
    var uvFrom: vec2<f32>;
    var uvTo: vec2<f32>;

    uv_7 = uv_6;
    let _e5 = uv_7;
    let _e6 = glt_img_displacementMap(_e5);
    let _e8 = p.strength;
    displacement = (_e6.x * _e8);
    let _e11 = uv_7;
    let _e13 = u.progress;
    let _e14 = displacement;
    let _e17 = uv_7;
    uvFrom = vec2<f32>((_e11.x + (_e13 * _e14)), _e17.y);
    let _e21 = uv_7;
    let _e24 = u.progress;
    let _e26 = displacement;
    let _e29 = uv_7;
    uvTo = vec2<f32>((_e21.x - ((1f - _e24) * _e26)), _e29.y);
    let _e33 = uvFrom;
    let _e34 = getFromColor(_e33);
    let _e35 = uvTo;
    let _e36 = getToColor(_e35);
    let _e37 = u.progress;
    return mix(_e34, _e36, vec4(_e37));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
