fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = src(uv);
    return vec4<f32>(mix(c.rgb, c.rgb * p.color.rgb, p.amount), c.a);
}
