// Author: KMojek
// License: MIT
// Description: A bow-tie wipe with an adjustable shape that can run in reverse
// Tags: wipe, shape
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

const height: f32 = 0.5f;

fn check(p1_: vec2<f32>, p2_: vec2<f32>, p3_: vec2<f32>) -> f32 {
    var p1_1: vec2<f32>;
    var p2_1: vec2<f32>;
    var p3_1: vec2<f32>;

    p1_1 = p1_;
    p2_1 = p2_;
    p3_1 = p3_;
    let _e10 = p1_1;
    let _e12 = p3_1;
    let _e15 = p2_1;
    let _e17 = p3_1;
    let _e21 = p2_1;
    let _e23 = p3_1;
    let _e26 = p1_1;
    let _e28 = p3_1;
    return (((_e10.x - _e12.x) * (_e15.y - _e17.y)) - ((_e21.x - _e23.x) * (_e26.y - _e28.y)));
}

fn pointInTriangle(pt: vec2<f32>, p1_2: vec2<f32>, p2_2: vec2<f32>, p3_2: vec2<f32>) -> bool {
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
    let _e12 = pt_1;
    let _e13 = p1_3;
    let _e14 = p2_3;
    let _e15 = check(_e12, _e13, _e14);
    b1_ = (_e15 < 0f);
    let _e19 = pt_1;
    let _e20 = p2_3;
    let _e21 = p3_3;
    let _e22 = check(_e19, _e20, _e21);
    b2_ = (_e22 < 0f);
    let _e26 = pt_1;
    let _e27 = p3_3;
    let _e28 = p1_3;
    let _e29 = check(_e26, _e27, _e28);
    b3_ = (_e29 < 0f);
    let _e33 = b1_;
    let _e34 = b2_;
    let _e36 = b2_;
    let _e37 = b3_;
    return ((_e33 == _e34) && (_e36 == _e37));
}

fn transition_firstHalf(uv_4: vec2<f32>, prog: f32) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var prog_1: f32;
    var botLeft: vec2<f32>;
    var botRight: vec2<f32>;
    var tip: vec2<f32>;
    var topLeft: vec2<f32>;
    var topRight: vec2<f32>;
    var tip_1: vec2<f32>;

    uv_5 = uv_4;
    prog_1 = prog;
    let _e9 = uv_5;
    if (_e9.y < 0.5f) {
        {
            let _e15 = prog_1;
            botLeft = vec2<f32>(-0f, (_e15 - height));
            let _e20 = prog_1;
            botRight = vec2<f32>(1f, (_e20 - height));
            let _e24 = p.adjust;
            let _e25 = prog_1;
            tip = vec2<f32>(_e24, _e25);
            let _e28 = uv_5;
            let _e29 = botLeft;
            let _e30 = botRight;
            let _e31 = tip;
            let _e32 = pointInTriangle(_e28, _e29, _e30, _e31);
            if _e32 {
                let _e33 = uv_5;
                let _e34 = getToColor(_e33);
                return _e34;
            }
        }
    } else {
        {
            let _e38 = prog_1;
            topLeft = vec2<f32>(-0f, ((1f - _e38) + height));
            let _e45 = prog_1;
            topRight = vec2<f32>(1f, ((1f - _e45) + height));
            let _e50 = p.adjust;
            let _e52 = prog_1;
            tip_1 = vec2<f32>(_e50, (1f - _e52));
            let _e56 = uv_5;
            let _e57 = topLeft;
            let _e58 = topRight;
            let _e59 = tip_1;
            let _e60 = pointInTriangle(_e56, _e57, _e58, _e59);
            if _e60 {
                let _e61 = uv_5;
                let _e62 = getToColor(_e61);
                return _e62;
            }
        }
    }
    let _e63 = uv_5;
    let _e64 = getFromColor(_e63);
    return _e64;
}

fn transition_secondHalf(uv_6: vec2<f32>, prog_2: f32) -> vec4<f32> {
    var uv_7: vec2<f32>;
    var prog_3: f32;
    var top: vec2<f32>;
    var bot: vec2<f32>;
    var tip_2: vec2<f32>;
    var top_1: vec2<f32>;
    var bot_1: vec2<f32>;
    var tip_3: vec2<f32>;

    uv_7 = uv_6;
    prog_3 = prog_2;
    let _e9 = uv_7;
    let _e11 = p.adjust;
    if (_e9.x > _e11) {
        {
            let _e13 = prog_3;
            top = vec2<f32>((_e13 + height), 1f);
            let _e18 = prog_3;
            bot = vec2<f32>((_e18 + height), -0f);
            let _e24 = p.adjust;
            let _e27 = prog_3;
            tip_2 = vec2<f32>(mix(_e24, 1f, (2f * (_e27 - 0.5f))), 0.5f);
            let _e35 = uv_7;
            let _e36 = top;
            let _e37 = bot;
            let _e38 = tip_2;
            let _e39 = pointInTriangle(_e35, _e36, _e37, _e38);
            if _e39 {
                let _e40 = uv_7;
                let _e41 = getFromColor(_e40);
                return _e41;
            }
        }
    } else {
        {
            let _e43 = prog_3;
            top_1 = vec2<f32>(((1f - _e43) - height), 1f);
            let _e50 = prog_3;
            bot_1 = vec2<f32>(((1f - _e50) - height), -0f);
            let _e57 = p.adjust;
            let _e60 = prog_3;
            tip_3 = vec2<f32>(mix(_e57, 0f, (2f * (_e60 - 0.5f))), 0.5f);
            let _e68 = uv_7;
            let _e69 = top_1;
            let _e70 = bot_1;
            let _e71 = tip_3;
            let _e72 = pointInTriangle(_e68, _e69, _e70, _e71);
            if _e72 {
                let _e73 = uv_7;
                let _e74 = getFromColor(_e73);
                return _e74;
            }
        }
    }
    let _e75 = uv_7;
    let _e76 = getToColor(_e75);
    return _e76;
}

fn glt_main(uv_8: vec2<f32>) -> vec4<f32> {
    var uv_9: vec2<f32>;
    var local: vec4<f32>;
    var local_1: vec4<f32>;

    uv_9 = uv_8;
    let _e7 = (p.reverse > 0.5);
    if _e7 {
        let _e8 = u.progress;
        if (_e8 < 0.5f) {
            let _e11 = uv_9;
            let _e13 = u.progress;
            let _e15 = transition_secondHalf(_e11, (1f - _e13));
            local = _e15;
        } else {
            let _e16 = uv_9;
            let _e18 = u.progress;
            let _e20 = transition_firstHalf(_e16, (1f - _e18));
            local = _e20;
        }
        let _e22 = local;
        return _e22;
    } else {
        let _e23 = u.progress;
        if (_e23 < 0.5f) {
            let _e26 = uv_9;
            let _e27 = u.progress;
            let _e28 = transition_firstHalf(_e26, _e27);
            local_1 = _e28;
        } else {
            let _e29 = uv_9;
            let _e30 = u.progress;
            let _e31 = transition_secondHalf(_e29, _e30);
            local_1 = _e31;
        }
        let _e33 = local_1;
        return _e33;
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
