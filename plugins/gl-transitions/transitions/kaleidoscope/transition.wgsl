// Author: nwoeanhinnogaehr
// License: MIT
// Description: A crossfade through a spinning kaleidoscope
// Tags: kaleidoscope, distort, blend
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
    var glt_p: vec2<f32>;
    var q: vec2<f32>;
    var t: f32;
    var i: i32 = 0i;

    uv_5 = uv_4;
    let _e7 = uv_5;
    glt_p = (_e7.xy / vec2(1f));
    let _e14 = glt_p;
    q = _e14;
    let _e16 = u.progress;
    let _e17 = p.power;
    let _e19 = p.speed;
    t = (pow(_e16, _e17) * _e19);
    let _e22 = glt_p;
    glt_p = (_e22 - vec2(0.5f));
    loop {
        let _e28 = i;
        if !((_e28 < 7i)) {
            break;
        }
        {
            let _e35 = t;
            let _e37 = glt_p;
            let _e40 = t;
            let _e42 = glt_p;
            let _e46 = t;
            let _e48 = glt_p;
            let _e51 = t;
            let _e53 = glt_p;
            glt_p = vec2<f32>(((sin(_e35) * _e37.x) + (cos(_e40) * _e42.y)), ((sin(_e46) * _e48.y) - (cos(_e51) * _e53.x)));
            let _e58 = t;
            let _e59 = p.angle;
            t = (_e58 + _e59);
            let _e61 = glt_p;
            let _e63 = vec2(2f);
            glt_p = abs(((_e61 - (floor((_e61 / _e63)) * _e63)) - vec2(1f)));
        }
        continuing {
            let _e32 = i;
            i = (_e32 + 1i);
        }
    }
    let _e72 = glt_p;
    let _e74 = vec2(1f);
    let _e80 = q;
    let _e81 = getFromColor(_e80);
    let _e82 = q;
    let _e83 = getToColor(_e82);
    let _e84 = u.progress;
    let _e87 = glt_p;
    let _e88 = getFromColor(_e87);
    let _e89 = glt_p;
    let _e90 = getToColor(_e89);
    let _e91 = u.progress;
    let _e96 = u.progress;
    return mix(mix(_e81, _e83, vec4(_e84)), mix(_e88, _e90, vec4(_e91)), vec4((1f - (2f * abs((_e96 - 0.5f))))));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
