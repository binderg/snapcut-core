// License: MIT
// Author: Sergey Kosarevsky
// Description: Both images swirl around the center while crossfading
// Tags: swirl, rotate, blend
// ( http://www.linderdaum.com )
// ported by gre from https://gist.github.com/corporateshark/cacfedb8cca0f5ce3f7c
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

fn glt_main(UV: vec2<f32>) -> vec4<f32> {
    var UV_1: vec2<f32>;
    var Radius: f32 = 1f;
    var T: f32;
    var Dist: f32;
    var Percent: f32;
    var local: f32;
    var A: f32;
    var Theta: f32;
    var S: f32;
    var C: f32;
    var C0_: vec4<f32>;
    var C1_: vec4<f32>;

    UV_1 = UV;
    let _e6 = u.progress;
    T = _e6;
    let _e8 = UV_1;
    UV_1 = (_e8 - vec2<f32>(0.5f, 0.5f));
    let _e13 = UV_1;
    Dist = length(_e13);
    let _e16 = Dist;
    let _e17 = Radius;
    if (_e16 < _e17) {
        {
            let _e19 = Radius;
            let _e20 = Dist;
            let _e22 = Radius;
            Percent = ((_e19 - _e20) / _e22);
            let _e25 = T;
            if (_e25 <= 0.5f) {
                let _e30 = T;
                local = mix(0f, 1f, (_e30 / 0.5f));
            } else {
                let _e36 = T;
                local = mix(1f, 0f, ((_e36 - 0.5f) / 0.5f));
            }
            let _e43 = local;
            A = _e43;
            let _e45 = Percent;
            let _e46 = Percent;
            let _e48 = A;
            Theta = ((((_e45 * _e46) * _e48) * 8f) * 3.14159f);
            let _e55 = Theta;
            S = sin(_e55);
            let _e58 = Theta;
            C = cos(_e58);
            let _e61 = UV_1;
            let _e62 = C;
            let _e63 = S;
            let _e67 = UV_1;
            let _e68 = S;
            let _e69 = C;
            UV_1 = vec2<f32>(dot(_e61, vec2<f32>(_e62, -(_e63))), dot(_e67, vec2<f32>(_e68, _e69)));
        }
    }
    let _e73 = UV_1;
    UV_1 = (_e73 + vec2<f32>(0.5f, 0.5f));
    let _e78 = UV_1;
    let _e79 = getFromColor(_e78);
    C0_ = _e79;
    let _e81 = UV_1;
    let _e82 = getToColor(_e81);
    C1_ = _e82;
    let _e84 = C0_;
    let _e85 = C1_;
    let _e86 = T;
    return mix(_e84, _e85, vec4(_e86));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
