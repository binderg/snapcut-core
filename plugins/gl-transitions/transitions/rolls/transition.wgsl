// Author: Mark Craig
// mrmcsoftware on github and youtube ( http://www.youtube.com/MrMcSoftware )
// License: MIT
// Description: The outgoing image rolls away around a corner, revealing the incoming one
// Tags: rotate, roll
//
// Rolls Transition by Mark Craig (Copyright © 2022)
//
// @range(0, 3, 1) Corner to roll around
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
    var theta: f32;
    var c1_: f32;
    var s1_: f32;
    var iResolution: vec2<f32>;
    var uvi: vec2<f32>;
    var local: f32;
    var local_1: f32;
    var local_2: f32;
    var local_3: f32;
    var uv2_: vec2<f32>;

    uv_5 = uv_4;
    let _e9 = (u.resolution.x / u.resolution.y);
    iResolution = vec2<f32>(_e9, 1f);
    let _e14 = i32(p.gl_type);
    if (_e14 == 0i) {
        {
            let _e17 = (p.rot_down > 0.5);
            if _e17 {
                local = 3.1415927f;
            } else {
                local = -3.1415927f;
            }
            let _e22 = local;
            let _e25 = u.progress;
            theta = ((_e22 / 2f) * _e25);
            let _e29 = uv_5;
            uvi.x = (1f - _e29.x);
            let _e33 = uv_5;
            uvi.y = _e33.y;
        }
    } else {
        let _e35 = i32(p.gl_type);
        if (_e35 == 1i) {
            {
                let _e38 = (p.rot_down > 0.5);
                if _e38 {
                    local_1 = 3.1415927f;
                } else {
                    local_1 = -3.1415927f;
                }
                let _e43 = local_1;
                let _e46 = u.progress;
                theta = ((_e43 / 2f) * _e46);
                let _e48 = uv_5;
                uvi = _e48;
            }
        } else {
            let _e49 = i32(p.gl_type);
            if (_e49 == 2i) {
                {
                    let _e52 = (p.rot_down > 0.5);
                    if _e52 {
                        local_2 = -3.1415927f;
                    } else {
                        local_2 = 3.1415927f;
                    }
                    let _e57 = local_2;
                    let _e60 = u.progress;
                    theta = ((_e57 / 2f) * _e60);
                    let _e63 = uv_5;
                    uvi.x = _e63.x;
                    let _e67 = uv_5;
                    uvi.y = (1f - _e67.y);
                }
            } else {
                let _e70 = i32(p.gl_type);
                if (_e70 == 3i) {
                    {
                        let _e73 = (p.rot_down > 0.5);
                        if _e73 {
                            local_3 = -3.1415927f;
                        } else {
                            local_3 = 3.1415927f;
                        }
                        let _e78 = local_3;
                        let _e81 = u.progress;
                        theta = ((_e78 / 2f) * _e81);
                        let _e84 = uv_5;
                        uvi = (vec2(1f) - _e84);
                    }
                }
            }
        }
    }
    let _e87 = theta;
    c1_ = cos(_e87);
    let _e89 = theta;
    s1_ = sin(_e89);
    let _e93 = uvi;
    let _e95 = iResolution;
    let _e98 = c1_;
    let _e100 = uvi;
    let _e102 = iResolution;
    let _e105 = s1_;
    uv2_.x = (((_e93.x * _e95.x) * _e98) - ((_e100.y * _e102.y) * _e105));
    let _e109 = uvi;
    let _e111 = iResolution;
    let _e114 = s1_;
    let _e116 = uvi;
    let _e118 = iResolution;
    let _e121 = c1_;
    uv2_.y = (((_e109.x * _e111.x) * _e114) + ((_e116.y * _e118.y) * _e121));
    let _e124 = uv2_;
    let _e128 = uv2_;
    let _e130 = iResolution;
    let _e134 = uv2_;
    let _e139 = uv2_;
    let _e141 = iResolution;
    if ((((_e124.x >= 0f) && (_e128.x <= _e130.x)) && (_e134.y >= 0f)) && (_e139.y <= _e141.y)) {
        {
            let _e145 = uv2_;
            let _e146 = iResolution;
            uv2_ = (_e145 / _e146);
            let _e148 = i32(p.gl_type);
            if (_e148 == 0i) {
                {
                    let _e153 = uv2_;
                    uv2_.x = (1f - _e153.x);
                }
            } else {
                let _e156 = i32(p.gl_type);
                if (_e156 == 2i) {
                    {
                        let _e161 = uv2_;
                        uv2_.y = (1f - _e161.y);
                    }
                } else {
                    let _e164 = i32(p.gl_type);
                    if (_e164 == 3i) {
                        {
                            let _e168 = uv2_;
                            uv2_ = (vec2(1f) - _e168);
                        }
                    }
                }
            }
            let _e171 = uv2_;
            let _e172 = getFromColor(_e171);
            return _e172;
        }
    }
    let _e173 = uv_5;
    let _e174 = getToColor(_e173);
    return _e174;
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
