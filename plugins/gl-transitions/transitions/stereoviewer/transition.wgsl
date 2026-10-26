// Tunable parameters
// How much to zoom (out) for the effect ~ 0.5 - 1.0
// @range(0.5, 1) How much the images zoom out
//
// Author: Ted Schundler
// License: BSD 2 Clause
// Description: Both images shrink into rounded cards that swap, like a stereo viewer toy
// Tags: cards, zoom, retro
// Free for use and modification by anyone with credit
//
// Copyright (c) 2016, Theodore K Schundler
// All rights reserved.
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
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

const c00_: vec2<f32> = vec2<f32>(0f, 0f);

const c01_: vec2<f32> = vec2<f32>(0f, 1f);

const c11_: vec2<f32> = vec2<f32>(1f, 1f);

const c10_: vec2<f32> = vec2<f32>(1f, 0f);

fn in_corner(glt_p: vec2<f32>, corner: vec2<f32>, radius: vec2<f32>) -> bool {
    var p_1: vec2<f32>;
    var corner_1: vec2<f32>;
    var radius_1: vec2<f32>;
    var axis: vec2<f32>;

    p_1 = glt_p;
    corner_1 = corner;
    radius_1 = radius;
    let _e15 = corner_1;
    let _e17 = corner_1;
    axis = ((c11_ - _e15) - _e17);
    let _e20 = p_1;
    let _e21 = corner_1;
    let _e22 = axis;
    let _e23 = radius_1;
    p_1 = (_e20 - (_e21 + (_e22 * _e23)));
    let _e27 = p_1;
    let _e28 = axis;
    let _e29 = radius_1;
    p_1 = (_e27 * (_e28 / _e29));
    let _e32 = p_1;
    let _e36 = p_1;
    let _e42 = p_1;
    let _e46 = p_1;
    let _e53 = p_1;
    let _e54 = p_1;
    return ((((_e32.x > 0f) && (_e36.y > -1f)) || ((_e42.y > 0f) && (_e46.x > -1f))) || (dot(_e53, _e54) < 1f));
}

fn test_rounded_mask(p_2: vec2<f32>, corner_size: vec2<f32>) -> bool {
    var p_3: vec2<f32>;
    var corner_size_1: vec2<f32>;

    p_3 = p_2;
    corner_size_1 = corner_size;
    let _e13 = p_3;
    let _e14 = corner_size_1;
    let _e15 = in_corner(_e13, c00_, _e14);
    let _e16 = p_3;
    let _e17 = corner_size_1;
    let _e18 = in_corner(_e16, c01_, _e17);
    let _e20 = p_3;
    let _e21 = corner_size_1;
    let _e22 = in_corner(_e20, c10_, _e21);
    let _e24 = p_3;
    let _e25 = corner_size_1;
    let _e26 = in_corner(_e24, c11_, _e25);
    return (((_e15 && _e18) && _e22) && _e26);
}

fn screen(a: vec4<f32>, b: vec4<f32>) -> vec4<f32> {
    var a_1: vec4<f32>;
    var b_1: vec4<f32>;

    a_1 = a;
    b_1 = b;
    let _e15 = a_1;
    let _e19 = b_1;
    return (vec4(1f) - ((vec4(1f) - _e15) * (vec4(1f) - _e19)));
}

fn unscreen(c: vec4<f32>) -> vec4<f32> {
    var c_1: vec4<f32>;

    c_1 = c;
    let _e13 = c_1;
    return (vec4(1f) - sqrt((vec4(1f) - _e13)));
}

fn sample_with_corners_from(p_4: vec2<f32>, corner_size_2: vec2<f32>) -> vec4<f32> {
    var p_5: vec2<f32>;
    var corner_size_3: vec2<f32>;

    p_5 = p_4;
    corner_size_3 = corner_size_2;
    let _e13 = p_5;
    let _e17 = p.zoom;
    p_5 = (((_e13 - vec2(0.5f)) / vec2(_e17)) + vec2(0.5f));
    let _e23 = p_5;
    let _e24 = corner_size_3;
    let _e25 = test_rounded_mask(_e23, _e24);
    if !(_e25) {
        {
            return black;
        }
    }
    let _e27 = p_5;
    let _e28 = getFromColor(_e27);
    let _e29 = unscreen(_e28);
    return _e29;
}

fn sample_with_corners_to(p_6: vec2<f32>, corner_size_4: vec2<f32>) -> vec4<f32> {
    var p_7: vec2<f32>;
    var corner_size_5: vec2<f32>;

    p_7 = p_6;
    corner_size_5 = corner_size_4;
    let _e13 = p_7;
    let _e17 = p.zoom;
    p_7 = (((_e13 - vec2(0.5f)) / vec2(_e17)) + vec2(0.5f));
    let _e23 = p_7;
    let _e24 = corner_size_5;
    let _e25 = test_rounded_mask(_e23, _e24);
    if !(_e25) {
        {
            return black;
        }
    }
    let _e27 = p_7;
    let _e28 = getToColor(_e27);
    let _e29 = unscreen(_e28);
    return _e29;
}

fn simple_sample_with_corners_from(p_8: vec2<f32>, corner_size_6: vec2<f32>, zoom_amt: f32) -> vec4<f32> {
    var p_9: vec2<f32>;
    var corner_size_7: vec2<f32>;
    var zoom_amt_1: f32;

    p_9 = p_8;
    corner_size_7 = corner_size_6;
    zoom_amt_1 = zoom_amt;
    let _e15 = p_9;
    let _e20 = zoom_amt_1;
    let _e22 = p.zoom;
    let _e23 = zoom_amt_1;
    p_9 = (((_e15 - vec2(0.5f)) / vec2(((1f - _e20) + (_e22 * _e23)))) + vec2(0.5f));
    let _e31 = p_9;
    let _e32 = corner_size_7;
    let _e33 = test_rounded_mask(_e31, _e32);
    if !(_e33) {
        {
            return black;
        }
    }
    let _e35 = p_9;
    let _e36 = getFromColor(_e35);
    return _e36;
}

fn simple_sample_with_corners_to(p_10: vec2<f32>, corner_size_8: vec2<f32>, zoom_amt_2: f32) -> vec4<f32> {
    var p_11: vec2<f32>;
    var corner_size_9: vec2<f32>;
    var zoom_amt_3: f32;

    p_11 = p_10;
    corner_size_9 = corner_size_8;
    zoom_amt_3 = zoom_amt_2;
    let _e15 = p_11;
    let _e20 = zoom_amt_3;
    let _e22 = p.zoom;
    let _e23 = zoom_amt_3;
    p_11 = (((_e15 - vec2(0.5f)) / vec2(((1f - _e20) + (_e22 * _e23)))) + vec2(0.5f));
    let _e31 = p_11;
    let _e32 = corner_size_9;
    let _e33 = test_rounded_mask(_e31, _e32);
    if !(_e33) {
        {
            return black;
        }
    }
    let _e35 = p_11;
    let _e36 = getToColor(_e35);
    return _e36;
}

fn rotate2d(angle: f32, ratio: f32) -> mat3x3<f32> {
    var angle_1: f32;
    var ratio_1: f32;
    var s: f32;
    var c_2: f32;

    angle_1 = angle;
    ratio_1 = ratio;
    let _e13 = angle_1;
    s = sin(_e13);
    let _e16 = angle_1;
    c_2 = cos(_e16);
    let _e19 = c_2;
    let _e20 = s;
    let _e22 = s;
    let _e24 = c_2;
    return mat3x3<f32>(vec3<f32>(_e19, _e20, 0f), vec3<f32>(-(_e22), _e24, 0f), vec3<f32>(0f, 0f, 1f));
}

fn translate2d(x: f32, y: f32) -> mat3x3<f32> {
    var x_1: f32;
    var y_1: f32;

    x_1 = x;
    y_1 = y;
    let _e19 = x_1;
    let _e21 = y_1;
    return mat3x3<f32>(vec3<f32>(1f, 0f, 0f), vec3<f32>(0f, 1f, 0f), vec3<f32>(-(_e19), -(_e21), 1f));
}

fn scale2d(x_2: f32, y_2: f32) -> mat3x3<f32> {
    var x_3: f32;
    var y_3: f32;

    x_3 = x_2;
    y_3 = y_2;
    let _e13 = x_3;
    let _e17 = y_3;
    return mat3x3<f32>(vec3<f32>(_e13, 0f, 0f), vec3<f32>(0f, _e17, 0f), vec3<f32>(0f, 0f, 1f));
}

fn get_cross_rotated(p3_: vec3<f32>, angle_2: f32, corner_size_10: vec2<f32>, ratio_2: f32) -> vec4<f32> {
    var p3_1: vec3<f32>;
    var angle_3: f32;
    var corner_size_11: vec2<f32>;
    var ratio_3: f32;
    var center_and_scale: mat3x3<f32>;
    var unscale_and_uncenter: mat3x3<f32>;
    var slide_left: mat3x3<f32>;
    var slide_right: mat3x3<f32>;
    var rotate: mat3x3<f32>;
    var op_a: mat3x3<f32>;
    var op_b: mat3x3<f32>;
    var a_2: vec4<f32>;
    var b_2: vec4<f32>;

    p3_1 = p3_;
    angle_3 = angle_2;
    corner_size_11 = corner_size_10;
    ratio_3 = ratio_2;
    let _e17 = angle_3;
    let _e18 = angle_3;
    angle_3 = (_e17 * _e18);
    let _e20 = angle_3;
    angle_3 = (_e20 / 2.4f);
    let _e27 = translate2d(-0.5f, -0.5f);
    let _e29 = ratio_3;
    let _e30 = scale2d(1f, _e29);
    center_and_scale = (_e27 * _e30);
    let _e35 = ratio_3;
    let _e37 = scale2d(1f, (1f / _e35));
    let _e40 = translate2d(0.5f, 0.5f);
    unscale_and_uncenter = (_e37 * _e40);
    let _e46 = translate2d(-2f, 0f);
    slide_left = _e46;
    let _e50 = translate2d(2f, 0f);
    slide_right = _e50;
    let _e52 = angle_3;
    let _e53 = ratio_3;
    let _e54 = rotate2d(_e52, _e53);
    rotate = _e54;
    let _e56 = center_and_scale;
    let _e57 = slide_right;
    let _e59 = rotate;
    let _e61 = slide_left;
    let _e63 = unscale_and_uncenter;
    op_a = ((((_e56 * _e57) * _e59) * _e61) * _e63);
    let _e66 = center_and_scale;
    let _e67 = slide_left;
    let _e69 = rotate;
    let _e71 = slide_right;
    let _e73 = unscale_and_uncenter;
    op_b = ((((_e66 * _e67) * _e69) * _e71) * _e73);
    let _e76 = op_a;
    let _e77 = p3_1;
    let _e80 = corner_size_11;
    let _e81 = sample_with_corners_from((_e76 * _e77).xy, _e80);
    a_2 = _e81;
    let _e83 = op_b;
    let _e84 = p3_1;
    let _e87 = corner_size_11;
    let _e88 = sample_with_corners_from((_e83 * _e84).xy, _e87);
    b_2 = _e88;
    let _e90 = a_2;
    let _e91 = b_2;
    let _e92 = screen(_e90, _e91);
    return _e92;
}

fn get_cross_masked(p3_2: vec3<f32>, angle_4: f32, corner_size_12: vec2<f32>, ratio_4: f32) -> vec4<f32> {
    var p3_3: vec3<f32>;
    var angle_5: f32;
    var corner_size_13: vec2<f32>;
    var ratio_5: f32;
    var img: vec4<f32>;
    var center_and_scale_1: mat3x3<f32>;
    var unscale_and_uncenter_1: mat3x3<f32>;
    var slide_left_1: mat3x3<f32>;
    var slide_right_1: mat3x3<f32>;
    var rotate_1: mat3x3<f32>;
    var op_a_1: mat3x3<f32>;
    var op_b_1: mat3x3<f32>;
    var mask_a: bool;
    var mask_b: bool;
    var local: vec4<f32>;
    var local_1: vec4<f32>;

    p3_3 = p3_2;
    angle_5 = angle_4;
    corner_size_13 = corner_size_12;
    ratio_5 = ratio_4;
    let _e18 = angle_5;
    angle_5 = (1f - _e18);
    let _e20 = angle_5;
    let _e21 = angle_5;
    angle_5 = (_e20 * _e21);
    let _e23 = angle_5;
    angle_5 = (_e23 / 2.4f);
    let _e31 = translate2d(-0.5f, -0.5f);
    let _e33 = ratio_5;
    let _e34 = scale2d(1f, _e33);
    center_and_scale_1 = (_e31 * _e34);
    let _e38 = p.zoom;
    let _e41 = p.zoom;
    let _e42 = ratio_5;
    let _e45 = scale2d((1f / _e38), (1f / (_e41 * _e42)));
    let _e48 = translate2d(0.5f, 0.5f);
    unscale_and_uncenter_1 = (_e45 * _e48);
    let _e54 = translate2d(-2f, 0f);
    slide_left_1 = _e54;
    let _e58 = translate2d(2f, 0f);
    slide_right_1 = _e58;
    let _e60 = angle_5;
    let _e61 = ratio_5;
    let _e62 = rotate2d(_e60, _e61);
    rotate_1 = _e62;
    let _e64 = center_and_scale_1;
    let _e65 = slide_right_1;
    let _e67 = rotate_1;
    let _e69 = slide_left_1;
    let _e71 = unscale_and_uncenter_1;
    op_a_1 = ((((_e64 * _e65) * _e67) * _e69) * _e71);
    let _e74 = center_and_scale_1;
    let _e75 = slide_left_1;
    let _e77 = rotate_1;
    let _e79 = slide_right_1;
    let _e81 = unscale_and_uncenter_1;
    op_b_1 = ((((_e74 * _e75) * _e77) * _e79) * _e81);
    let _e84 = op_a_1;
    let _e85 = p3_3;
    let _e88 = corner_size_13;
    let _e89 = test_rounded_mask((_e84 * _e85).xy, _e88);
    mask_a = _e89;
    let _e91 = op_b_1;
    let _e92 = p3_3;
    let _e95 = corner_size_13;
    let _e96 = test_rounded_mask((_e91 * _e92).xy, _e95);
    mask_b = _e96;
    let _e98 = mask_a;
    let _e99 = mask_b;
    if (_e98 || _e99) {
        {
            let _e101 = p3_3;
            let _e103 = corner_size_13;
            let _e104 = sample_with_corners_to(_e101.xy, _e103);
            img = _e104;
            let _e105 = mask_a;
            if _e105 {
                let _e106 = img;
                local = _e106;
            } else {
                local = black;
            }
            let _e108 = local;
            let _e109 = mask_b;
            if _e109 {
                let _e110 = img;
                local_1 = _e110;
            } else {
                local_1 = black;
            }
            let _e112 = local_1;
            let _e113 = screen(_e108, _e112);
            return _e113;
        }
    } else {
        {
            return black;
        }
    }
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var a_3: f32;
    var p_12: vec2<f32>;
    var p3_4: vec3<f32>;
    var corner_size_14: vec2<f32>;

    uv_5 = uv_4;
    let _e12 = uv_5;
    p_12 = (_e12.xy / vec2(1f));
    let _e19 = p_12;
    let _e20 = _e19.xy;
    p3_4 = vec3<f32>(_e20.x, _e20.y, 1f);
    let _e26 = p.corner_radius;
    let _e27 = (u.resolution.x / u.resolution.y);
    let _e29 = p.corner_radius;
    corner_size_14 = vec2<f32>((_e26 / _e27), _e29);
    let _e32 = u.progress;
    if (_e32 <= 0f) {
        {
            let _e35 = p_12;
            let _e36 = getFromColor(_e35);
            return _e36;
        }
    } else {
        let _e37 = u.progress;
        if (_e37 < 0.1f) {
            {
                let _e40 = u.progress;
                a_3 = (_e40 / 0.1f);
                let _e43 = p_12;
                let _e44 = corner_size_14;
                let _e45 = a_3;
                let _e47 = a_3;
                let _e48 = simple_sample_with_corners_from(_e43, (_e44 * _e45), _e47);
                return _e48;
            }
        } else {
            let _e49 = u.progress;
            if (_e49 < 0.48f) {
                {
                    let _e52 = u.progress;
                    a_3 = ((_e52 - 0.1f) / 0.38f);
                    let _e57 = p3_4;
                    let _e58 = a_3;
                    let _e59 = corner_size_14;
                    let _e60 = (u.resolution.x / u.resolution.y);
                    let _e61 = get_cross_rotated(_e57, _e58, _e59, _e60);
                    return _e61;
                }
            } else {
                let _e62 = u.progress;
                if (_e62 < 0.9f) {
                    {
                        let _e65 = p3_4;
                        let _e66 = u.progress;
                        let _e71 = corner_size_14;
                        let _e72 = (u.resolution.x / u.resolution.y);
                        let _e73 = get_cross_masked(_e65, ((_e66 - 0.52f) / 0.38f), _e71, _e72);
                        return _e73;
                    }
                } else {
                    let _e74 = u.progress;
                    if (_e74 < 1f) {
                        {
                            let _e78 = u.progress;
                            a_3 = ((1f - _e78) / 0.1f);
                            let _e82 = p_12;
                            let _e83 = corner_size_14;
                            let _e84 = a_3;
                            let _e86 = a_3;
                            let _e87 = simple_sample_with_corners_to(_e82, (_e83 * _e84), _e86);
                            return _e87;
                        }
                    } else {
                        {
                            let _e88 = p_12;
                            let _e89 = getToColor(_e88);
                            return _e89;
                        }
                    }
                }
            }
        }
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
