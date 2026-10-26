// Author: Ben Lucas
// License: MIT
// Description: Cuts to the incoming image halfway, hidden behind a burst of TV static
// Tags: static, noise, retro
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
    return fract((sin(dot(_e6.xy, vec2<f32>(10.530234f, 70.23493f))) * 12345.545f));
}

fn staticNoise(st_2: vec2<f32>, offset: f32, luminosity: f32) -> vec4<f32> {
    var st_3: vec2<f32>;
    var offset_1: f32;
    var luminosity_1: f32;
    var staticR: f32;
    var staticG: f32;
    var staticB: f32;

    st_3 = st_2;
    offset_1 = offset;
    luminosity_1 = luminosity;
    let _e10 = luminosity_1;
    let _e11 = st_3;
    let _e12 = offset_1;
    let _e15 = offset_1;
    let _e20 = rnd((_e11 * vec2<f32>((_e12 * 2f), (_e15 * 3f))));
    staticR = (_e10 * _e20);
    let _e23 = luminosity_1;
    let _e24 = st_3;
    let _e25 = offset_1;
    let _e28 = offset_1;
    let _e33 = rnd((_e24 * vec2<f32>((_e25 * 3f), (_e28 * 5f))));
    staticG = (_e23 * _e33);
    let _e36 = luminosity_1;
    let _e37 = st_3;
    let _e38 = offset_1;
    let _e41 = offset_1;
    let _e46 = rnd((_e37 * vec2<f32>((_e38 * 5f), (_e41 * 7f))));
    staticB = (_e36 * _e46);
    let _e49 = staticR;
    let _e50 = staticG;
    let _e51 = staticB;
    return vec4<f32>(_e49, _e50, _e51, 1f);
}

fn staticIntensity(t: f32) -> f32 {
    var t_1: f32;
    var transitionProgress: f32;
    var transformedThreshold: f32;

    t_1 = t;
    let _e7 = t_1;
    transitionProgress = abs((2f * (_e7 - 0.5f)));
    let _e15 = transitionProgress;
    transformedThreshold = ((1.2f * (1f - _e15)) - 0.1f);
    let _e22 = transformedThreshold;
    return min(1f, _e22);
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var baseMix: f32;
    var transitionMix: vec4<f32>;
    var uvStatic: vec2<f32>;
    var staticColor: vec4<f32>;
    var staticThresh: f32;
    var staticMix: f32;

    uv_5 = uv_4;
    let _e7 = u.progress;
    baseMix = step(0.5f, _e7);
    let _e10 = uv_5;
    let _e11 = getFromColor(_e10);
    let _e12 = uv_5;
    let _e13 = getToColor(_e12);
    let _e14 = baseMix;
    transitionMix = mix(_e11, _e13, vec4(_e14));
    let _e18 = uv_5;
    let _e19 = p.n_noise_pixels;
    let _e22 = p.n_noise_pixels;
    uvStatic = (floor((_e18 * _e19)) / vec2(_e22));
    let _e26 = uvStatic;
    let _e27 = u.progress;
    let _e28 = p.static_luminosity;
    let _e29 = staticNoise(_e26, _e27, _e28);
    staticColor = _e29;
    let _e31 = u.progress;
    let _e32 = staticIntensity(_e31);
    staticThresh = _e32;
    let _e34 = uvStatic;
    let _e35 = rnd(_e34);
    let _e36 = staticThresh;
    staticMix = step(_e35, _e36);
    let _e39 = transitionMix;
    let _e40 = staticColor;
    let _e41 = staticMix;
    return mix(_e39, _e40, vec4(_e41));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
