// Author: lbl
// License: MIT
// Description: The outgoing image shatters into fragments that fly away
// Tags: shatter, fragments
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

fn random(par: vec2<f32>) -> f32 {
    var par_1: vec2<f32>;

    par_1 = par;
    let _e4 = par_1;
    return fract((sin(dot(_e4.xy, vec2<f32>(12.9898f, 78.233f))) * 43758.547f));
}

fn random2_(par_2: vec2<f32>) -> vec2<f32> {
    var par_3: vec2<f32>;
    var rand: f32;

    par_3 = par_2;
    let _e4 = par_3;
    let _e5 = random(_e4);
    rand = _e5;
    let _e7 = rand;
    let _e8 = par_3;
    let _e9 = rand;
    let _e12 = random((_e8 + vec2(_e9)));
    return vec2<f32>(_e7, _e12);
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var duration: f32 = 8f;
    var time: f32;
    var point: array<vec2<f32>, 10>;
    var i: i32 = 0i;
    var col: vec4<f32>;
    var i_1: i32 = 0i;
    var dir: vec2<f32>;
    var v: f32;
    var ofst: vec2<f32>;
    var U: vec2<f32>;
    var dist_i: f32;
    var closest: bool;
    var j: i32;

    uv_5 = uv_4;
    let _e4 = u.progress;
    if (_e4 <= 0f) {
        let _e7 = uv_5;
        let _e8 = getFromColor(_e7);
        return _e8;
    }
    let _e9 = u.progress;
    if (_e9 >= 1f) {
        let _e12 = uv_5;
        let _e13 = getToColor(_e12);
        return _e13;
    }
    let _e16 = u.progress;
    let _e17 = duration;
    time = (_e16 * _e17);
    loop {
        let _e23 = i;
        if !((_e23 < 10i)) {
            break;
        }
        {
            let _e30 = i;
            let _e32 = i;
            let _e35 = random2_(vec2(f32(_e32)));
            point[_e30] = _e35;
        }
        continuing {
            let _e27 = i;
            i = (_e27 + 1i);
        }
    }
    let _e36 = uv_5;
    let _e37 = getToColor(_e36);
    col = _e37;
    loop {
        let _e41 = i_1;
        if !((_e41 < 10i)) {
            break;
        }
        {
            let _e48 = i_1;
            let _e50 = i_1;
            let _e55 = random2_(vec2<f32>(f32(_e48), (f32(_e50) + 11f)));
            dir = normalize(_e55);
            let _e59 = dir;
            let _e60 = random(_e59);
            v = ((1f + (_e60 * 0.5f)) * 0.2f);
            let _e67 = dir;
            let _e68 = time;
            let _e72 = duration;
            let _e75 = v;
            ofst = ((_e67 * clamp((_e68 - 0.5f), 0f, _e72)) * _e75);
            let _e78 = uv_5;
            let _e79 = ofst;
            U = (_e78 - _e79);
            let _e82 = U;
            let _e86 = U;
            let _e91 = U;
            let _e96 = U;
            if ((((_e82.x < 0f) || (_e86.x > 1f)) || (_e91.y < 0f)) || (_e96.y > 1f)) {
                continue;
            }
            let _e101 = U;
            let _e102 = i_1;
            let _e104 = point[_e102];
            dist_i = distance(_e101, _e104);
            closest = true;
            j = 0i;
            loop {
                let _e111 = j;
                if !((_e111 < 10i)) {
                    break;
                }
                {
                    let _e118 = U;
                    let _e119 = j;
                    let _e121 = point[_e119];
                    let _e123 = dist_i;
                    if (distance(_e118, _e121) < _e123) {
                        {
                            closest = false;
                            break;
                        }
                    }
                }
                continuing {
                    let _e115 = j;
                    j = (_e115 + 1i);
                }
            }
            let _e126 = closest;
            if _e126 {
                {
                    let _e127 = U;
                    let _e128 = getFromColor(_e127);
                    col = _e128;
                    break;
                }
            }
        }
        continuing {
            let _e45 = i_1;
            i_1 = (_e45 + 1i);
        }
    }
    let _e129 = col;
    return _e129;
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
