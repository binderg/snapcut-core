// Author: zhmy
// License: MIT
// Description: The outgoing image shrinks away upward while the incoming one slides in from the bottom
// Tags: slide, zoom, vertical
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
    let _e7 = p_1;
    let _e10 = p_1;
    return (all((boundMin < _e7)) && all((_e10 < boundMax)));
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var spfr: vec2<f32>;
    var spto: vec2<f32> = vec2(-1f);
    var size: f32;

    uv_5 = uv_4;
    let _e14 = u.progress;
    size = mix(1f, 3f, (_e14 * 0.2f));
    let _e19 = uv_5;
    let _e26 = size;
    let _e27 = size;
    spto = (((_e19 + vec2<f32>(-0.5f, -0.5f)) * vec2<f32>(_e26, _e27)) + vec2<f32>(0.5f, 0.5f));
    let _e34 = uv_5;
    let _e37 = u.progress;
    spfr = (_e34 + vec2<f32>(0f, (1f - _e37)));
    let _e41 = spfr;
    let _e42 = inBounds(_e41);
    if _e42 {
        {
            let _e43 = spfr;
            let _e44 = getToColor(_e43);
            return _e44;
        }
    } else {
        let _e45 = spto;
        let _e46 = inBounds(_e45);
        if _e46 {
            {
                let _e47 = spto;
                let _e48 = getFromColor(_e47);
                let _e50 = u.progress;
                return (_e48 * (1f - _e50));
            }
        } else {
            {
                return black;
            }
        }
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
