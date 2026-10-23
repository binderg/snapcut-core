# render.*

Renders read the project as it is; none of them change it. Every render
shows the finished picture: all video tracks stacked, speed changes, clip
effects and transitions, exactly as the export will look. A frame, a contact
sheet, a preview and an export of the same time show the same picture.
Frame-exact: a cut at a frame boundary lands on that frame. No UI is needed;
this is how an agent sees what is "on screen".

There are two renderers and the engine picks one per call. A frame or range
with any effect or transition goes through the GPU compositor; everything
else through one ffmpeg filter graph (faster, and the same pixels where both
apply). Clients don't choose.

Ranges are `from` (default timeline start) and `to` (exclusive, default
timeline end). Results carry `warnings` for things rendered differently than
the document says, e.g. a clip on an audio track whose media has no sound.
Missing media is not a warning: the call fails with `render_failure` naming
the clip. A missing or broken effect fails the same way.

## `render.frame`

| Param | Default | |
| --- | --- | --- |
| `at` | required | Timeline time, snapped down to its frame |
| `width` | 640 | Height follows the project's aspect |
| `format` | `jpeg` | or `png` |

```json
{"at": "00:00:01:00", "width": 640, "height": 360, "mime": "image/jpeg", "data": "<base64>", "warnings": []}
```

Show it with `<img src="data:image/jpeg;base64,...">`. Without effects,
each call starts ffmpeg: about 250 ms per frame on small media, more for 4K
or long-GOP phone footage far from a keyframe. With effects or transitions
the GPU renders it; the first such frame in a daemon takes a few seconds (GPU
setup), later ones reuse warm decoders. Fine for a viewer that updates when
scrubbing pauses or a few times a second, not for playback. There is no frame
cache yet.

## `render.contact_sheet`

| Param | Default | |
| --- | --- | --- |
| `from`, `to` | whole timeline | |
| `count` | 12 | Frames, evenly spaced (1-100) |
| `columns` | near-square | |
| `width` | 320 | Width of each cell |
| `format` | `jpeg` | |

Result: one image (`data`, `mime`, `width`, `height`) plus `columns`, `rows`
and `times` (the timecode of each cell, row by row). Each cell is the same
picture `render.frame` gives at that time, effects and transitions included
(a range with any renders every cell on the GPU, which is slower). Good for
filmstrip thumbnails on a timeline: one call, then slice the grid.

## `render.range_preview`

`{from, to, width (640)}`. A low-resolution MP4 of the range.

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
