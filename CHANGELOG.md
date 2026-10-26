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
- Moved media: assets also store their path relative to the project
  folder, so a project folder moved with its media relinks itself on open.
  `media.relink` (`ve relink`) points an asset at a new file or searches a
  folder for every missing asset by content; `media.assets` lists `missing`.
- Project format versions: an engine upgrades older project files on open
  (`upgraded_from` in the `project.open` result) and refuses newer ones.
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
- Blend modes per clip (`blend` on `edit.set_transform`, `ve transform
  --blend`): the W3C set (multiply, screen, overlay, darken, lighten,
  color dodge/burn, hard/soft light, difference, exclusion, hue,
  saturation, color, luminosity) plus add, subtract and linear burn, mixed
  in sRGB values as Photoshop does. GPU only.
- Every edit: optimistic concurrency (`base_rev`), dry runs, an actor, and
  a JSON Patch result. Undo and redo per actor, undo groups, history that
  survives restarts.

### Effects

- WGSL effect and transition runtime with a manifest, JSON Schema params,
  validation with errors on the author's line, and a GPU test render.
- Built in: blur, desaturate, exposure, tint, vignette; dissolve, dip,
  slide, wipe transitions. Project effects via `fx.define`.
- Keyframe curves for effect params, transitions and transforms: linear,
  hold, the CSS eases and `cubic-bezier(...)`, or graph-editor `in`/`out`
  handles per key (`--key opacity@0s=0:ease-out` on the CLI).

### Rendering and export

- Frame- and sample-exact FFmpeg filtergraph renderer for plain cuts, and a
  wgpu compositor for effects, transitions and transforms (it matches the
  FFmpeg renderer pixel for pixel on plain frames).
- 20 ms audio crossfade on every cut.
- `ve frame`, `ve sheet` (contact sheets), `ve preview`, `ve export` with
  presets: master, youtube-1080p, youtube-720p, vertical-1080x1920,
  draft-540p. Encoder: libx264 when available, else hardware, openh264 or
  mpeg4.
- Export formats (`format` on `render.export`, `ve export --format`, or
  from the `-o` extension): MP4 H.264, MP4 HEVC, ProRes 422 HQ, ProRes 4444
  with alpha, WebM VP9/Opus, and audio only as WAV, M4A, MP3 or FLAC.
- Captions: `transcript.get` with `format: srt` or `vtt` (`ve transcript
  --format srt`), timed to the edited timeline.
- Content-keyed frame and preview caches with background render-ahead.
- `ve cache` shows and `ve cache clear` deletes the rebuildable caches
  (frames, previews, renders, measurements, extracted audio).

### Analysis and measurement

- Silence and scene-cut detection writing markers.
- Loudness (EBU R128, true peak), levels (RMS, peak, DC, clipping, noise
  floor, balance, correlation), waveform, pitch.

### Plugins and transcripts

- Process plugins: a folder with `plugin.json` adds methods and `ve`
  commands; its program (any language) speaks JSON-RPC over stdio, can call
  any engine method back, reports progress, honours cancel, and stops when
  idle. Found in the project's `plugins/` folder and the user's plugin
  folder. `plugin.list`, `ve plugin list | install | secret set | secret rm`.
  API keys live in the OS keychain, never in the project.
- Plugin methods show up in `system.describe`, run as jobs when marked so,
  and their edits undo as one step.
- Transcripts as core data: per source asset, words with exact times,
  speakers and confidence, stored in the project's `transcripts/` folder.
  `transcript.set | get | list | remove`; `ve transcript` shows the words
  that play on the timeline as readable lines with timecodes.
- `media.extract_audio`: an asset's audio as WAV or FLAC for plugins.
- First speech-to-text plugin: `plugins/deepgram` (bring your own key;
  install with `ve plugin install plugins/deepgram`).
- Effect packs: a plugin's `contributes.effects` adds effects and
  transitions with no process. `fx.list` shows them as `origin: "pack"`,
  and the first use in a project copies the effect into its `effects/`
  folder, so projects render without the pack installed.
- `plugins/gl-transitions`: 125 transitions from gl-transitions.com as an
  effect pack (ids `gl.transition/<name>`), each keeping its author's
  license header. Install with `ve plugin install plugins/gl-transitions`.
- A project may hold two majors of one effect id side by side
  (`id@1`, `id@2`); before, the second was refused as a duplicate.

### Protocol and tools

- Protocol stability: within a protocol major, changes are additive; a
  snapshot test fails the build on a breaking change. Deprecated methods are
  marked in `describe` and keep working until the next major.
- `ve plugin install` refuses plugins whose `engine` range or `protocol`
  does not match (`--force` to copy anyway).

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
- No titles, text or captions yet (planned next). Plugins run with the
  user's permissions (no sandbox).
- Exports with effects take several times longer than plain cuts.
