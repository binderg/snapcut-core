// Author: Tianshuo
// License: MIT
// Description: The incoming image zooms out into place while fading in
// Tags: zoom, fade
//
// @range(0.2, 1) Part of the transition spent zooming
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

fn zoom(uv_4: vec2<f32>, amount: f32) -> vec2<f32> {
    var uv_5: vec2<f32>;
    var amount_1: f32;

    uv_5 = uv_4;
    amount_1 = amount;
    let _e9 = uv_5;
    let _e14 = amount_1;
    return (vec2(0.5f) + ((_e9 - vec2(0.5f)) * (1f - _e14)));
}

fn glt_main(uv_6: vec2<f32>) -> vec4<f32> {
    var uv_7: vec2<f32>;
    var nQuick: f32;
    var local: f32;
    var local_1: f32;

    uv_7 = uv_6;
    let _e6 = p.zoom_quickness;
    nQuick = clamp(_e6, 0.2f, 1f);
    let _e11 = uv_7;
    let _e12 = getFromColor(_e11);
    let _e13 = uv_7;
    let _e16 = nQuick;
    let _e19 = u.progress;
    let _e22 = zoom(_e13, (1f - smoothstep((1f - _e16), 1f, _e19)));
    let _e23 = getToColor(_e22);
    let _e24 = (p.fade > 0.5);
    if _e24 {
        let _e26 = nQuick;
        let _e29 = u.progress;
        local_1 = smoothstep((1f - _e26), 1f, _e29);
    } else {
        let _e31 = u.progress;
        let _e33 = nQuick;
        if (_e31 < (1f - _e33)) {
            local = 0f;
        } else {
            local = 1f;
        }
        let _e39 = local;
        local_1 = _e39;
    }
    let _e41 = local_1;
    return mix(_e12, _e23, vec4(_e41));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
