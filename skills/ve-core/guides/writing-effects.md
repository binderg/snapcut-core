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
| `{"type": "array", "items": {"type": "number"}, "minItems": N, "maxItems": N}`, N from 2 to 4 | `p.name: vec2<f32>`, `vec3<f32>` or `vec4<f32>` | Exactly N numbers, passed as is. `minimum` / `maximum` (on the array or on `items`) apply to each number. `"items": {"type": "integer"}` also works and still arrives as floats |
| `{"type": "string", "format": "image", "default": "luma.png"}` | a function named after the param: `name(uv) -> vec4<f32>` | A PNG or JPEG file name in the effect's folder (see Images). Not keyframable |

Add `"default"` to every param (image params must have one), and
`"x-animatable": true` to params that may be keyframed (numbers, colors and
vectors; vectors interpolate per number). Param names: lowercase letters, digits and `_`; not `u`, `p`,
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

## Images

A param with `"format": "image"` gives the shader a picture of its own: a
luma map for a wipe, a displacement map, a texture. The file sits next to
the manifest:

```
effects/luma_wipe/
  effect.json
  luma_wipe.wgsl
  luma.png
```

```json
"params": {"type": "object", "properties": {
  "luma": {"type": "string", "format": "image", "default": "luma.png"},
  "softness": {"type": "number", "minimum": 0, "maximum": 1, "default": 0.1}
}}
```

```wgsl
fn transition(uv: vec2<f32>) -> vec4<f32> {
    // Dark parts of the map change first; all outgoing at 0, all incoming at 1.
    let s = max(p.softness, 0.001);
    let l = luma(uv).r * (1.0 - s);
    return mix(outgoing(uv), incoming(uv), smoothstep(l, l + s, u.progress));
}
```

- The function is named after the param and takes `uv` like `src`: 0..1,
  (0, 0) at the top left of the picture, stretched over the frame, linear
  filtering, clamped at the edges.
- It returns the stored values (0..1 per channel, straight alpha), not
  linearized: a luma or displacement map is data, not light. Convert it
  yourself if you use the image as a picture.
- PNG or JPEG, 1 to 4096 pixels per side. File names: letters, digits, `_`,
  `-` and `.`, no folders, ending in `.png`, `.jpg` or `.jpeg`.
- A clip may pick another file the folder has: `{"luma": "spiral.png"}`.
  Every PNG and JPEG in the folder is loaded, and a value must name one of them.
- Images cannot be keyframed. They take no space in `p`.

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
ve frame --at 00:00:06:00 -o check.jpg          # renders on the GPU when the clip has the effect
```

Errors name your file, line and column:

```
error: invert.wgsl line 3, column 48: invalid field accessor `amont`
```

`ve fx define` sends the PNG and JPEG files in the folder along with the
code. Over the protocol: `fx.define {manifest, source, files, dry_run}`
([../methods/effects.md](../methods/effects.md#fxdefine)), with `files` as
`{"luma.png": "<base64>"}`.

Built-in effects in `plugins/core/effects/` follow exactly this contract
and are good examples.
