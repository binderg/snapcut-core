// Author: OllyOllyOlly
// License: MIT
// Description: The outgoing image slides out as two halves moving horizontally in opposite directions
// Tags: slide, split, horizontal
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
    var local_1: f32;
    var modifier: f32;
    var p_2: vec2<f32>;
    var local_2: vec4<f32>;

    uv_5 = uv_4;
    let _e7 = uv_5;
    if (_e7.y > 0.5f) {
        local = 1f;
    } else {
        local = -1f;
    }
    let _e15 = local;
    let _e16 = (p.reverse > 0.5);
    if _e16 {
        local_1 = -1f;
    } else {
        local_1 = 1f;
    }
    let _e21 = local_1;
    modifier = (_e15 * _e21);
    let _e24 = uv_5;
    let _e26 = u.progress;
    let _e27 = modifier;
    let _e30 = uv_5;
    p_2 = vec2<f32>((_e24.x + (_e26 * _e27)), _e30.y);
    let _e34 = p_2;
    let _e35 = inBounds(_e34);
    if _e35 {
        let _e36 = p_2;
        let _e37 = getFromColor(_e36);
        local_2 = _e37;
    } else {
        let _e38 = uv_5;
        let _e39 = getToColor(_e38);
        local_2 = _e39;
    }
    let _e41 = local_2;
    return _e41;
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
