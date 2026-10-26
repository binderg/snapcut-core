// Author: OllyOllyOlly
// License: MIT
// Description: The incoming image slides in as two halves moving vertically in opposite directions
// Tags: slide, split, vertical
//
// @param Swap the directions of the halves
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

const boundMin: vec2<f32> = vec2<f32>(0f, 0f);

const boundMax: vec2<f32> = vec2<f32>(1f, 1f);

fn inBounds(glt_p: vec2<f32>) -> bool {
    var p_1: vec2<f32>;

    p_1 = glt_p;
    let _e7 = p_1;
    let _e10 = p_1;
    return (all((boundMin < _e7)) && all((_e10 < boundMax)));
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var local: f32;
    var modifier: f32;
    var local_1: vec2<f32>;
    var p_2: vec2<f32>;
    var local_2: vec4<f32>;

    uv_5 = uv_4;
    let _e7 = (p.reverse > 0.5);
    if _e7 {
        local = -1f;
    } else {
        local = 1f;
    }
    let _e12 = local;
    modifier = _e12;
    let _e14 = uv_5;
    if (_e14.x > 0.5f) {
        let _e18 = uv_5;
        let _e20 = uv_5;
        let _e22 = u.progress;
        let _e23 = modifier;
        let _e26 = modifier;
        local_1 = vec2<f32>(_e18.x, ((_e20.y - (_e22 * _e23)) + _e26));
    } else {
        let _e29 = uv_5;
        let _e31 = uv_5;
        let _e33 = u.progress;
        let _e34 = modifier;
        let _e37 = modifier;
        local_1 = vec2<f32>(_e29.x, ((_e31.y + (_e33 * _e34)) - _e37));
    }
    let _e41 = local_1;
    p_2 = _e41;
    let _e43 = p_2;
    let _e44 = inBounds(_e43);
    if _e44 {
        let _e45 = p_2;
        let _e46 = getToColor(_e45);
        local_2 = _e46;
    } else {
        let _e47 = uv_5;
        let _e48 = getFromColor(_e47);
        local_2 = _e48;
    }
    let _e50 = local_2;
    return _e50;
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
