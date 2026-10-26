// Author: numb3r23
// License: MIT
// Description: The incoming image is revealed tile by tile in a diagonal wave
// Tags: tiles, wave, wipe
// Ported from https://gist.github.com/numb3r23/169781bb76f310e2bfde
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

fn glt_main(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var tileSize: vec2<f32>;
    var posInTile: vec2<f32>;
    var tileNum: vec2<f32>;
    var countTiles: f32;
    var offset: f32;
    var timeOffset: f32;
    var sinTime: f32;
    var texC: vec2<f32>;
    var local: f32;
    var local_1: f32;
    var globalUV: vec2<f32>;
    var local_2: f32;
    var local_3: f32;
    var globalUV_1: vec2<f32>;

    uv_5 = uv_4;
    let _e8 = vec2<i32>(p.tile_count);
    tileSize = (vec2(1f) / vec2<f32>(_e8));
    let _e13 = uv_5;
    let _e14 = vec2<i32>(p.tile_count);
    posInTile = fract((_e13 * vec2<f32>(_e14)));
    let _e19 = uv_5;
    let _e20 = vec2<i32>(p.tile_count);
    tileNum = floor((_e19 * vec2<f32>(_e20)));
    let _e25 = vec2<i32>(p.tile_count);
    let _e27 = vec2<i32>(p.tile_count);
    countTiles = f32((_e25.x * _e27.y));
    let _e32 = tileNum;
    let _e34 = tileNum;
    let _e36 = vec2<i32>(p.tile_count);
    let _e41 = countTiles;
    offset = ((_e32.y + (_e34.x * f32(_e36.y))) / _e41);
    let _e44 = u.progress;
    let _e45 = offset;
    let _e47 = countTiles;
    timeOffset = clamp(((_e44 - _e45) * _e47), 0f, 0.5f);
    let _e54 = timeOffset;
    sinTime = (1f - abs(cos((fract(_e54) * 3.1415925f))));
    let _e62 = posInTile;
    texC = _e62;
    let _e64 = sinTime;
    if (_e64 <= 0.5f) {
        {
            let _e67 = (p.flip_x > 0.5);
            if _e67 {
                {
                    let _e68 = texC;
                    let _e70 = sinTime;
                    let _e72 = texC;
                    let _e75 = sinTime;
                    if ((_e68.x < _e70) || (_e72.x > (1f - _e75))) {
                        let _e79 = uv_5;
                        let _e80 = getFromColor(_e79);
                        return _e80;
                    }
                    let _e82 = texC;
                    if (_e82.x < 0.5f) {
                        let _e86 = texC;
                        let _e88 = sinTime;
                        let _e93 = sinTime;
                        local = (((_e86.x - _e88) * 0.5f) / (0.5f - _e93));
                    } else {
                        let _e96 = texC;
                        let _e103 = sinTime;
                        local = ((((_e96.x - 0.5f) * 0.5f) / (0.5f - _e103)) + 0.5f);
                    }
                    let _e109 = local;
                    texC.x = _e109;
                }
            }
            let _e110 = (p.flip_y > 0.5);
            if _e110 {
                {
                    let _e111 = texC;
                    let _e113 = sinTime;
                    let _e115 = texC;
                    let _e118 = sinTime;
                    if ((_e111.y < _e113) || (_e115.y > (1f - _e118))) {
                        let _e122 = uv_5;
                        let _e123 = getFromColor(_e122);
                        return _e123;
                    }
                    let _e125 = texC;
                    if (_e125.y < 0.5f) {
                        let _e129 = texC;
                        let _e131 = sinTime;
                        let _e136 = sinTime;
                        local_1 = (((_e129.y - _e131) * 0.5f) / (0.5f - _e136));
                    } else {
                        let _e139 = texC;
                        let _e146 = sinTime;
                        local_1 = ((((_e139.y - 0.5f) * 0.5f) / (0.5f - _e146)) + 0.5f);
                    }
                    let _e152 = local_1;
                    texC.y = _e152;
                }
            }
            let _e153 = tileNum;
            let _e154 = tileSize;
            let _e156 = texC;
            let _e157 = tileSize;
            globalUV = ((_e153 * _e154) + (_e156 * _e157));
            let _e161 = globalUV;
            let _e162 = getFromColor(_e161);
            return _e162;
        }
    } else {
        {
            let _e163 = (p.flip_x > 0.5);
            if _e163 {
                {
                    let _e164 = texC;
                    let _e166 = sinTime;
                    let _e168 = texC;
                    let _e171 = sinTime;
                    if ((_e164.x > _e166) || (_e168.x < (1f - _e171))) {
                        let _e175 = uv_5;
                        let _e176 = getToColor(_e175);
                        return _e176;
                    }
                    let _e178 = texC;
                    if (_e178.x < 0.5f) {
                        let _e182 = texC;
                        let _e184 = sinTime;
                        let _e189 = sinTime;
                        local_2 = (((_e182.x - _e184) * 0.5f) / (0.5f - _e189));
                    } else {
                        let _e192 = texC;
                        let _e199 = sinTime;
                        local_2 = ((((_e192.x - 0.5f) * 0.5f) / (0.5f - _e199)) + 0.5f);
                    }
                    let _e205 = local_2;
                    texC.x = _e205;
                    let _e208 = texC;
                    texC.x = (1f - _e208.x);
                }
            }
            let _e211 = (p.flip_y > 0.5);
            if _e211 {
                {
                    let _e212 = texC;
                    let _e214 = sinTime;
                    let _e216 = texC;
                    let _e219 = sinTime;
                    if ((_e212.y > _e214) || (_e216.y < (1f - _e219))) {
                        let _e223 = uv_5;
                        let _e224 = getToColor(_e223);
                        return _e224;
                    }
                    let _e226 = texC;
                    if (_e226.y < 0.5f) {
                        let _e230 = texC;
                        let _e232 = sinTime;
                        let _e237 = sinTime;
                        local_3 = (((_e230.y - _e232) * 0.5f) / (0.5f - _e237));
                    } else {
                        let _e240 = texC;
                        let _e247 = sinTime;
                        local_3 = ((((_e240.y - 0.5f) * 0.5f) / (0.5f - _e247)) + 0.5f);
                    }
                    let _e253 = local_3;
                    texC.y = _e253;
                    let _e256 = texC;
                    texC.y = (1f - _e256.y);
                }
            }
            let _e259 = tileNum;
            let _e260 = tileSize;
            let _e262 = texC;
            let _e263 = tileSize;
            globalUV_1 = ((_e259 * _e260) + (_e262 * _e263));
            let _e267 = globalUV_1;
            let _e268 = getToColor(_e267);
            return _e268;
        }
    }
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
