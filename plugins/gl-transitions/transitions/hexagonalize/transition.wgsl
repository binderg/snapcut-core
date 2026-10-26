// Author: Fernando Kuteken
// License: MIT
// Description: Both images turn into hexagons, crossfade, then sharpen again
// Tags: pixelate, hexagon, blend
// Hexagonal math from: http://www.redblobgames.com/grids/hexagons/
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

struct Hexagon {
    q: f32,
    r: f32,
    s: f32,
}

fn createHexagon(q: f32, r: f32) -> Hexagon {
    var q_1: f32;
    var r_1: f32;
    var hex: Hexagon;

    q_1 = q;
    r_1 = r;
    let _e10 = q_1;
    hex.q = _e10;
    let _e12 = r_1;
    hex.r = _e12;
    let _e14 = q_1;
    let _e16 = r_1;
    hex.s = (-(_e14) - _e16);
    let _e18 = hex;
    return _e18;
}

fn roundHexagon(hex_1: Hexagon) -> Hexagon {
    var hex_2: Hexagon;
    var q_2: f32;
    var r_2: f32;
    var s: f32;
    var deltaQ: f32;
    var deltaR: f32;
    var deltaS: f32;

    hex_2 = hex_1;
    let _e6 = hex_2;
    q_2 = floor((_e6.q + 0.5f));
    let _e12 = hex_2;
    r_2 = floor((_e12.r + 0.5f));
    let _e18 = hex_2;
    s = floor((_e18.s + 0.5f));
    let _e24 = q_2;
    let _e25 = hex_2;
    deltaQ = abs((_e24 - _e25.q));
    let _e30 = r_2;
    let _e31 = hex_2;
    deltaR = abs((_e30 - _e31.r));
    let _e36 = s;
    let _e37 = hex_2;
    deltaS = abs((_e36 - _e37.s));
    let _e42 = deltaQ;
    let _e43 = deltaR;
    let _e45 = deltaQ;
    let _e46 = deltaS;
    if ((_e42 > _e43) && (_e45 > _e46)) {
        let _e49 = r_2;
        let _e51 = s;
        q_2 = (-(_e49) - _e51);
    } else {
        let _e53 = deltaR;
        let _e54 = deltaS;
        if (_e53 > _e54) {
            let _e56 = q_2;
            let _e58 = s;
            r_2 = (-(_e56) - _e58);
        } else {
            let _e60 = q_2;
            let _e62 = r_2;
            s = (-(_e60) - _e62);
        }
    }
    let _e64 = q_2;
    let _e65 = r_2;
    let _e66 = createHexagon(_e64, _e65);
    return _e66;
}

fn hexagonFromPoint(point: vec2<f32>, size: f32) -> Hexagon {
    var point_1: vec2<f32>;
    var size_1: f32;
    var q_3: f32;
    var r_3: f32;
    var hex_3: Hexagon;

    point_1 = point;
    size_1 = size;
    let _e9 = point_1;
    let _e11 = (u.resolution.x / u.resolution.y);
    point_1.y = (_e9.y / _e11);
    let _e13 = point_1;
    let _e17 = size_1;
    point_1 = ((_e13 - vec2(0.5f)) / vec2(_e17));
    let _e24 = point_1;
    let _e31 = point_1;
    q_3 = ((0.57735026f * _e24.x) + (-0.33333334f * _e31.y));
    let _e37 = point_1;
    let _e43 = point_1;
    r_3 = ((0f * _e37.x) + (0.6666667f * _e43.y));
    let _e48 = q_3;
    let _e49 = r_3;
    let _e50 = createHexagon(_e48, _e49);
    hex_3 = _e50;
    let _e52 = hex_3;
    let _e53 = roundHexagon(_e52);
    return _e53;
}

fn pointFromHexagon(hex_4: Hexagon, size_2: f32) -> vec2<f32> {
    var hex_5: Hexagon;
    var size_3: f32;
    var x: f32;
    var y: f32;

    hex_5 = hex_4;
    size_3 = size_2;
    let _e10 = hex_5;
    let _e17 = hex_5;
    let _e21 = size_3;
    x = ((((1.7320508f * _e10.q) + (0.8660254f * _e17.r)) * _e21) + 0.5f);
    let _e27 = hex_5;
    let _e33 = hex_5;
    let _e37 = size_3;
    y = ((((0f * _e27.q) + (1.5f * _e33.r)) * _e37) + 0.5f);
    let _e42 = x;
    let _e43 = y;
    let _e44 = (u.resolution.x / u.resolution.y);
    return vec2<f32>(_e42, (_e43 * _e44));
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var dist: f32;
    var local: f32;
    var size_4: f32;
    var local_1: vec2<f32>;
    var point_2: vec2<f32>;

    uv_5 = uv_4;
    let _e7 = u.progress;
    let _e9 = u.progress;
    dist = (2f * min(_e7, (1f - _e9)));
    let _e14 = i32(p.steps);
    if (_e14 > 0i) {
        let _e17 = dist;
        let _e18 = i32(p.steps);
        let _e22 = i32(p.steps);
        local = (ceil((_e17 * f32(_e18))) / f32(_e22));
    } else {
        let _e25 = dist;
        local = _e25;
    }
    let _e27 = local;
    dist = _e27;
    let _e32 = dist;
    let _e34 = p.horizontal_hexagons;
    size_4 = ((0.57735026f * _e32) / _e34);
    let _e37 = dist;
    if (_e37 > 0f) {
        let _e40 = uv_5;
        let _e41 = size_4;
        let _e42 = hexagonFromPoint(_e40, _e41);
        let _e43 = size_4;
        let _e44 = pointFromHexagon(_e42, _e43);
        local_1 = _e44;
    } else {
        let _e45 = uv_5;
        local_1 = _e45;
    }
    let _e47 = local_1;
    point_2 = _e47;
    let _e49 = point_2;
    let _e50 = getFromColor(_e49);
    let _e51 = point_2;
    let _e52 = getToColor(_e51);
    let _e53 = u.progress;
    return mix(_e50, _e52, vec4(_e53));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
