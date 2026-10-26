// Author: gre
// License: MIT
// Description: The outgoing image opens like double doors as the incoming one comes forward, with a floor reflection
// Tags: doors, 3d, reflection
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

fn bgColor(p_4: vec2<f32>, pto: vec2<f32>) -> vec4<f32> {
    var p_5: vec2<f32>;
    var pto_1: vec2<f32>;
    var c: vec4<f32> = black;

    p_5 = p_4;
    pto_1 = pto;
    let _e13 = pto_1;
    let _e14 = project(_e13);
    pto_1 = _e14;
    let _e15 = pto_1;
    let _e16 = inBounds(_e15);
    if _e16 {
        {
            let _e17 = c;
            let _e18 = pto_1;
            let _e19 = getToColor(_e18);
            let _e20 = p.reflection;
            let _e23 = pto_1;
            c = (_e17 + mix(black, _e19, vec4((_e20 * mix(1f, 0f, _e23.y)))));
        }
    }
    let _e30 = c;
    return _e30;
}

fn glt_main(p_6: vec2<f32>) -> vec4<f32> {
    var p_7: vec2<f32>;
    var pfr: vec2<f32> = vec2(-1f);
    var pto_2: vec2<f32> = vec2(-1f);
    var middleSlit: f32;
    var local: f32;
    var d: f32;
    var size: f32;

    p_7 = p_6;
    let _e19 = p_7;
    let _e25 = u.progress;
    middleSlit = ((2f * abs((_e19.x - 0.5f))) - _e25);
    let _e28 = middleSlit;
    if (_e28 > 0f) {
        {
            let _e31 = p_7;
            let _e32 = p_7;
            if (_e32.x > 0.5f) {
                local = -1f;
            } else {
                local = 1f;
            }
            let _e40 = local;
            let _e42 = u.progress;
            pfr = (_e31 + (_e40 * vec2<f32>((0.5f * _e42), 0f)));
            let _e50 = p.perspective;
            let _e51 = u.progress;
            let _e54 = middleSlit;
            d = (1f / (1f + ((_e50 * _e51) * (1f - _e54))));
            let _e61 = pfr;
            let _e63 = d;
            pfr.y = (_e61.y - (_e63 / 2f));
            let _e68 = pfr;
            let _e70 = d;
            pfr.y = (_e68.y * _e70);
            let _e73 = pfr;
            let _e75 = d;
            pfr.y = (_e73.y + (_e75 / 2f));
        }
    }
    let _e80 = p.depth;
    let _e82 = u.progress;
    size = mix(1f, _e80, (1f - _e82));
    let _e86 = p_7;
    let _e93 = size;
    let _e94 = size;
    pto_2 = (((_e86 + vec2<f32>(-0.5f, -0.5f)) * vec2<f32>(_e93, _e94)) + vec2<f32>(0.5f, 0.5f));
    let _e101 = pfr;
    let _e102 = inBounds(_e101);
    if _e102 {
        {
            let _e103 = pfr;
            let _e104 = getFromColor(_e103);
            return _e104;
        }
    } else {
        let _e105 = pto_2;
        let _e106 = inBounds(_e105);
        if _e106 {
            {
                let _e107 = pto_2;
                let _e108 = getToColor(_e107);
                return _e108;
            }
        } else {
            {
                let _e109 = p_7;
                let _e110 = pto_2;
                let _e111 = bgColor(_e109, _e110);
                return _e111;
            }
        }
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
