// Author: gre
// License: MIT
// Description: A crossfade through a blur
// Tags: blur, blend
// @param Amount of blur
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

const passes: i32 = 6i;

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var c1_: vec4<f32> = vec4(0f);
    var c2_: vec4<f32> = vec4(0f);
    var disp: f32;
    var xi: i32 = 0i;
    var x: f32;
    var yi: i32;
    var y: f32;
    var v: vec2<f32>;
    var d: f32;

    uv_5 = uv_4;
    let _e12 = p.intensity;
    let _e15 = u.progress;
    disp = (_e12 * (0.5f - distance(0.5f, _e15)));
    loop {
        let _e22 = xi;
        if !((_e22 < passes)) {
            break;
        }
        {
            let _e28 = xi;
            x = ((f32(_e28) / 6f) - 0.5f);
            yi = 0i;
            loop {
                let _e38 = yi;
                if !((_e38 < passes)) {
                    break;
                }
                {
                    let _e44 = yi;
                    y = ((f32(_e44) / 6f) - 0.5f);
                    let _e52 = x;
                    let _e53 = y;
                    v = vec2<f32>(_e52, _e53);
                    let _e56 = disp;
                    d = _e56;
                    let _e58 = c1_;
                    let _e59 = uv_5;
                    let _e60 = d;
                    let _e61 = v;
                    let _e64 = getFromColor((_e59 + (_e60 * _e61)));
                    c1_ = (_e58 + _e64);
                    let _e66 = c2_;
                    let _e67 = uv_5;
                    let _e68 = d;
                    let _e69 = v;
                    let _e72 = getToColor((_e67 + (_e68 * _e69)));
                    c2_ = (_e66 + _e72);
                }
                continuing {
                    let _e41 = yi;
                    yi = (_e41 + 1i);
                }
            }
        }
        continuing {
            let _e25 = xi;
            xi = (_e25 + 1i);
        }
    }
    let _e74 = c1_;
    c1_ = (_e74 / vec4(36f));
    let _e81 = c2_;
    c2_ = (_e81 / vec4(36f));
    let _e88 = c1_;
    let _e89 = c2_;
    let _e90 = u.progress;
    return mix(_e88, _e89, vec4(_e90));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
