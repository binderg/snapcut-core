// License: MIT
// Author: Xaychru
// Description: The view pans across a mosaic of rotated tiles from both images and settles on the incoming one
// Tags: mosaic, tiles, zoom
// ported by gre from https://gist.github.com/Xaychru/130bb7b7affedbda9df5
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

fn Rand(v: vec2<f32>) -> f32 {
    var v_1: vec2<f32>;

    v_1 = v;
    let _e6 = v_1;
    return fract((sin(dot(_e6.xy, vec2<f32>(12.9898f, 78.233f))) * 43758.547f));
}

fn Rotate(v_2: vec2<f32>, a: f32) -> vec2<f32> {
    var v_3: vec2<f32>;
    var a_1: f32;
    var rm: mat2x2<f32>;

    v_3 = v_2;
    a_1 = a;
    let _e8 = a_1;
    let _e10 = a_1;
    let _e13 = a_1;
    let _e15 = a_1;
    rm = mat2x2<f32>(vec2<f32>(cos(_e8), -(sin(_e10))), vec2<f32>(sin(_e13), cos(_e15)));
    let _e21 = rm;
    let _e22 = v_3;
    return (_e21 * _e22);
}

fn CosInterpolation(x: f32) -> f32 {
    var x_1: f32;

    x_1 = x;
    let _e6 = x_1;
    return ((-(cos((_e6 * 3.1415927f))) / 2f) + 0.5f);
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var glt_p: vec2<f32>;
    var rp: vec2<f32>;
    var rpr: f32;
    var z: f32;
    var az: f32;
    var mrp: vec2<f32>;
    var crp: vec2<f32>;
    var onEnd: bool;
    var ang: f32;

    uv_5 = uv_4;
    let _e6 = uv_5;
    glt_p = ((_e6.xy / vec2(1f)) - vec2(0.5f));
    let _e16 = glt_p;
    rp = _e16;
    let _e18 = u.progress;
    rpr = ((_e18 * 2f) - 1f);
    let _e24 = rpr;
    let _e25 = rpr;
    z = (-(((_e24 * _e25) * 2f)) + 3f);
    let _e33 = z;
    az = abs(_e33);
    let _e36 = rp;
    let _e37 = az;
    rp = (_e36 * _e37);
    let _e39 = rp;
    let _e43 = i32(p.endx);
    let _e47 = i32(p.endy);
    let _e52 = u.progress;
    let _e53 = CosInterpolation(_e52);
    let _e54 = u.progress;
    let _e55 = CosInterpolation(_e54);
    rp = (_e39 + mix(vec2<f32>(0.5f, 0.5f), vec2<f32>((f32(_e43) + 0.5f), (f32(_e47) + 0.5f)), vec2((_e53 * _e55))));
    let _e60 = rp;
    let _e62 = vec2(1f);
    mrp = (_e60 - (floor((_e60 / _e62)) * _e62));
    let _e68 = rp;
    crp = _e68;
    let _e70 = crp;
    let _e74 = i32(p.endx);
    let _e76 = crp;
    let _e80 = i32(p.endy);
    onEnd = ((i32(floor(_e70.x)) == _e74) && (i32(floor(_e76.y)) == _e80));
    let _e84 = onEnd;
    if !(_e84) {
        {
            let _e86 = crp;
            let _e88 = Rand(floor(_e86));
            ang = ((f32(i32((_e88 * 4f))) * 0.5f) * 3.1415927f);
            let _e100 = mrp;
            let _e104 = ang;
            let _e105 = Rotate((_e100 - vec2(0.5f)), _e104);
            mrp = (vec2(0.5f) + _e105);
        }
    }
    let _e107 = onEnd;
    let _e108 = crp;
    let _e110 = Rand(floor(_e108));
    if (_e107 || (_e110 > 0.5f)) {
        {
            let _e114 = mrp;
            let _e115 = getToColor(_e114);
            return _e115;
        }
    } else {
        {
            let _e116 = mrp;
            let _e117 = getFromColor(_e116);
            return _e117;
        }
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
