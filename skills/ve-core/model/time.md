# Time

The engine works in exact rational time internally (no floating point), so
cuts never drift by a frame. The protocol follows from that.

## Sending times

Every time parameter is a **string**, read at the project's frame rate:

| Form | Example | Notes |
| --- | --- | --- |
| SMPTE timecode | `00:01:12:12` | Hours, minutes, seconds, frames |
| Drop-frame timecode | `00:01:00;02` | A `;` anywhere; only at 29.97 and 59.94 |
| Frames | `f2172` or `2172f` | Frame count from zero |
| Seconds | `72.4s` | Exact decimal; there is no `ms` form, write `0.5s` |
| Signed offset | `+15f`, `-1s` | Where a param is a change (`edit.trim`'s `in`/`out`) |

Timeline times snap to the project's frame grid. Media times
(`edit.insert`'s `source_in` / `source_out`) are at the media's own rate.

Non-drop timecode at an NTSC rate counts frames at the nominal rate
(`00:00:01:00` at 29.97 is frame 30), as every editor does.

## Times you get back

Results give times as SMPTE timecode at the project rate (`"00:00:04:00"`),
or as `<n>f` for frame counts in a few places. Measurement series give a
`step` the same way.

## Times inside the document

OTIO stores `RationalTime` as two doubles: `{"value": 100.0, "rate": 25.0}`,
meaning 100 frames at 25 fps (4 s). Seconds = `value / rate`. At NTSC rates
OTIO writes the rate as a double like `29.97002997002997`; the exact rate is
in `metadata.ve.project.output.rate` (`[30000, 1001]`) and on each asset
(`media.video.rate`). For drawing, doubles are fine. For anything you send
back, send a timecode or frame string built from frame counts, not a float.

```js
const seconds = (rt) => rt.value / rt.rate;
const frames = (rt, fps) => Math.round(seconds(rt) * fps);   // fps = num/den of the project rate
```

## Rates

Rates are exact `[num, den]` pairs in `metadata.ve`: `[30, 1]`, `[30000, 1001]`
(29.97), `[24000, 1001]` (23.976), `[25, 1]`, `[60000, 1001]`. `project.init`
accepts `30`, `29.97`, `23.976` or `30000/1001`.

## Speed

`metadata.ve.clip.speed` is an exact ratio. A clip's `source_range.duration`
is its length **on the timeline**; the media it uses is `duration x speed`
(OTIO's time-warp convention). So a 2 s clip at `[2, 1]` plays 4 s of media.
`edit.set_speed` takes `3/2`, `1.5`, `0.5`, or `2x`.
