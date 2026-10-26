// License: MIT
// Author: rectalogic
// Description: A crossfade through a strong zoom blur
// Tags: zoom, blur, blend
// ported by gre from https://gist.github.com/rectalogic/b86b90161503a0023231
//
// Converted from https://github.com/rectalogic/rendermix-basic-effects/blob/master/assets/com/rendermix/CrossZoom/CrossZoom.frag
// Which is based on https://github.com/evanw/glfx.js/blob/master/src/filters/blur/zoomblur.js
// With additional easing functions from https://github.com/rectalogic/rendermix-basic-effects/blob/master/assets/com/rendermix/Easing/Easing.glsllib
//
// @range(0, 1) Blur strength
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

const PI: f32 = 3.1415927f;

fn Linear_ease(begin: f32, change: f32, duration: f32, time: f32) -> f32 {
    var begin_1: f32;
    var change_1: f32;
    var duration_1: f32;
    var time_1: f32;

    begin_1 = begin;
    change_1 = change;
    duration_1 = duration;
    time_1 = time;
    let _e12 = change_1;
    let _e13 = time_1;
    let _e15 = duration_1;
    let _e17 = begin_1;
    return (((_e12 * _e13) / _e15) + _e17);
}

fn Exponential_easeInOut(begin_2: f32, change_2: f32, duration_2: f32, time_2: f32) -> f32 {
    var begin_3: f32;
    var change_3: f32;
    var duration_3: f32;
    var time_3: f32;

    begin_3 = begin_2;
    change_3 = change_2;
    duration_3 = duration_2;
    time_3 = time_2;
    let _e12 = time_3;
    if (_e12 == 0f) {
        let _e15 = begin_3;
        return _e15;
    } else {
        let _e16 = time_3;
        let _e17 = duration_3;
        if (_e16 == _e17) {
            let _e19 = begin_3;
            let _e20 = change_3;
            return (_e19 + _e20);
        }
    }
    let _e22 = time_3;
    let _e23 = duration_3;
    time_3 = (_e22 / (_e23 / 2f));
    let _e27 = time_3;
    if (_e27 < 1f) {
        let _e30 = change_3;
        let _e35 = time_3;
        let _e41 = begin_3;
        return (((_e30 / 2f) * pow(2f, (10f * (_e35 - 1f)))) + _e41);
    }
    let _e43 = change_3;
    let _e49 = time_3;
    let _e58 = begin_3;
    return (((_e43 / 2f) * (-(pow(2f, (-10f * (_e49 - 1f)))) + 2f)) + _e58);
}

fn Sinusoidal_easeInOut(begin_4: f32, change_4: f32, duration_4: f32, time_4: f32) -> f32 {
    var begin_5: f32;
    var change_5: f32;
    var duration_5: f32;
    var time_5: f32;

    begin_5 = begin_4;
    change_5 = change_4;
    duration_5 = duration_4;
    time_5 = time_4;
    let _e12 = change_5;
    let _e16 = time_5;
    let _e18 = duration_5;
    let _e24 = begin_5;
    return (((-(_e12) / 2f) * (cos(((PI * _e16) / _e18)) - 1f)) + _e24);
}

fn rand(co: vec2<f32>) -> f32 {
    var co_1: vec2<f32>;

    co_1 = co;
    let _e6 = co_1;
    return fract((sin(dot(_e6.xy, vec2<f32>(12.9898f, 78.233f))) * 43758.547f));
}

fn crossFade(uv_4: vec2<f32>, dissolve: f32) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var dissolve_1: f32;

    uv_5 = uv_4;
    dissolve_1 = dissolve;
    let _e8 = uv_5;
    let _e9 = getFromColor(_e8);
    let _e10 = uv_5;
    let _e11 = getToColor(_e10);
    let _e12 = dissolve_1;
    return mix(_e9, _e11, vec4(_e12));
}

fn glt_main(uv_6: vec2<f32>) -> vec4<f32> {
    var uv_7: vec2<f32>;
    var texCoord: vec2<f32>;
    var center: vec2<f32>;
    var dissolve_2: f32;
    var strength: f32;
    var color: vec4<f32> = vec4(0f);
    var total: f32 = 0f;
    var toCenter: vec2<f32>;
    var offset: f32;
    var t: f32 = 0f;
    var percent: f32;
    var weight: f32;

    uv_7 = uv_6;
    let _e6 = uv_7;
    texCoord = (_e6.xy / vec2(1f));
    let _e16 = u.progress;
    let _e17 = Linear_ease(0.25f, 0.5f, 1f, _e16);
    center = vec2<f32>(_e17, 0.5f);
    let _e24 = u.progress;
    let _e25 = Exponential_easeInOut(0f, 1f, 1f, _e24);
    dissolve_2 = _e25;
    let _e28 = p.strength;
    let _e30 = u.progress;
    let _e31 = Sinusoidal_easeInOut(0f, _e28, 0.5f, _e30);
    strength = _e31;
    let _e38 = center;
    let _e39 = texCoord;
    toCenter = (_e38 - _e39);
    let _e42 = uv_7;
    let _e43 = rand(_e42);
    offset = _e43;
    loop {
        let _e47 = t;
        if !((_e47 <= 40f)) {
            break;
        }
        {
            let _e54 = t;
            let _e55 = offset;
            percent = ((_e54 + _e55) / 40f);
            let _e61 = percent;
            let _e62 = percent;
            let _e63 = percent;
            weight = (4f * (_e61 - (_e62 * _e63)));
            let _e68 = color;
            let _e69 = texCoord;
            let _e70 = toCenter;
            let _e71 = percent;
            let _e73 = strength;
            let _e76 = dissolve_2;
            let _e77 = crossFade((_e69 + ((_e70 * _e71) * _e73)), _e76);
            let _e78 = weight;
            color = (_e68 + (_e77 * _e78));
            let _e81 = total;
            let _e82 = weight;
            total = (_e81 + _e82);
        }
        continuing {
            let _e51 = t;
            t = (_e51 + 1f);
        }
    }
    let _e84 = color;
    let _e85 = total;
    return (_e84 / vec4(_e85));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
