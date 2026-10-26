// Author: Woohyun Kim
// License: MIT
// Description: Both images fade through black with their edges glowing
// Tags: edges, fade
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

fn detectEdgeColor(c: array<vec3<f32>, 9>) -> vec4<f32> {
    var c_1: array<vec3<f32>, 9>;
    var dx: vec3<f32>;
    var dy: vec3<f32>;
    var delta: f32;

    c_1 = c;
    let _e9 = c_1[7];
    let _e12 = c_1[1];
    let _e18 = c_1[2];
    let _e21 = c_1[6];
    let _e27 = c_1[8];
    let _e30 = c_1[0];
    dx = (((2f * abs((_e9 - _e12))) + abs((_e18 - _e21))) + abs((_e27 - _e30)));
    let _e38 = c_1[3];
    let _e41 = c_1[5];
    let _e47 = c_1[6];
    let _e50 = c_1[8];
    let _e56 = c_1[0];
    let _e59 = c_1[2];
    dy = (((2f * abs((_e38 - _e41))) + abs((_e47 - _e50))) + abs((_e56 - _e59)));
    let _e65 = dx;
    let _e66 = dy;
    delta = length(((0.25f * (_e65 + _e66)) * 0.5f));
    let _e73 = p.edge_brightness;
    let _e74 = delta;
    let _e81 = c_1[4];
    let _e82 = (clamp((_e73 * _e74), 0f, 1f) * _e81);
    return vec4<f32>(_e82.x, _e82.y, _e82.z, 1f);
}

fn getFromEdgeColor(uv_4: vec2<f32>) -> vec4<f32> {
    var uv_5: vec2<f32>;
    var c_2: array<vec3<f32>, 9>;
    var i: i32 = 0i;
    var j: i32;
    var color: vec4<f32>;

    uv_5 = uv_4;
    loop {
        let _e9 = i;
        if !((_e9 < 3i)) {
            break;
        }
        j = 0i;
        loop {
            let _e18 = j;
            if !((_e18 < 3i)) {
                break;
            }
            {
                let _e25 = uv_5;
                let _e26 = p.edge_thickness;
                let _e27 = i;
                let _e30 = j;
                let _e38 = getFromColor((_e25 + (_e26 * vec2<f32>(f32((_e27 - 1i)), f32((_e30 - 1i))))));
                color = _e38;
                let _e41 = i;
                let _e43 = j;
                let _e46 = color;
                c_2[((3i * _e41) + _e43)] = _e46.xyz;
            }
            continuing {
                let _e22 = j;
                j = (_e22 + 1i);
            }
        }
        continuing {
            let _e13 = i;
            i = (_e13 + 1i);
        }
    }
    let _e48 = c_2;
    let _e49 = detectEdgeColor(_e48);
    return _e49;
}

fn getToEdgeColor(uv_6: vec2<f32>) -> vec4<f32> {
    var uv_7: vec2<f32>;
    var c_3: array<vec3<f32>, 9>;
    var i_1: i32 = 0i;
    var j_1: i32;
    var color_1: vec4<f32>;

    uv_7 = uv_6;
    loop {
        let _e9 = i_1;
        if !((_e9 < 3i)) {
            break;
        }
        j_1 = 0i;
        loop {
            let _e18 = j_1;
            if !((_e18 < 3i)) {
                break;
            }
            {
                let _e25 = uv_7;
                let _e26 = p.edge_thickness;
                let _e27 = i_1;
                let _e30 = j_1;
                let _e38 = getToColor((_e25 + (_e26 * vec2<f32>(f32((_e27 - 1i)), f32((_e30 - 1i))))));
                color_1 = _e38;
                let _e41 = i_1;
                let _e43 = j_1;
                let _e46 = color_1;
                c_3[((3i * _e41) + _e43)] = _e46.xyz;
            }
            continuing {
                let _e22 = j_1;
                j_1 = (_e22 + 1i);
            }
        }
        continuing {
            let _e13 = i_1;
            i_1 = (_e13 + 1i);
        }
    }
    let _e48 = c_3;
    let _e49 = detectEdgeColor(_e48);
    return _e49;
}

fn glt_main(uv_8: vec2<f32>) -> vec4<f32> {
    var uv_9: vec2<f32>;
    var start: vec4<f32>;
    var end: vec4<f32>;

    uv_9 = uv_8;
    let _e6 = uv_9;
    let _e7 = getFromColor(_e6);
    let _e8 = uv_9;
    let _e9 = getFromEdgeColor(_e8);
    let _e11 = u.progress;
    start = mix(_e7, _e9, vec4(clamp((2f * _e11), 0f, 1f)));
    let _e19 = uv_9;
    let _e20 = getToEdgeColor(_e19);
    let _e21 = uv_9;
    let _e22 = getToColor(_e21);
    let _e24 = u.progress;
    end = mix(_e20, _e22, vec4(clamp((2f * (_e24 - 0.5f)), 0f, 1f)));
    let _e34 = start;
    let _e35 = end;
    let _e36 = u.progress;
    return mix(_e34, _e35, vec4(_e36));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
