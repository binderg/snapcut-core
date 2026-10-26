// Author: huynx
// License: MIT
// Description: Two triangles grow from the top and bottom edges into a bow tie that reveals the incoming image
// Tags: wipe, shape, vertical
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

fn check(p1_: vec2<f32>, p2_: vec2<f32>, p3_: vec2<f32>) -> f32 {
    var p1_1: vec2<f32>;
    var p2_1: vec2<f32>;
    var p3_1: vec2<f32>;

    p1_1 = p1_;
    p2_1 = p2_;
    p3_1 = p3_;
    let _e8 = p1_1;
    let _e10 = p3_1;
    let _e13 = p2_1;
    let _e15 = p3_1;
    let _e19 = p2_1;
    let _e21 = p3_1;
    let _e24 = p1_1;
    let _e26 = p3_1;
    return (((_e8.x - _e10.x) * (_e13.y - _e15.y)) - ((_e19.x - _e21.x) * (_e24.y - _e26.y)));
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
    let _e13 = pt_1;
    let _e14 = p1_3;
    let _e15 = p2_3;
    let _e16 = check(_e13, _e14, _e15);
    b1_ = (_e16 < 0f);
    let _e19 = pt_1;
    let _e20 = p2_3;
    let _e21 = p3_3;
    let _e22 = check(_e19, _e20, _e21);
    b2_ = (_e22 < 0f);
    let _e25 = pt_1;
    let _e26 = p3_3;
    let _e27 = p1_3;
    let _e28 = check(_e25, _e26, _e27);
    b3_ = (_e28 < 0f);
    let _e31 = b1_;
    let _e32 = b2_;
    let _e34 = b2_;
    let _e35 = b3_;
    return ((_e31 == _e32) && (_e34 == _e35));
}

fn in_top_triangle(glt_p: vec2<f32>) -> bool {
    var p_1: vec2<f32>;
    var vertex1_: vec2<f32>;
    var vertex2_: vec2<f32>;
    var vertex3_: vec2<f32>;

    p_1 = glt_p;
    let _e8 = u.progress;
    vertex1_ = vec2<f32>(0.5f, _e8);
    let _e11 = u.progress;
    vertex2_ = vec2<f32>((0.5f - _e11), 0f);
    let _e16 = u.progress;
    vertex3_ = vec2<f32>((0.5f + _e16), 0f);
    let _e20 = p_1;
    let _e21 = vertex1_;
    let _e22 = vertex2_;
    let _e23 = vertex3_;
    let _e24 = PointInTriangle(_e20, _e21, _e22, _e23);
    if _e24 {
        {
            return true;
        }
    }
    return false;
}

fn in_bottom_triangle(p_2: vec2<f32>) -> bool {
    var p_3: vec2<f32>;
    var vertex1_1: vec2<f32>;
    var vertex2_1: vec2<f32>;
    var vertex3_1: vec2<f32>;

    p_3 = p_2;
    let _e9 = u.progress;
    vertex1_1 = vec2<f32>(0.5f, (1f - _e9));
    let _e13 = u.progress;
    vertex2_1 = vec2<f32>((0.5f - _e13), 1f);
    let _e18 = u.progress;
    vertex3_1 = vec2<f32>((0.5f + _e18), 1f);
    let _e22 = p_3;
    let _e23 = vertex1_1;
    let _e24 = vertex2_1;
    let _e25 = vertex3_1;
    let _e26 = PointInTriangle(_e22, _e23, _e24, _e25);
    if _e26 {
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
    let _e10 = bot1_1;
    let _e11 = top_1;
    lineDir = (_e10 - _e11);
    let _e14 = lineDir;
    let _e16 = lineDir;
    perpDir = vec2<f32>(_e14.y, -(_e16.x));
    let _e21 = bot1_1;
    let _e22 = testPt_1;
    dirToPt1_ = (_e21 - _e22);
    let _e25 = perpDir;
    let _e27 = dirToPt1_;
    dist1_ = abs(dot(normalize(_e25), _e27));
    let _e31 = bot2_1;
    let _e32 = top_1;
    lineDir = (_e31 - _e32);
    let _e34 = lineDir;
    let _e36 = lineDir;
    perpDir = vec2<f32>(_e34.y, -(_e36.x));
    let _e40 = bot2_1;
    let _e41 = testPt_1;
    dirToPt1_ = (_e40 - _e41);
    let _e43 = perpDir;
    let _e45 = dirToPt1_;
    let _e48 = dist1_;
    min_dist = min(abs(dot(normalize(_e43), _e45)), _e48);
    let _e51 = min_dist;
    if (_e51 < 0.005f) {
        {
            let _e54 = min_dist;
            return (_e54 / 0.005f);
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
    let _e4 = uv_5;
    let _e5 = in_top_triangle(_e4);
    if _e5 {
        {
            let _e6 = u.progress;
            if (_e6 < 0.1f) {
                {
                    let _e9 = uv_5;
                    let _e10 = getFromColor(_e9);
                    return _e10;
                }
            }
            let _e11 = uv_5;
            if (_e11.y < 0.5f) {
                {
                    let _e16 = u.progress;
                    vertex1_2 = vec2<f32>(0.5f, _e16);
                    let _e20 = u.progress;
                    vertex2_2 = vec2<f32>((0.5f - _e20), 0f);
                    let _e26 = u.progress;
                    vertex3_2 = vec2<f32>((0.5f + _e26), 0f);
                    let _e31 = uv_5;
                    let _e32 = getFromColor(_e31);
                    let _e33 = uv_5;
                    let _e34 = getToColor(_e33);
                    let _e35 = vertex2_2;
                    let _e36 = vertex3_2;
                    let _e37 = vertex1_2;
                    let _e38 = uv_5;
                    let _e39 = blur_edge(_e35, _e36, _e37, _e38);
                    return mix(_e32, _e34, vec4(_e39));
                }
            } else {
                {
                    let _e42 = u.progress;
                    if (_e42 > 0f) {
                        {
                            let _e45 = uv_5;
                            let _e46 = getToColor(_e45);
                            return _e46;
                        }
                    } else {
                        {
                            let _e47 = uv_5;
                            let _e48 = getFromColor(_e47);
                            return _e48;
                        }
                    }
                }
            }
        }
    } else {
        let _e49 = uv_5;
        let _e50 = in_bottom_triangle(_e49);
        if _e50 {
            {
                let _e51 = uv_5;
                if (_e51.y >= 0.5f) {
                    {
                        let _e57 = u.progress;
                        vertex1_3 = vec2<f32>(0.5f, (1f - _e57));
                        let _e62 = u.progress;
                        vertex2_3 = vec2<f32>((0.5f - _e62), 1f);
                        let _e68 = u.progress;
                        vertex3_3 = vec2<f32>((0.5f + _e68), 1f);
                        let _e73 = uv_5;
                        let _e74 = getFromColor(_e73);
                        let _e75 = uv_5;
                        let _e76 = getToColor(_e75);
                        let _e77 = vertex2_3;
                        let _e78 = vertex3_3;
                        let _e79 = vertex1_3;
                        let _e80 = uv_5;
                        let _e81 = blur_edge(_e77, _e78, _e79, _e80);
                        return mix(_e74, _e76, vec4(_e81));
                    }
                } else {
                    {
                        let _e84 = uv_5;
                        let _e85 = getFromColor(_e84);
                        return _e85;
                    }
                }
            }
        } else {
            {
                let _e86 = uv_5;
                let _e87 = getFromColor(_e86);
                return _e87;
            }
        }
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
