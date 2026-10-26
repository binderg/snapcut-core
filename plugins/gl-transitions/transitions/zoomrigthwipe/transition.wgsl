// Author: Handk
// License: MIT
// Description: Zooms into the outgoing image, then the incoming one wipes in from left to right
// Tags: zoom, wipe, horizontal
//
// @param Part of the first half spent zooming (capped at 0.5)
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
    let _e7 = amount_1;
    if (_e7 < 0.5f) {
        let _e11 = uv_5;
        let _e16 = amount_1;
        return (vec2(0.5f) + ((_e11 - vec2(0.5f)) * (1f - _e16)));
    } else {
        let _e22 = uv_5;
        let _e26 = amount_1;
        return (vec2(0.5f) + ((_e22 - vec2(0.5f)) * _e26));
    }
}

fn glt_main(uv_6: vec2<f32>) -> vec4<f32> {
    var uv_7: vec2<f32>;
    var nQuick: f32;
    var c: vec4<f32>;
    var glt_p: vec2<f32>;
    var d: vec4<f32>;
    var e: vec4<f32>;
    var f: vec4<f32>;

    uv_7 = uv_6;
    let _e5 = p.zoom_quickness;
    nQuick = clamp(_e5, 0f, 0.5f);
    let _e10 = u.progress;
    if (_e10 < 0.5f) {
        {
            let _e13 = uv_7;
            let _e15 = nQuick;
            let _e16 = u.progress;
            let _e18 = zoom(_e13, smoothstep(0f, _e15, _e16));
            let _e19 = getFromColor(_e18);
            let _e20 = uv_7;
            let _e21 = getToColor(_e20);
            let _e23 = u.progress;
            c = mix(_e19, _e21, vec4(step(0.5f, _e23)));
            let _e28 = c;
            return _e28;
        }
    } else {
        {
            let _e29 = uv_7;
            glt_p = (_e29.xy / vec2(1f));
            let _e36 = glt_p;
            let _e37 = getFromColor(_e36);
            d = _e37;
            let _e39 = glt_p;
            let _e40 = getToColor(_e39);
            e = _e40;
            let _e42 = d;
            let _e43 = e;
            let _e45 = glt_p;
            let _e48 = u.progress;
            f = mix(_e42, _e43, vec4(step((0f + _e45.x), ((_e48 - 0.5f) * 2f))));
            let _e57 = f;
            return _e57;
        }
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
