// Author: Mark Craig
// mrmcsoftware on github and youtube ( http://www.youtube.com/MrMcSoftware )
// License: MIT
// Description: The outgoing image spins and shrinks away over the incoming one
// Tags: rotate, zoom, spin
//
// RotateScaleVanish Transition by Mark Craig (Copyright © 2022)
//
// @param Fade in the incoming image
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
    var iResolution: vec2<f32>;
    var local: f32;
    var t: f32;
    var local_1: f32;
    var theta: f32;
    var c1_: f32;
    var s1_: f32;
    var rad: f32;
    var xc1_: f32;
    var yc1_: f32;
    var xc2_: f32;
    var yc2_: f32;
    var uv2_: vec2<f32>;
    var col3_: vec4<f32>;
    var local_2: vec4<f32>;
    var ColorTo: vec4<f32>;
    var local_3: vec4<f32>;
    var local_4: vec4<f32>;

    uv_5 = uv_4;
    let _e7 = (u.resolution.x / u.resolution.y);
    iResolution = vec2<f32>(_e7, 1f);
    let _e11 = (p.reverse_effect > 0.5);
    if _e11 {
        let _e13 = u.progress;
        local = (1f - _e13);
    } else {
        let _e15 = u.progress;
        local = _e15;
    }
    let _e17 = local;
    t = _e17;
    let _e19 = (p.reverse_rotation > 0.5);
    if _e19 {
        let _e21 = t;
        local_1 = (6.2831855f * _e21);
    } else {
        let _e25 = t;
        local_1 = (-6.2831855f * _e25);
    }
    let _e28 = local_1;
    theta = _e28;
    let _e30 = theta;
    c1_ = cos(_e30);
    let _e33 = theta;
    s1_ = sin(_e33);
    let _e38 = t;
    rad = max(0.00001f, (1f - _e38));
    let _e42 = uv_5;
    let _e46 = iResolution;
    xc1_ = ((_e42.x - 0.5f) * _e46.x);
    let _e50 = uv_5;
    let _e54 = iResolution;
    yc1_ = ((_e50.y - 0.5f) * _e54.y);
    let _e58 = xc1_;
    let _e59 = c1_;
    let _e61 = yc1_;
    let _e62 = s1_;
    let _e65 = rad;
    xc2_ = (((_e58 * _e59) - (_e61 * _e62)) / _e65);
    let _e68 = xc1_;
    let _e69 = s1_;
    let _e71 = yc1_;
    let _e72 = c1_;
    let _e75 = rad;
    yc2_ = (((_e68 * _e69) + (_e71 * _e72)) / _e75);
    let _e78 = xc2_;
    let _e79 = iResolution;
    let _e84 = yc2_;
    let _e85 = iResolution;
    uv2_ = vec2<f32>((_e78 + (_e79.x / 2f)), (_e84 + (_e85.y / 2f)));
    let _e93 = (p.reverse_effect > 0.5);
    if _e93 {
        let _e94 = uv_5;
        let _e95 = getFromColor(_e94);
        local_2 = _e95;
    } else {
        let _e96 = uv_5;
        let _e97 = getToColor(_e96);
        local_2 = _e97;
    }
    let _e99 = local_2;
    ColorTo = _e99;
    let _e101 = uv2_;
    let _e105 = uv2_;
    let _e107 = iResolution;
    let _e111 = uv2_;
    let _e116 = uv2_;
    let _e118 = iResolution;
    if ((((_e101.x >= 0f) && (_e105.x <= _e107.x)) && (_e111.y >= 0f)) && (_e116.y <= _e118.y)) {
        {
            let _e122 = uv2_;
            let _e123 = iResolution;
            uv2_ = (_e122 / _e123);
            let _e125 = (p.reverse_effect > 0.5);
            if _e125 {
                let _e126 = uv2_;
                let _e127 = getToColor(_e126);
                local_3 = _e127;
            } else {
                let _e128 = uv2_;
                let _e129 = getFromColor(_e128);
                local_3 = _e129;
            }
            let _e131 = local_3;
            col3_ = _e131;
        }
    } else {
        {
            let _e132 = (p.fade_in_second > 0.5);
            if _e132 {
                local_4 = vec4<f32>(0f, 0f, 0f, 1f);
            } else {
                let _e138 = ColorTo;
                local_4 = _e138;
            }
            let _e140 = local_4;
            col3_ = _e140;
        }
    }
    let _e142 = t;
    let _e144 = col3_;
    let _e146 = t;
    let _e147 = ColorTo;
    return (((1f - _e142) * _e144) + (_e146 * _e147));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
