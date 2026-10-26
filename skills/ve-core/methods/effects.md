# fx.*, effect.*, transition.*

Effects change a clip's picture (exposure, blur, vignette); transitions blend
one clip into the next at a cut (dissolve, wipe). Both are plugins: a
manifest plus code, run by the GPU compositor. How to write your own:
[../guides/writing-effects.md](../guides/writing-effects.md).

**Where they show.** `render.frame`, `render.range_preview` and
`render.export` use the GPU compositor automatically when the range has an
effect or a transition; every other range stays on the ffmpeg renderer, so
plain projects render as before. Audio is the same either way (the ffmpeg
graph, with its crossfades and J/L cuts), and the GPU export is one
background-capable job with progress and cancel like any other. Contact
sheets still use ffmpeg, which shows plain cuts and unprocessed clips and says
so in `warnings`. `VE_RENDERER=gpu` or `ffmpeg` where the daemon starts forces
a backend (for tests and debugging).

## `fx.list`

No params. Everything this project can use: built-ins, the project's
`effects/` folder, and installed effect packs (see below).

```json
{"effects": [
  {"id": "core.color/exposure", "reference": "core.color/exposure@1", "version": "1.0.0",
   "kind": "effect", "runtime": "wgsl", "name": "Exposure", "description": "Brighten or darken in photographic stops.",
   "origin": "builtin",
   "params": {"type": "object", "properties": {"stops": {"type": "number", "minimum": -5, "maximum": 5, "default": 0, "x-animatable": true}}}}
 ],
 "problems": ["...effects/broken: invert.wgsl line 3: ..."]}
```

`params` is JSON Schema: build a settings panel straight from it.
`x-animatable: true` means the param takes keyframes. `problems` lists
project and pack effects that failed to load.

`origin` is `builtin`, `project` (in the project's `effects/`) or `pack`
(from an installed effect pack, with `plugin` naming it). Packs are plugins
that contribute effects and transitions
([../guides/writing-plugins.md](../guides/writing-plugins.md#effect-packs)).
The first `effect.add`, `transition.add` or `transition.set` that uses a
pack effect copies its folder into the project's `effects/`, so the project
renders on machines without the pack; from then on it lists as `project`.
A dry run copies nothing, and neither does an edit that fails. The
project's copy wins over the pack: a newer `1.x` in the pack does not
change a project that already has `1.0`. Ask for a new major explicitly
(`acme.looks/glitch@2`); it is copied next to the old one.

Built-ins:

| Reference | Kind | Params |
| --- | --- | --- |
| `core.color/desaturate@1` | effect | `amount` 0..1 (1) |
| `core.color/exposure@1` | effect | `stops` -5..5 (0) |
| `core.color/tint@1` | effect | `color` sRGB [r,g,b] ([1,0.75,0.5]), `amount` 0..1 (0.5) |
| `core.look/vignette@1` | effect | `amount` (0.5), `size` (0.5), `softness` (0.5), all 0..1 |
| `core.look/blur@1` | effect | `radius` 0..100 px (8) |
| `core.transition/dissolve@1` | transition | none |
| `core.transition/dip@1` | transition | `color` ([0,0,0]) |
| `core.transition/wipe@1` | transition | `angle` degrees (0: left to right), `softness` 0..0.5 (0.05) |
| `core.transition/slide@1` | transition | `angle` degrees (0: moves right) |

## `fx.define`

| Param | | |
| --- | --- | --- |
| `manifest` | object | The `effect.json` (see the writing guide). `id` must start with `project.` |
| `source` | string | The WGSL code |
| `dry_run` | bool | Validate and test-render only |

Checks the manifest, validates the shader with naga (errors name your file,
line and column), compiles it on the GPU and renders three 64x64 test
frames with a 2 s limit, failing on NaN or infinite output. On success it
writes `<project>/effects/<name>/effect.json` and the code, and the effect
is usable at once. Not undoable (it writes files, not the timeline); calling
it again with the same id replaces it.

```json
{"effect": { ...as in fx.list... }, "path": "C:\\...\\my-edit\\effects\\invert", "tested_on": "NVIDIA GeForce RTX 4070 Laptop GPU (Vulkan)"}
```

## `effect.add` (edit)

| Param | Default | |
| --- | --- | --- |
| `clip` | required | A video clip (audio effects come later) |
| `effect` | required | `core.color/exposure` or `core.color/exposure@1`; stored as `id@major` |
| `params` | `{}` | Values to set; others use defaults. Checked against the schema |
| `index` | last | Position in the clip's stack; 0 runs first |

Effects stay with the clip through edits: a split copies the stack to both
halves and shifts the right half's keyframes.

## `effect.set` (edit)

| Param | | |
| --- | --- | --- |
| `id` | required | The effect instance, `fx_...` |
| `params` | object | Merged in; `null` resets a param to its default |
| `keyframes` | object | `{"amount": [{"at": "0s", "value": 0}, {"at": "2s", "value": 1, "ease": "linear"}]}`. Replaces that param's keyframes; `[]` removes them. `at` counts from the clip's start. `ease`: `linear` (default) or `hold` |
| `index` | int | Move it in the stack |

Keyframed params hold their first value before the first key and their
last after the last; numbers and colors interpolate linearly.

## `effect.remove` (edit)

`{"id": "fx_..."}`.

Effect results:

```json
{"clip": "clip_616450", "effect": "fx_5abb5a",
 "effects": [{"id": "fx_5abb5a", "effect": "core.look/vignette@1", "params": {"amount": 0.8}, "keyframed": ["amount"]}],
 "revision": 5, "changed": true, "patch": [...]}
```

## `transition.add` (edit)

| Param | Default | |
| --- | --- | --- |
| `clip` | | The outgoing clip; the transition goes on the cut at its end |
| `at` | | Or the time of the cut |
| `track` | first video track with a cut at `at` | |
| `transition` | `core.transition/dissolve` | |
| `duration` | `0.5s` | Snapped to frames |
| `align` | `center` | `center` (half before the cut), `start` (begins at the cut) or `end` (ends at it) |
| `params` | `{}` | |

The two clips must touch on a video track. A transition plays both clips at
once over its range, so the outgoing clip needs media past its out point
(for the part after the cut) and the incoming clip media before its in point
(for the part before). When a clip has none, the error says how much it has
and suggests `align: start` or `end`, a shorter duration, or a trim. Audio
cuts already get a 20 ms crossfade and take no transitions.

The transition is an OTIO `Transition` (`in_offset`, `out_offset`) between
the clips, so the cut and its length survive a handoff to Resolve or
Premiere (a dissolve as `SMPTE_Dissolve`, others as `Custom_Transition`).
Its kind and params are in `metadata.ve.transition`.

```json
{"id": "tr_73a179", "transition": "core.transition/wipe@1", "track": "V1",
 "start": "00:00:03:15", "end": "00:00:04:15", "cut": "00:00:04:00",
 "outgoing": "clip_616450", "incoming": "clip_f92a12", "revision": 6, "changed": true, "patch": [...]}
```

## `transition.set` (edit)

`{"id": "tr_...", "duration"?, "align"?, "transition"?, "params"?}`: change
any of them (params merge; `null` resets). Handles are checked again.

## `transition.remove` (edit)

`{"id": "tr_..."}`: back to a plain cut.

Edits that remove either clip, or open a gap at the cut, drop the
transition (a transition needs a clip on both sides).
