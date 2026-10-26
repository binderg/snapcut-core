// Author: Zeh Fernando
// License: MIT
// Description: Zooms and rotates into a bright flash, then out to the incoming image
// Tags: zoom, rotate, flash
//
// Definitions --------
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
    var local: f32;
    var phase: f32;
    var local_1: f32;
    var angleOffset: f32;
    var local_2: f32;
    var newScale: f32;
    var center: vec2<f32> = vec2<f32>(0f, 0f);
    var assumedCenter: vec2<f32> = vec2<f32>(0.5f, 0.5f);
    var glt_p: vec2<f32>;
    var angle: f32;
    var dist: f32;
    var local_3: vec4<f32>;
    var c: vec4<f32>;
    var local_4: f32;

    uv_5 = uv_4;
    let _e6 = u.progress;
    if (_e6 < 0.5f) {
        let _e9 = u.progress;
        local = (_e9 * 2f);
    } else {
        let _e12 = u.progress;
        local = ((_e12 - 0.5f) * 2f);
    }
    let _e18 = local;
    phase = _e18;
    let _e20 = u.progress;
    if (_e20 < 0.5f) {
        let _e24 = p.rotation;
        let _e27 = phase;
        local_1 = mix(0f, (_e24 * 0.03926991f), _e27);
    } else {
        let _e29 = p.rotation;
        let _e34 = phase;
        local_1 = mix((-(_e29) * 0.03926991f), 0f, _e34);
    }
    let _e37 = local_1;
    angleOffset = _e37;
    let _e39 = u.progress;
    if (_e39 < 0.5f) {
        let _e43 = p.scale;
        let _e44 = phase;
        local_2 = mix(1f, _e43, _e44);
    } else {
        let _e46 = p.scale;
        let _e48 = phase;
        local_2 = mix(_e46, 1f, _e48);
    }
    let _e51 = local_2;
    newScale = _e51;
    let _e63 = uv_5;
    let _e69 = newScale;
    let _e72 = (u.resolution.x / u.resolution.y);
    glt_p = (((_e63.xy - vec2<f32>(0.5f, 0.5f)) / vec2(_e69)) * vec2<f32>(_e72, 1f));
    let _e77 = glt_p;
    let _e79 = glt_p;
    let _e82 = angleOffset;
    angle = (atan2(_e77.y, _e79.x) + _e82);
    let _e85 = center;
    let _e86 = glt_p;
    dist = distance(_e85, _e86);
    let _e90 = angle;
    let _e92 = dist;
    let _e94 = (u.resolution.x / u.resolution.y);
    glt_p.x = (((cos(_e90) * _e92) / _e94) + 0.5f);
    let _e99 = angle;
    let _e101 = dist;
    glt_p.y = ((sin(_e99) * _e101) + 0.5f);
    let _e105 = u.progress;
    if (_e105 < 0.5f) {
        let _e108 = glt_p;
        let _e109 = getFromColor(_e108);
        local_3 = _e109;
    } else {
        let _e110 = glt_p;
        let _e111 = getToColor(_e110);
        local_3 = _e111;
    }
    let _e113 = local_3;
    c = _e113;
    let _e115 = c;
    let _e116 = u.progress;
    if (_e116 < 0.5f) {
        let _e121 = phase;
        local_4 = mix(0f, 1f, _e121);
    } else {
        let _e125 = phase;
        local_4 = mix(1f, 0f, _e125);
    }
    let _e128 = local_4;
    return (_e115 + vec4(_e128));
}

fn transition(uv: vec2<f32>) -> vec4<f32> {
    return glt_enc(glt_main(glt_flip(uv)));
}
