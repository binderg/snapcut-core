// Author: Hewlett-Packard
// License: BSD 3 Clause
// Description: The outgoing image curls away like a page, revealing the incoming one
// Tags: page, curl, 3d
// Adapted by Sergey Kosarevsky from:
// http://rectalogic.github.io/webvfx/examples_2transition-shader-pagecurl_8html-example.html
//
//
// Copyright (c) 2010 Hewlett-Packard Development Company, L.P. All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are
// met:
//
// * Redistributions of source code must retain the above copyright
// notice, this list of conditions and the following disclaimer.
// * Redistributions in binary form must reproduce the above
// copyright notice, this list of conditions and the following disclaimer
// in the documentation and/or other materials provided with the
// distribution.
// * Neither the name of Hewlett-Packard nor the names of its
// contributors may be used to endorse or promote products derived from
// this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
// "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
// LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
// A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
// OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
// LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
// in vec2 texCoord;
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

const MIN_AMOUNT: f32 = -0.16f;

const MAX_AMOUNT: f32 = 1.5f;

const PI: f32 = 3.1415927f;

const scale: f32 = 512f;

const sharpness: f32 = 3f;

const cylinderRadius: f32 = 0.15915494f;

var<private> amount: f32;

var<private> cylinderCenter: f32;

var<private> cylinderAngle: f32;

fn hitPoint(hitAngle: f32, yc: f32, point: vec3<f32>, rrotation: mat3x3<f32>) -> vec3<f32> {
    var hitAngle_1: f32;
    var yc_1: f32;
    var point_1: vec3<f32>;
    var rrotation_1: mat3x3<f32>;
    var hitPoint_1: f32;

    hitAngle_1 = hitAngle;
    yc_1 = yc;
    point_1 = point;
    rrotation_1 = rrotation;
    let _e19 = hitAngle_1;
    hitPoint_1 = (_e19 / 6.2831855f);
    let _e26 = hitPoint_1;
    point_1.y = _e26;
    let _e27 = rrotation_1;
    let _e28 = point_1;
    return (_e27 * _e28);
}

fn antiAlias(color1_: vec4<f32>, color2_: vec4<f32>, distanc: f32) -> vec4<f32> {
    var color1_1: vec4<f32>;
    var color2_1: vec4<f32>;
    var distanc_1: f32;
    var dd: f32;

    color1_1 = color1_;
    color2_1 = color2_;
    distanc_1 = distanc;
    let _e17 = distanc_1;
    distanc_1 = (_e17 * scale);
    let _e19 = distanc_1;
    if (_e19 < 0f) {
        let _e22 = color2_1;
        return _e22;
    }
    let _e23 = distanc_1;
    if (_e23 > 2f) {
        let _e26 = color1_1;
        return _e26;
    }
    let _e28 = distanc_1;
    dd = pow((1f - (_e28 / 2f)), sharpness);
    let _e34 = color2_1;
    let _e35 = color1_1;
    let _e37 = dd;
    let _e39 = color1_1;
    return (((_e34 - _e35) * _e37) + _e39);
}

fn distanceToEdge(point_2: vec3<f32>) -> f32 {
    var point_3: vec3<f32>;
    var local: f32;
    var dx: f32;
    var local_1: f32;
    var dy: f32;

    point_3 = point_2;
    let _e13 = point_3;
    if (_e13.x > 0.5f) {
        let _e18 = point_3;
        local = (1f - _e18.x);
    } else {
        let _e21 = point_3;
        local = _e21.x;
    }
    let _e24 = local;
    dx = abs(_e24);
    let _e27 = point_3;
    if (_e27.y > 0.5f) {
        let _e32 = point_3;
        local_1 = (1f - _e32.y);
    } else {
        let _e35 = point_3;
        local_1 = _e35.y;
    }
    let _e38 = local_1;
    dy = abs(_e38);
    let _e41 = point_3;
    if (_e41.x < 0f) {
        let _e45 = point_3;
        dx = -(_e45.x);
    }
    let _e48 = point_3;
    if (_e48.x > 1f) {
        let _e52 = point_3;
        dx = (_e52.x - 1f);
    }
    let _e56 = point_3;
    if (_e56.y < 0f) {
        let _e60 = point_3;
        dy = -(_e60.y);
    }
    let _e63 = point_3;
    if (_e63.y > 1f) {
        let _e67 = point_3;
        dy = (_e67.y - 1f);
    }
    let _e71 = point_3;
    let _e75 = point_3;
    let _e80 = point_3;
    let _e84 = point_3;
    if (((_e71.x < 0f) || (_e75.x > 1f)) && ((_e80.y < 0f) || (_e84.y > 1f))) {
        let _e90 = dx;
        let _e91 = dx;
        let _e93 = dy;
        let _e94 = dy;
        return sqrt(((_e90 * _e91) + (_e93 * _e94)));
    }
    let _e98 = dx;
    let _e99 = dy;
    return min(_e98, _e99);
}

fn seeThrough(yc_2: f32, glt_p: vec2<f32>, rotation: mat3x3<f32>, rrotation_2: mat3x3<f32>) -> vec4<f32> {
    var yc_3: f32;
    var p_1: vec2<f32>;
    var rotation_1: mat3x3<f32>;
    var rrotation_3: mat3x3<f32>;
    var hitAngle_2: f32;
    var point_4: vec3<f32>;
    var color: vec4<f32>;
    var tcolor: vec4<f32> = vec4(0f);

    yc_3 = yc_2;
    p_1 = glt_p;
    rotation_1 = rotation;
    rrotation_3 = rrotation_2;
    let _e19 = yc_3;
    let _e26 = cylinderAngle;
    hitAngle_2 = (PI - (acos(clamp((_e19 / cylinderRadius), -1f, 1f)) - _e26));
    let _e30 = hitAngle_2;
    let _e31 = yc_3;
    let _e32 = rotation_1;
    let _e33 = p_1;
    let _e39 = rrotation_3;
    let _e40 = hitPoint(_e30, _e31, (_e32 * vec3<f32>(_e33.x, _e33.y, 1f)), _e39);
    point_4 = _e40;
    let _e42 = yc_3;
    let _e45 = point_4;
    let _e49 = point_4;
    let _e54 = point_4;
    let _e59 = point_4;
    if ((_e42 <= 0f) && ((((_e45.x < 0f) || (_e49.y < 0f)) || (_e54.x > 1f)) || (_e59.y > 1f))) {
        {
            let _e65 = p_1;
            let _e66 = getToColor(_e65);
            return _e66;
        }
    }
    let _e67 = yc_3;
    if (_e67 > 0f) {
        let _e70 = p_1;
        let _e71 = getFromColor(_e70);
        return _e71;
    }
    let _e72 = point_4;
    let _e74 = getFromColor(_e72.xy);
    color = _e74;
    let _e79 = color;
    let _e80 = tcolor;
    let _e81 = point_4;
    let _e82 = distanceToEdge(_e81);
    let _e83 = antiAlias(_e79, _e80, _e82);
    return _e83;
}

fn seeThroughWithShadow(yc_4: f32, p_2: vec2<f32>, point_5: vec3<f32>, rotation_2: mat3x3<f32>, rrotation_4: mat3x3<f32>) -> vec4<f32> {
    var yc_5: f32;
    var p_3: vec2<f32>;
    var point_6: vec3<f32>;
    var rotation_3: mat3x3<f32>;
    var rrotation_5: mat3x3<f32>;
    var shadow: f32;
    var shadowColor: vec4<f32>;

    yc_5 = yc_4;
    p_3 = p_2;
    point_6 = point_5;
    rotation_3 = rotation_2;
    rrotation_5 = rrotation_4;
    let _e21 = point_6;
    let _e22 = distanceToEdge(_e21);
    shadow = (_e22 * 30f);
    let _e27 = shadow;
    shadow = ((1f - _e27) / 3f);
    let _e31 = shadow;
    if (_e31 < 0f) {
        shadow = 0f;
    } else {
        let _e35 = shadow;
        let _e36 = amount;
        shadow = (_e35 * _e36);
    }
    let _e38 = yc_5;
    let _e39 = p_3;
    let _e40 = rotation_3;
    let _e41 = rrotation_5;
    let _e42 = seeThrough(_e38, _e39, _e40, _e41);
    shadowColor = _e42;
    let _e45 = shadowColor;
    let _e47 = shadow;
    shadowColor.x = (_e45.x - _e47);
    let _e50 = shadowColor;
    let _e52 = shadow;
    shadowColor.y = (_e50.y - _e52);
    let _e55 = shadowColor;
    let _e57 = shadow;
    shadowColor.z = (_e55.z - _e57);
    let _e59 = shadowColor;
    return _e59;
}

fn backside(yc_6: f32, point_7: vec3<f32>) -> vec4<f32> {
    var yc_7: f32;
    var point_8: vec3<f32>;
    var color_1: vec4<f32>;
    var gray: f32;

    yc_7 = yc_6;
    point_8 = point_7;
    let _e15 = point_8;
    let _e17 = getFromColor(_e15.xy);
    color_1 = _e17;
    let _e19 = color_1;
    let _e21 = color_1;
    let _e24 = color_1;
    gray = (((_e19.x + _e21.z) + _e24.y) / 15f);
    let _e30 = gray;
    let _e36 = yc_7;
    gray = (_e30 + (0.8f * ((pow(max(0f, (1f - abs((_e36 / cylinderRadius)))), 0.2f) / 2f) + 0.5f)));
    let _e53 = color_1;
    let _e55 = gray;
    let _e56 = vec3(_e55);
    color_1.x = _e56.x;
    color_1.y = _e56.y;
    color_1.z = _e56.z;
    let _e63 = color_1;
    return _e63;
}

fn behindSurface(p_4: vec2<f32>, yc_8: f32, point_9: vec3<f32>, rrotation_6: mat3x3<f32>) -> vec4<f32> {
    var p_5: vec2<f32>;
    var yc_9: f32;
    var point_10: vec3<f32>;
    var rrotation_7: mat3x3<f32>;
    var local_2: f32;
    var safeAmount: f32;
    var shado: f32;
    var hitAngle_3: f32;
    var dx_1: f32;
    var dy_1: f32;
    var nyc: f32;

    p_5 = p_4;
    yc_9 = yc_8;
    point_10 = point_9;
    rrotation_7 = rrotation_6;
    let _e19 = amount;
    if (_e19 >= 0f) {
        let _e22 = amount;
        local_2 = max(_e22, 0.0001f);
    } else {
        let _e25 = amount;
        local_2 = min(_e25, -0.0001f);
    }
    let _e30 = local_2;
    safeAmount = _e30;
    let _e35 = yc_9;
    let _e37 = safeAmount;
    shado = ((1f - (((-0.15915494f - _e35) / _e37) * 7f)) / 6f);
    let _e45 = shado;
    let _e47 = point_10;
    shado = (_e45 * (1f - abs((_e47.x - 0.5f))));
    let _e58 = yc_9;
    yc_9 = (-0.31830987f - _e58);
    let _e60 = yc_9;
    let _e67 = cylinderAngle;
    hitAngle_3 = ((acos(clamp((_e60 / cylinderRadius), -1f, 1f)) + _e67) - PI);
    let _e71 = hitAngle_3;
    let _e72 = yc_9;
    let _e73 = point_10;
    let _e74 = rrotation_7;
    let _e75 = hitPoint(_e71, _e72, _e73, _e74);
    point_10 = _e75;
    let _e76 = yc_9;
    let _e79 = point_10;
    let _e84 = point_10;
    let _e89 = point_10;
    let _e94 = point_10;
    let _e99 = hitAngle_3;
    let _e101 = amount;
    if ((((((_e76 < 0f) && (_e79.x >= 0f)) && (_e84.y >= 0f)) && (_e89.x <= 1f)) && (_e94.y <= 1f)) && ((_e99 < PI) || (_e101 > 0.5f))) {
        {
            let _e106 = point_10;
            dx_1 = (_e106.x - 0.5f);
            let _e111 = point_10;
            dy_1 = (_e111.y - 0.5f);
            let _e117 = dx_1;
            let _e118 = dx_1;
            let _e120 = dy_1;
            let _e121 = dy_1;
            shado = (1f - (sqrt(((_e117 * _e118) + (_e120 * _e121))) / 0.71f));
            let _e130 = yc_9;
            nyc = (-(_e130) / cylinderRadius);
            let _e134 = shado;
            let _e135 = nyc;
            let _e136 = nyc;
            let _e138 = nyc;
            shado = (_e134 * ((_e135 * _e136) * _e138));
            let _e141 = shado;
            shado = (_e141 * 0.5f);
        }
    } else {
        {
            shado = 0f;
        }
    }
    let _e145 = p_5;
    let _e146 = getToColor(_e145);
    let _e148 = shado;
    let _e150 = (_e146.xyz - vec3(_e148));
    return vec4<f32>(_e150.x, _e150.y, _e150.z, 1f);
}

fn glt_main(p_6: vec2<f32>) -> vec4<f32> {
    var p_7: vec2<f32>;
    var angle: f32 = 1.7453293f;
    var c: f32;
    var s: f32;
    var rotation_4: mat3x3<f32>;
    var rrotation_8: mat3x3<f32>;
    var point_11: vec3<f32>;
    var yc_10: f32;
    var hitAngle_4: f32;
    var hitAngleMod: f32;
    var color_2: vec4<f32>;
    var otherColor: vec4<f32>;
    var dx2_: f32;
    var dy2_: f32;
    var shado_1: f32;
    var nyc2_: f32;
    var cl: vec4<f32>;
    var dist: f32;

    p_7 = p_6;
    let _e13 = u.progress;
    amount = ((_e13 * 1.66f) + MIN_AMOUNT);
    let _e19 = amount;
    cylinderCenter = _e19;
    let _e23 = amount;
    cylinderAngle = (6.2831855f * _e23);
    let _e31 = angle;
    c = cos(-(_e31));
    let _e35 = angle;
    s = sin(-(_e35));
    let _e39 = c;
    let _e40 = s;
    let _e42 = s;
    let _e44 = c;
    rotation_4 = mat3x3<f32>(vec3<f32>(_e39, _e40, 0f), vec3<f32>(-(_e42), _e44, 0f), vec3<f32>(-0.801f, 0.89f, 1f));
    let _e58 = angle;
    c = cos(_e58);
    let _e60 = angle;
    s = sin(_e60);
    let _e62 = c;
    let _e63 = s;
    let _e65 = s;
    let _e67 = c;
    rrotation_8 = mat3x3<f32>(vec3<f32>(_e62, _e63, 0f), vec3<f32>(-(_e65), _e67, 0f), vec3<f32>(0.985f, 0.985f, 1f));
    let _e80 = rotation_4;
    let _e81 = p_7;
    point_11 = (_e80 * vec3<f32>(_e81.x, _e81.y, 1f));
    let _e88 = point_11;
    let _e90 = cylinderCenter;
    yc_10 = (_e88.y - _e90);
    let _e93 = yc_10;
    if (_e93 < -0.15915494f) {
        {
            let _e97 = p_7;
            let _e98 = yc_10;
            let _e99 = point_11;
            let _e100 = rrotation_8;
            let _e101 = behindSurface(_e97, _e98, _e99, _e100);
            return _e101;
        }
    }
    let _e102 = yc_10;
    if (_e102 > cylinderRadius) {
        {
            let _e104 = p_7;
            let _e105 = getFromColor(_e104);
            return _e105;
        }
    }
    let _e106 = yc_10;
    let _e113 = cylinderAngle;
    hitAngle_4 = ((acos(clamp((_e106 / cylinderRadius), -1f, 1f)) + _e113) - PI);
    let _e117 = hitAngle_4;
    hitAngleMod = (_e117 - (floor((_e117 / 6.2831855f)) * 6.2831855f));
    let _e126 = hitAngleMod;
    let _e128 = amount;
    let _e132 = hitAngleMod;
    let _e137 = amount;
    if (((_e126 > PI) && (_e128 < 0.5f)) || ((_e132 > 1.5707964f) && (_e137 < 0f))) {
        {
            let _e142 = yc_10;
            let _e143 = p_7;
            let _e144 = rotation_4;
            let _e145 = rrotation_8;
            let _e146 = seeThrough(_e142, _e143, _e144, _e145);
            return _e146;
        }
    }
    let _e147 = hitAngle_4;
    let _e148 = yc_10;
    let _e149 = point_11;
    let _e150 = rrotation_8;
    let _e151 = hitPoint(_e147, _e148, _e149, _e150);
    point_11 = _e151;
    let _e152 = point_11;
    let _e156 = point_11;
    let _e161 = point_11;
    let _e166 = point_11;
    if ((((_e152.x < 0f) || (_e156.y < 0f)) || (_e161.x > 1f)) || (_e166.y > 1f)) {
        {
            let _e171 = yc_10;
            let _e172 = p_7;
            let _e173 = point_11;
            let _e174 = rotation_4;
            let _e175 = rrotation_8;
            let _e176 = seeThroughWithShadow(_e171, _e172, _e173, _e174, _e175);
            return _e176;
        }
    }
    let _e177 = yc_10;
    let _e178 = point_11;
    let _e179 = backside(_e177, _e178);
    color_2 = _e179;
    let _e182 = yc_10;
    if (_e182 < 0f) {
        {
            let _e185 = point_11;
            dx2_ = (_e185.x - 0.5f);
            let _e190 = point_11;
            dy2_ = (_e190.y - 0.5f);
            let _e196 = dx2_;
            let _e197 = dx2_;
            let _e199 = dy2_;
            let _e200 = dy2_;
            shado_1 = (1f - (sqrt(((_e196 * _e197) + (_e199 * _e200))) / 0.71f));
            let _e208 = yc_10;
            nyc2_ = (-(_e208) / cylinderRadius);
            let _e212 = shado_1;
            let _e213 = nyc2_;
            let _e214 = nyc2_;
            let _e216 = nyc2_;
            shado_1 = (_e212 * ((_e213 * _e214) * _e216));
            let _e219 = shado_1;
            shado_1 = (_e219 * 0.5f);
            let _e225 = shado_1;
            otherColor = vec4<f32>(0f, 0f, 0f, _e225);
        }
    } else {
        {
            let _e227 = p_7;
            let _e228 = getFromColor(_e227);
            otherColor = _e228;
        }
    }
    let _e229 = color_2;
    let _e230 = otherColor;
    let _e231 = yc_10;
    let _e234 = antiAlias(_e229, _e230, (cylinderRadius - abs(_e231)));
    color_2 = _e234;
    let _e235 = yc_10;
    let _e236 = p_7;
    let _e237 = point_11;
    let _e238 = rotation_4;
    let _e239 = rrotation_8;
    let _e240 = seeThroughWithShadow(_e235, _e236, _e237, _e238, _e239);
    cl = _e240;
    let _e242 = point_11;
    let _e243 = distanceToEdge(_e242);
    dist = _e243;
    let _e245 = color_2;
    let _e246 = cl;
    let _e247 = dist;
    let _e248 = antiAlias(_e245, _e246, _e247);
    return _e248;
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
