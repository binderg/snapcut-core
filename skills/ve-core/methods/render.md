# render.*

Renders read the project as it is; none of them change it. They run ffmpeg
with one filter graph compiled from the timeline, so a frame, a preview and
an export of the same time show the same picture. Frame-exact: a cut at a
frame boundary lands on that frame.

Ranges are `from` (default timeline start) and `to` (exclusive, default
timeline end). Results carry `warnings` for things rendered differently than
the document says, e.g. "2 transition(s) render as cuts until the Phase 2
compositor", or a clip on an audio track whose media has no sound. Missing
media is not a warning: the call fails with `render_failure` naming the clip.

## `render.frame`

| Param | Default | |
| --- | --- | --- |
| `at` | required | Timeline time, snapped down to its frame |
| `width` | 640 | Height follows the project's aspect |
| `format` | `jpeg` | or `png` |

```json
{"at": "00:00:01:00", "width": 640, "height": 360, "mime": "image/jpeg", "data": "<base64>", "warnings": []}
```

Show it with `<img src="data:image/jpeg;base64,...">`.

**Frame cache.** Frames are kept by what is in them (source frame, size,
transform, effect and transition values at that frame), not by revision, in
memory and on disk in the engine's cache folder. Asking again for a frame
nothing changed answers from the cache (`"cached": true`, a few ms), also
after unrelated edits and after a ripple that only moved it in time. A miss
renders it: about 100 to 250 ms on small media, more for 4K or long-GOP phone
footage far from a keyframe.

## `render.layers`

What is on screen and what is heard, as data instead of pixels and
samples. Nothing is decoded, so it answers in milliseconds even over a
whole timeline.

| Param | Default | |
| --- | --- | --- |
| `at` | | One frame (snapped down to its frame): every layer and audio clip in detail |
| `from`, `to` | whole timeline | Instead of `at`: which tracks show and are heard, as spans |
| `only` | both | `video` or `audio`: answer for that side only. The other side's fields are left out, and range spans join where only the other side changed |

**One frame** (`{"at": "00:00:42:00"}`). `visible` and `audible` are the
short answers; `layers` (bottom to top) and `audio` are the detail:

```json
{"at": "00:00:42:00", "canvas": {"width": 1920, "height": 1080},
 "visible": ["V2", "V3"],
 "layers": [
  {"track": "V1", "track_index": 0, "clip": "clip_7f3a", "asset": "asset_834f84", "link": "link_2c1d",
   "source_time": "118.400s", "rect": {"x": 0.0, "y": 0.0, "w": 1920.0, "h": 1080.0}, "opacity": 1.0,
   "alpha": "none", "solid": true, "visibility": "hidden", "visible_percent": 0, "covered_by": ["V2"]},
  {"track": "V2", "track_index": 1, "clip": "clip_91c0", "asset": "asset_aa01",
   "source_time": "3.200s", "rect": {"x": 0.0, "y": 0.0, "w": 1920.0, "h": 1080.0}, "opacity": 1.0,
   "alpha": "none", "solid": true, "visibility": "full", "visible_percent": 100, "overlapped_by": ["V3"]},
  {"track": "V3", "track_index": 2, "clip": "clip_logo", "asset": "asset_5c2e", "still": true,
   "rect": {"x": 1520.0, "y": 0.0, "w": 400.0, "h": 225.0}, "opacity": 1.0,
   "transform": {"x": 0.4, "y": -0.4, "scale": 0.2},
   "effects": [{"id": "fx_12", "effect": "core.color/exposure@1", "name": "Exposure", "params": {"stops": 0.3}}],
   "alpha": "transparent", "solid": false, "visibility": "full", "visible_percent": 100}],
 "audible": ["A1", "A2"],
 "audio": [
  {"track": "A1", "track_index": 0, "clip": "clip_7f3b", "asset": "asset_834f84", "link": "link_2c1d",
   "source_time": "118.400s", "gain_db": 2.5, "audible": true},
  {"track": "A2", "track_index": 1, "clip": "clip_c0de", "asset": "asset_9a7b",
   "source_time": "41.000s", "gain_db": -18.0, "audible": true}]}
```

Full-screen b-roll on V2 hides the talking head on V1, but the talking
head is still heard: its audio on A1 shares V1's `link`. The logo on V3 is
a cut-out, so it is drawn over V2 without hiding it. A2 is a music bed 18
dB down.

| Field | |
| --- | --- |
| `rect` | The drawn picture in project canvas pixels, after crop, before `rotation` (degrees clockwise, absent when 0) |
| `source_time` | Media time shown; absent for a still |
| `transform` | The clip's transform at this frame, keyframes evaluated ([edit.md](edit.md#editset_transform)); absent when it has none |
| `effects` | Each effect with every param's value at this frame: keyframes evaluated, defaults filled in. `missing: true` when the effect is not installed (then only what the clip stores) |
| `transition` | Mid-transition: `{id, transition, name, params, progress (0-1), role: "outgoing"/"incoming", with: <other clip>}`. The track then has two layers |
| `alpha` | The asset's transparency ([../model/project.md](../model/project.md#asset)) |
| `solid` | Hides what is under it wherever it is drawn |
| `visibility` | `full`, `partial` or `hidden`; `visible_percent` is the share of its on-canvas area that shows |
| `covered_by` | Tracks above whose solid layers hide some of it |
| `overlapped_by` | Tracks above drawn over it that may let it show through |
| `note` | Why it is hidden when nothing covers it: `off the canvas`, `opacity 0` |
| `link` | The clip's link group; a video layer and an audio clip with the same `link` came in together (picture and its sound). Different `source_time`s on a pair mean a J or L cut |
| `disabled` (top level) | Video tracks turned off with `track.set`; never drawn |
| `audio[].gain_db` | The clip's gain: `edit.set_volume`, plus what `edit.normalize` added. 0 is unity |
| `audio[].speed` | Playback speed, when not 1 |
| `audio[].effects` | Audio effects in stack order, each with every param's value at this frame, like `effects` on video layers ([effects.md](effects.md#audio-effects)) |
| `audio[].audible` | False when the gain is at the -100 dB floor (silenced); the clip is still listed |
| `muted` (top level) | Audio tracks turned off with `track.set`; never mixed |

**How loud.** `gain_db` is the setting, not a measurement: a clip at 0 dB
can still be quiet if its recording is. To hear how loud the mix actually
is at a point, measure a short range around it with `measure.levels` or
`measure.loudness` ([measure.md](measure.md)). The 20 ms crossfade at audio
cuts and the short fades at gap edges are not listed; at a frame on a cut,
only the clip that starts there is.

**What counts as covering.** The answer errs toward "shows", never toward
"hidden". A layer is `solid` only when its picture has no see-through
pixels (`alpha` `none` or `opaque`), opacity is 1, it has no effects (an
effect can make parts clear), it is not mid-transition, and it is turned by
a multiple of 90 degrees. A see-through or faded layer is listed in
`overlapped_by`, never `covered_by`. For an exact look, `render.frame`.

**A range** (`{"from": "00:01:00:00", "to": "00:02:00:00"}`, or `{}` for
the whole timeline): stretches where the same tracks show and are heard,
checked frame by frame so keyframed moves and fades count. Stretches with
no clip on any track are left out.

```json
{"from": "00:01:00:00", "to": "00:02:00:00", "spans": [
  {"from": "00:01:00:00", "to": "00:01:12:00", "visible": ["V1"], "audible": ["A1"]},
  {"from": "00:01:12:00", "to": "00:01:15:00", "visible": ["V1", "V3"], "audible": ["A1"]},
  {"from": "00:01:15:00", "to": "00:01:40:00", "visible": ["V2"], "hidden": ["V1"], "audible": ["A1", "A2"]},
  {"from": "00:01:40:00", "to": "00:02:00:00", "visible": ["V2"], "hidden": ["V1"], "audible": []}]}
```

`hidden` lists tracks with a clip there that is entirely covered. Use the
spans to check that an overlay is on screen when it should be, that a track
is ever seen at all, or to find stretches of silence (`audible` empty) or
of picture with no sound.

CLI: `ve layers --at 00:00:42:00`, or `ve layers --from ... --to ...`.

## `render.contact_sheet`

| Param | Default | |
| --- | --- | --- |
| `from`, `to` | whole timeline | |
| `count` | 12 | Frames, evenly spaced (1-100) |
| `columns` | near-square | |
| `width` | 320 | Width of each cell |
| `format` | `jpeg` | |

Result: one image (`data`, `mime`, `width`, `height`) plus `columns`, `rows`
and `times` (the timecode of each cell, row by row). Good for filmstrip
thumbnails on a timeline: one call, then slice the grid.

## `render.range_preview`

`{from, to, width (640)}`. A low-resolution MP4 of the range, for playing.

Previews are kept by content: asking again for a range where nothing
changed (same frames, same sound, same width) returns the same file at once
with `"cached": true`, even after edits elsewhere. They live in the
project's cache folder (`previews/`, about 1 GB, oldest removed first)
rather than `renders/`, so they are not cleared after a day; copy one out if
you want to keep it. A new preview uses frames already in the frame cache
when it has them all, which is much faster than rendering.

## `render.export`

`{from, to, preset}`. The final render with a preset from `render.presets`
(default `master`: the project's own size).

Both return a **file in the engine's cache**, not bytes:

```json
{"path": "C:\\Users\\me\\AppData\\Local\\ve\\projects\\6b69...\\renders\\export-rev12-1761090000.mp4",
 "url": "file:///C:/Users/me/AppData/Local/ve/projects/6b69.../renders/export-rev12-1761090000.mp4",
 "mime": "video/mp4", "from": "00:00:00:00", "to": "00:10:02:15", "duration": "00:10:02:15",
 "frames": 18075, "width": 1920, "height": 1080, "size_bytes": 412345678, "encoder": "libx264", "warnings": []}
```

Move or copy it where the user wants it; files left in the cache are deleted
after a day. (The CLI does this for you with `-o`.) A browser page cannot
open `file://` paths; a desktop shell (Tauri, Electron) can. Encoder: libx264
when the local ffmpeg has it, else a hardware encoder, openh264 or mpeg4.

By default the call returns when the file is done (a 10 minute 1080p export
took about 4.5 minutes on 4 cores). Other calls keep working meanwhile: the
export renders a copy of the timeline taken when it started. With
`background: true` (also on `render.contact_sheet`) it answers `{job_id}` at
once, with progress and cancel: see [jobs.md](jobs.md).

## `render.presets` (no project needed)

```json
{"presets": [
  {"name": "master", "description": "project size, high quality H.264 + AAC (default)"},
  {"name": "youtube-1080p", "description": "1920x1080 H.264 + AAC for upload", "size": [1920, 1080]},
  {"name": "youtube-720p", "description": "1280x720 H.264 + AAC", "size": [1280, 720]},
  {"name": "vertical-1080x1920", "description": "1080x1920 for Shorts/Reels/TikTok; landscape sources are center-cropped", "size": [1080, 1920]},
  {"name": "draft-540p", "description": "960x540, fast and small, for review", "size": [960, 540]}]}
```
