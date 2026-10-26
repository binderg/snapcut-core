// License: MIT
// Author: pthrasher
// Description: A circle with a wavy edge burns out from the center to reveal the incoming image
// Tags: circle, burn, wipe
// adapted by gre from https://gist.github.com/pthrasher/8e6226b215548ba12734
//
// @param Softness of the edge
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

const M_PI: f32 = 3.1415927f;

fn quadraticInOut(t: f32) -> f32 {
    var t_1: f32;
    var glt_p: f32;
    var local: f32;

    t_1 = t;
    let _e9 = t_1;
    let _e11 = t_1;
    glt_p = ((2f * _e9) * _e11);
    let _e14 = t_1;
    if (_e14 < 0.5f) {
        let _e17 = glt_p;
        local = _e17;
    } else {
        let _e18 = glt_p;
        let _e21 = t_1;
        local = ((-(_e18) + (4f * _e21)) - 1f);
    }
    let _e27 = local;
    return _e27;
}

fn getGradient(r: f32, dist: f32) -> f32 {
    var r_1: f32;
    var dist_1: f32;
    var d: f32;

    r_1 = r;
    dist_1 = dist;
    let _e10 = r_1;
    let _e11 = dist_1;
    d = (_e10 - _e11);
    let _e14 = p.smoothness;
    let _e17 = r_1;
    let _e18 = dist_1;
    let _e20 = p.smoothness;
    let _e28 = d;
    let _e33 = d;
    let _e35 = d;
    return mix(smoothstep(-(_e14), 0f, (_e17 - (_e18 * (1f + _e20)))), (-1f - step(0.005f, _e28)), (step(-0.005f, _e33) * step(_e35, 0.01f)));
}

fn getWave(p_1: vec2<f32>) -> f32 {
    var p_2: vec2<f32>;
    var _p: vec2<f32>;
    var rads: f32;
    var degs: f32;
    var range: vec2<f32> = vec2<f32>(0f, 94.24778f);
    var domain: vec2<f32> = vec2<f32>(0f, 360f);
    var ratio: f32 = 0.2617994f;
    var x: f32;
    var magnitude: f32;
    var offset: f32;
    var ease_degs: f32;
    var deg_wave_pos: f32;

    p_2 = p_1;
    let _e8 = p_2;
    let _e9 = p.center;
    _p = (_e8 - _e9);
    let _e12 = _p;
    let _e14 = _p;
    rads = atan2(_e12.y, _e14.x);
    let _e18 = rads;
    degs = (degrees(_e18) + 180f);
    let _e39 = degs;
    let _e40 = ratio;
    degs = (_e39 * _e40);
    let _e42 = u.progress;
    x = _e42;
    let _e48 = x;
    magnitude = mix(0.02f, 0.09f, smoothstep(0f, 1f, _e48));
    let _e56 = x;
    offset = mix(40f, 30f, smoothstep(0f, 1f, _e56));
    let _e60 = degs;
    let _e62 = quadraticInOut(sin(_e60));
    ease_degs = _e62;
    let _e64 = ease_degs;
    let _e65 = magnitude;
    let _e67 = x;
    let _e68 = offset;
    deg_wave_pos = ((_e64 * _e65) * sin((_e67 * _e68)));
    let _e73 = x;
    let _e74 = p.smoothness;
    let _e80 = x;
    let _e85 = deg_wave_pos;
    return ((_e73 - (max(_e74, 0.01f) * (1f - smoothstep(0f, 0.1f, _e80)))) + _e85);
}

fn glt_main(p_3: vec2<f32>) -> vec4<f32> {
    var p_4: vec2<f32>;
    var dist_2: f32;
    var m: f32;
    var cfrom: vec4<f32>;
    var cto: vec4<f32>;

    p_4 = p_3;
    let _e8 = p.center;
    let _e9 = p_4;
    dist_2 = distance(_e8, _e9);
    let _e12 = p_4;
    let _e13 = getWave(_e12);
    let _e14 = dist_2;
    let _e15 = getGradient(_e13, _e14);
    m = _e15;
    let _e17 = p_4;
    let _e18 = getFromColor(_e17);
    cfrom = _e18;
    let _e20 = p_4;
    let _e21 = getToColor(_e20);
    cto = _e21;
    let _e23 = cfrom;
    let _e24 = cto;
    let _e25 = m;
    let _e28 = cfrom;
    let _e29 = p.color;
    let _e38 = m;
    return mix(mix(_e23, _e24, vec4(_e25)), mix(_e28, vec4<f32>(_e29.x, _e29.y, _e29.z, 1f), vec4(0.75f)), vec4(step(_e38, -2f)));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
