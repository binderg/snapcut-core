// Author: Mark Craig
// mrmcsoftware on github and youtube ( http://www.youtube.com/MrMcSoftware )
// License: MIT
// Description: The outgoing image slides out, or the incoming one slides in, toward an edge, a corner or the center
// Tags: slide, directional
//
// Slides Transition by Mark Craig (Copyright © 2022)
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
    var uv0_: vec2<f32>;
    var local: f32;
    var rad: f32;
    var xc1_: f32;
    var yc1_: f32;
    var uv2_: vec2<f32>;
    var local_1: vec4<f32>;
    var local_2: vec4<f32>;

    uv_5 = uv_4;
    let _e6 = uv_5;
    uv0_ = _e6;
    let _e8 = (p.in > 0.5);
    if _e8 {
        let _e9 = u.progress;
        local = _e9;
    } else {
        let _e11 = u.progress;
        local = (1f - _e11);
    }
    let _e14 = local;
    rad = _e14;
    let _e18 = i32(p.gl_type);
    if (_e18 == 0i) {
        {
            let _e22 = rad;
            xc1_ = (0.5f - (_e22 / 2f));
            yc1_ = 0f;
        }
    } else {
        let _e27 = i32(p.gl_type);
        if (_e27 == 1i) {
            {
                let _e31 = rad;
                xc1_ = (1f - _e31);
                let _e34 = rad;
                yc1_ = (0.5f - (_e34 / 2f));
            }
        } else {
            let _e38 = i32(p.gl_type);
            if (_e38 == 2i) {
                {
                    let _e42 = rad;
                    xc1_ = (0.5f - (_e42 / 2f));
                    let _e47 = rad;
                    yc1_ = (1f - _e47);
                }
            } else {
                let _e49 = i32(p.gl_type);
                if (_e49 == 3i) {
                    {
                        xc1_ = 0f;
                        let _e54 = rad;
                        yc1_ = (0.5f - (_e54 / 2f));
                    }
                } else {
                    let _e58 = i32(p.gl_type);
                    if (_e58 == 4i) {
                        {
                            let _e62 = rad;
                            xc1_ = (1f - _e62);
                            yc1_ = 0f;
                        }
                    } else {
                        let _e65 = i32(p.gl_type);
                        if (_e65 == 5i) {
                            {
                                let _e69 = rad;
                                xc1_ = (1f - _e69);
                                let _e72 = rad;
                                yc1_ = (1f - _e72);
                            }
                        } else {
                            let _e74 = i32(p.gl_type);
                            if (_e74 == 6i) {
                                {
                                    xc1_ = 0f;
                                    let _e79 = rad;
                                    yc1_ = (1f - _e79);
                                }
                            } else {
                                let _e81 = i32(p.gl_type);
                                if (_e81 == 7i) {
                                    {
                                        xc1_ = 0f;
                                        yc1_ = 0f;
                                    }
                                } else {
                                    let _e86 = i32(p.gl_type);
                                    if (_e86 == 8i) {
                                        {
                                            let _e90 = rad;
                                            xc1_ = (0.5f - (_e90 / 2f));
                                            let _e95 = rad;
                                            yc1_ = (0.5f - (_e95 / 2f));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let _e101 = uv_5;
    uv_5.y = (1f - _e101.y);
    let _e105 = uv_5;
    let _e107 = xc1_;
    let _e109 = uv_5;
    let _e111 = xc1_;
    let _e112 = rad;
    let _e116 = uv_5;
    let _e118 = yc1_;
    let _e121 = uv_5;
    let _e123 = yc1_;
    let _e124 = rad;
    if ((((_e105.x >= _e107) && (_e109.x <= (_e111 + _e112))) && (_e116.y >= _e118)) && (_e121.y <= (_e123 + _e124))) {
        {
            let _e128 = uv_5;
            let _e130 = xc1_;
            let _e132 = rad;
            let _e135 = uv_5;
            let _e137 = yc1_;
            let _e139 = rad;
            uv2_ = vec2<f32>(((_e128.x - _e130) / _e132), (1f - ((_e135.y - _e137) / _e139)));
            let _e143 = (p.in > 0.5);
            if _e143 {
                let _e144 = uv2_;
                let _e145 = getToColor(_e144);
                local_1 = _e145;
            } else {
                let _e146 = uv2_;
                let _e147 = getFromColor(_e146);
                local_1 = _e147;
            }
            let _e149 = local_1;
            return _e149;
        }
    }
    let _e150 = (p.in > 0.5);
    if _e150 {
        let _e151 = uv0_;
        let _e152 = getFromColor(_e151);
        local_2 = _e152;
    } else {
        let _e153 = uv0_;
        let _e154 = getToColor(_e153);
        local_2 = _e154;
    }
    let _e156 = local_2;
    return _e156;
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
