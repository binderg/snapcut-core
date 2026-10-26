// Author: gre
// License: MIT
// Description: The two images swap places in 3D, with a floor reflection
// Tags: 3d, swap, reflection
// General parameters
// @param Strength of the reflection
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

const black: vec4<f32> = vec4<f32>(0f, 0f, 0f, 1f);

const boundMin: vec2<f32> = vec2<f32>(0f, 0f);

const boundMax: vec2<f32> = vec2<f32>(1f, 1f);

fn inBounds(glt_p: vec2<f32>) -> bool {
    var p_1: vec2<f32>;

    p_1 = glt_p;
    let _e10 = p_1;
    let _e13 = p_1;
    return (all((boundMin < _e10)) && all((_e13 < boundMax)));
}

fn project(p_2: vec2<f32>) -> vec2<f32> {
    var p_3: vec2<f32>;

    p_3 = p_2;
    let _e10 = p_3;
    return ((_e10 * vec2<f32>(1f, -1.2f)) + vec2<f32>(0f, -0.02f));
}

fn bgColor(p_4: vec2<f32>, pfr: vec2<f32>, pto: vec2<f32>) -> vec4<f32> {
    var p_5: vec2<f32>;
    var pfr_1: vec2<f32>;
    var pto_1: vec2<f32>;
    var c: vec4<f32> = black;

    p_5 = p_4;
    pfr_1 = pfr;
    pto_1 = pto;
    let _e15 = pfr_1;
    let _e16 = project(_e15);
    pfr_1 = _e16;
    let _e17 = pfr_1;
    let _e18 = inBounds(_e17);
    if _e18 {
        {
            let _e19 = c;
            let _e20 = pfr_1;
            let _e21 = getFromColor(_e20);
            let _e22 = p.reflection;
            let _e25 = pfr_1;
            c = (_e19 + mix(black, _e21, vec4((_e22 * mix(1f, 0f, _e25.y)))));
        }
    }
    let _e32 = pto_1;
    let _e33 = project(_e32);
    pto_1 = _e33;
    let _e34 = pto_1;
    let _e35 = inBounds(_e34);
    if _e35 {
        {
            let _e36 = c;
            let _e37 = pto_1;
            let _e38 = getToColor(_e37);
            let _e39 = p.reflection;
            let _e42 = pto_1;
            c = (_e36 + mix(black, _e38, vec4((_e39 * mix(1f, 0f, _e42.y)))));
        }
    }
    let _e49 = c;
    return _e49;
}

fn glt_main(p_6: vec2<f32>) -> vec4<f32> {
    var p_7: vec2<f32>;
    var pfr_2: vec2<f32>;
    var pto_2: vec2<f32> = vec2(-1f);
    var size: f32;
    var persp: f32;

    p_7 = p_6;
    let _e16 = p.depth;
    let _e17 = u.progress;
    size = mix(1f, _e16, _e17);
    let _e20 = p.perspective;
    let _e21 = u.progress;
    persp = (_e20 * _e21);
    let _e24 = p_7;
    let _e31 = size;
    let _e33 = p.perspective;
    let _e34 = u.progress;
    let _e38 = size;
    let _e40 = size;
    let _e41 = persp;
    let _e43 = p_7;
    pfr_2 = (((_e24 + vec2<f32>(-0f, -0.5f)) * vec2<f32>((_e31 / (1f - (_e33 * _e34))), (_e38 / (1f - ((_e40 * _e41) * _e43.x))))) + vec2<f32>(0f, 0.5f));
    let _e55 = p.depth;
    let _e57 = u.progress;
    size = mix(1f, _e55, (1f - _e57));
    let _e60 = p.perspective;
    let _e62 = u.progress;
    persp = (_e60 * (1f - _e62));
    let _e65 = p_7;
    let _e72 = size;
    let _e74 = p.perspective;
    let _e76 = u.progress;
    let _e81 = size;
    let _e83 = size;
    let _e84 = persp;
    let _e87 = p_7;
    pto_2 = (((_e65 + vec2<f32>(-1f, -0.5f)) * vec2<f32>((_e72 / (1f - (_e74 * (1f - _e76)))), (_e81 / (1f - ((_e83 * _e84) * (0.5f - _e87.x)))))) + vec2<f32>(1f, 0.5f));
    let _e99 = u.progress;
    if (_e99 < 0.5f) {
        {
            let _e102 = pfr_2;
            let _e103 = inBounds(_e102);
            if _e103 {
                {
                    let _e104 = pfr_2;
                    let _e105 = getFromColor(_e104);
                    return _e105;
                }
            }
            let _e106 = pto_2;
            let _e107 = inBounds(_e106);
            if _e107 {
                {
                    let _e108 = pto_2;
                    let _e109 = getToColor(_e108);
                    return _e109;
                }
            }
        }
    }
    let _e110 = pto_2;
    let _e111 = inBounds(_e110);
    if _e111 {
        {
            let _e112 = pto_2;
            let _e113 = getToColor(_e112);
            return _e113;
        }
    }
    let _e114 = pfr_2;
    let _e115 = inBounds(_e114);
    if _e115 {
        {
            let _e116 = pfr_2;
            let _e117 = getFromColor(_e116);
            return _e117;
        }
    }
    let _e118 = p_7;
    let _e119 = pfr_2;
    let _e120 = pto_2;
    let _e121 = bgColor(_e118, _e119, _e120);
    return _e121;
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
