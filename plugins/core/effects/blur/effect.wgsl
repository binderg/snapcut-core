// 48 samples on a golden-angle spiral covering a disk of `radius` pixels.
fn effect(uv: vec2<f32>) -> vec4<f32> {
    let px = vec2<f32>(p.radius) / u.resolution;
    var acc = vec4<f32>(0.0);
    for (var i = 0u; i < 48u; i++) {
        let f = f32(i);
        let r = sqrt((f + 0.5) / 48.0);
        let a = f * 2.39996323;
        acc += src(uv + vec2<f32>(cos(a), sin(a)) * r * px);
    }
    return acc / 48.0;
}
