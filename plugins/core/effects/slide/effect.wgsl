fn transition(uv: vec2<f32>) -> vec4<f32> {
    let a = radians(p.angle);
    let d = vec2<f32>(cos(a), sin(a));
    let shift = d * u.progress;
    let uf = uv - shift;
    if (all(uf >= vec2<f32>(0.0)) && all(uf <= vec2<f32>(1.0))) {
        return outgoing(uf);
    }
    return incoming(uf + d);
}
