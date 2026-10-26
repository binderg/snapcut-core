// Author: gre
// License: MIT
// Description: The two images are faces of a rotating 3D cube, with a floor reflection
// Tags: 3d, cube, rotate, reflection
// @range(0, 1) Amount of perspective
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

fn project(glt_p: vec2<f32>) -> vec2<f32> {
    var p_1: vec2<f32>;

    p_1 = glt_p;
    let _e8 = p_1;
    let _e15 = p.floating;
    return ((_e8 * vec2<f32>(1f, -1.2f)) + vec2<f32>(0f, (-(_e15) / 100f)));
}

fn inBounds(p_2: vec2<f32>) -> bool {
    var p_3: vec2<f32>;

    p_3 = p_2;
    let _e10 = p_3;
    let _e13 = p_3;
    return (all((vec2(0f) < _e10)) && all((_e13 < vec2(1f))));
}

fn bgColor(p_4: vec2<f32>, pfr: vec2<f32>, pto: vec2<f32>) -> vec4<f32> {
    var p_5: vec2<f32>;
    var pfr_1: vec2<f32>;
    var pto_1: vec2<f32>;
    var c: vec4<f32> = vec4<f32>(0f, 0f, 0f, 1f);

    p_5 = p_4;
    pfr_1 = pfr;
    pto_1 = pto;
    let _e18 = pfr_1;
    let _e19 = project(_e18);
    pfr_1 = _e19;
    let _e20 = pfr_1;
    let _e21 = inBounds(_e20);
    if _e21 {
        {
            let _e22 = c;
            let _e25 = pfr_1;
            let _e26 = getFromColor(_e25);
            let _e27 = p.reflection;
            let _e30 = pfr_1;
            c = (_e22 + mix(vec4(0f), _e26, vec4((_e27 * mix(1f, 0f, _e30.y)))));
        }
    }
    let _e37 = pto_1;
    let _e38 = project(_e37);
    pto_1 = _e38;
    let _e39 = pto_1;
    let _e40 = inBounds(_e39);
    if _e40 {
        {
            let _e41 = c;
            let _e44 = pto_1;
            let _e45 = getToColor(_e44);
            let _e46 = p.reflection;
            let _e49 = pto_1;
            c = (_e41 + mix(vec4(0f), _e45, vec4((_e46 * mix(1f, 0f, _e49.y)))));
        }
    }
    let _e56 = c;
    return _e56;
}

fn xskew(p_6: vec2<f32>, persp: f32, center: f32) -> vec2<f32> {
    var p_7: vec2<f32>;
    var persp_1: f32;
    var center_1: f32;
    var x: f32;
    var local: f32;
    var local_1: f32;

    p_7 = p_6;
    persp_1 = persp;
    center_1 = center;
    let _e12 = p_7;
    let _e15 = p_7;
    let _e18 = center_1;
    x = mix(_e12.x, (1f - _e15.x), _e18);
    let _e21 = x;
    let _e22 = p_7;
    let _e26 = persp_1;
    let _e29 = x;
    let _e33 = persp_1;
    let _e36 = x;
    let _e42 = center_1;
    let _e50 = center_1;
    let _e54 = center_1;
    if (_e54 < 0.5f) {
        local = 1f;
    } else {
        local = -1f;
    }
    let _e61 = local;
    let _e66 = center_1;
    if (_e66 < 0.5f) {
        local_1 = 0f;
    } else {
        local_1 = 1f;
    }
    let _e72 = local_1;
    return (((vec2<f32>(_e21, ((_e22.y - ((0.5f * (1f - _e26)) * _e29)) / (1f + ((_e33 - 1f) * _e36)))) - vec2<f32>((0.5f - distance(_e42, 0.5f)), 0f)) * vec2<f32>(((0.5f / distance(_e50, 0.5f)) * _e61), 1f)) + vec2<f32>(_e72, 0f));
}

fn glt_main(op: vec2<f32>) -> vec4<f32> {
    var op_1: vec2<f32>;
    var uz: f32;
    var p_8: vec2<f32>;
    var fromP: vec2<f32>;
    var toP: vec2<f32>;

    op_1 = op;
    let _e8 = p.unzoom;
    let _e13 = u.progress;
    uz = ((_e8 * 2f) * (0.5f - distance(0.5f, _e13)));
    let _e18 = uz;
    let _e23 = uz;
    let _e25 = op_1;
    p_8 = (vec2((-(_e18) * 0.5f)) + ((1f + _e23) * _e25));
    let _e30 = p_8;
    let _e31 = u.progress;
    let _e36 = u.progress;
    let _e42 = u.progress;
    let _e44 = p.persp;
    let _e48 = xskew(((_e30 - vec2<f32>(_e31, 0f)) / vec2<f32>((1f - _e36), 1f)), (1f - mix(_e42, 0f, _e44)), 0f);
    fromP = _e48;
    let _e50 = p_8;
    let _e51 = u.progress;
    let _e55 = u.progress;
    let _e59 = p.persp;
    let _e62 = xskew((_e50 / vec2<f32>(_e51, 1f)), mix(pow(_e55, 2f), 1f, _e59), 1f);
    toP = _e62;
    let _e64 = fromP;
    let _e65 = inBounds(_e64);
    if _e65 {
        {
            let _e66 = fromP;
            let _e67 = getFromColor(_e66);
            return _e67;
        }
    } else {
        let _e68 = toP;
        let _e69 = inBounds(_e68);
        if _e69 {
            {
                let _e70 = toP;
                let _e71 = getToColor(_e70);
                return _e71;
            }
        }
    }
    let _e72 = op_1;
    let _e73 = fromP;
    let _e74 = toP;
    let _e75 = bgColor(_e72, _e73, _e74);
    return _e75;
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
