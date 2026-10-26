// Name: Power Kaleido
// Author: Boundless
// License: MIT
// Description: The image folds into an animated kaleidoscope before revealing the incoming one
// Tags: kaleidoscope, distort
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

const rad: f32 = 120f;

const deg: f32 = 2.0943952f;

fn refl(glt_p: vec2<f32>, o: vec2<f32>, n: vec2<f32>) -> vec2<f32> {
    var p_1: vec2<f32>;
    var o_1: vec2<f32>;
    var n_1: vec2<f32>;

    p_1 = glt_p;
    o_1 = o;
    n_1 = n;
    let _e14 = o_1;
    let _e17 = n_1;
    let _e19 = p_1;
    let _e20 = o_1;
    let _e22 = n_1;
    let _e26 = p_1;
    return (((2f * _e14) + ((2f * _e17) * dot((_e19 - _e20), _e22))) - _e26);
}

fn rot(p_2: vec2<f32>, o_2: vec2<f32>, a: f32) -> vec2<f32> {
    var p_3: vec2<f32>;
    var o_3: vec2<f32>;
    var a_1: f32;
    var s: f32;
    var c: f32;

    p_3 = p_2;
    o_3 = o_2;
    a_1 = a;
    let _e13 = a_1;
    s = sin(_e13);
    let _e16 = a_1;
    c = cos(_e16);
    let _e19 = o_3;
    let _e20 = c;
    let _e21 = s;
    let _e23 = s;
    let _e24 = c;
    let _e28 = p_3;
    let _e29 = o_3;
    return (_e19 + (mat2x2<f32>(vec2<f32>(_e20, -(_e21)), vec2<f32>(_e23, _e24)) * (_e28 - _e29)));
}

fn mainImage(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var dist: f32;
    var uv0_: vec2<f32>;
    var theta: f32;
    var iter: i32 = 0i;
    var i: f32;
    var local: f32;
    var ts: f32;
    var uvMix: vec2<f32>;
    var color: vec4<f32>;

    uv_5 = uv_4;
    let _e9 = p.scale;
    dist = (_e9 / 10f);
    let _e13 = uv_5;
    uv0_ = _e13;
    let _e15 = uv_5;
    uv_5 = (_e15 - vec2(0.5f));
    let _e20 = uv_5;
    let _e22 = (u.resolution.x / u.resolution.y);
    uv_5.x = (_e20.x * _e22);
    let _e24 = uv_5;
    let _e25 = p.z;
    uv_5 = (_e24 * _e25);
    let _e27 = uv_5;
    let _e30 = u.progress;
    let _e31 = p.speed;
    let _e33 = rot(_e27, vec2(0f), (_e30 * _e31));
    uv_5 = _e33;
    let _e34 = u.progress;
    theta = ((_e34 * 6f) + 6.2831855f);
    loop {
        let _e44 = iter;
        if !((_e44 < 10i)) {
            break;
        }
        {
            i = 0f;
            loop {
                let _e53 = i;
                if !((_e53 < 6.2831855f)) {
                    break;
                }
                {
                    let _e61 = i;
                    if (sign(asin(cos(_e61))) == 1f) {
                        local = 1f;
                    } else {
                        local = 0f;
                    }
                    let _e70 = local;
                    ts = _e70;
                    let _e72 = ts;
                    let _e75 = uv_5;
                    let _e77 = dist;
                    let _e78 = i;
                    let _e82 = i;
                    let _e84 = uv_5;
                    let _e86 = dist;
                    let _e87 = i;
                    let _e94 = ts;
                    let _e97 = uv_5;
                    let _e99 = dist;
                    let _e100 = i;
                    let _e104 = i;
                    let _e106 = uv_5;
                    let _e108 = dist;
                    let _e109 = i;
                    if (((_e72 == 1f) && ((_e75.y - (_e77 * cos(_e78))) > (tan(_e82) * (_e84.x + (_e86 * sin(_e87)))))) || ((_e94 == 0f) && ((_e97.y - (_e99 * cos(_e100))) < (tan(_e104) * (_e106.x + (_e108 * sin(_e109))))))) {
                        {
                            let _e117 = uv_5;
                            let _e119 = i;
                            let _e121 = dist;
                            let _e126 = uv_5;
                            let _e128 = i;
                            let _e130 = dist;
                            let _e139 = i;
                            let _e141 = i;
                            let _e144 = refl(vec2<f32>((_e117.x + ((sin(_e119) * _e121) * 2f)), (_e126.y - ((cos(_e128) * _e130) * 2f))), vec2<f32>(0f, 0f), vec2<f32>(cos(_e139), sin(_e141)));
                            uv_5 = _e144;
                        }
                    }
                }
                continuing {
                    let _e59 = i;
                    i = (_e59 + deg);
                }
            }
        }
        continuing {
            let _e48 = iter;
            iter = (_e48 + 1i);
        }
    }
    let _e145 = uv_5;
    uv_5 = (_e145 + vec2(0.5f));
    let _e149 = uv_5;
    let _e152 = u.progress;
    let _e153 = p.speed;
    let _e156 = rot(_e149, vec2(0.5f), (_e152 * -(_e153)));
    uv_5 = _e156;
    let _e157 = uv_5;
    uv_5 = (_e157 - vec2(0.5f));
    let _e162 = uv_5;
    let _e164 = (u.resolution.x / u.resolution.y);
    uv_5.x = (_e162.x / _e164);
    let _e166 = uv_5;
    uv_5 = (_e166 + vec2(0.5f));
    let _e171 = uv_5;
    let _e175 = uv_5;
    uv_5 = (2f * abs(((_e171 / vec2(2f)) - floor(((_e175 / vec2(2f)) + vec2(0.5f))))));
    let _e186 = uv_5;
    let _e187 = uv0_;
    let _e188 = u.progress;
    uvMix = mix(_e186, _e187, vec2(((cos(((_e188 * 3.1415927f) * 2f)) / 2f) + 0.5f)));
    let _e201 = uvMix;
    let _e202 = getFromColor(_e201);
    let _e203 = uvMix;
    let _e204 = getToColor(_e203);
    let _e205 = u.progress;
    color = mix(_e202, _e204, vec4(((cos(((_e205 - 1f) * 3.1415927f)) / 2f) + 0.5f)));
    let _e218 = color;
    return _e218;
}

fn glt_main(uv_6: vec2<f32>) -> vec4<f32> {
    var uv_7: vec2<f32>;
    var color_1: vec4<f32>;

    uv_7 = uv_6;
    let _e9 = uv_7;
    let _e10 = mainImage(_e9);
    color_1 = _e10;
    let _e12 = color_1;
    return _e12;
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
