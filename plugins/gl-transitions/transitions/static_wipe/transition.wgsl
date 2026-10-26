// Author: Ben Lucas
// License: MIT
// Description: A wipe from top to bottom with a band of TV static at its edge
// Tags: wipe, static, noise, vertical
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

fn rnd(st: vec2<f32>) -> f32 {
    var st_1: vec2<f32>;

    st_1 = st;
    let _e6 = st_1;
    return fract((sin(dot(_e6.xy, vec2<f32>(10f, 70f))) * 12345.545f));
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var span: f32;
    var local: f32;
    var transitionEdge: f32;
    var mixRatio: f32;
    var transitionMix: vec4<f32>;
    var noiseEnvelope: f32;
    var noise: vec4<f32>;

    uv_5 = uv_4;
    let _e6 = p.u_max_static_span;
    let _e8 = u.progress;
    span = (_e6 * pow(sin((3.1415927f * _e8)), 0.5f));
    let _e15 = (p.u_transition_up_to_down > 0.5);
    if _e15 {
        let _e17 = uv_5;
        local = (1f - _e17.y);
    } else {
        let _e20 = uv_5;
        local = _e20.y;
    }
    let _e23 = local;
    transitionEdge = _e23;
    let _e26 = u.progress;
    let _e27 = transitionEdge;
    mixRatio = (1f - step(_e26, _e27));
    let _e31 = uv_5;
    let _e32 = getFromColor(_e31);
    let _e33 = uv_5;
    let _e34 = getToColor(_e33);
    let _e35 = mixRatio;
    transitionMix = mix(_e32, _e34, vec4(_e35));
    let _e39 = u.progress;
    let _e40 = span;
    let _e42 = u.progress;
    let _e43 = transitionEdge;
    let _e46 = u.progress;
    let _e47 = u.progress;
    let _e48 = span;
    let _e50 = transitionEdge;
    noiseEnvelope = (smoothstep((_e39 - _e40), _e42, _e43) * (1f - smoothstep(_e46, (_e47 + _e48), _e50)));
    let _e55 = uv_5;
    let _e57 = u.progress;
    let _e60 = rnd((_e55 * (1f + _e57)));
    let _e61 = vec3(_e60);
    noise = vec4<f32>(_e61.x, _e61.y, _e61.z, 1f);
    let _e68 = transitionMix;
    let _e69 = noise;
    let _e70 = noiseEnvelope;
    return mix(_e68, _e69, vec4(_e70));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
