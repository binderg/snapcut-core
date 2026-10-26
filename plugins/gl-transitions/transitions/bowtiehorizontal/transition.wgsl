// Author: huynx
// License: MIT
// Description: Two triangles grow from the left and right edges into a bow tie that reveals the incoming image
// Tags: wipe, shape, horizontal
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

const bottom_left: vec2<f32> = vec2<f32>(0f, 1f);

const bottom_right: vec2<f32> = vec2<f32>(1f, 1f);

const top_left: vec2<f32> = vec2<f32>(0f, 0f);

const top_right: vec2<f32> = vec2<f32>(1f, 0f);

const center: vec2<f32> = vec2<f32>(0.5f, 0.5f);

fn check(p1_: vec2<f32>, p2_: vec2<f32>, p3_: vec2<f32>) -> f32 {
    var p1_1: vec2<f32>;
    var p2_1: vec2<f32>;
    var p3_1: vec2<f32>;

    p1_1 = p1_;
    p2_1 = p2_;
    p3_1 = p3_;
    let _e13 = p1_1;
    let _e15 = p3_1;
    let _e18 = p2_1;
    let _e20 = p3_1;
    let _e24 = p2_1;
    let _e26 = p3_1;
    let _e29 = p1_1;
    let _e31 = p3_1;
    return (((_e13.x - _e15.x) * (_e18.y - _e20.y)) - ((_e24.x - _e26.x) * (_e29.y - _e31.y)));
}

fn PointInTriangle(pt: vec2<f32>, p1_2: vec2<f32>, p2_2: vec2<f32>, p3_2: vec2<f32>) -> bool {
    var pt_1: vec2<f32>;
    var p1_3: vec2<f32>;
    var p2_3: vec2<f32>;
    var p3_3: vec2<f32>;
    var b1_: bool;
    var b2_: bool;
    var b3_: bool;

    pt_1 = pt;
    p1_3 = p1_2;
    p2_3 = p2_2;
    p3_3 = p3_2;
    let _e18 = pt_1;
    let _e19 = p1_3;
    let _e20 = p2_3;
    let _e21 = check(_e18, _e19, _e20);
    b1_ = (_e21 < 0f);
    let _e24 = pt_1;
    let _e25 = p2_3;
    let _e26 = p3_3;
    let _e27 = check(_e24, _e25, _e26);
    b2_ = (_e27 < 0f);
    let _e30 = pt_1;
    let _e31 = p3_3;
    let _e32 = p1_3;
    let _e33 = check(_e30, _e31, _e32);
    b3_ = (_e33 < 0f);
    let _e36 = b1_;
    let _e37 = b2_;
    let _e39 = b2_;
    let _e40 = b3_;
    return ((_e36 == _e37) && (_e39 == _e40));
}

fn in_left_triangle(glt_p: vec2<f32>) -> bool {
    var p_1: vec2<f32>;
    var vertex1_: vec2<f32>;
    var vertex2_: vec2<f32>;
    var vertex3_: vec2<f32>;

    p_1 = glt_p;
    let _e12 = u.progress;
    vertex1_ = vec2<f32>(_e12, 0.5f);
    let _e17 = u.progress;
    vertex2_ = vec2<f32>(0f, (0.5f - _e17));
    let _e22 = u.progress;
    vertex3_ = vec2<f32>(0f, (0.5f + _e22));
    let _e25 = p_1;
    let _e26 = vertex1_;
    let _e27 = vertex2_;
    let _e28 = vertex3_;
    let _e29 = PointInTriangle(_e25, _e26, _e27, _e28);
    if _e29 {
        {
            return true;
        }
    }
    return false;
}

fn in_right_triangle(p_2: vec2<f32>) -> bool {
    var p_3: vec2<f32>;
    var vertex1_1: vec2<f32>;
    var vertex2_1: vec2<f32>;
    var vertex3_1: vec2<f32>;

    p_3 = p_2;
    let _e13 = u.progress;
    vertex1_1 = vec2<f32>((1f - _e13), 0.5f);
    let _e19 = u.progress;
    vertex2_1 = vec2<f32>(1f, (0.5f - _e19));
    let _e24 = u.progress;
    vertex3_1 = vec2<f32>(1f, (0.5f + _e24));
    let _e27 = p_3;
    let _e28 = vertex1_1;
    let _e29 = vertex2_1;
    let _e30 = vertex3_1;
    let _e31 = PointInTriangle(_e27, _e28, _e29, _e30);
    if _e31 {
        {
            return true;
        }
    }
    return false;
}

fn blur_edge(bot1_: vec2<f32>, bot2_: vec2<f32>, top: vec2<f32>, testPt: vec2<f32>) -> f32 {
    var bot1_1: vec2<f32>;
    var bot2_1: vec2<f32>;
    var top_1: vec2<f32>;
    var testPt_1: vec2<f32>;
    var lineDir: vec2<f32>;
    var perpDir: vec2<f32>;
    var dirToPt1_: vec2<f32>;
    var dist1_: f32;
    var min_dist: f32;

    bot1_1 = bot1_;
    bot2_1 = bot2_;
    top_1 = top;
    testPt_1 = testPt;
    let _e15 = bot1_1;
    let _e16 = top_1;
    lineDir = (_e15 - _e16);
    let _e19 = lineDir;
    let _e21 = lineDir;
    perpDir = vec2<f32>(_e19.y, -(_e21.x));
    let _e26 = bot1_1;
    let _e27 = testPt_1;
    dirToPt1_ = (_e26 - _e27);
    let _e30 = perpDir;
    let _e32 = dirToPt1_;
    dist1_ = abs(dot(normalize(_e30), _e32));
    let _e36 = bot2_1;
    let _e37 = top_1;
    lineDir = (_e36 - _e37);
    let _e39 = lineDir;
    let _e41 = lineDir;
    perpDir = vec2<f32>(_e39.y, -(_e41.x));
    let _e45 = bot2_1;
    let _e46 = testPt_1;
    dirToPt1_ = (_e45 - _e46);
    let _e48 = perpDir;
    let _e50 = dirToPt1_;
    let _e53 = dist1_;
    min_dist = min(abs(dot(normalize(_e48), _e50)), _e53);
    let _e56 = min_dist;
    if (_e56 < 0.005f) {
        {
            let _e59 = min_dist;
            return (_e59 / 0.005f);
        }
    } else {
        {
            return 1f;
        }
    }
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var vertex1_2: vec2<f32>;
    var vertex2_2: vec2<f32>;
    var vertex3_2: vec2<f32>;
    var vertex1_3: vec2<f32>;
    var vertex2_3: vec2<f32>;
    var vertex3_3: vec2<f32>;

    uv_5 = uv_4;
    let _e9 = uv_5;
    let _e10 = in_left_triangle(_e9);
    if _e10 {
        {
            let _e11 = u.progress;
            if (_e11 < 0.1f) {
                {
                    let _e14 = uv_5;
                    let _e15 = getFromColor(_e14);
                    return _e15;
                }
            }
            let _e16 = uv_5;
            if (_e16.x < 0.5f) {
                {
                    let _e20 = u.progress;
                    vertex1_2 = vec2<f32>(_e20, 0.5f);
                    let _e26 = u.progress;
                    vertex2_2 = vec2<f32>(0f, (0.5f - _e26));
                    let _e32 = u.progress;
                    vertex3_2 = vec2<f32>(0f, (0.5f + _e32));
                    let _e36 = uv_5;
                    let _e37 = getFromColor(_e36);
                    let _e38 = uv_5;
                    let _e39 = getToColor(_e38);
                    let _e40 = vertex2_2;
                    let _e41 = vertex3_2;
                    let _e42 = vertex1_2;
                    let _e43 = uv_5;
                    let _e44 = blur_edge(_e40, _e41, _e42, _e43);
                    return mix(_e37, _e39, vec4(_e44));
                }
            } else {
                {
                    let _e47 = u.progress;
                    if (_e47 > 0f) {
                        {
                            let _e50 = uv_5;
                            let _e51 = getToColor(_e50);
                            return _e51;
                        }
                    } else {
                        {
                            let _e52 = uv_5;
                            let _e53 = getFromColor(_e52);
                            return _e53;
                        }
                    }
                }
            }
        }
    } else {
        let _e54 = uv_5;
        let _e55 = in_right_triangle(_e54);
        if _e55 {
            {
                let _e56 = uv_5;
                if (_e56.x >= 0.5f) {
                    {
                        let _e61 = u.progress;
                        vertex1_3 = vec2<f32>((1f - _e61), 0.5f);
                        let _e68 = u.progress;
                        vertex2_3 = vec2<f32>(1f, (0.5f - _e68));
                        let _e74 = u.progress;
                        vertex3_3 = vec2<f32>(1f, (0.5f + _e74));
                        let _e78 = uv_5;
                        let _e79 = getFromColor(_e78);
                        let _e80 = uv_5;
                        let _e81 = getToColor(_e80);
                        let _e82 = vertex2_3;
                        let _e83 = vertex3_3;
                        let _e84 = vertex1_3;
                        let _e85 = uv_5;
                        let _e86 = blur_edge(_e82, _e83, _e84, _e85);
                        return mix(_e79, _e81, vec4(_e86));
                    }
                } else {
                    {
                        let _e89 = uv_5;
                        let _e90 = getFromColor(_e89);
                        return _e90;
                    }
                }
            }
        } else {
            {
                let _e91 = uv_5;
                let _e92 = getFromColor(_e91);
                return _e92;
            }
        }
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
