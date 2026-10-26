// Author: gre
// License: MIT
// Description: A ragged left-to-right wipe, as if the image was blown away by the wind
// Tags: wipe, horizontal, noise
//
// @range(0, 1) Width of the ragged edge
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

fn rand(co: vec2<f32>) -> f32 {
    var co_1: vec2<f32>;

    co_1 = co;
    let _e6 = co_1;
    return fract((sin(dot(_e6.xy, vec2<f32>(12.9898f, 78.233f))) * 43758.547f));
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var local: f32;
    var x: f32;
    var r: f32;
    var m: f32;

    uv_5 = uv_4;
    let _e6 = (p.reversed > 0.5);
    if _e6 {
        let _e8 = uv_5;
        local = (1f - _e8.x);
    } else {
        let _e11 = uv_5;
        local = _e11.x;
    }
    let _e14 = local;
    x = _e14;
    let _e17 = uv_5;
    let _e21 = rand(vec2<f32>(0f, _e17.y));
    r = _e21;
    let _e24 = p.size;
    let _e26 = x;
    let _e28 = p.size;
    let _e31 = p.size;
    let _e32 = r;
    let _e35 = u.progress;
    let _e37 = p.size;
    m = smoothstep(0f, -(_e24), (((_e26 * (1f - _e28)) + (_e31 * _e32)) - (_e35 * (1f + _e37))));
    let _e43 = uv_5;
    let _e44 = getFromColor(_e43);
    let _e45 = uv_5;
    let _e46 = getToColor(_e45);
    let _e47 = m;
    return mix(_e44, _e46, vec4(_e47));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
