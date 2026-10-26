// Author: Sergey Kosarevsky
// License: MIT
// Description: A crossfade through an out-of-focus blur
// Tags: blur, blend
// Ported from https://gist.github.com/corporateshark/b9f8e5675c647e615419
//
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

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var T: f32;
    var half: f32 = 0.5f;
    var local: f32;
    var D: f32;
    var C0_: vec4<f32>;
    var C1_: vec4<f32>;

    uv_5 = uv_4;
    let _e5 = u.progress;
    T = _e5;
    let _e9 = T;
    let _e10 = half;
    if (_e9 < _e10) {
        let _e13 = p.blur_size;
        let _e14 = T;
        let _e15 = half;
        local = mix(0f, _e13, (_e14 / _e15));
    } else {
        let _e18 = p.blur_size;
        let _e20 = T;
        let _e21 = half;
        let _e23 = half;
        local = mix(_e18, 0f, ((_e20 - _e21) / _e23));
    }
    let _e27 = local;
    D = _e27;
    let _e29 = uv_5;
    let _e30 = getFromColor(_e29);
    C0_ = _e30;
    let _e32 = uv_5;
    let _e33 = getToColor(_e32);
    C1_ = _e33;
    let _e35 = C0_;
    let _e41 = D;
    let _e43 = uv_5;
    let _e45 = getFromColor(((vec2<f32>(-0.326f, -0.406f) * _e41) + _e43));
    C0_ = (_e35 + _e45);
    let _e47 = C1_;
    let _e53 = D;
    let _e55 = uv_5;
    let _e57 = getToColor(((vec2<f32>(-0.326f, -0.406f) * _e53) + _e55));
    C1_ = (_e47 + _e57);
    let _e59 = C0_;
    let _e65 = D;
    let _e67 = uv_5;
    let _e69 = getFromColor(((vec2<f32>(-0.84f, -0.074f) * _e65) + _e67));
    C0_ = (_e59 + _e69);
    let _e71 = C1_;
    let _e77 = D;
    let _e79 = uv_5;
    let _e81 = getToColor(((vec2<f32>(-0.84f, -0.074f) * _e77) + _e79));
    C1_ = (_e71 + _e81);
    let _e83 = C0_;
    let _e88 = D;
    let _e90 = uv_5;
    let _e92 = getFromColor(((vec2<f32>(-0.696f, 0.457f) * _e88) + _e90));
    C0_ = (_e83 + _e92);
    let _e94 = C1_;
    let _e99 = D;
    let _e101 = uv_5;
    let _e103 = getToColor(((vec2<f32>(-0.696f, 0.457f) * _e99) + _e101));
    C1_ = (_e94 + _e103);
    let _e105 = C0_;
    let _e110 = D;
    let _e112 = uv_5;
    let _e114 = getFromColor(((vec2<f32>(-0.203f, 0.621f) * _e110) + _e112));
    C0_ = (_e105 + _e114);
    let _e116 = C1_;
    let _e121 = D;
    let _e123 = uv_5;
    let _e125 = getToColor(((vec2<f32>(-0.203f, 0.621f) * _e121) + _e123));
    C1_ = (_e116 + _e125);
    let _e127 = C0_;
    let _e132 = D;
    let _e134 = uv_5;
    let _e136 = getFromColor(((vec2<f32>(0.962f, -0.195f) * _e132) + _e134));
    C0_ = (_e127 + _e136);
    let _e138 = C1_;
    let _e143 = D;
    let _e145 = uv_5;
    let _e147 = getToColor(((vec2<f32>(0.962f, -0.195f) * _e143) + _e145));
    C1_ = (_e138 + _e147);
    let _e149 = C0_;
    let _e154 = D;
    let _e156 = uv_5;
    let _e158 = getFromColor(((vec2<f32>(0.473f, -0.48f) * _e154) + _e156));
    C0_ = (_e149 + _e158);
    let _e160 = C1_;
    let _e165 = D;
    let _e167 = uv_5;
    let _e169 = getToColor(((vec2<f32>(0.473f, -0.48f) * _e165) + _e167));
    C1_ = (_e160 + _e169);
    let _e171 = C0_;
    let _e175 = D;
    let _e177 = uv_5;
    let _e179 = getFromColor(((vec2<f32>(0.519f, 0.767f) * _e175) + _e177));
    C0_ = (_e171 + _e179);
    let _e181 = C1_;
    let _e185 = D;
    let _e187 = uv_5;
    let _e189 = getToColor(((vec2<f32>(0.519f, 0.767f) * _e185) + _e187));
    C1_ = (_e181 + _e189);
    let _e191 = C0_;
    let _e196 = D;
    let _e198 = uv_5;
    let _e200 = getFromColor(((vec2<f32>(0.185f, -0.893f) * _e196) + _e198));
    C0_ = (_e191 + _e200);
    let _e202 = C1_;
    let _e207 = D;
    let _e209 = uv_5;
    let _e211 = getToColor(((vec2<f32>(0.185f, -0.893f) * _e207) + _e209));
    C1_ = (_e202 + _e211);
    let _e213 = C0_;
    let _e217 = D;
    let _e219 = uv_5;
    let _e221 = getFromColor(((vec2<f32>(0.507f, 0.064f) * _e217) + _e219));
    C0_ = (_e213 + _e221);
    let _e223 = C1_;
    let _e227 = D;
    let _e229 = uv_5;
    let _e231 = getToColor(((vec2<f32>(0.507f, 0.064f) * _e227) + _e229));
    C1_ = (_e223 + _e231);
    let _e233 = C0_;
    let _e237 = D;
    let _e239 = uv_5;
    let _e241 = getFromColor(((vec2<f32>(0.896f, 0.412f) * _e237) + _e239));
    C0_ = (_e233 + _e241);
    let _e243 = C1_;
    let _e247 = D;
    let _e249 = uv_5;
    let _e251 = getToColor(((vec2<f32>(0.896f, 0.412f) * _e247) + _e249));
    C1_ = (_e243 + _e251);
    let _e253 = C0_;
    let _e259 = D;
    let _e261 = uv_5;
    let _e263 = getFromColor(((vec2<f32>(-0.322f, -0.933f) * _e259) + _e261));
    C0_ = (_e253 + _e263);
    let _e265 = C1_;
    let _e271 = D;
    let _e273 = uv_5;
    let _e275 = getToColor(((vec2<f32>(-0.322f, -0.933f) * _e271) + _e273));
    C1_ = (_e265 + _e275);
    let _e277 = C0_;
    let _e283 = D;
    let _e285 = uv_5;
    let _e287 = getFromColor(((vec2<f32>(-0.792f, -0.598f) * _e283) + _e285));
    C0_ = (_e277 + _e287);
    let _e289 = C1_;
    let _e295 = D;
    let _e297 = uv_5;
    let _e299 = getToColor(((vec2<f32>(-0.792f, -0.598f) * _e295) + _e297));
    C1_ = (_e289 + _e299);
    let _e301 = C0_;
    C0_ = (_e301 / vec4(13f));
    let _e305 = C1_;
    C1_ = (_e305 / vec4(13f));
    let _e309 = C0_;
    let _e310 = C1_;
    let _e311 = T;
    return mix(_e309, _e310, vec4(_e311));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
