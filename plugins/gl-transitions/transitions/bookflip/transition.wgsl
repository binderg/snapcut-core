// Author: hong
// License: MIT
// Description: The outgoing image turns like a book page from right to left, revealing the incoming one
// Tags: page, flip, 3d
//
// Avoids dividing by zero when the page is edge-on (progress = 0.5).
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

fn nonZero(x: f32) -> f32 {
    var x_1: f32;
    var local: f32;
    var local_1: f32;

    x_1 = x;
    let _e4 = x_1;
    if (abs(_e4) < 0.0001f) {
        let _e8 = x_1;
        if (_e8 < 0f) {
            local = -0.0001f;
        } else {
            local = 0.0001f;
        }
        let _e15 = local;
        local_1 = _e15;
    } else {
        let _e16 = x_1;
        local_1 = _e16;
    }
    let _e18 = local_1;
    return _e18;
}

fn skewRight(glt_p: vec2<f32>) -> vec2<f32> {
    var p_1: vec2<f32>;
    var skewX: f32;
    var skewY: f32;

    p_1 = glt_p;
    let _e4 = p_1;
    let _e6 = u.progress;
    let _e9 = u.progress;
    let _e11 = nonZero((0.5f - _e9));
    skewX = (((_e4.x - _e6) / _e11) * 0.5f);
    let _e16 = p_1;
    let _e21 = u.progress;
    let _e22 = p_1;
    skewY = ((((_e16.y - 0.5f) / (0.5f + ((_e21 * (_e22.x - 0.5f)) / 0.5f))) * 0.5f) + 0.5f);
    let _e36 = skewX;
    let _e37 = skewY;
    return vec2<f32>(_e36, _e37);
}

fn skewLeft(p_2: vec2<f32>) -> vec2<f32> {
    var p_3: vec2<f32>;
    var skewX_1: f32;
    var skewY_1: f32;

    p_3 = p_2;
    let _e4 = p_3;
    let _e8 = u.progress;
    let _e11 = nonZero((_e8 - 0.5f));
    skewX_1 = ((((_e4.x - 0.5f) / _e11) * 0.5f) + 0.5f);
    let _e18 = p_3;
    let _e24 = u.progress;
    let _e27 = p_3;
    skewY_1 = ((((_e18.y - 0.5f) / (0.5f + (((1f - _e24) * (0.5f - _e27.x)) / 0.5f))) * 0.5f) + 0.5f);
    let _e40 = skewX_1;
    let _e41 = skewY_1;
    return vec2<f32>(_e40, _e41);
}

fn addShade() -> vec4<f32> {
    var shadeVal: f32;

    let _e3 = u.progress;
    shadeVal = max(0.7f, (abs((_e3 - 0.5f)) * 2f));
    let _e11 = shadeVal;
    let _e12 = vec3(_e11);
    return vec4<f32>(_e12.x, _e12.y, _e12.z, 1f);
}

fn glt_main(p_4: vec2<f32>) -> vec4<f32> {
    var p_5: vec2<f32>;
    var pr: f32;

    p_5 = p_4;
    let _e5 = u.progress;
    let _e7 = p_5;
    pr = step((1f - _e5), _e7.x);
    let _e11 = p_5;
    if (_e11.x < 0.5f) {
        {
            let _e15 = p_5;
            let _e16 = getFromColor(_e15);
            let _e17 = p_5;
            let _e18 = skewLeft(_e17);
            let _e19 = getToColor(_e18);
            let _e20 = addShade();
            let _e22 = pr;
            return mix(_e16, (_e19 * _e20), vec4(_e22));
        }
    } else {
        {
            let _e25 = p_5;
            let _e26 = skewRight(_e25);
            let _e27 = getFromColor(_e26);
            let _e28 = addShade();
            let _e30 = p_5;
            let _e31 = getToColor(_e30);
            let _e32 = pr;
            return mix((_e27 * _e28), _e31, vec4(_e32));
        }
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
