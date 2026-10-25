# Changelog

All notable changes to this project. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org). Before 1.0, a minor version may
change the protocol or the project format; each such change is listed here.

## [0.1.0-rc.1] (unreleased)

The first public release candidate. Alpha: the editing core is complete
enough to cut real footage end to end, but the protocol and the
`metadata.ve` project data may still change before 1.0. The product name is
not final (the command is `ve`).

### Project and media

- A project is a folder holding `project.otio` (OpenTimelineIO JSON, read
  and written field for field like Python `opentimelineio` 0.18) plus
  `effects/`. Caches live in the OS cache folder, never in the project.
- Exclusive project lock, atomic saves, stable IDs (`clip_7f3a`) for every
  clip, track, marker, asset and effect.
- `ve probe`, `ve import` (deduplicated by content hash), `media.remove`.
  Probing records duration, rate, rotation, variable frame rate and colour
  (bit depth, range, primaries, transfer, HDR10/HLG).
- Exact time everywhere (rationals, no floats); SMPTE timecode including
  drop-frame, frame counts and seconds as input.

### Editing

- Insert, split, trim (ripple or roll), move (overwrite or reorder), ripple
  delete (ranges, marker ranges or clips), lift, speed (exact ratios, written
  as OTIO `LinearTimeWarp` too), volume, unlink, loudness normalize.
- Video and its audio are linked and edited together; edit one side for
  J and L cuts. A ripple on one side never pulls later linked pairs apart:
  the partner track follows, or the edit is refused when it has no room.
- Any number of video and audio tracks; hide and mute.
- Still images (PNG, JPEG, WebP, BMP, TIFF, single-frame GIF) as clips of
  any length, with transparency kept: logos, title cards, photos.
- Per-clip transforms (position, scale, rotation, opacity, crop) with
  keyframes.
- Every edit: optimistic concurrency (`base_rev`), dry runs, an actor, and
  a JSON Patch result. Undo and redo per actor, undo groups, history that
  survives restarts.

### Effects

- WGSL effect and transition runtime with a manifest, JSON Schema params,
  validation with errors on the author's line, and a GPU test render.
- Built in: blur, desaturate, exposure, tint, vignette; dissolve, dip,
  slide, wipe transitions. Project effects via `fx.define`.

### Rendering and export

- Frame- and sample-exact FFmpeg filtergraph renderer for plain cuts, and a
  wgpu compositor for effects, transitions and transforms (it matches the
  FFmpeg renderer pixel for pixel on plain frames).
- 20 ms audio crossfade on every cut.
- `ve frame`, `ve sheet` (contact sheets), `ve preview`, `ve export` with
  presets: master, youtube-1080p, youtube-720p, vertical-1080x1920,
  draft-540p. Encoder: libx264 when available, else hardware, openh264 or
  mpeg4.
- Content-keyed frame and preview caches with background render-ahead.
- `ve cache` shows and `ve cache clear` deletes the rebuildable caches
  (frames, previews, renders, measurements).

### Analysis and measurement

- Silence and scene-cut detection writing markers.
- Loudness (EBU R128, true peak), levels (RMS, peak, DC, clipping, noise
  floor, balance, correlation), waveform, pitch.

### Protocol and tools

- JSON-RPC 2.0 over a local socket and a WebSocket (token, loopback only),
  one daemon per project started on demand, `project.changed` events.
- OpenRPC document generated from the code (`ve describe`).
- Background jobs with progress and cancel.
- Agent skill `skills/video-edit/` and API reference `skills/ve-core/`.
- `ui.*` methods for human editors are experimental and may change or move
  without notice.

### Known limits

- Tested mostly on Windows so far.
- No proxies yet, so long-GOP phone footage seeks slowly.
- Contact sheets ignore effects and transforms.
- No titles or text yet (planned next), and no plugin host (no
  transcription or captions).
- Exports with effects take several times longer than plain cuts.
