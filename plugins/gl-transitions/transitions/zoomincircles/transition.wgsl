// License: MIT
// Author: dycm8009
// Description: Concentric rings of the outgoing image zoom at different speeds before revealing the incoming one
// Tags: zoom, circle
// ported by gre from https://gist.github.com/dycm8009/948e99b1800e81ad909a
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

fn zoom(uv_4: vec2<f32>, amount: f32) -> vec2<f32> {
    var uv_5: vec2<f32>;
    var amount_1: f32;

    uv_5 = uv_4;
    amount_1 = amount;
    let _e7 = uv_5;
    let _e11 = amount_1;
    return (vec2(0.5f) + ((_e7 - vec2(0.5f)) * _e11));
}

fn glt_main(uv_6: vec2<f32>) -> vec4<f32> {
    var uv_7: vec2<f32>;
    var ratio2_: vec2<f32>;
    var r: vec2<f32>;
    var pro: f32;
    var z: f32;
    var t: f32 = 0f;

    uv_7 = uv_6;
    let _e6 = (u.resolution.x / u.resolution.y);
    ratio2_ = vec2<f32>(1f, (1f / _e6));
    let _e11 = uv_7;
    let _e17 = ratio2_;
    r = (2f * ((vec2<f32>(_e11.xy) - vec2(0.5f)) * _e17));
    let _e21 = u.progress;
    pro = (_e21 / 0.8f);
    let _e25 = pro;
    z = (_e25 * 0.2f);
    let _e31 = pro;
    if (_e31 > 1f) {
        {
            let _e35 = pro;
            z = (0.2f + ((_e35 - 1f) * 5f));
            let _e41 = u.progress;
            t = clamp(((_e41 - 0.8f) / 0.07f), 0f, 1f);
        }
    }
    let _e49 = r;
    let _e52 = z;
    if (length(_e49) < (0.5f + _e52)) {
        {
        }
    } else {
        let _e55 = r;
        let _e58 = z;
        if (length(_e55) < (0.8f + (_e58 * 1.5f))) {
            {
                let _e63 = uv_7;
                let _e66 = pro;
                let _e69 = zoom(_e63, (1f - (0.15f * _e66)));
                uv_7 = _e69;
                let _e70 = t;
                t = (_e70 * 0.5f);
            }
        } else {
            let _e73 = r;
            let _e76 = z;
            if (length(_e73) < (1.2f + (_e76 * 2.5f))) {
                {
                    let _e81 = uv_7;
                    let _e84 = pro;
                    let _e87 = zoom(_e81, (1f - (0.2f * _e84)));
                    uv_7 = _e87;
                    let _e88 = t;
                    t = (_e88 * 0.2f);
                }
            } else {
                {
                    let _e91 = uv_7;
                    let _e94 = pro;
                    let _e97 = zoom(_e91, (1f - (0.25f * _e94)));
                    uv_7 = _e97;
                }
            }
        }
    }
    let _e98 = uv_7;
    let _e99 = getFromColor(_e98);
    let _e100 = uv_7;
    let _e101 = getToColor(_e100);
    let _e102 = t;
    return mix(_e99, _e101, vec4(_e102));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
