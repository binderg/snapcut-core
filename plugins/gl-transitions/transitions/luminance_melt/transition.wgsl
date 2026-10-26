// Author: 0gust1
// License: MIT
// Description: The outgoing image melts away in noisy vertical streaks, depending on its brightness
// Tags: melt, noise, luma
//My own first transition — based on crosshatch code (from pthrasher), using  simplex noise formula (copied and pasted)
//-> cooler with high contrasted images (isolated dark subject on light background f.e.)
//TODO : try to rebase it on DoomTransition (from zeh)?
//optimizations :
//luminance (see http://stackoverflow.com/questions/596216/formula-to-determine-brightness-of-rgb-color#answer-596241)
// Y = (R+R+B+G+G+G)/6
//or Y = (R+R+R+B+G+G+G+G)>>3
//
//direction of movement :  0 : up, 1, down
// @param Direction of the melt (false: up, true: down)
//
// Simplex noise :
// Description : Array and textureless GLSL 2D simplex noise function.
//      Author : Ian McEwan, Ashima Arts.
//  Maintainer : ijm
//     Lastmod : 20110822 (ijm)
//     License : MIT
//               2011 Ashima Arts. All rights reserved.
//               Distributed under the MIT License. See LICENSE file.
//               https://github.com/ashima/webgl-noise
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

fn rand(co: vec2<f32>) -> f32 {
    var co_1: vec2<f32>;

    co_1 = co;
    let _e7 = co_1;
    return fract((sin(dot(_e7.xy, vec2<f32>(12.9898f, 78.233f))) * 43758.547f));
}

fn mod289_(x: vec3<f32>) -> vec3<f32> {
    var x_1: vec3<f32>;

    x_1 = x;
    let _e7 = x_1;
    let _e8 = x_1;
    return (_e7 - (floor((_e8 * 0.0034602077f)) * 289f));
}

fn mod289_1(x_2: vec2<f32>) -> vec2<f32> {
    var x_3: vec2<f32>;

    x_3 = x_2;
    let _e7 = x_3;
    let _e8 = x_3;
    return (_e7 - (floor((_e8 * 0.0034602077f)) * 289f));
}

fn permute(x_4: vec3<f32>) -> vec3<f32> {
    var x_5: vec3<f32>;

    x_5 = x_4;
    let _e7 = x_5;
    let _e13 = x_5;
    let _e15 = mod289_((((_e7 * 34f) + vec3(1f)) * _e13));
    return _e15;
}

fn snoise(v: vec2<f32>) -> f32 {
    var v_1: vec2<f32>;
    var C: vec4<f32> = vec4<f32>(0.21132487f, 0.36602542f, -0.57735026f, 0.024390243f);
    var i: vec2<f32>;
    var x0_: vec2<f32>;
    var i1_: vec2<f32>;
    var local: vec2<f32>;
    var x12_: vec4<f32>;
    var glt_p: vec3<f32>;
    var m: vec3<f32>;
    var x_6: vec3<f32>;
    var h: vec3<f32>;
    var ox: vec3<f32>;
    var a0_: vec3<f32>;
    var g: vec3<f32>;

    v_1 = v;
    let _e14 = v_1;
    let _e15 = v_1;
    let _e16 = C;
    i = floor((_e14 + vec2(dot(_e15, _e16.yy))));
    let _e23 = v_1;
    let _e24 = i;
    let _e26 = i;
    let _e27 = C;
    x0_ = ((_e23 - _e24) + vec2(dot(_e26, _e27.xx)));
    let _e34 = x0_;
    let _e36 = x0_;
    if (_e34.x > _e36.y) {
        local = vec2<f32>(1f, 0f);
    } else {
        local = vec2<f32>(0f, 1f);
    }
    let _e46 = local;
    i1_ = _e46;
    let _e47 = x0_;
    let _e49 = C;
    x12_ = (_e47.xyxy + _e49.xxzz);
    let _e53 = x12_;
    let _e55 = x12_;
    let _e57 = i1_;
    let _e58 = (_e55.xy - _e57);
    x12_.x = _e58.x;
    x12_.y = _e58.y;
    let _e63 = i;
    let _e64 = mod289_1(_e63);
    i = _e64;
    let _e65 = i;
    let _e68 = i1_;
    let _e74 = permute((vec3(_e65.y) + vec3<f32>(0f, _e68.y, 1f)));
    let _e75 = i;
    let _e80 = i1_;
    let _e85 = permute(((_e74 + vec3(_e75.x)) + vec3<f32>(0f, _e80.x, 1f)));
    glt_p = _e85;
    let _e88 = x0_;
    let _e89 = x0_;
    let _e91 = x12_;
    let _e93 = x12_;
    let _e96 = x12_;
    let _e98 = x12_;
    m = max((vec3(0.5f) - vec3<f32>(dot(_e88, _e89), dot(_e91.xy, _e93.xy), dot(_e96.zw, _e98.zw))), vec3(0f));
    let _e108 = m;
    let _e109 = m;
    m = (_e108 * _e109);
    let _e111 = m;
    let _e112 = m;
    m = (_e111 * _e112);
    let _e115 = glt_p;
    let _e116 = C;
    x_6 = ((2f * fract((_e115 * _e116.www))) - vec3(1f));
    let _e125 = x_6;
    h = (abs(_e125) - vec3(0.5f));
    let _e131 = x_6;
    ox = floor((_e131 + vec3(0.5f)));
    let _e137 = x_6;
    let _e138 = ox;
    a0_ = (_e137 - _e138);
    let _e141 = m;
    let _e144 = a0_;
    let _e145 = a0_;
    let _e147 = h;
    let _e148 = h;
    m = (_e141 * (vec3(1.7928429f) - (0.85373473f * ((_e144 * _e145) + (_e147 * _e148)))));
    let _e157 = a0_;
    let _e159 = x0_;
    let _e162 = h;
    let _e164 = x0_;
    g.x = ((_e157.x * _e159.x) + (_e162.x * _e164.y));
    let _e168 = g;
    let _e170 = a0_;
    let _e172 = x12_;
    let _e175 = h;
    let _e177 = x12_;
    let _e180 = ((_e170.yz * _e172.xz) + (_e175.yz * _e177.yw));
    g.y = _e180.x;
    g.z = _e180.y;
    let _e186 = m;
    let _e187 = g;
    return (130f * dot(_e186, _e187));
}

fn luminance(color: vec4<f32>) -> f32 {
    var color_1: vec4<f32>;

    color_1 = color;
    let _e7 = color_1;
    let _e11 = color_1;
    let _e16 = color_1;
    return (((_e7.x * 0.299f) + (_e11.y * 0.587f)) + (_e16.z * 0.114f));
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var center: vec2<f32>;
    var p_1: vec2<f32>;
    var x_7: f32;
    var dist: f32;
    var r: f32;
    var m_1: f32;
    var local_1: f32;
    var local_2: f32;

    uv_5 = uv_4;
    let _e8 = (p.direction > 0.5);
    center = vec2<f32>(1f, f32(_e8));
    let _e12 = uv_5;
    p_1 = (_e12.xy / vec2(1f));
    let _e19 = u.progress;
    if (_e19 == 0f) {
        {
            let _e22 = p_1;
            let _e23 = getFromColor(_e22);
            return _e23;
        }
    } else {
        let _e24 = u.progress;
        if (_e24 == 1f) {
            {
                let _e27 = p_1;
                let _e28 = getToColor(_e27);
                return _e28;
            }
        } else {
            {
                let _e29 = u.progress;
                x_7 = _e29;
                let _e31 = center;
                let _e32 = p_1;
                let _e34 = u.progress;
                let _e35 = p_1;
                let _e39 = snoise(vec2<f32>(_e35.x, 0f));
                dist = (distance(_e31, _e32) - (_e34 * exp(_e39)));
                let _e44 = x_7;
                let _e45 = p_1;
                let _e49 = rand(vec2<f32>(_e45.x, 0.1f));
                r = (_e44 - _e49);
                let _e53 = (p.above > 0.5);
                if _e53 {
                    {
                        let _e54 = dist;
                        let _e55 = r;
                        let _e57 = p_1;
                        let _e58 = getFromColor(_e57);
                        let _e59 = luminance(_e58);
                        let _e60 = p.l_threshold;
                        if ((_e54 <= _e55) && (_e59 > _e60)) {
                            local_1 = 1f;
                        } else {
                            let _e64 = u.progress;
                            let _e65 = u.progress;
                            let _e67 = u.progress;
                            local_1 = ((_e64 * _e65) * _e67);
                        }
                        let _e70 = local_1;
                        m_1 = _e70;
                    }
                } else {
                    {
                        let _e71 = dist;
                        let _e72 = r;
                        let _e74 = p_1;
                        let _e75 = getFromColor(_e74);
                        let _e76 = luminance(_e75);
                        let _e77 = p.l_threshold;
                        if ((_e71 <= _e72) && (_e76 < _e77)) {
                            local_2 = 1f;
                        } else {
                            let _e81 = u.progress;
                            let _e82 = u.progress;
                            let _e84 = u.progress;
                            local_2 = ((_e81 * _e82) * _e84);
                        }
                        let _e87 = local_2;
                        m_1 = _e87;
                    }
                }
                let _e88 = p_1;
                let _e89 = getFromColor(_e88);
                let _e90 = p_1;
                let _e91 = getToColor(_e90);
                let _e92 = m_1;
                return mix(_e89, _e91, vec4(_e92));
            }
        }
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
