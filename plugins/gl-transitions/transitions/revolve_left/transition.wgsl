// Author: bread
// License: MIT
// Description: The outgoing image swirls and zooms with motion blur into the incoming one
// Tags: swirl, zoom, rotate, blur
// gl-transitions v1 compatible
//
// @param Center of the swirl
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

fn sat(x: f32) -> f32 {
    var x_1: f32;

    x_1 = x;
    let _e14 = x_1;
    return clamp(_e14, 0f, 1f);
}

fn ease(x_2: f32) -> f32 {
    var x_3: f32;

    x_3 = x_2;
    let _e14 = x_3;
    let _e15 = sat(_e14);
    x_3 = _e15;
    let _e16 = x_3;
    let _e17 = x_3;
    let _e21 = x_3;
    return ((_e16 * _e17) * (3f - (2f * _e21)));
}

fn revolveEnvelope(t: f32) -> f32 {
    var t_1: f32;
    var rise: f32;
    var fall: f32;

    t_1 = t;
    let _e14 = t_1;
    let _e19 = ease(((_e14 - 0.1f) / 0.33f));
    rise = _e19;
    let _e22 = t_1;
    let _e27 = ease(((_e22 - 0.43f) / 0.29f));
    fall = (1f - _e27);
    let _e30 = rise;
    let _e31 = fall;
    return (_e30 * _e31);
}

fn rotate2_(glt_p: vec2<f32>, a: f32) -> vec2<f32> {
    var p_1: vec2<f32>;
    var a_1: f32;
    var s: f32;
    var c: f32;

    p_1 = glt_p;
    a_1 = a;
    let _e16 = a_1;
    s = sin(_e16);
    let _e19 = a_1;
    c = cos(_e19);
    let _e22 = c;
    let _e23 = p_1;
    let _e26 = s;
    let _e27 = p_1;
    let _e31 = s;
    let _e32 = p_1;
    let _e35 = c;
    let _e36 = p_1;
    return vec2<f32>(((_e22 * _e23.x) - (_e26 * _e27.y)), ((_e31 * _e32.x) + (_e35 * _e36.y)));
}

fn warpUv(uv_4: vec2<f32>, t_2: f32) -> vec2<f32> {
    var uv_5: vec2<f32>;
    var t_3: f32;
    var e: f32;
    var p_2: vec2<f32>;
    var r: f32;
    var edgeSpin: f32;
    var coreSpin: f32;
    var visibleAngle: f32;
    var sc: f32;
    var rr: f32;

    uv_5 = uv_4;
    t_3 = t_2;
    let _e16 = t_3;
    let _e17 = revolveEnvelope(_e16);
    e = _e17;
    let _e19 = uv_5;
    let _e20 = p.center;
    p_2 = (_e19 - _e20);
    let _e24 = p_2;
    let _e26 = (u.resolution.x / u.resolution.y);
    p_2.x = (_e24.x * _e26);
    let _e28 = p_2;
    r = length(_e28);
    let _e31 = p.max_rotation;
    let _e32 = e;
    edgeSpin = (_e31 * _e32);
    let _e35 = p.swirl;
    let _e36 = e;
    let _e39 = r;
    let _e42 = sat((_e39 / 0.96f));
    coreSpin = ((_e35 * _e36) * pow((1f - _e42), 1.55f));
    let _e48 = p.direction;
    let _e49 = edgeSpin;
    let _e50 = coreSpin;
    visibleAngle = (_e48 * (_e49 + _e50));
    let _e54 = p_2;
    let _e55 = visibleAngle;
    let _e57 = rotate2_(_e54, -(_e55));
    p_2 = _e57;
    let _e59 = p.peak_zoom;
    let _e62 = e;
    sc = (1f + ((_e59 - 1f) * pow(_e62, 0.85f)));
    let _e68 = p_2;
    let _e69 = sc;
    p_2 = (_e68 / vec2(_e69));
    let _e72 = p_2;
    rr = length(_e72);
    let _e75 = p_2;
    let _e77 = p.barrel;
    let _e78 = e;
    let _e80 = rr;
    let _e82 = rr;
    p_2 = (_e75 * (1f + ((((_e77 * _e78) * _e80) * _e82) * 2.8f)));
    let _e89 = p_2;
    let _e91 = (u.resolution.x / u.resolution.y);
    p_2.x = (_e89.x / _e91);
    let _e93 = p_2;
    let _e94 = p.center;
    return clamp((_e93 + _e94), vec2(0.001f), vec2(0.999f));
}

fn sampleRevolve(uv_6: vec2<f32>, t_4: f32) -> vec4<f32> {
    var uv_7: vec2<f32>;
    var t_5: f32;
    var p_3: vec2<f32>;
    var reveal: f32;

    uv_7 = uv_6;
    t_5 = t_4;
    let _e16 = uv_7;
    let _e17 = t_5;
    let _e18 = warpUv(_e16, _e17);
    p_3 = _e18;
    let _e20 = p.switch_start;
    let _e21 = p.switch_end;
    let _e22 = t_5;
    reveal = smoothstep(_e20, _e21, _e22);
    let _e25 = p_3;
    let _e26 = getFromColor(_e25);
    let _e27 = p_3;
    let _e28 = getToColor(_e27);
    let _e29 = reveal;
    return mix(_e26, _e28, vec4(_e29));
}

fn glt_main(uv_8: vec2<f32>) -> vec4<f32> {
    var uv_9: vec2<f32>;
    var e_1: f32;
    var span: f32;
    var color: vec4<f32> = vec4(0f);
    var total: f32 = 0f;
    var i: i32 = -8i;
    var x_4: f32;
    var w: f32;
    var t_6: f32;
    var q: vec2<f32>;
    var vignette: f32;

    uv_9 = uv_8;
    let _e14 = u.progress;
    if (_e14 <= 0f) {
        let _e17 = uv_9;
        let _e18 = getFromColor(_e17);
        return _e18;
    }
    let _e19 = u.progress;
    if (_e19 >= 1f) {
        let _e22 = uv_9;
        let _e23 = getToColor(_e22);
        return _e23;
    }
    let _e24 = u.progress;
    let _e25 = revolveEnvelope(_e24);
    e_1 = _e25;
    let _e28 = p.motion_blur;
    let _e30 = e_1;
    span = ((0.06f * _e28) * _e30);
    loop {
        let _e41 = i;
        if !((_e41 <= 8i)) {
            break;
        }
        {
            let _e48 = i;
            x_4 = (f32(_e48) / 8f);
            let _e54 = x_4;
            w = (1f - abs(_e54));
            let _e58 = w;
            let _e59 = w;
            w = ((_e58 * _e59) + 0.01f);
            let _e63 = u.progress;
            let _e64 = x_4;
            let _e65 = span;
            let _e68 = sat((_e63 + (_e64 * _e65)));
            t_6 = _e68;
            let _e70 = color;
            let _e71 = uv_9;
            let _e72 = t_6;
            let _e73 = sampleRevolve(_e71, _e72);
            let _e74 = w;
            color = (_e70 + (_e73 * _e74));
            let _e77 = total;
            let _e78 = w;
            total = (_e77 + _e78);
        }
        continuing {
            let _e45 = i;
            i = (_e45 + 1i);
        }
    }
    let _e80 = color;
    let _e81 = total;
    color = (_e80 / vec4(_e81));
    let _e84 = uv_9;
    q = (_e84 - vec2(0.5f));
    let _e90 = q;
    let _e92 = (u.resolution.x / u.resolution.y);
    q.x = (_e90.x * _e92);
    let _e95 = p.shadow;
    let _e96 = e_1;
    let _e100 = q;
    vignette = (1f - ((_e95 * _e96) * smoothstep(0.35f, 0.95f, length(_e100))));
    let _e106 = color;
    let _e108 = color;
    let _e110 = vignette;
    let _e111 = (_e108.xyz * _e110);
    color.x = _e111.x;
    color.y = _e111.y;
    color.z = _e111.z;
    let _e118 = color;
    return _e118;
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
