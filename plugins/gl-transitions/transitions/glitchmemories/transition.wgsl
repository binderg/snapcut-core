// Author: Gunnar Roth
// based on work from natewave
// License: MIT
// Description: A crossfade with glitchy, color-shifted blocks
// Tags: glitch, chromatic, blend
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

fn glt_main(glt_p: vec2<f32>) -> vec4<f32> {
    var p_1: vec2<f32>;
    var block: vec2<f32>;
    var uv_noise: vec2<f32>;
    var local: vec2<f32>;
    var dist: vec2<f32>;
    var red: vec2<f32>;
    var green: vec2<f32>;
    var blue: vec2<f32>;

    p_1 = glt_p;
    let _e4 = p_1;
    block = floor((_e4.xy / vec2(16f)));
    let _e12 = block;
    uv_noise = (_e12 / vec2(64f));
    let _e18 = uv_noise;
    let _e19 = u.progress;
    uv_noise = (_e18 + (floor((vec2(_e19) * vec2<f32>(1200f, 3500f))) / vec2(64f)));
    let _e31 = u.progress;
    if (_e31 > 0f) {
        let _e34 = uv_noise;
        let _e42 = u.progress;
        local = (((fract(_e34) - vec2(0.5f)) * 0.3f) * (1f - _e42));
    } else {
        local = vec2(0f);
    }
    let _e48 = local;
    dist = _e48;
    let _e50 = p_1;
    let _e51 = dist;
    red = (_e50 + (_e51 * 0.2f));
    let _e56 = p_1;
    let _e57 = dist;
    green = (_e56 + (_e57 * 0.3f));
    let _e62 = p_1;
    let _e63 = dist;
    blue = (_e62 + (_e63 * 0.5f));
    let _e68 = red;
    let _e69 = getFromColor(_e68);
    let _e70 = red;
    let _e71 = getToColor(_e70);
    let _e72 = u.progress;
    let _e76 = green;
    let _e77 = getFromColor(_e76);
    let _e78 = green;
    let _e79 = getToColor(_e78);
    let _e80 = u.progress;
    let _e84 = blue;
    let _e85 = getFromColor(_e84);
    let _e86 = blue;
    let _e87 = getToColor(_e86);
    let _e88 = u.progress;
    return vec4<f32>(mix(_e69, _e71, vec4(_e72)).x, mix(_e77, _e79, vec4(_e80)).y, mix(_e85, _e87, vec4(_e88)).z, 1f);
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
