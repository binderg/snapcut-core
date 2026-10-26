// Author: Brandon Anzaldi
// License: MIT
// Description: Cuts to full-screen TV static, then to the incoming image
// Tags: static, noise, retro
// @param Part of the transition, at each end, that shows the images instead of static
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

fn noise(co: vec2<f32>) -> f32 {
    var co_1: vec2<f32>;
    var a: f32 = 12.9898f;
    var b: f32 = 78.233f;
    var c: f32 = 43758.547f;
    var dt: f32;
    var sn: f32;

    co_1 = co;
    let _e11 = co_1;
    let _e13 = u.progress;
    let _e15 = a;
    let _e16 = b;
    dt = dot((_e11.xy * _e13), vec2<f32>(_e15, _e16));
    let _e20 = dt;
    sn = (_e20 - (floor((_e20 / 3.14f)) * 3.14f));
    let _e27 = sn;
    let _e29 = c;
    return fract((sin(_e27) * _e29));
}

fn glt_main(glt_p: vec2<f32>) -> vec4<f32> {
    var p_1: vec2<f32>;

    p_1 = glt_p;
    let _e5 = u.progress;
    let _e6 = p.offset;
    if (_e5 < _e6) {
        {
            let _e8 = p_1;
            let _e9 = getFromColor(_e8);
            return _e9;
        }
    } else {
        let _e10 = u.progress;
        let _e12 = p.offset;
        if (_e10 > (1f - _e12)) {
            {
                let _e15 = p_1;
                let _e16 = getToColor(_e15);
                return _e16;
            }
        } else {
            {
                let _e17 = p_1;
                let _e18 = noise(_e17);
                let _e19 = vec3(_e18);
                return vec4<f32>(_e19.x, _e19.y, _e19.z, 1f);
            }
        }
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
