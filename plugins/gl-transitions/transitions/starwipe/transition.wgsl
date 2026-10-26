// Author: Ben Lucas
// License: MIT
// Description: A growing star reveals the incoming image
// Tags: wipe, shape, star
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

fn rotate(v: vec2<f32>, theta: f32) -> vec2<f32> {
    var v_1: vec2<f32>;
    var theta_1: f32;
    var cosTheta: f32;
    var sinTheta: f32;

    v_1 = v;
    theta_1 = theta;
    let _e10 = theta_1;
    cosTheta = cos(_e10);
    let _e13 = theta_1;
    sinTheta = sin(_e13);
    let _e16 = cosTheta;
    let _e17 = v_1;
    let _e20 = sinTheta;
    let _e21 = v_1;
    let _e25 = sinTheta;
    let _e26 = v_1;
    let _e29 = cosTheta;
    let _e30 = v_1;
    return vec2<f32>(((_e16 * _e17.x) - (_e20 * _e21.y)), ((_e25 * _e26.x) + (_e29 * _e30.y)));
}

fn inStar(uv_4: vec2<f32>, center: vec2<f32>, radius: f32) -> bool {
    var uv_5: vec2<f32>;
    var center_1: vec2<f32>;
    var radius_1: f32;
    var uv_centered: vec2<f32>;
    var theta_2: f32;
    var uv_rotated: vec2<f32>;
    var slope: f32 = 0.3f;

    uv_5 = uv_4;
    center_1 = center;
    radius_1 = radius;
    let _e12 = uv_5;
    let _e13 = center_1;
    uv_centered = (_e12 - _e13);
    let _e16 = uv_centered;
    let _e17 = p.star_rotation;
    let _e20 = rotate(_e16, (_e17 * 1.2566371f));
    uv_centered = _e20;
    let _e21 = uv_centered;
    let _e23 = uv_centered;
    theta_2 = (atan2(_e21.y, _e23.x) + 3.1415927f);
    let _e29 = uv_centered;
    let _e32 = theta_2;
    let _e39 = rotate(_e29, (-1.2566371f * (floor((_e32 / 1.2566371f)) + 0.5f)));
    uv_rotated = _e39;
    let _e43 = uv_rotated;
    if (_e43.y > 0f) {
        {
            let _e47 = radius_1;
            let _e48 = uv_rotated;
            let _e50 = slope;
            let _e53 = uv_rotated;
            return ((_e47 + (_e48.x * _e50)) > _e53.y);
        }
    } else {
        {
            let _e56 = radius_1;
            let _e58 = uv_rotated;
            let _e60 = slope;
            let _e63 = uv_rotated;
            return ((-(_e56) - (_e58.x * _e60)) < _e63.y);
        }
    }
}

fn glt_main(uv_6: vec2<f32>) -> vec4<f32> {
    var uv_7: vec2<f32>;
    var progressScaled: f32;

    uv_7 = uv_6;
    let _e9 = p.border_thickness;
    let _e13 = u.progress;
    let _e15 = p.border_thickness;
    progressScaled = ((((2f * _e9) + 1f) * _e13) - _e15);
    let _e18 = uv_7;
    let _e19 = p.star_center;
    let _e20 = progressScaled;
    let _e21 = inStar(_e18, _e19, _e20);
    if _e21 {
        {
            let _e22 = uv_7;
            let _e23 = getToColor(_e22);
            return _e23;
        }
    } else {
        let _e24 = uv_7;
        let _e25 = p.star_center;
        let _e26 = progressScaled;
        let _e27 = p.border_thickness;
        let _e29 = inStar(_e24, _e25, (_e26 + _e27));
        if _e29 {
            {
                let _e30 = p.border_color;
                return _e30;
            }
        } else {
            {
                let _e31 = uv_7;
                let _e32 = getFromColor(_e31);
                return _e32;
            }
        }
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
