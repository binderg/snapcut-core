# Writing an effect or transition

An effect or transition is a folder with a manifest and one WGSL function.
This guide is the whole contract for the `wgsl` runtime. (Other runtimes,
`wasm` and `ofx`, are planned; see ARCHITECTURE.md.)

```
effects/invert/
  effect.json
  invert.wgsl
```

## The manifest

```json
{
  "id": "project.invert",
  "version": "1.0.0",
  "kind": "effect",
  "runtime": "wgsl",
  "name": "Invert",
  "description": "Negative image, partly or fully.",
  "shader": "invert.wgsl",
  "params": {
    "type": "object",
    "properties": {
      "amount": {"type": "number", "minimum": 0, "maximum": 1, "default": 1, "x-animatable": true}
    }
  }
}
```

| Field | |
| --- | --- |
| `id` | Lowercase, dotted. Project effects start with `project.` |
| `version` | Semver. Clips store `id@major`, so bump the major version when old params stop meaning the same thing |
| `kind` | `effect` or `transition` |
| `runtime` | `wgsl` |
| `shader` | The code file, next to the manifest |
| `params` | JSON Schema; each property is one param |
| `skill` | Optional `SKILL.md` telling agents when to use it |

Param types:

| Schema | In the shader | Values |
| --- | --- | --- |
| `{"type": "number"}` or `"integer"` | `p.name: f32` | `minimum` / `maximum` are enforced |
| `{"type": "boolean"}` | `p.name: f32` (0 or 1) | |
| `{"type": "array", "format": "color", ...}` | `p.name: vec4<f32>`, linear RGB, straight alpha | Callers send sRGB `[r, g, b]` or `[r, g, b, a]`, 0..1; the engine converts |

Add `"default"` to every param, and `"x-animatable": true` to params that may
be keyframed. Param names: lowercase letters, digits and `_`; not `u`, `p`,
`src`, `outgoing`, `incoming`, `effect`, `transition`, `uv`, nor starting with `ve_`.

## The code

An effect defines:

```wgsl
fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = src(uv);
    return vec4<f32>(mix(c.rgb, c.a - c.rgb, p.amount), c.a);
}
```

A transition defines:

```wgsl
fn transition(uv: vec2<f32>) -> vec4<f32> {
    return mix(outgoing(uv), incoming(uv), u.progress);
}
```

(`from` is a reserved word in WGSL, hence `outgoing` / `incoming`.)

What the engine provides:

| Name | |
| --- | --- |
| `uv` | Output position, 0..1, (0, 0) at the top left |
| `src(uv)` | Effects: the clip's picture at `uv`, after the effects before this one |
| `outgoing(uv)`, `incoming(uv)` | Transitions: the two clips, each with its own effects, placed on the canvas |
| `u.time` | Seconds since the clip (or transition) starts |
| `u.progress` | 0..1 across the clip (or transition) |
| `u.resolution` | Pixels being drawn: the clip's size for effects, the canvas for transitions |
| `p.<param>` | Param values for this frame, keyframes already applied |

Colors are **linear light with premultiplied alpha**: mix and add freely
(that is what makes blends look right), and keep `rgb <= a` when you change
alpha. Return the same. Samples outside 0..1 clamp to the edge.

Any number of `src`/`outgoing`/`incoming` calls is fine, including in loops
(a blur samples many times). You may define helper functions and constants.

## Rules

- **Deterministic.** Output depends only on the inputs, `u` and `p`: no
  state between frames, no randomness that is not derived from them (hash
  `uv` and `u.time` for grain). This is what makes frames cacheable and
  preview equal to export.
- **Cheap.** A 64x64 test frame must finish within 2 s; aim for well under
  a millisecond at 1080p. Loops need a fixed upper bound.
- **No NaN.** Guard divisions, `sqrt` and `log`; the test render rejects NaN
  or infinite output.

## Trying it

```bash
ve fx define effects-src/invert --dry-run     # validate and test-render only
ve fx define effects-src/invert               # save to the project's effects/ folder
ve effect add clip_7f3a project.invert --param amount=0.5
ve frame --at 00:00:06:00 -o check.jpg          # with VE_RENDERER=gpu on the daemon
```

Errors name your file, line and column:

```
error: invert.wgsl line 3, column 48: invalid field accessor `amont`
```

Or over the protocol: `fx.define {manifest, source, dry_run}`
([../methods/effects.md](../methods/effects.md#fxdefine)).

Built-in effects in `plugins/core/effects/` follow exactly this contract
and are good examples.
