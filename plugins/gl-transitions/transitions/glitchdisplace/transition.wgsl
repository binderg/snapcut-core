// Author: Matt DesLauriers
// License: MIT
// Description: Blocks of the image are displaced in a glitch that flashes into the incoming image
// Tags: glitch, displace
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

fn random(co: vec2<f32>) -> f32 {
    var co_1: vec2<f32>;
    var a: f32 = 12.9898f;
    var b: f32 = 78.233f;
    var c: f32 = 43758.547f;
    var dt: f32;
    var sn: f32;

    co_1 = co;
    let _e10 = co_1;
    let _e12 = a;
    let _e13 = b;
    dt = dot(_e10.xy, vec2<f32>(_e12, _e13));
    let _e17 = dt;
    sn = (_e17 - (floor((_e17 / 3.14f)) * 3.14f));
    let _e24 = sn;
    let _e26 = c;
    return fract((sin(_e24) * _e26));
}

fn voronoi(x: vec2<f32>) -> f32 {
    var x_1: vec2<f32>;
    var glt_p: vec2<f32>;
    var f: vec2<f32>;
    var res: f32 = 8f;
    var j: f32 = -1f;
    var i: f32;
    var b_1: vec2<f32>;
    var r: vec2<f32>;
    var d: f32;

    x_1 = x;
    let _e4 = x_1;
    glt_p = floor(_e4);
    let _e7 = x_1;
    f = fract(_e7);
    loop {
        let _e15 = j;
        if !((_e15 <= 1f)) {
            break;
        }
        i = -1f;
        loop {
            let _e25 = i;
            if !((_e25 <= 1f)) {
                break;
            }
            {
                let _e32 = i;
                let _e33 = j;
                b_1 = vec2<f32>(_e32, _e33);
                let _e36 = b_1;
                let _e37 = f;
                let _e39 = glt_p;
                let _e40 = b_1;
                let _e42 = random((_e39 + _e40));
                r = ((_e36 - _e37) + vec2(_e42));
                let _e46 = r;
                let _e47 = r;
                d = dot(_e46, _e47);
                let _e50 = res;
                let _e51 = d;
                res = min(_e50, _e51);
            }
            continuing {
                let _e29 = i;
                i = (_e29 + 1f);
            }
        }
        continuing {
            let _e19 = j;
            j = (_e19 + 1f);
        }
    }
    let _e53 = res;
    return sqrt(_e53);
}

fn displace(tex: vec4<f32>, texCoord: vec2<f32>, dotDepth: f32, textureDepth: f32, strength: f32) -> vec2<f32> {
    var tex_1: vec4<f32>;
    var texCoord_1: vec2<f32>;
    var dotDepth_1: f32;
    var textureDepth_1: f32;
    var strength_1: f32;
    var b_2: f32;
    var g: f32;
    var r_1: f32;
    var dt_1: vec4<f32>;
    var dis: vec4<f32>;
    var res_uv: vec2<f32>;

    tex_1 = tex;
    texCoord_1 = texCoord;
    dotDepth_1 = dotDepth;
    textureDepth_1 = textureDepth;
    strength_1 = strength;
    let _e13 = texCoord_1;
    let _e18 = voronoi(((0.003f * _e13) + vec2(2f)));
    b_2 = _e18;
    let _e21 = texCoord_1;
    let _e23 = voronoi((0.2f * _e21));
    g = _e23;
    let _e25 = texCoord_1;
    let _e29 = voronoi((_e25 - vec2(1f)));
    r_1 = _e29;
    let _e31 = tex_1;
    dt_1 = (_e31 * 1f);
    let _e35 = dt_1;
    let _e36 = dotDepth_1;
    let _e41 = tex_1;
    let _e42 = textureDepth_1;
    dis = (((_e35 * _e36) + vec4(1f)) - (_e41 * _e42));
    let _e47 = dis;
    let _e51 = textureDepth_1;
    let _e52 = dotDepth_1;
    dis.x = ((_e47.x - 1f) + (_e51 * _e52));
    let _e56 = dis;
    let _e60 = textureDepth_1;
    let _e61 = dotDepth_1;
    dis.y = ((_e56.y - 1f) + (_e60 * _e61));
    let _e65 = dis;
    let _e67 = strength_1;
    dis.x = (_e65.x * _e67);
    let _e70 = dis;
    let _e72 = strength_1;
    dis.y = (_e70.y * _e72);
    let _e74 = texCoord_1;
    res_uv = _e74;
    let _e77 = res_uv;
    let _e79 = dis;
    res_uv.x = ((_e77.x + _e79.x) - 0f);
    let _e85 = res_uv;
    let _e87 = dis;
    res_uv.y = (_e85.y + _e87.y);
    let _e90 = res_uv;
    return _e90;
}

fn ease1_(t: f32) -> f32 {
    var t_1: f32;
    var local: f32;
    var local_1: f32;

    t_1 = t;
    let _e4 = t_1;
    let _e7 = t_1;
    if ((_e4 == 0f) || (_e7 == 1f)) {
        let _e11 = t_1;
        local_1 = _e11;
    } else {
        let _e12 = t_1;
        if (_e12 < 0.5f) {
            let _e18 = t_1;
            local = (0.5f * pow(2f, ((20f * _e18) - 10f)));
        } else {
            let _e28 = t_1;
            local = ((-0.5f * pow(2f, (10f - (_e28 * 20f)))) + 1f);
        }
        let _e37 = local;
        local_1 = _e37;
    }
    let _e39 = local_1;
    return _e39;
}

fn ease2_(t_2: f32) -> f32 {
    var t_3: f32;
    var local_2: f32;

    t_3 = t_2;
    let _e4 = t_3;
    if (_e4 == 1f) {
        let _e7 = t_3;
        local_2 = _e7;
    } else {
        let _e12 = t_3;
        local_2 = (1f - pow(2f, (-10f * _e12)));
    }
    let _e17 = local_2;
    return _e17;
}

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var p_1: vec2<f32>;
    var color1_: vec4<f32>;
    var color2_: vec4<f32>;
    var disp: vec2<f32>;
    var disp2_: vec2<f32>;
    var dColor1_: vec4<f32>;
    var dColor2_: vec4<f32>;
    var val: f32;
    var gray: vec3<f32>;

    uv_5 = uv_4;
    let _e4 = uv_5;
    p_1 = (_e4.xy / vec2(1f));
    let _e11 = p_1;
    let _e12 = getFromColor(_e11);
    color1_ = _e12;
    let _e14 = p_1;
    let _e15 = getToColor(_e14);
    color2_ = _e15;
    let _e17 = color1_;
    let _e18 = p_1;
    let _e22 = u.progress;
    let _e23 = ease1_(_e22);
    let _e25 = displace(_e17, _e18, 0.33f, 0.7f, (1f - _e23));
    disp = _e25;
    let _e27 = color2_;
    let _e28 = p_1;
    let _e31 = u.progress;
    let _e32 = ease2_(_e31);
    let _e33 = displace(_e27, _e28, 0.33f, 0.5f, _e32);
    disp2_ = _e33;
    let _e35 = disp;
    let _e36 = getToColor(_e35);
    dColor1_ = _e36;
    let _e38 = disp2_;
    let _e39 = getFromColor(_e38);
    dColor2_ = _e39;
    let _e41 = u.progress;
    let _e42 = ease1_(_e41);
    val = _e42;
    let _e44 = dColor2_;
    let _e45 = dColor1_;
    gray = vec3(dot(min(_e44, _e45).xyz, vec3<f32>(0.299f, 0.587f, 0.114f)));
    let _e55 = gray;
    dColor2_ = vec4<f32>(_e55.x, _e55.y, _e55.z, 1f);
    let _e61 = dColor2_;
    dColor2_ = (_e61 * 2f);
    let _e64 = color1_;
    let _e65 = dColor2_;
    let _e68 = u.progress;
    color1_ = mix(_e64, _e65, vec4(smoothstep(0f, 0.5f, _e68)));
    let _e72 = color2_;
    let _e73 = dColor1_;
    let _e76 = u.progress;
    color2_ = mix(_e72, _e73, vec4(smoothstep(1f, 0.5f, _e76)));
    let _e80 = color1_;
    let _e81 = color2_;
    let _e82 = val;
    return mix(_e80, _e81, vec4(_e82));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
