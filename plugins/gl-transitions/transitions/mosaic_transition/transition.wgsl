// Author: YueDev
// License: MIT
// Description: Both images break into mosaic blocks that blend, then sharpen into the incoming image
// Tags: mosaic, pixelate, blend
//
// @param Number of mosaic blocks
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

fn getMosaicUV(uv_4: vec2<f32>) -> vec2<f32> {
    var uv_5: vec2<f32>;
    var mosaicWidth: f32;
    var mX: f32;
    var mY: f32;

    uv_5 = uv_4;
    let _e6 = p.mosaic_num;
    let _e8 = u.progress;
    let _e10 = u.progress;
    mosaicWidth = ((2f / _e6) * min(_e8, (1f - _e10)));
    let _e15 = uv_5;
    let _e17 = mosaicWidth;
    mX = (floor((_e15.x / _e17)) + 0.5f);
    let _e23 = uv_5;
    let _e25 = mosaicWidth;
    mY = (floor((_e23.y / _e25)) + 0.5f);
    let _e31 = mX;
    let _e32 = mosaicWidth;
    let _e34 = mY;
    let _e35 = mosaicWidth;
    return vec2<f32>((_e31 * _e32), (_e34 * _e35));
}

fn glt_main(uv_6: vec2<f32>) -> vec4<f32> {
    var uv_7: vec2<f32>;
    var local: vec2<f32>;
    var mosaicUV: vec2<f32>;

    uv_7 = uv_6;
    let _e5 = u.progress;
    let _e7 = u.progress;
    if (min(_e5, (1f - _e7)) == 0f) {
        let _e12 = uv_7;
        local = _e12;
    } else {
        let _e13 = uv_7;
        let _e14 = getMosaicUV(_e13);
        local = _e14;
    }
    let _e16 = local;
    mosaicUV = _e16;
    let _e18 = mosaicUV;
    let _e19 = getFromColor(_e18);
    let _e20 = mosaicUV;
    let _e21 = getToColor(_e20);
    let _e22 = u.progress;
    let _e23 = u.progress;
    return mix(_e19, _e21, vec4((_e22 * _e23)));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
