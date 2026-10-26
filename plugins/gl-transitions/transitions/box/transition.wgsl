// Author: lql
// License: MIT
// Description: The outgoing image shrinks into a box, toward the center or a corner, revealing the incoming one
// Tags: box, zoom, shape
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
    var local: f32;
    var glt_p: f32;
    var x1_: f32;
    var y1_: f32;
    var x2_: f32;
    var y2_: f32;
    var local_1: f32;
    var local_2: f32;
    var local_3: f32;
    var local_4: f32;
    var in_rect: f32;
    var local_5: f32;

    uv_5 = uv_4;
    let _e6 = i32(p.rect_in);
    if (_e6 == 1i) {
        let _e10 = u.progress;
        local = (1f - _e10);
    } else {
        let _e12 = u.progress;
        local = _e12;
    }
    let _e14 = local;
    glt_p = _e14;
    let _e20 = i32(p.location);
    if (_e20 == 0i) {
        {
            let _e25 = glt_p;
            let _e27 = (0.5f * (1f - _e25));
            y1_ = _e27;
            x1_ = _e27;
            let _e29 = x1_;
            let _e30 = (1f - _e29);
            y2_ = _e30;
            x2_ = _e30;
        }
    } else {
        {
            let _e31 = i32(p.location);
            let _e34 = i32(p.location);
            if ((_e31 == 1i) || (_e34 == 2i)) {
                local_1 = 0f;
            } else {
                let _e40 = glt_p;
                local_1 = (1f - _e40);
            }
            let _e43 = local_1;
            x1_ = _e43;
            let _e44 = i32(p.location);
            let _e47 = i32(p.location);
            if ((_e44 == 1i) || (_e47 == 3i)) {
                let _e52 = glt_p;
                local_2 = (1f - _e52);
            } else {
                local_2 = 0f;
            }
            let _e56 = local_2;
            y1_ = _e56;
            let _e57 = i32(p.location);
            let _e60 = i32(p.location);
            if ((_e57 == 1i) || (_e60 == 2i)) {
                let _e64 = glt_p;
                local_3 = _e64;
            } else {
                local_3 = 1f;
            }
            let _e67 = local_3;
            x2_ = _e67;
            let _e68 = i32(p.location);
            let _e71 = i32(p.location);
            if ((_e68 == 1i) || (_e71 == 3i)) {
                local_4 = 1f;
            } else {
                let _e76 = glt_p;
                local_4 = _e76;
            }
            let _e78 = local_4;
            y2_ = _e78;
        }
    }
    let _e79 = x1_;
    let _e80 = uv_5;
    let _e83 = uv_5;
    let _e85 = x2_;
    let _e88 = y1_;
    let _e89 = uv_5;
    let _e93 = uv_5;
    let _e95 = y2_;
    in_rect = (((step(_e79, _e80.x) * step(_e83.x, _e85)) * step(_e88, _e89.y)) * step(_e93.y, _e95));
    let _e99 = i32(p.rect_in);
    if (_e99 == 1i) {
        let _e103 = in_rect;
        local_5 = (1f - _e103);
    } else {
        let _e105 = in_rect;
        local_5 = _e105;
    }
    let _e107 = local_5;
    in_rect = _e107;
    let _e108 = uv_5;
    let _e109 = getFromColor(_e108);
    let _e110 = uv_5;
    let _e111 = getToColor(_e110);
    let _e112 = in_rect;
    return mix(_e109, _e111, vec4(_e112));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
