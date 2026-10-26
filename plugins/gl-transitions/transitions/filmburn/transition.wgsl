// Author: Anastasia Dunbar
// License: MIT
// Description: Glowing film-burn light leaks wash over the image and reveal the incoming one
// Tags: burn, film, light-leak
// @param Random seed
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

fn sigmoid(x: f32, a: f32) -> f32 {
    var x_1: f32;
    var a_1: f32;
    var b: f32;

    x_1 = x;
    a_1 = a;
    let _e7 = x_1;
    let _e10 = a_1;
    b = (pow((_e7 * 2f), _e10) / 2f);
    let _e15 = x_1;
    if (_e15 > 0.5f) {
        {
            let _e20 = x_1;
            let _e24 = a_1;
            b = (1f - (pow((2f - (_e20 * 2f)), _e24) / 2f));
        }
    }
    let _e29 = b;
    return _e29;
}

fn rand(co: f32) -> f32 {
    var co_1: f32;

    co_1 = co;
    let _e5 = co_1;
    let _e8 = p.seed;
    return fract((sin(((_e5 * 24.9898f) + _e8)) * 43758.547f));
}

fn rand_1(co_2: vec2<f32>) -> f32 {
    var co_3: vec2<f32>;

    co_3 = co_2;
    let _e5 = co_3;
    return fract((sin(dot(_e5.xy, vec2<f32>(12.9898f, 78.233f))) * 43758.547f));
}

fn apow(a_2: f32, b_1: f32) -> f32 {
    var a_3: f32;
    var b_2: f32;

    a_3 = a_2;
    b_2 = b_1;
    let _e7 = a_3;
    let _e9 = b_2;
    let _e11 = b_2;
    return (pow(abs(_e7), _e9) * sign(_e11));
}

fn pow3_(a_4: vec3<f32>, b_3: vec3<f32>) -> vec3<f32> {
    var a_5: vec3<f32>;
    var b_4: vec3<f32>;

    a_5 = a_4;
    b_4 = b_3;
    let _e7 = a_5;
    let _e9 = b_4;
    let _e11 = apow(_e7.x, _e9.x);
    let _e12 = a_5;
    let _e14 = b_4;
    let _e16 = apow(_e12.y, _e14.y);
    let _e17 = a_5;
    let _e19 = b_4;
    let _e21 = apow(_e17.z, _e19.z);
    return vec3<f32>(_e11, _e16, _e21);
}

fn smooth_mix(a_6: f32, b_5: f32, c: f32) -> f32 {
    var a_7: f32;
    var b_6: f32;
    var c_1: f32;

    a_7 = a_6;
    b_6 = b_5;
    c_1 = c;
    let _e9 = a_7;
    let _e10 = b_6;
    let _e11 = c_1;
    let _e13 = sigmoid(_e11, 2f);
    return mix(_e9, _e10, _e13);
}

fn random(co_4: vec2<f32>, shft: f32) -> f32 {
    var co_5: vec2<f32>;
    var shft_1: f32;

    co_5 = co_4;
    shft_1 = shft;
    let _e7 = co_5;
    co_5 = (_e7 + vec2(10f));
    let _e11 = co_5;
    let _e14 = shft_1;
    let _e20 = p.seed;
    let _e28 = co_5;
    let _e31 = shft_1;
    let _e39 = p.seed;
    let _e47 = shft_1;
    let _e49 = smooth_mix(fract((sin(dot(_e11.xy, vec2<f32>((12.9898f + (floor(_e14) * 0.5f)), (78.233f + _e20)))) * 43758.547f)), fract((sin(dot(_e28.xy, vec2<f32>((12.9898f + (floor((_e31 + 1f)) * 0.5f)), (78.233f + _e39)))) * 43758.547f)), fract(_e47));
    return _e49;
}

fn smooth_random(co_6: vec2<f32>, shft_2: f32) -> f32 {
    var co_7: vec2<f32>;
    var shft_3: f32;

    co_7 = co_6;
    shft_3 = shft_2;
    let _e7 = co_7;
    let _e9 = shft_3;
    let _e10 = random(floor(_e7), _e9);
    let _e11 = co_7;
    let _e17 = shft_3;
    let _e18 = random(floor((_e11 + vec2<f32>(1f, 0f))), _e17);
    let _e19 = co_7;
    let _e22 = smooth_mix(_e10, _e18, fract(_e19.x));
    let _e23 = co_7;
    let _e29 = shft_3;
    let _e30 = random(floor((_e23 + vec2<f32>(0f, 1f))), _e29);
    let _e31 = co_7;
    let _e37 = shft_3;
    let _e38 = random(floor((_e31 + vec2<f32>(1f, 1f))), _e37);
    let _e39 = co_7;
    let _e42 = smooth_mix(_e30, _e38, fract(_e39.x));
    let _e43 = co_7;
    let _e46 = smooth_mix(_e22, _e42, fract(_e43.y));
    return _e46;
}

fn blendedColor(glt_p: vec2<f32>) -> vec4<f32> {
    var p_1: vec2<f32>;

    p_1 = glt_p;
    let _e5 = p_1;
    let _e6 = getFromColor(_e5);
    let _e7 = p_1;
    let _e8 = getToColor(_e7);
    let _e9 = u.progress;
    let _e11 = sigmoid(_e9, 10f);
    return mix(_e6, _e8, vec4(_e11));
}

fn glt_main(p_2: vec2<f32>) -> vec4<f32> {
    var p_3: vec2<f32>;
    var f: vec3<f32> = vec3(0f);
    var i: f32 = 0f;
    var blurred_image: vec4<f32> = vec4(0f);
    var bluramount: f32;
    var i_1: f32 = 0f;
    var q: vec2<f32>;
    var uv2_: vec2<f32>;

    p_3 = p_2;
    loop {
        let _e10 = i;
        if !((_e10 < 13f)) {
            break;
        }
        {
            let _e17 = f;
            let _e18 = p_3;
            let _e20 = i;
            let _e21 = rand(_e20);
            let _e25 = u.progress;
            let _e29 = i;
            let _e32 = rand((_e29 + 1.43f));
            let _e35 = p_3;
            let _e37 = i;
            let _e40 = rand((_e37 + 4.4f));
            let _e44 = u.progress;
            let _e48 = i;
            let _e51 = rand((_e48 + 2.4f));
            f = (_e17 + vec3((sin(((((_e18.x * _e21) * 6f) + (_e25 * 8f)) + _e32)) * sin(((((_e35.y * _e40) * 6f) + (_e44 * 6f)) + _e51)))));
            let _e57 = f;
            let _e59 = p_3;
            let _e60 = u.progress;
            let _e64 = i;
            let _e67 = smooth_random(vec2((_e60 * 1.3f)), (_e64 + 1f));
            let _e68 = u.progress;
            let _e72 = i;
            let _e75 = smooth_random(vec2((_e68 * 0.5f)), (_e72 + 6.25f));
            let _e81 = i;
            let _e82 = rand(_e81);
            f = (_e57 + vec3((1f - clamp((length((_e59 - vec2<f32>(_e67, _e75))) * mix(20f, 70f, _e82)), 0f, 1f))));
        }
        continuing {
            let _e14 = i;
            i = (_e14 + 1f);
        }
    }
    let _e91 = f;
    f = (_e91 + vec3(4f));
    let _e95 = f;
    f = (_e95 / vec3(11f));
    let _e99 = f;
    let _e107 = u.progress;
    let _e114 = pow3_((_e99 * vec3<f32>(1f, 0.7f, 0.6f)), vec3<f32>(1f, (2f - sin((_e107 * 3.1415927f))), 1.3f));
    f = _e114;
    let _e115 = f;
    let _e116 = u.progress;
    f = (_e115 * sin((_e116 * 3.1415927f)));
    let _e121 = p_3;
    p_3 = (_e121 - vec2(0.5f));
    let _e125 = p_3;
    let _e127 = u.progress;
    let _e132 = smooth_random(vec2((_e127 * 5f)), 6.3f);
    let _e133 = u.progress;
    p_3 = (_e125 * (1f + ((_e132 * sin((_e133 * 3.1415927f))) * 0.05f)));
    let _e142 = p_3;
    p_3 = (_e142 + vec2(0.5f));
    let _e149 = u.progress;
    bluramount = (sin((_e149 * 3.1415927f)) * 0.03f);
    loop {
        let _e158 = i_1;
        if !((_e158 < 50f)) {
            break;
        }
        {
            let _e165 = i_1;
            let _e172 = i_1;
            let _e180 = i_1;
            let _e181 = p_3;
            let _e183 = p_3;
            let _e187 = rand_1(vec2<f32>(_e180, (_e181.x + _e183.y)));
            let _e188 = bluramount;
            q = (vec2<f32>(cos(degrees(((_e165 / 50f) * 360f))), sin(degrees(((_e172 / 50f) * 360f)))) * (_e187 + _e188));
            let _e192 = p_3;
            let _e193 = q;
            let _e194 = bluramount;
            uv2_ = (_e192 + (_e193 * _e194));
            let _e198 = blurred_image;
            let _e199 = uv2_;
            let _e200 = blendedColor(_e199);
            blurred_image = (_e198 + _e200);
        }
        continuing {
            let _e162 = i_1;
            i_1 = (_e162 + 1f);
        }
    }
    let _e202 = blurred_image;
    blurred_image = (_e202 / vec4(50f));
    let _e206 = blurred_image;
    let _e207 = f;
    return (_e206 + vec4<f32>(_e207.x, _e207.y, _e207.z, 0f));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
