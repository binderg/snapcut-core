// Author: Zeh Fernando
// License: MIT
// Description: The outgoing image melts down in uneven columns, like the Doom screen wipe
// Tags: melt, columns, retro
//
// Transition parameters --------
//
// Number of total bars/columns
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

fn rand(num: i32) -> f32 {
    var num_1: i32;

    num_1 = num;
    let _e9 = num_1;
    let _e12 = (f32(_e9) * 67123.31f);
    let _e18 = num_1;
    let _e24 = num_1;
    return fract((((_e12 - (floor((_e12 / 12f)) * 12f)) * sin((f32(_e18) * 10.3f))) * cos(f32(_e24))));
}

fn wave(num_2: i32) -> f32 {
    var num_3: i32;
    var fn_: f32;

    num_3 = num_2;
    let _e9 = num_3;
    let _e11 = p.frequency;
    let _e15 = i32(p.bars);
    fn_ = (((f32(_e9) * _e11) * 0.1f) * f32(_e15));
    let _e19 = fn_;
    let _e23 = fn_;
    let _e28 = fn_;
    return ((((cos((_e19 * 0.5f)) * cos((_e23 * 0.13f))) * sin(((_e28 + 10f) * 0.3f))) / 2f) + 0.5f);
}

fn drip(num_4: i32) -> f32 {
    var num_5: i32;

    num_5 = num_4;
    let _e9 = num_5;
    let _e11 = i32(p.bars);
    let _e19 = p.drip_scale;
    return (sin(((f32(_e9) / f32((_e11 - 1i))) * 3.141592f)) * _e19);
}

fn pos(num_6: i32) -> f32 {
    var num_7: i32;
    var local: f32;
    var local_1: f32;

    num_7 = num_6;
    let _e9 = p.noise;
    if (_e9 == 0f) {
        let _e12 = num_7;
        let _e13 = wave(_e12);
        local = _e13;
    } else {
        let _e14 = num_7;
        let _e15 = wave(_e14);
        let _e16 = num_7;
        let _e17 = rand(_e16);
        let _e18 = p.noise;
        local = mix(_e15, _e17, _e18);
    }
    let _e21 = local;
    let _e22 = p.drip_scale;
    if (_e22 == 0f) {
        local_1 = 0f;
    } else {
        let _e26 = num_7;
        let _e27 = drip(_e26);
        local_1 = _e27;
    }
    let _e29 = local_1;
    return (_e21 + _e29);
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var bar: i32;
    var scale: f32;
    var phase: f32;
    var posY: f32;
    var glt_p: vec2<f32>;
    var c: vec4<f32>;

    uv_5 = uv_4;
    let _e9 = uv_5;
    let _e11 = i32(p.bars);
    bar = i32((_e9.x * f32(_e11)));
    let _e17 = bar;
    let _e18 = pos(_e17);
    let _e19 = p.amplitude;
    scale = (1f + (_e18 * _e19));
    let _e23 = u.progress;
    let _e24 = scale;
    phase = (_e23 * _e24);
    let _e27 = uv_5;
    posY = (_e27.y / 1f);
    let _e35 = phase;
    let _e36 = posY;
    if ((_e35 + _e36) < 1f) {
        {
            let _e40 = uv_5;
            let _e42 = uv_5;
            let _e47 = phase;
            glt_p = (vec2<f32>(_e40.x, (_e42.y + mix(0f, 1f, _e47))) / vec2(1f));
            let _e55 = glt_p;
            let _e56 = getFromColor(_e55);
            c = _e56;
        }
    } else {
        {
            let _e57 = uv_5;
            glt_p = (_e57.xy / vec2(1f));
            let _e63 = glt_p;
            let _e64 = getToColor(_e63);
            c = _e64;
        }
    }
    let _e65 = c;
    return _e65;
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
