// Author: JustKirillS
// License: MIT
// Description: The image splits into a grid of tiles that flip one by one to the incoming image
// Tags: grid, tiles, flip
// Ported from https://gist.github.com/JustKirillS/714f095318834f4d2375de872c53af1e
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
    return fract((sin(dot(_e7, vec2<f32>(12.9898f, 78.233f))) * 43758.547f));
}

fn getDelta(glt_p: vec2<f32>) -> f32 {
    var p_1: vec2<f32>;
    var rectangleSize: vec2<f32>;
    var rectanglePos: vec2<f32>;
    var top: f32;
    var bottom: f32;
    var left: f32;
    var right: f32;
    var minX: f32;
    var minY: f32;

    p_1 = glt_p;
    let _e8 = vec2<i32>(p.size);
    rectangleSize = (vec2(1f) / vec2<f32>(_e8));
    let _e13 = vec2<i32>(p.size);
    let _e15 = p_1;
    rectanglePos = floor((vec2<f32>(_e13) * _e15));
    let _e19 = rectangleSize;
    let _e21 = rectanglePos;
    top = (_e19.y * (_e21.y + 1f));
    let _e27 = rectangleSize;
    let _e29 = rectanglePos;
    bottom = (_e27.y * _e29.y);
    let _e33 = rectangleSize;
    let _e35 = rectanglePos;
    left = (_e33.x * _e35.x);
    let _e39 = rectangleSize;
    let _e41 = rectanglePos;
    right = (_e39.x * (_e41.x + 1f));
    let _e47 = p_1;
    let _e49 = left;
    let _e52 = p_1;
    let _e54 = right;
    minX = min(abs((_e47.x - _e49)), abs((_e52.x - _e54)));
    let _e59 = p_1;
    let _e61 = top;
    let _e64 = p_1;
    let _e66 = bottom;
    minY = min(abs((_e59.y - _e61)), abs((_e64.y - _e66)));
    let _e71 = minX;
    let _e72 = minY;
    return min(_e71, _e72);
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var currentProg: f32;
    var a: f32 = 1f;
    var currentProg_1: f32;
    var rectanglePos_1: vec2<f32>;
    var r: f32;
    var cp: f32;
    var rectangleSize_1: f32;
    var delta: f32;
    var offset: f32;
    var p_2: vec2<f32>;
    var a_1: vec4<f32>;
    var b: vec4<f32>;
    var s: f32;
    var currentProg_2: f32;
    var a_2: f32 = 1f;

    uv_5 = uv_4;
    let _e7 = u.progress;
    let _e8 = p.pause;
    if (_e7 < _e8) {
        {
            let _e10 = u.progress;
            let _e11 = p.pause;
            currentProg = (_e10 / _e11);
            let _e16 = uv_5;
            let _e17 = getDelta(_e16);
            let _e18 = p.divider_width;
            if (_e17 < _e18) {
                {
                    let _e21 = currentProg;
                    a = (1f - _e21);
                }
            }
            let _e28 = uv_5;
            let _e29 = getFromColor(_e28);
            let _e30 = a;
            return mix(vec4<f32>(0f, 0f, 0f, 1f), _e29, vec4(_e30));
        }
    } else {
        let _e33 = u.progress;
        let _e35 = p.pause;
        if (_e33 < (1f - _e35)) {
            {
                let _e38 = uv_5;
                let _e39 = getDelta(_e38);
                let _e40 = p.divider_width;
                if (_e39 < _e40) {
                    {
                        return vec4<f32>(0f, 0f, 0f, 1f);
                    }
                }
                let _e47 = u.progress;
                let _e48 = p.pause;
                let _e51 = p.pause;
                currentProg_1 = ((_e47 - _e48) / (1f - (_e51 * 2f)));
                let _e57 = vec2<i32>(p.size);
                let _e59 = uv_5;
                rectanglePos_1 = floor((vec2<f32>(_e57) * _e59));
                let _e63 = rectanglePos_1;
                let _e64 = rand(_e63);
                r = (_e64 - 0.1f);
                let _e70 = r;
                let _e72 = currentProg_1;
                cp = smoothstep(0f, (1f - _e70), _e72);
                let _e76 = vec2<i32>(p.size);
                rectangleSize_1 = (1f / f32(_e76.x));
                let _e81 = rectanglePos_1;
                let _e83 = rectangleSize_1;
                delta = (_e81.x * _e83);
                let _e86 = rectangleSize_1;
                let _e89 = delta;
                offset = ((_e86 / 2f) + _e89);
                let _e92 = uv_5;
                p_2 = _e92;
                let _e95 = p_2;
                let _e97 = offset;
                let _e99 = cp;
                let _e106 = offset;
                p_2.x = ((((_e95.x - _e97) / abs((_e99 - 0.5f))) * 0.5f) + _e106);
                let _e108 = p_2;
                let _e109 = getFromColor(_e108);
                a_1 = _e109;
                let _e111 = p_2;
                let _e112 = getToColor(_e111);
                b = _e112;
                let _e114 = vec2<i32>(p.size);
                let _e117 = uv_5;
                let _e119 = delta;
                let _e125 = cp;
                s = step(abs(((f32(_e114.x) * (_e117.x - _e119)) - 0.5f)), abs((_e125 - 0.5f)));
                let _e131 = b;
                let _e132 = a_1;
                let _e133 = cp;
                let _e139 = s;
                let _e140 = (mix(_e131, _e132, vec4(step(_e133, 0.5f))).xyz * _e139);
                return vec4<f32>(_e140.x, _e140.y, _e140.z, 1f);
            }
        } else {
            {
                let _e146 = u.progress;
                let _e149 = p.pause;
                let _e151 = p.pause;
                currentProg_2 = (((_e146 - 1f) + _e149) / _e151);
                let _e156 = uv_5;
                let _e157 = getDelta(_e156);
                let _e158 = p.divider_width;
                if (_e157 < _e158) {
                    {
                        let _e160 = currentProg_2;
                        a_2 = _e160;
                    }
                }
                let _e166 = uv_5;
                let _e167 = getToColor(_e166);
                let _e168 = a_2;
                return mix(vec4<f32>(0f, 0f, 0f, 1f), _e167, vec4(_e168));
            }
        }
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
