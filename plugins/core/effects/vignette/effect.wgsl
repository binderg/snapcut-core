fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = src(uv);
    let aspect = u.resolution.x / u.resolution.y;
    let q = (uv - vec2<f32>(0.5)) * vec2<f32>(aspect, 1.0);
    let r = length(q) / length(vec2<f32>(0.5 * aspect, 0.5));
    let v = 1.0 - p.amount * smoothstep(p.size, p.size + p.softness, r);
    return vec4<f32>(c.rgb * v, c.a);
}
