fn transition(uv: vec2<f32>) -> vec4<f32> {
    return mix(outgoing(uv), incoming(uv), u.progress);
}
