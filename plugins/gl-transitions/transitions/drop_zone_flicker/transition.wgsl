// Author: bread
// License: MIT
// Description: A flickering glitch with RGB offset, ghosting and scanlines that flashes back to the outgoing image
// Tags: glitch, flicker, chromatic, scanlines
// Drop_Zone_Flicker.glsl
// gl-transitions compatible: progress, ratio, getFromColor, getToColor
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

fn sat(x: f32) -> f32 {
    var x_1: f32;

    x_1 = x;
    let _e10 = x_1;
    return clamp(_e10, 0f, 1f);
}

fn hash12_(glt_p: vec2<f32>) -> f32 {
    var p_1: vec2<f32>;

    p_1 = glt_p;
    let _e10 = p_1;
    p_1 = fract((_e10 * vec2<f32>(443.8975f, 397.2973f)));
    let _e16 = p_1;
    let _e17 = p_1;
    let _e18 = p_1;
    p_1 = (_e16 + vec2(dot(_e17, (_e18.yx + vec2(19.19f)))));
    let _e26 = p_1;
    let _e28 = p_1;
    return fract((_e26.x * _e28.y));
}

fn safeUv(uv_4: vec2<f32>) -> vec2<f32> {
    var uv_5: vec2<f32>;

    uv_5 = uv_4;
    let _e10 = uv_5;
    return clamp(_e10, vec2(0.001f), vec2(0.999f));
}

fn frameReveal(f: f32) -> f32 {
    var f_1: f32;

    f_1 = f;
    let _e10 = f_1;
    if (_e10 < 0.5f) {
        return 0f;
    }
    let _e14 = f_1;
    if (_e14 < 1.5f) {
        return 0.28f;
    }
    let _e18 = f_1;
    if (_e18 < 2.5f) {
        return 0.43f;
    }
    let _e22 = f_1;
    if (_e22 < 3.5f) {
        return 0.46f;
    }
    let _e26 = f_1;
    if (_e26 < 4.5f) {
        return 0.38f;
    }
    let _e30 = f_1;
    if (_e30 < 5.5f) {
        return 0.48f;
    }
    let _e34 = f_1;
    if (_e34 < 6.5f) {
        return 0.26f;
    }
    let _e38 = f_1;
    if (_e38 < 7.5f) {
        return 0.1f;
    }
    let _e42 = f_1;
    if (_e42 < 8.5f) {
        return 0f;
    }
    let _e46 = f_1;
    if (_e46 < 9.5f) {
        return 0f;
    }
    let _e50 = f_1;
    if (_e50 < 10.5f) {
        return 0.3f;
    }
    let _e54 = f_1;
    if (_e54 < 11.5f) {
        return 0.58f;
    }
    let _e58 = f_1;
    if (_e58 < 12.5f) {
        return 1f;
    }
    let _e62 = f_1;
    if (_e62 < 13.5f) {
        return 0.7f;
    }
    let _e66 = f_1;
    if (_e66 < 14.5f) {
        return 0.42f;
    }
    let _e70 = f_1;
    if (_e70 < 15.5f) {
        return 0.55f;
    }
    let _e74 = f_1;
    if (_e74 < 16.5f) {
        return 0.72f;
    }
    let _e78 = f_1;
    if (_e78 < 17.5f) {
        return 0.88f;
    }
    let _e82 = f_1;
    if (_e82 < 18.5f) {
        return 0.34f;
    }
    let _e86 = f_1;
    if (_e86 < 19.5f) {
        return 0.48f;
    }
    let _e90 = f_1;
    if (_e90 < 20.5f) {
        return 0.56f;
    }
    let _e94 = f_1;
    if (_e94 < 21.5f) {
        return 0.76f;
    }
    let _e98 = f_1;
    if (_e98 < 22.5f) {
        return 0.93f;
    }
    return 1f;
}

fn frameGlitch(f_2: f32) -> f32 {
    var f_3: f32;

    f_3 = f_2;
    let _e10 = f_3;
    if (_e10 < 0.5f) {
        return 0f;
    }
    let _e14 = f_3;
    if (_e14 < 6.5f) {
        return 0.92f;
    }
    let _e18 = f_3;
    if (_e18 < 10.5f) {
        return 0.38f;
    }
    let _e22 = f_3;
    if (_e22 < 11.5f) {
        return 0.78f;
    }
    let _e26 = f_3;
    if (_e26 < 12.5f) {
        return 0.12f;
    }
    let _e30 = f_3;
    if (_e30 < 17.5f) {
        return 0.74f;
    }
    let _e34 = f_3;
    if (_e34 < 22.5f) {
        return 0.88f;
    }
    let _e38 = f_3;
    if (_e38 < 23.5f) {
        return 0.28f;
    }
    return 0f;
}

fn chromaFrom(uv_6: vec2<f32>, amt: f32) -> vec4<f32> {
    var uv_7: vec2<f32>;
    var amt_1: f32;
    var o: vec2<f32>;

    uv_7 = uv_6;
    amt_1 = amt;
    let _e12 = amt_1;
    o = vec2<f32>(_e12, 0f);
    let _e16 = uv_7;
    let _e17 = o;
    let _e19 = safeUv((_e16 + _e17));
    let _e20 = getFromColor(_e19);
    let _e22 = uv_7;
    let _e23 = safeUv(_e22);
    let _e24 = getFromColor(_e23);
    let _e26 = uv_7;
    let _e27 = o;
    let _e29 = safeUv((_e26 - _e27));
    let _e30 = getFromColor(_e29);
    return vec4<f32>(_e20.x, _e24.y, _e30.z, 1f);
}

fn chromaTo(uv_8: vec2<f32>, amt_2: f32) -> vec4<f32> {
    var uv_9: vec2<f32>;
    var amt_3: f32;
    var o_1: vec2<f32>;

    uv_9 = uv_8;
    amt_3 = amt_2;
    let _e12 = amt_3;
    o_1 = vec2<f32>(_e12, 0f);
    let _e16 = uv_9;
    let _e17 = o_1;
    let _e19 = safeUv((_e16 - _e17));
    let _e20 = getToColor(_e19);
    let _e22 = uv_9;
    let _e23 = safeUv(_e22);
    let _e24 = getToColor(_e23);
    let _e26 = uv_9;
    let _e27 = o_1;
    let _e29 = safeUv((_e26 + _e27));
    let _e30 = getToColor(_e29);
    return vec4<f32>(_e20.x, _e24.y, _e30.z, 1f);
}

fn blockMask(uv_10: vec2<f32>, f_4: f32) -> f32 {
    var uv_11: vec2<f32>;
    var f_5: f32;
    var big: vec2<f32>;
    var small: vec2<f32>;
    var wide: f32;
    var chunks: f32;
    var verticalCut: f32;

    uv_11 = uv_10;
    f_5 = f_4;
    let _e12 = uv_11;
    big = floor((_e12 * vec2<f32>(4f, 2f)));
    let _e19 = uv_11;
    small = floor((_e19 * vec2<f32>(8f, 4f)));
    let _e27 = big;
    let _e29 = f_5;
    let _e33 = hash12_(vec2<f32>(_e27.x, (_e29 * 1.37f)));
    wide = step(0.48f, _e33);
    let _e37 = small;
    let _e38 = f_5;
    let _e41 = f_5;
    let _e46 = hash12_((_e37 + vec2<f32>((_e38 * 2.11f), (_e41 * 0.73f))));
    chunks = step(0.56f, _e46);
    let _e52 = uv_11;
    let _e56 = f_5;
    let _e59 = hash12_(vec2<f32>(_e56, 4.7f));
    verticalCut = smoothstep(-0.035f, 0.035f, (_e52.x - mix(0.18f, 0.78f, _e59)));
    let _e64 = wide;
    let _e65 = chunks;
    let _e70 = verticalCut;
    let _e74 = sat(((mix(_e64, _e65, 0.38f) * 0.72f) + (_e70 * 0.28f)));
    return _e74;
}

fn glt_main(uv_12: vec2<f32>) -> vec4<f32> {
    var uv_13: vec2<f32>;
    var f_6: f32;
    var reveal: f32;
    var glitch: f32;
    var rnd: f32;
    var jitter: vec2<f32>;
    var fromUv: vec2<f32>;
    var toUv: vec2<f32>;
    var block: f32;
    var localReveal: f32;
    var oldClip: vec4<f32>;
    var newClip: vec4<f32>;
    var color: vec4<f32>;
    var leftWash: f32;
    var cyanWash: f32;
    var redGhost: vec3<f32>;
    var cyanGhost: vec3<f32>;
    var oldReturn: f32;
    var lines: f32;
    var exposurePulse: f32;

    uv_13 = uv_12;
    let _e10 = u.progress;
    if (_e10 <= 0f) {
        let _e13 = uv_13;
        let _e14 = getFromColor(_e13);
        return _e14;
    }
    let _e15 = u.progress;
    if (_e15 >= 1f) {
        let _e18 = uv_13;
        let _e19 = getToColor(_e18);
        return _e19;
    }
    let _e20 = u.progress;
    let _e21 = p.frame_rate;
    f_6 = floor((_e20 * _e21));
    let _e25 = f_6;
    let _e26 = frameReveal(_e25);
    reveal = _e26;
    let _e28 = f_6;
    let _e29 = frameGlitch(_e28);
    glitch = _e29;
    let _e31 = f_6;
    let _e34 = hash12_(vec2<f32>(_e31, 9.13f));
    rnd = _e34;
    let _e36 = rnd;
    let _e41 = f_6;
    let _e44 = hash12_(vec2<f32>(_e41, 2.71f));
    let _e50 = glitch;
    jitter = (vec2<f32>(((_e36 - 0.5f) * 0.042f), ((_e44 - 0.5f) * 0.01f)) * _e50);
    let _e53 = uv_13;
    let _e54 = jitter;
    let _e56 = safeUv((_e53 + _e54));
    fromUv = _e56;
    let _e58 = uv_13;
    let _e59 = jitter;
    let _e63 = safeUv((_e58 - (_e59 * 0.55f)));
    toUv = _e63;
    let _e65 = uv_13;
    let _e66 = f_6;
    let _e67 = blockMask(_e65, _e66);
    block = _e67;
    let _e69 = reveal;
    let _e70 = block;
    let _e73 = p.block_amount;
    let _e75 = glitch;
    let _e78 = f_6;
    let _e79 = uv_13;
    let _e85 = hash12_(vec2<f32>(_e78, floor((_e79.y * 9f))));
    let _e90 = glitch;
    let _e93 = sat(((_e69 + (((_e70 - 0.5f) * _e73) * _e75)) + (((_e85 - 0.5f) * 0.18f) * _e90)));
    localReveal = _e93;
    let _e97 = localReveal;
    localReveal = smoothstep(0.22f, 0.78f, _e97);
    let _e99 = fromUv;
    let _e100 = p.rgb_offset;
    let _e101 = glitch;
    let _e103 = chromaFrom(_e99, (_e100 * _e101));
    oldClip = _e103;
    let _e105 = toUv;
    let _e106 = p.rgb_offset;
    let _e107 = glitch;
    let _e109 = chromaTo(_e105, (_e106 * _e107));
    newClip = _e109;
    let _e111 = oldClip;
    let _e112 = newClip;
    let _e113 = localReveal;
    color = mix(_e111, _e112, vec4(_e113));
    let _e120 = uv_13;
    let _e124 = glitch;
    leftWash = ((1f - smoothstep(0.1f, 0.78f, _e120.x)) * _e124);
    let _e129 = uv_13;
    let _e135 = uv_13;
    let _e140 = glitch;
    cyanWash = ((smoothstep(0.08f, 0.62f, _e129.x) * (1f - smoothstep(0.86f, 1f, _e135.x))) * _e140);
    let _e143 = oldClip;
    redGhost = (_e143.xyz * vec3<f32>(1.34f, 0.42f, 0.38f));
    let _e151 = newClip;
    cyanGhost = (_e151.xyz * vec3<f32>(0.48f, 1.12f, 1.24f));
    let _e159 = color;
    let _e161 = color;
    let _e163 = redGhost;
    let _e164 = leftWash;
    let _e165 = p.red_cyan;
    let _e170 = mix(_e161.xyz, _e163, vec3(((_e164 * _e165) * 0.42f)));
    color.x = _e170.x;
    color.y = _e170.y;
    color.z = _e170.z;
    let _e177 = color;
    let _e179 = color;
    let _e181 = cyanGhost;
    let _e182 = cyanWash;
    let _e183 = p.red_cyan;
    let _e188 = mix(_e179.xyz, _e181, vec3(((_e182 * _e183) * 0.3f)));
    color.x = _e188.x;
    color.y = _e188.y;
    color.z = _e188.z;
    let _e195 = glitch;
    let _e199 = reveal;
    oldReturn = (_e195 * (1f - smoothstep(0.94f, 1f, _e199)));
    let _e204 = oldReturn;
    let _e208 = localReveal;
    oldReturn = (_e204 * (0.18f + (0.52f * (1f - (abs((_e208 - 0.5f)) * 2f)))));
    let _e218 = color;
    let _e220 = color;
    let _e222 = oldClip;
    let _e224 = oldReturn;
    let _e225 = p.ghost_amount;
    let _e228 = mix(_e220.xyz, _e222.xyz, vec3((_e224 * _e225)));
    color.x = _e228.x;
    color.y = _e228.y;
    color.z = _e228.z;
    let _e235 = uv_13;
    let _e237 = f_6;
    lines = sin(((_e235.y + (_e237 * 0.017f)) * 1080f));
    let _e245 = color;
    let _e247 = color;
    let _e249 = lines;
    let _e250 = p.scanline;
    let _e252 = glitch;
    let _e255 = (_e247.xyz + vec3(((_e249 * _e250) * _e252)));
    color.x = _e255.x;
    color.y = _e255.y;
    color.z = _e255.z;
    let _e262 = f_6;
    let _e265 = hash12_(vec2<f32>(_e262, 12.4f));
    let _e270 = glitch;
    exposurePulse = (((_e265 - 0.35f) * 0.1f) * _e270);
    let _e273 = color;
    let _e275 = color;
    let _e277 = exposurePulse;
    let _e279 = (_e275.xyz + vec3(_e277));
    color.x = _e279.x;
    color.y = _e279.y;
    color.z = _e279.z;
    let _e286 = color;
    let _e288 = sat(_e286.x);
    let _e289 = color;
    let _e291 = sat(_e289.y);
    let _e292 = color;
    let _e294 = sat(_e292.z);
    return vec4<f32>(_e288, _e291, _e294, 1f);
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
