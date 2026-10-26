// License: MIT
// Author: TimDonselaar
// Description: The image splits into a grid of tiles that flip one by one to the incoming image
// Tags: grid, tiles, flip
// ported by gre from https://gist.github.com/TimDonselaar/9bcd1c4b5934ba60087bdb55c2ea92e5
//
// @param Number of tiles (columns, rows)
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
    let _e9 = co_1;
    return fract((sin(dot(_e9.xy, vec2<f32>(12.9898f, 78.233f))) * 43758.547f));
}

fn getDelta(glt_p: vec2<f32>) -> f32 {
    var p_1: vec2<f32>;
    var rectanglePos: vec2<f32>;
    var rectangleSize: vec2<f32>;
    var top: f32;
    var bottom: f32;
    var left: f32;
    var right: f32;
    var minX: f32;
    var minY: f32;

    p_1 = glt_p;
    let _e9 = vec2<i32>(p.size);
    let _e11 = p_1;
    rectanglePos = floor((vec2<f32>(_e9) * _e11));
    let _e16 = vec2<i32>(p.size);
    let _e21 = vec2<i32>(p.size);
    rectangleSize = vec2<f32>((1f / vec2<f32>(_e16).x), (1f / vec2<f32>(_e21).y));
    let _e27 = rectangleSize;
    let _e29 = rectanglePos;
    top = (_e27.y * (_e29.y + 1f));
    let _e35 = rectangleSize;
    let _e37 = rectanglePos;
    bottom = (_e35.y * _e37.y);
    let _e41 = rectangleSize;
    let _e43 = rectanglePos;
    left = (_e41.x * _e43.x);
    let _e47 = rectangleSize;
    let _e49 = rectanglePos;
    right = (_e47.x * (_e49.x + 1f));
    let _e55 = p_1;
    let _e57 = left;
    let _e60 = p_1;
    let _e62 = right;
    minX = min(abs((_e55.x - _e57)), abs((_e60.x - _e62)));
    let _e67 = p_1;
    let _e69 = top;
    let _e72 = p_1;
    let _e74 = bottom;
    minY = min(abs((_e67.y - _e69)), abs((_e72.y - _e74)));
    let _e79 = minX;
    let _e80 = minY;
    return min(_e79, _e80);
}

fn getDividerSize() -> f32 {
    var rectangleSize_1: vec2<f32>;

    let _e8 = vec2<i32>(p.size);
    let _e13 = vec2<i32>(p.size);
    rectangleSize_1 = vec2<f32>((1f / vec2<f32>(_e8).x), (1f / vec2<f32>(_e13).y));
    let _e19 = rectangleSize_1;
    let _e21 = rectangleSize_1;
    let _e24 = p.divider_width;
    return (min(_e19.x, _e21.y) * _e24);
}

fn glt_main(p_2: vec2<f32>) -> vec4<f32> {
    var p_3: vec2<f32>;
    var currentProg: f32;
    var a: f32 = 1f;
    var currentProg_1: f32;
    var q: vec2<f32>;
    var rectanglePos_1: vec2<f32>;
    var r: f32;
    var cp: f32;
    var rectangleSize_2: f32;
    var delta: f32;
    var offset: f32;
    var a_1: vec4<f32>;
    var b: vec4<f32>;
    var s: f32;
    var currentProg_2: f32;
    var a_2: f32 = 1f;

    p_3 = p_2;
    let _e9 = u.progress;
    let _e10 = p.pause;
    if (_e9 < _e10) {
        {
            let _e12 = u.progress;
            let _e13 = p.pause;
            currentProg = (_e12 / _e13);
            let _e18 = p_3;
            let _e19 = getDelta(_e18);
            let _e20 = getDividerSize();
            if (_e19 < _e20) {
                {
                    let _e23 = currentProg;
                    a = (1f - _e23);
                }
            }
            let _e25 = p.bgcolor;
            let _e26 = p_3;
            let _e27 = getFromColor(_e26);
            let _e28 = a;
            return mix(_e25, _e27, vec4(_e28));
        }
    } else {
        let _e31 = u.progress;
        let _e33 = p.pause;
        if (_e31 < (1f - _e33)) {
            {
                let _e36 = p_3;
                let _e37 = getDelta(_e36);
                let _e38 = getDividerSize();
                if (_e37 < _e38) {
                    {
                        let _e40 = p.bgcolor;
                        return _e40;
                    }
                } else {
                    {
                        let _e41 = u.progress;
                        let _e42 = p.pause;
                        let _e45 = p.pause;
                        currentProg_1 = ((_e41 - _e42) / (1f - (_e45 * 2f)));
                        let _e51 = p_3;
                        q = _e51;
                        let _e53 = vec2<i32>(p.size);
                        let _e55 = q;
                        rectanglePos_1 = floor((vec2<f32>(_e53) * _e55));
                        let _e59 = rectanglePos_1;
                        let _e60 = rand(_e59);
                        let _e61 = p.randomness;
                        r = (_e60 - _e61);
                        let _e66 = r;
                        let _e68 = currentProg_1;
                        cp = smoothstep(0f, (1f - _e66), _e68);
                        let _e72 = vec2<i32>(p.size);
                        rectangleSize_2 = (1f / vec2<f32>(_e72).x);
                        let _e77 = rectanglePos_1;
                        let _e79 = rectangleSize_2;
                        delta = (_e77.x * _e79);
                        let _e82 = rectangleSize_2;
                        let _e85 = delta;
                        offset = ((_e82 / 2f) + _e85);
                        let _e89 = p_3;
                        let _e91 = offset;
                        let _e93 = cp;
                        let _e100 = offset;
                        p_3.x = ((((_e89.x - _e91) / abs((_e93 - 0.5f))) * 0.5f) + _e100);
                        let _e102 = p_3;
                        let _e103 = getFromColor(_e102);
                        a_1 = _e103;
                        let _e105 = p_3;
                        let _e106 = getToColor(_e105);
                        b = _e106;
                        let _e108 = vec2<i32>(p.size);
                        let _e111 = q;
                        let _e113 = delta;
                        let _e119 = cp;
                        s = step(abs(((vec2<f32>(_e108).x * (_e111.x - _e113)) - 0.5f)), abs((_e119 - 0.5f)));
                        let _e125 = p.bgcolor;
                        let _e126 = b;
                        let _e127 = a_1;
                        let _e128 = cp;
                        let _e133 = s;
                        return mix(_e125, mix(_e126, _e127, vec4(step(_e128, 0.5f))), vec4(_e133));
                    }
                }
            }
        } else {
            {
                let _e136 = u.progress;
                let _e139 = p.pause;
                let _e141 = p.pause;
                currentProg_2 = (((_e136 - 1f) + _e139) / _e141);
                let _e146 = p_3;
                let _e147 = getDelta(_e146);
                let _e148 = getDividerSize();
                if (_e147 < _e148) {
                    {
                        let _e150 = currentProg_2;
                        a_2 = _e150;
                    }
                }
                let _e151 = p.bgcolor;
                let _e152 = p_3;
                let _e153 = getToColor(_e152);
                let _e154 = a_2;
                return mix(_e151, _e153, vec4(_e154));
            }
        }
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
