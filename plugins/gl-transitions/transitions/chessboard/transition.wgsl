// Author: lql
// License: MIT
// Description: A chessboard pattern of squares reveals the incoming image
// Tags: grid, squares, pattern
//
// @param Number of squares per row
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
    var st: vec2<f32>;
    var idx: vec2<f32>;
    var grid: vec2<f32>;
    var a: vec4<f32>;
    var b: vec4<f32>;
    var checker: f32;
    var mixFactor: f32;
    var local: f32;
    var local_1: f32;

    uv_5 = uv_4;
    let _e5 = uv_5;
    let _e6 = p.grid_num;
    st = (_e5 * _e6);
    let _e9 = st;
    idx = floor(_e9);
    let _e12 = st;
    grid = fract(_e12);
    let _e15 = uv_5;
    let _e16 = getFromColor(_e15);
    a = _e16;
    let _e18 = uv_5;
    let _e19 = getToColor(_e18);
    b = _e19;
    let _e21 = idx;
    let _e23 = idx;
    let _e25 = (_e21.x + _e23.y);
    checker = (_e25 - (floor((_e25 / 2f)) * 2f));
    let _e33 = u.progress;
    if (_e33 <= 0.5f) {
        {
            let _e36 = checker;
            if (_e36 > 0.5f) {
                let _e39 = grid;
                let _e41 = u.progress;
                local = step(_e39.x, (_e41 * 2f));
            } else {
                local = 0f;
            }
            let _e47 = local;
            mixFactor = _e47;
        }
    } else {
        {
            let _e48 = checker;
            if (_e48 < 0.5f) {
                let _e51 = grid;
                let _e53 = u.progress;
                local_1 = step(_e51.x, ((_e53 - 0.5f) * 2f));
            } else {
                local_1 = 1f;
            }
            let _e61 = local_1;
            mixFactor = _e61;
        }
    }
    let _e62 = a;
    let _e63 = b;
    let _e64 = mixFactor;
    return mix(_e62, _e63, vec4(_e64));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
