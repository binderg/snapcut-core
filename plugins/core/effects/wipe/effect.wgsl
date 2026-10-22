fn transition(uv: vec2<f32>) -> vec4<f32> {
    let a = radians(p.angle);
    let dir = vec2<f32>(cos(a), sin(a));
    // Position along the direction, 0 where the edge starts, 1 where it ends.
    let extent = 0.5 * (abs(dir.x) + abs(dir.y));
    let x = (dot(uv - vec2<f32>(0.5), dir) + extent) / (2.0 * extent);
    let s = p.softness;
    let edge = u.progress * (1.0 + s);
    let keep_from = smoothstep(edge - s, edge, x);
    return mix(incoming(uv), outgoing(uv), keep_from);
}
