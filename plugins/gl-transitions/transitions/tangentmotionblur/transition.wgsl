// License: MIT
// Author: chenkai
// Description: The outgoing image is flung away along a curve with motion blur
// Tags: motion-blur, rotate, slide
// ported from https://codertw.com/%E7%A8%8B%E5%BC%8F%E8%AA%9E%E8%A8%80/671116/
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
    let _e4 = co_1;
    return fract((sin(dot(_e4.xy, vec2<f32>(12.9898f, 78.233f))) * 43758.547f));
}

fn motionBlurFrom(_st: vec2<f32>, speed: vec2<f32>) -> vec4<f32> {
    var _st_1: vec2<f32>;
    var speed_1: vec2<f32>;
    var texCoord: vec2<f32>;
    var color: vec3<f32> = vec3(0f);
    var total: f32 = 0f;
    var offset: f32;
    var t: f32 = 0f;
    var percent: f32;
    var weight: f32;
    var newuv: vec2<f32>;

    _st_1 = _st;
    speed_1 = speed;
    let _e6 = _st_1;
    texCoord = (_e6.xy / vec2(1f));
    let _e18 = _st_1;
    let _e19 = rand(_e18);
    offset = _e19;
    loop {
        let _e23 = t;
        if !((_e23 <= 20f)) {
            break;
        }
        {
            let _e30 = t;
            let _e31 = offset;
            percent = ((_e30 + _e31) / 20f);
            let _e37 = percent;
            let _e38 = percent;
            let _e39 = percent;
            weight = (4f * (_e37 - (_e38 * _e39)));
            let _e44 = texCoord;
            let _e45 = speed_1;
            let _e46 = percent;
            newuv = (_e44 + (_e45 * _e46));
            let _e50 = newuv;
            newuv = fract(_e50);
            let _e52 = color;
            let _e53 = newuv;
            let _e54 = getFromColor(_e53);
            let _e56 = weight;
            color = (_e52 + (_e54.xyz * _e56));
            let _e59 = total;
            let _e60 = weight;
            total = (_e59 + _e60);
        }
        continuing {
            let _e27 = t;
            t = (_e27 + 1f);
        }
    }
    let _e62 = color;
    let _e63 = total;
    let _e65 = (_e62 / vec3(_e63));
    return vec4<f32>(_e65.x, _e65.y, _e65.z, 1f);
}

fn motionBlurTo(_st_2: vec2<f32>, speed_2: vec2<f32>) -> vec4<f32> {
    var _st_3: vec2<f32>;
    var speed_3: vec2<f32>;
    var texCoord_1: vec2<f32>;
    var color_1: vec3<f32> = vec3(0f);
    var total_1: f32 = 0f;
    var offset_1: f32;
    var t_1: f32 = 0f;
    var percent_1: f32;
    var weight_1: f32;
    var newuv_1: vec2<f32>;

    _st_3 = _st_2;
    speed_3 = speed_2;
    let _e6 = _st_3;
    texCoord_1 = (_e6.xy / vec2(1f));
    let _e18 = _st_3;
    let _e19 = rand(_e18);
    offset_1 = _e19;
    loop {
        let _e23 = t_1;
        if !((_e23 <= 20f)) {
            break;
        }
        {
            let _e30 = t_1;
            let _e31 = offset_1;
            percent_1 = ((_e30 + _e31) / 20f);
            let _e37 = percent_1;
            let _e38 = percent_1;
            let _e39 = percent_1;
            weight_1 = (4f * (_e37 - (_e38 * _e39)));
            let _e44 = texCoord_1;
            let _e45 = speed_3;
            let _e46 = percent_1;
            newuv_1 = (_e44 + (_e45 * _e46));
            let _e50 = newuv_1;
            newuv_1 = fract(_e50);
            let _e52 = color_1;
            let _e53 = newuv_1;
            let _e54 = getToColor(_e53);
            let _e56 = weight_1;
            color_1 = (_e52 + (_e54.xyz * _e56));
            let _e59 = total_1;
            let _e60 = weight_1;
            total_1 = (_e59 + _e60);
        }
        continuing {
            let _e27 = t_1;
            t_1 = (_e27 + 1f);
        }
    }
    let _e62 = color_1;
    let _e63 = total_1;
    let _e65 = (_e62 / vec3(_e63));
    return vec4<f32>(_e65.x, _e65.y, _e65.z, 1f);
}

fn A(aA1_: f32, aA2_: f32) -> f32 {
    var aA1_1: f32;
    var aA2_1: f32;

    aA1_1 = aA1_;
    aA2_1 = aA2_;
    let _e8 = aA2_1;
    let _e12 = aA1_1;
    return ((1f - (3f * _e8)) + (3f * _e12));
}

fn B(aA1_2: f32, aA2_2: f32) -> f32 {
    var aA1_3: f32;
    var aA2_3: f32;

    aA1_3 = aA1_2;
    aA2_3 = aA2_2;
    let _e7 = aA2_3;
    let _e10 = aA1_3;
    return ((3f * _e7) - (6f * _e10));
}

fn C(aA1_4: f32) -> f32 {
    var aA1_5: f32;

    aA1_5 = aA1_4;
    let _e5 = aA1_5;
    return (3f * _e5);
}

fn GetSlope(aT: f32, aA1_6: f32, aA2_4: f32) -> f32 {
    var aT_1: f32;
    var aA1_7: f32;
    var aA2_5: f32;

    aT_1 = aT;
    aA1_7 = aA1_6;
    aA2_5 = aA2_4;
    let _e9 = aA1_7;
    let _e10 = aA2_5;
    let _e11 = A(_e9, _e10);
    let _e13 = aT_1;
    let _e15 = aT_1;
    let _e18 = aA1_7;
    let _e19 = aA2_5;
    let _e20 = B(_e18, _e19);
    let _e22 = aT_1;
    let _e25 = aA1_7;
    let _e26 = C(_e25);
    return (((((3f * _e11) * _e13) * _e15) + ((2f * _e20) * _e22)) + _e26);
}

fn CalcBezier(aT_2: f32, aA1_8: f32, aA2_6: f32) -> f32 {
    var aT_3: f32;
    var aA1_9: f32;
    var aA2_7: f32;

    aT_3 = aT_2;
    aA1_9 = aA1_8;
    aA2_7 = aA2_6;
    let _e8 = aA1_9;
    let _e9 = aA2_7;
    let _e10 = A(_e8, _e9);
    let _e11 = aT_3;
    let _e13 = aA1_9;
    let _e14 = aA2_7;
    let _e15 = B(_e13, _e14);
    let _e17 = aT_3;
    let _e19 = aA1_9;
    let _e20 = C(_e19);
    let _e22 = aT_3;
    return (((((_e10 * _e11) + _e15) * _e17) + _e20) * _e22);
}

fn GetTForX(aX: f32, mX1_: f32, mX2_: f32) -> f32 {
    var aX_1: f32;
    var mX1_1: f32;
    var mX2_1: f32;
    var aGuessT: f32;
    var i: i32 = 0i;
    var currentSlope: f32;
    var currentX: f32;

    aX_1 = aX;
    mX1_1 = mX1_;
    mX2_1 = mX2_;
    let _e8 = aX_1;
    aGuessT = _e8;
    loop {
        let _e12 = i;
        if !((_e12 < 4i)) {
            break;
        }
        {
            let _e19 = aGuessT;
            let _e20 = mX1_1;
            let _e21 = mX2_1;
            let _e22 = GetSlope(_e19, _e20, _e21);
            currentSlope = _e22;
            let _e24 = currentSlope;
            if (_e24 == 0f) {
                let _e27 = aGuessT;
                return _e27;
            }
            let _e28 = aGuessT;
            let _e29 = mX1_1;
            let _e30 = mX2_1;
            let _e31 = CalcBezier(_e28, _e29, _e30);
            let _e32 = aX_1;
            currentX = (_e31 - _e32);
            let _e35 = aGuessT;
            let _e36 = currentX;
            let _e37 = currentSlope;
            aGuessT = (_e35 - (_e36 / _e37));
        }
        continuing {
            let _e16 = i;
            i = (_e16 + 1i);
        }
    }
    let _e40 = aGuessT;
    return _e40;
}

fn KeySpline(aX_2: f32, mX1_2: f32, mY1_: f32, mX2_2: f32, mY2_: f32) -> f32 {
    var aX_3: f32;
    var mX1_3: f32;
    var mY1_1: f32;
    var mX2_3: f32;
    var mY2_1: f32;

    aX_3 = aX_2;
    mX1_3 = mX1_2;
    mY1_1 = mY1_;
    mX2_3 = mX2_2;
    mY2_1 = mY2_;
    let _e12 = mX1_3;
    let _e13 = mY1_1;
    let _e15 = mX2_3;
    let _e16 = mY2_1;
    if ((_e12 == _e13) && (_e15 == _e16)) {
        let _e19 = aX_3;
        return _e19;
    }
    let _e20 = aX_3;
    let _e21 = mX1_3;
    let _e22 = mX2_3;
    let _e23 = GetTForX(_e20, _e21, _e22);
    let _e24 = mY1_1;
    let _e25 = mY2_1;
    let _e26 = CalcBezier(_e23, _e24, _e25);
    return _e26;
}

fn normpdf(x: f32) -> f32 {
    var x_1: f32;
    var d: f32;

    x_1 = x;
    let _e4 = x_1;
    d = (_e4 - 0.5f);
    let _e10 = d;
    let _e12 = d;
    return exp(((-20f * _e10) * _e12));
}

fn rotateUv(uv_4: vec2<f32>, angle: f32, anchor: vec2<f32>, zDirection: f32) -> vec2<f32> {
    var uv_5: vec2<f32>;
    var angle_1: f32;
    var anchor_1: vec2<f32>;
    var zDirection_1: f32;
    var s: f32;
    var c: f32;
    var m: mat2x2<f32>;

    uv_5 = uv_4;
    angle_1 = angle;
    anchor_1 = anchor;
    zDirection_1 = zDirection;
    let _e10 = uv_5;
    let _e11 = anchor_1;
    uv_5 = (_e10 - _e11);
    let _e13 = angle_1;
    s = sin(_e13);
    let _e16 = angle_1;
    c = cos(_e16);
    let _e19 = c;
    let _e20 = s;
    let _e22 = s;
    let _e23 = c;
    m = mat2x2<f32>(vec2<f32>(_e19, -(_e20)), vec2<f32>(_e22, _e23));
    let _e28 = m;
    let _e29 = uv_5;
    uv_5 = (_e28 * _e29);
    let _e31 = uv_5;
    let _e32 = anchor_1;
    uv_5 = (_e31 + _e32);
    let _e34 = uv_5;
    return _e34;
}

fn glt_main(uv_6: vec2<f32>) -> vec4<f32> {
    var uv_7: vec2<f32>;
    var myst: vec2<f32>;
    var animationTime: f32;
    var easingTime: f32;
    var blur: f32;
    var r: f32 = 0f;
    var rotation: f32 = 3.14159f;
    var mystCurrent: vec2<f32>;
    var timeInterval: f32 = 0.0334f;
    var mystNext: vec2<f32>;
    var speed_4: vec2<f32>;

    uv_7 = uv_6;
    let _e4 = uv_7;
    myst = _e4;
    let _e6 = u.progress;
    animationTime = _e6;
    let _e8 = animationTime;
    let _e13 = KeySpline(_e8, 0.68f, 0.01f, 0.17f, 0.98f);
    easingTime = _e13;
    let _e15 = easingTime;
    let _e16 = normpdf(_e15);
    blur = _e16;
    let _e26 = easingTime;
    if (_e26 <= 0.5f) {
        {
            let _e29 = rotation;
            let _e30 = easingTime;
            r = (_e29 * _e30);
        }
    } else {
        {
            let _e32 = rotation;
            let _e34 = rotation;
            let _e35 = easingTime;
            r = (-(_e32) + (_e34 * _e35));
        }
    }
    let _e38 = myst;
    mystCurrent = _e38;
    let _e41 = mystCurrent;
    let _e44 = (u.resolution.x / u.resolution.y);
    mystCurrent.y = (_e41.y * (1f / _e44));
    let _e47 = mystCurrent;
    let _e48 = r;
    let _e54 = rotateUv(_e47, _e48, vec2<f32>(1f, 0f), -1f);
    mystCurrent = _e54;
    let _e56 = mystCurrent;
    let _e58 = (u.resolution.x / u.resolution.y);
    mystCurrent.y = (_e56.y * _e58);
    let _e64 = easingTime;
    if (_e64 <= 0.5f) {
        {
            let _e67 = rotation;
            let _e68 = easingTime;
            let _e69 = timeInterval;
            r = (_e67 * (_e68 + _e69));
        }
    } else {
        {
            let _e72 = rotation;
            let _e74 = rotation;
            let _e75 = easingTime;
            let _e76 = timeInterval;
            r = (-(_e72) + (_e74 * (_e75 + _e76)));
        }
    }
    let _e80 = myst;
    mystNext = _e80;
    let _e83 = mystNext;
    let _e86 = (u.resolution.x / u.resolution.y);
    mystNext.y = (_e83.y * (1f / _e86));
    let _e89 = mystNext;
    let _e90 = r;
    let _e96 = rotateUv(_e89, _e90, vec2<f32>(1f, 0f), -1f);
    mystNext = _e96;
    let _e98 = mystNext;
    let _e100 = (u.resolution.x / u.resolution.y);
    mystNext.y = (_e98.y * _e100);
    let _e102 = mystNext;
    let _e103 = mystCurrent;
    let _e105 = timeInterval;
    let _e108 = blur;
    speed_4 = ((((_e102 - _e103) / vec2(_e105)) * _e108) * 0.5f);
    let _e113 = easingTime;
    if (_e113 <= 0.5f) {
        {
            let _e116 = mystCurrent;
            let _e117 = speed_4;
            let _e118 = motionBlurFrom(_e116, _e117);
            return _e118;
        }
    } else {
        {
            let _e119 = mystCurrent;
            let _e120 = speed_4;
            let _e121 = motionBlurTo(_e119, _e120);
            return _e121;
        }
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
