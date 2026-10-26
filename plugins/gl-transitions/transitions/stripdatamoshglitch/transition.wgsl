// Author: bread
// License: MIT
// Description: A datamosh glitch: torn strips, chroma shifts and scan noise drag the outgoing image into the incoming one
// Tags: glitch, datamosh, chromatic
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

const PI: f32 = 3.1415927f;

fn hash(n: f32) -> f32 {
    var n_1: f32;

    n_1 = n;
    let _e14 = n_1;
    return fract((sin(_e14) * 43758.547f));
}

fn hash_1(glt_p: vec2<f32>) -> f32 {
    var p_1: vec2<f32>;

    p_1 = glt_p;
    let _e14 = p_1;
    return fract((sin(dot(_e14, vec2<f32>(127.1f, 311.7f))) * 43758.547f));
}

fn sat(v: f32) -> f32 {
    var v_1: f32;

    v_1 = v;
    let _e14 = v_1;
    return clamp(_e14, 0f, 1f);
}

fn burst() -> f32 {
    let _e13 = u.progress;
    let _e19 = p.strength;
    return (pow(max(0f, sin((_e13 * PI))), 0.42f) * _e19);
}

fn safeUv(uv_4: vec2<f32>) -> vec2<f32> {
    var uv_5: vec2<f32>;

    uv_5 = uv_4;
    let _e14 = uv_5;
    return clamp(_e14, vec2(0f), vec2(1f));
}

fn stripeY(uv_6: vec2<f32>, density: f32, seed: f32, minWidth: f32, maxWidth: f32) -> f32 {
    var uv_7: vec2<f32>;
    var density_1: f32;
    var seed_1: f32;
    var minWidth_1: f32;
    var maxWidth_1: f32;
    var y: f32;
    var id: f32;
    var f: f32;
    var c: f32;
    var w: f32;

    uv_7 = uv_6;
    density_1 = density;
    seed_1 = seed;
    minWidth_1 = minWidth;
    maxWidth_1 = maxWidth;
    let _e22 = uv_7;
    let _e24 = density_1;
    let _e26 = seed_1;
    y = ((_e22.y * _e24) + (_e26 * 0.137f));
    let _e31 = y;
    id = floor(_e31);
    let _e34 = y;
    f = fract(_e34);
    let _e37 = id;
    let _e38 = seed_1;
    let _e40 = hash_1(vec2<f32>(_e37, _e38));
    c = _e40;
    let _e42 = minWidth_1;
    let _e43 = maxWidth_1;
    let _e44 = id;
    let _e47 = seed_1;
    let _e51 = hash_1(vec2<f32>((_e44 + 9.17f), (_e47 + 2.31f)));
    w = mix(_e42, _e43, _e51);
    let _e55 = w;
    let _e56 = w;
    let _e59 = f;
    let _e60 = c;
    return (1f - smoothstep(_e55, (_e56 + 0.018f), abs((_e59 - _e60))));
}

fn stripeX(uv_8: vec2<f32>, density_2: f32, seed_2: f32, minWidth_2: f32, maxWidth_2: f32) -> f32 {
    var uv_9: vec2<f32>;
    var density_3: f32;
    var seed_3: f32;
    var minWidth_3: f32;
    var maxWidth_3: f32;
    var x: f32;
    var id_1: f32;
    var f_1: f32;
    var c_1: f32;
    var w_1: f32;

    uv_9 = uv_8;
    density_3 = density_2;
    seed_3 = seed_2;
    minWidth_3 = minWidth_2;
    maxWidth_3 = maxWidth_2;
    let _e22 = uv_9;
    let _e24 = density_3;
    let _e26 = seed_3;
    x = ((_e22.x * _e24) + (_e26 * 0.091f));
    let _e31 = x;
    id_1 = floor(_e31);
    let _e34 = x;
    f_1 = fract(_e34);
    let _e37 = id_1;
    let _e38 = seed_3;
    let _e42 = hash_1(vec2<f32>(_e37, (_e38 + 41f)));
    c_1 = _e42;
    let _e44 = minWidth_3;
    let _e45 = maxWidth_3;
    let _e46 = id_1;
    let _e49 = seed_3;
    let _e53 = hash_1(vec2<f32>((_e46 + 4.7f), (_e49 + 8.9f)));
    w_1 = mix(_e44, _e45, _e53);
    let _e57 = w_1;
    let _e58 = w_1;
    let _e61 = f_1;
    let _e62 = c_1;
    return (1f - smoothstep(_e57, (_e58 + 0.012f), abs((_e61 - _e62))));
}

fn brokenGate(uv_10: vec2<f32>, row: f32, rnd: f32, frame: f32) -> f32 {
    var uv_11: vec2<f32>;
    var row_1: f32;
    var rnd_1: f32;
    var frame_1: f32;
    var segs: f32;
    var seg: f32;

    uv_11 = uv_10;
    row_1 = row;
    rnd_1 = rnd;
    frame_1 = frame;
    let _e22 = row_1;
    let _e23 = frame_1;
    let _e27 = hash_1(vec2<f32>(_e22, (_e23 + 44f)));
    segs = mix(1f, 9f, _e27);
    let _e30 = uv_11;
    let _e32 = segs;
    seg = floor((_e30.x * _e32));
    let _e37 = seg;
    let _e38 = row_1;
    let _e39 = frame_1;
    let _e43 = rnd_1;
    let _e46 = hash_1(vec2<f32>(_e37, ((_e38 + (_e39 * 3f)) + _e43)));
    return step(0.16f, _e46);
}

fn horizontalMask(uv_12: vec2<f32>, frame_2: f32) -> f32 {
    var uv_13: vec2<f32>;
    var frame_3: f32;
    var r1_: f32;
    var r2_: f32;
    var r3_: f32;
    var thick: f32;
    var mid: f32;
    var hair: f32;

    uv_13 = uv_12;
    frame_3 = frame_2;
    let _e16 = uv_13;
    let _e18 = frame_3;
    let _e19 = hash(_e18);
    let _e23 = p.horizontal_bars;
    r1_ = floor((((_e16.y + (_e19 * 0.031f)) * _e23) * 0.38f));
    let _e29 = uv_13;
    let _e31 = frame_3;
    let _e34 = hash((_e31 + 2f));
    let _e38 = p.horizontal_bars;
    r2_ = floor(((_e29.y + (_e34 * 0.013f)) * _e38));
    let _e42 = uv_13;
    let _e44 = frame_3;
    let _e47 = hash((_e44 + 7f));
    let _e51 = p.horizontal_bars;
    r3_ = floor((((_e42.y + (_e47 * 0.006f)) * _e51) * 3.4f));
    let _e57 = uv_13;
    let _e58 = p.horizontal_bars;
    let _e61 = frame_3;
    let _e66 = stripeY(_e57, (_e58 * 0.38f), (_e61 + 1f), 0.035f, 0.22f);
    thick = _e66;
    let _e68 = uv_13;
    let _e69 = p.horizontal_bars;
    let _e70 = frame_3;
    let _e75 = stripeY(_e68, _e69, (_e70 + 4f), 0.014f, 0.11f);
    mid = _e75;
    let _e77 = uv_13;
    let _e78 = p.horizontal_bars;
    let _e81 = frame_3;
    let _e86 = stripeY(_e77, (_e78 * 3.4f), (_e81 + 9f), 0.004f, 0.035f);
    hair = _e86;
    let _e88 = thick;
    let _e90 = r1_;
    let _e91 = frame_3;
    let _e95 = hash_1(vec2<f32>(_e90, (_e91 + 10f)));
    thick = (_e88 * step(0.42f, _e95));
    let _e98 = mid;
    let _e100 = r2_;
    let _e101 = frame_3;
    let _e105 = hash_1(vec2<f32>(_e100, (_e101 + 20f)));
    mid = (_e98 * step(0.48f, _e105));
    let _e108 = hair;
    let _e110 = r3_;
    let _e111 = frame_3;
    let _e115 = hash_1(vec2<f32>(_e110, (_e111 + 30f)));
    hair = (_e108 * step(0.62f, _e115));
    let _e118 = thick;
    let _e119 = uv_13;
    let _e120 = r1_;
    let _e121 = r1_;
    let _e122 = frame_3;
    let _e124 = hash_1(vec2<f32>(_e121, _e122));
    let _e125 = frame_3;
    let _e126 = brokenGate(_e119, _e120, _e124, _e125);
    thick = (_e118 * _e126);
    let _e128 = mid;
    let _e129 = uv_13;
    let _e130 = r2_;
    let _e131 = r2_;
    let _e132 = frame_3;
    let _e134 = hash_1(vec2<f32>(_e131, _e132));
    let _e135 = frame_3;
    let _e138 = brokenGate(_e129, _e130, _e134, (_e135 + 3f));
    mid = (_e128 * _e138);
    let _e140 = thick;
    let _e141 = mid;
    let _e142 = hair;
    let _e145 = sat(max(_e140, max(_e141, _e142)));
    return _e145;
}

fn verticalMask(uv_14: vec2<f32>, frame_4: f32) -> f32 {
    var uv_15: vec2<f32>;
    var frame_5: f32;
    var col: f32;
    var slit: f32;

    uv_15 = uv_14;
    frame_5 = frame_4;
    let _e16 = uv_15;
    let _e18 = frame_5;
    let _e21 = hash((_e18 + 12f));
    let _e25 = p.vertical_slits;
    col = floor(((_e16.x + (_e21 * 0.017f)) * _e25));
    let _e29 = uv_15;
    let _e30 = p.vertical_slits;
    let _e31 = frame_5;
    let _e36 = stripeX(_e29, _e30, (_e31 + 13f), 0.01f, 0.075f);
    slit = _e36;
    let _e38 = slit;
    let _e40 = col;
    let _e41 = frame_5;
    let _e45 = hash_1(vec2<f32>(_e40, (_e41 + 19f)));
    slit = (_e38 * step(0.66f, _e45));
    let _e48 = slit;
    let _e49 = sat(_e48);
    return _e49;
}

fn chromaFrom(uv_16: vec2<f32>, s: vec2<f32>) -> vec4<f32> {
    var uv_17: vec2<f32>;
    var s_1: vec2<f32>;

    uv_17 = uv_16;
    s_1 = s;
    let _e16 = uv_17;
    let _e17 = safeUv(_e16);
    uv_17 = _e17;
    let _e18 = uv_17;
    let _e19 = s_1;
    let _e21 = safeUv((_e18 + _e19));
    let _e22 = getFromColor(_e21);
    let _e24 = uv_17;
    let _e25 = getFromColor(_e24);
    let _e27 = uv_17;
    let _e28 = s_1;
    let _e30 = safeUv((_e27 - _e28));
    let _e31 = getFromColor(_e30);
    return vec4<f32>(_e22.x, _e25.y, _e31.z, 1f);
}

fn chromaTo(uv_18: vec2<f32>, s_2: vec2<f32>) -> vec4<f32> {
    var uv_19: vec2<f32>;
    var s_3: vec2<f32>;

    uv_19 = uv_18;
    s_3 = s_2;
    let _e16 = uv_19;
    let _e17 = safeUv(_e16);
    uv_19 = _e17;
    let _e18 = uv_19;
    let _e19 = s_3;
    let _e21 = safeUv((_e18 - _e19));
    let _e22 = getToColor(_e21);
    let _e24 = uv_19;
    let _e25 = getToColor(_e24);
    let _e27 = uv_19;
    let _e28 = s_3;
    let _e30 = safeUv((_e27 + _e28));
    let _e31 = getToColor(_e30);
    return vec4<f32>(_e22.x, _e25.y, _e31.z, 1f);
}

fn distortUv(uv_20: vec2<f32>, dir: f32, b: f32, h: f32, v_2: f32, frame_6: f32) -> vec2<f32> {
    var uv_21: vec2<f32>;
    var dir_1: f32;
    var b_1: f32;
    var h_1: f32;
    var v_3: f32;
    var frame_7: f32;
    var row_2: f32;
    var col_1: f32;
    var rowRnd: f32;
    var colRnd: f32;
    var xTear: f32;
    var yDrag: f32;
    var micro: f32;

    uv_21 = uv_20;
    dir_1 = dir;
    b_1 = b;
    h_1 = h;
    v_3 = v_2;
    frame_7 = frame_6;
    let _e24 = uv_21;
    let _e26 = p.horizontal_bars;
    row_2 = floor((_e24.y * _e26));
    let _e30 = uv_21;
    let _e32 = p.vertical_slits;
    col_1 = floor((_e30.x * _e32));
    let _e36 = row_2;
    let _e37 = frame_7;
    let _e39 = hash_1(vec2<f32>(_e36, _e37));
    rowRnd = _e39;
    let _e41 = col_1;
    let _e42 = frame_7;
    let _e46 = hash_1(vec2<f32>(_e41, (_e42 + 27f)));
    colRnd = _e46;
    let _e48 = rowRnd;
    let _e53 = p.tear;
    let _e55 = b_1;
    let _e57 = h_1;
    xTear = (((((_e48 - 0.5f) * 2f) * _e53) * _e55) * _e57);
    let _e60 = xTear;
    let _e61 = uv_21;
    let _e65 = u.progress;
    let _e72 = b_1;
    xTear = (_e60 + ((sin(((_e61.y * 120f) + (_e65 * 95f))) * 0.006f) * _e72));
    let _e75 = colRnd;
    let _e80 = b_1;
    let _e82 = v_3;
    yDrag = ((((_e75 - 0.5f) * 0.13f) * _e80) * _e82);
    let _e85 = row_2;
    let _e86 = col_1;
    let _e87 = frame_7;
    let _e90 = hash_1(vec2<f32>(_e85, (_e86 + _e87)));
    let _e95 = b_1;
    let _e97 = h_1;
    let _e98 = v_3;
    micro = ((((_e90 - 0.5f) * 0.018f) * _e95) * max(_e97, _e98));
    let _e102 = uv_21;
    let _e103 = xTear;
    let _e104 = dir_1;
    let _e106 = micro;
    let _e108 = yDrag;
    return (_e102 + vec2<f32>(((_e103 * _e104) + _e106), _e108));
}

fn glt_main(uv_22: vec2<f32>) -> vec4<f32> {
    var uv_23: vec2<f32>;
    var b_2: f32;
    var frame_8: f32;
    var h_2: f32;
    var v_4: f32;
    var glitch: f32;
    var row_3: f32;
    var rowRnd_1: f32;
    var bandDelay: f32;
    var reveal: f32;
    var split: vec2<f32>;
    var fromUv: vec2<f32>;
    var toUv: vec2<f32>;
    var color: vec4<f32>;
    var smearUv: vec2<f32>;
    var sliceReveal: f32;
    var sliceColor: vec4<f32>;
    var hairLine: f32;
    var scan: f32;
    var nCell: vec2<f32>;
    var n_2: f32;
    var luma: f32;
    var strobe: f32;

    uv_23 = uv_22;
    let _e14 = u.progress;
    if (_e14 <= 0f) {
        let _e17 = uv_23;
        let _e18 = getFromColor(_e17);
        return _e18;
    }
    let _e19 = u.progress;
    if (_e19 >= 1f) {
        let _e22 = uv_23;
        let _e23 = getToColor(_e22);
        return _e23;
    }
    let _e24 = burst();
    b_2 = _e24;
    let _e26 = u.progress;
    frame_8 = floor((_e26 * 30f));
    let _e31 = uv_23;
    let _e32 = frame_8;
    let _e33 = horizontalMask(_e31, _e32);
    h_2 = _e33;
    let _e35 = uv_23;
    let _e36 = frame_8;
    let _e37 = verticalMask(_e35, _e36);
    v_4 = _e37;
    let _e39 = h_2;
    let _e40 = v_4;
    let _e44 = sat(max(_e39, (_e40 * 0.75f)));
    glitch = _e44;
    let _e46 = uv_23;
    let _e48 = p.horizontal_bars;
    row_3 = floor((_e46.y * _e48));
    let _e52 = row_3;
    let _e53 = frame_8;
    let _e57 = hash_1(vec2<f32>(_e52, (_e53 + 5f)));
    rowRnd_1 = _e57;
    let _e59 = rowRnd_1;
    let _e64 = h_2;
    bandDelay = (((_e59 - 0.5f) * 0.3f) * _e64);
    let _e69 = u.progress;
    let _e70 = bandDelay;
    reveal = smoothstep(0.18f, 0.84f, (_e69 + _e70));
    let _e74 = p.chroma;
    let _e75 = b_2;
    let _e79 = glitch;
    let _e83 = p.chroma;
    let _e86 = b_2;
    let _e88 = v_4;
    split = vec2<f32>(((_e74 * _e75) * (1f + (1.7f * _e79))), (((_e83 * 0.22f) * _e86) * _e88));
    let _e92 = uv_23;
    let _e94 = b_2;
    let _e95 = h_2;
    let _e96 = v_4;
    let _e97 = frame_8;
    let _e98 = distortUv(_e92, 1f, _e94, _e95, _e96, _e97);
    fromUv = _e98;
    let _e100 = uv_23;
    let _e103 = b_2;
    let _e104 = h_2;
    let _e105 = v_4;
    let _e106 = frame_8;
    let _e107 = distortUv(_e100, -1f, _e103, _e104, _e105, _e106);
    toUv = _e107;
    let _e109 = fromUv;
    let _e110 = split;
    let _e111 = chromaFrom(_e109, _e110);
    let _e112 = toUv;
    let _e113 = split;
    let _e114 = chromaTo(_e112, _e113);
    let _e115 = reveal;
    color = mix(_e111, _e114, vec4(_e115));
    let _e119 = uv_23;
    smearUv = _e119;
    let _e122 = smearUv;
    let _e124 = rowRnd_1;
    let _e129 = b_2;
    let _e131 = h_2;
    smearUv.x = (_e122.x + ((((_e124 - 0.5f) * 0.46f) * _e129) * _e131));
    let _e135 = smearUv;
    let _e137 = row_3;
    let _e138 = frame_8;
    let _e142 = hash_1(vec2<f32>(_e137, (_e138 + 31f)));
    let _e147 = b_2;
    let _e149 = h_2;
    smearUv.y = (_e135.y + ((((_e142 - 0.5f) * 0.045f) * _e147) * _e149));
    let _e154 = u.progress;
    let _e155 = rowRnd_1;
    sliceReveal = smoothstep(0.28f, 0.78f, (_e154 + ((_e155 - 0.5f) * 0.22f)));
    let _e163 = smearUv;
    let _e164 = split;
    let _e167 = chromaFrom(_e163, (_e164 * 1.65f));
    let _e168 = smearUv;
    let _e169 = rowRnd_1;
    let _e174 = b_2;
    let _e179 = split;
    let _e182 = chromaTo((_e168 - vec2<f32>((((_e169 - 0.5f) * 0.18f) * _e174), 0f)), (_e179 * 1.65f));
    let _e183 = sliceReveal;
    sliceColor = mix(_e167, _e182, vec4(_e183));
    let _e187 = color;
    let _e188 = sliceColor;
    let _e189 = h_2;
    let _e190 = b_2;
    let _e192 = p.residue;
    color = mix(_e187, _e188, vec4(((_e189 * _e190) * _e192)));
    let _e196 = uv_23;
    let _e198 = frame_8;
    let _e203 = stripeY(_e196, 190f, (_e198 + 55f), 0.002f, 0.012f);
    hairLine = _e203;
    let _e205 = hairLine;
    let _e207 = uv_23;
    let _e212 = frame_8;
    let _e216 = hash_1(vec2<f32>(floor((_e207.y * 190f)), (_e212 + 56f)));
    hairLine = (_e205 * step(0.7f, _e216));
    let _e219 = color;
    let _e221 = color;
    let _e227 = hairLine;
    let _e229 = b_2;
    let _e233 = (_e221.xyz + (((vec3<f32>(0.72f, 0.9f, 1f) * _e227) * _e229) * 0.28f));
    color.x = _e233.x;
    color.y = _e233.y;
    color.z = _e233.z;
    let _e242 = uv_23;
    let _e246 = u.progress;
    scan = (0.5f + (0.5f * sin(((_e242.y * 980f) + (_e246 * 130f)))));
    let _e254 = color;
    let _e256 = color;
    let _e259 = p.scan_amount;
    let _e260 = b_2;
    let _e262 = scan;
    let _e265 = (_e256.xyz * (1f - ((_e259 * _e260) * _e262)));
    color.x = _e265.x;
    color.y = _e265.y;
    color.z = _e265.z;
    let _e272 = uv_23;
    let _e274 = (u.resolution.x / u.resolution.y);
    nCell = floor((_e272 * vec2<f32>((360f * _e274), 210f)));
    let _e281 = nCell;
    let _e282 = frame_8;
    let _e285 = frame_8;
    let _e290 = hash_1((_e281 + vec2<f32>((_e282 * 7f), (_e285 * 13f))));
    n_2 = _e290;
    let _e292 = color;
    let _e294 = color;
    let _e296 = n_2;
    let _e299 = p.noise_amount;
    let _e301 = b_2;
    let _e304 = glitch;
    let _e308 = (_e294.xyz + vec3(((((_e296 - 0.5f) * _e299) * _e301) * (0.55f + _e304))));
    color.x = _e308.x;
    color.y = _e308.y;
    color.z = _e308.z;
    let _e315 = color;
    luma = dot(_e315.xyz, vec3<f32>(0.299f, 0.587f, 0.114f));
    let _e323 = color;
    let _e325 = color;
    let _e327 = luma;
    let _e330 = b_2;
    let _e332 = glitch;
    let _e335 = mix(_e325.xyz, vec3(_e327), vec3(((0.18f * _e330) * _e332)));
    color.x = _e335.x;
    color.y = _e335.y;
    color.z = _e335.z;
    let _e343 = frame_8;
    let _e346 = hash_1(vec2<f32>(_e343, 3.14f));
    let _e348 = b_2;
    strobe = (step(0.78f, _e346) * pow(_e348, 1.65f));
    let _e353 = color;
    let _e355 = color;
    let _e357 = strobe;
    let _e358 = p.flash_amount;
    let _e361 = (_e355.xyz + vec3((_e357 * _e358)));
    color.x = _e361.x;
    color.y = _e361.y;
    color.z = _e361.z;
    let _e368 = color;
    let _e374 = clamp(_e368.xyz, vec3(0f), vec3(1f));
    return vec4<f32>(_e374.x, _e374.y, _e374.z, 1f);
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
