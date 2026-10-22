fn transition(uv: vec2<f32>) -> vec4<f32> {
    let c = vec4<f32>(p.color.rgb, 1.0);
    if (u.progress < 0.5) {
        return mix(outgoing(uv), c, u.progress * 2.0);
    }
    return mix(c, incoming(uv), (u.progress - 0.5) * 2.0);
}
