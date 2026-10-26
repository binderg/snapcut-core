// Author: Fernando Kuteken
// License: MIT
// Description: The outgoing image spins and zooms while fading to the incoming one, over a background color
// Tags: rotate, zoom, fade
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
    var difference: vec2<f32>;
    var dir: vec2<f32>;
    var dist: f32;
    var angle: f32;
    var c: f32;
    var s: f32;
    var currentScale: f32;
    var rotatedDir: vec2<f32>;
    var rotatedUv: vec2<f32>;

    uv_5 = uv_4;
    let _e8 = uv_5;
    let _e9 = p.center;
    difference = (_e8 - _e9);
    let _e12 = difference;
    dir = normalize(_e12);
    let _e15 = difference;
    dist = length(_e15);
    let _e21 = p.rotations;
    let _e23 = u.progress;
    angle = ((6.2831855f * _e21) * _e23);
    let _e26 = angle;
    c = cos(_e26);
    let _e29 = angle;
    s = sin(_e29);
    let _e32 = p.scale;
    let _e35 = u.progress;
    currentScale = mix(_e32, 1f, (2f * abs((_e35 - 0.5f))));
    let _e42 = dir;
    let _e44 = c;
    let _e46 = dir;
    let _e48 = s;
    let _e51 = dir;
    let _e53 = s;
    let _e55 = dir;
    let _e57 = c;
    rotatedDir = vec2<f32>(((_e42.x * _e44) - (_e46.y * _e48)), ((_e51.x * _e53) + (_e55.y * _e57)));
    let _e62 = p.center;
    let _e63 = rotatedDir;
    let _e64 = dist;
    let _e66 = currentScale;
    rotatedUv = (_e62 + ((_e63 * _e64) / vec2(_e66)));
    let _e71 = rotatedUv;
    let _e75 = rotatedUv;
    let _e80 = rotatedUv;
    let _e85 = rotatedUv;
    if ((((_e71.x < 0f) || (_e75.x > 1f)) || (_e80.y < 0f)) || (_e85.y > 1f)) {
        let _e90 = p.back_color;
        return _e90;
    }
    let _e91 = rotatedUv;
    let _e92 = getFromColor(_e91);
    let _e93 = rotatedUv;
    let _e94 = getToColor(_e93);
    let _e95 = u.progress;
    return mix(_e92, _e94, vec4(_e95));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
