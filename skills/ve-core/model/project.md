# The project document

## On disk

A project is a folder:

```
my-edit/
  project.otio     the whole project (OTIO JSON, two-space indent)
  effects/         project-local effects (reserved, not used yet)
  plugins/         project-local plugins (reserved, not used yet)
```

Engine state that is not the project (locks, undo history, renders,
measurement cache, the daemon's socket and log) lives in the OS cache folder
(`<OS cache>/ve/`), never in the project folder, so cloud-synced folders stay
clean. Saves are atomic. Edits made to `project.otio` by other tools while
the daemon runs are picked up on the next call (`project.reloaded` event).

## Getting it

`project.get` returns `{revision, timeline}` where `timeline` is the full
OTIO `Timeline.1` document. `project.summary` returns the same as compact
text, made for agents to read. A frontend should use `project.get` and then
apply `project.changed` patches (see [../editing/events.md](../editing/events.md)).

## Shape

Standard OTIO, as written by Python `opentimelineio` 0.18, so tools like
DaVinci Resolve and Premiere (through OTIO adapters) can read it. Engine
data lives under `metadata.ve` on each object; other tools ignore it.

```
Timeline.1
  name
  metadata.ve.project      schema_version, revision, output settings
  metadata.ve.assets[]     the asset table (imported media)
  tracks: Stack.1
    markers[]              timeline markers (Marker.2)
    children[]: Track.1    kind "Video" or "Audio", name ("V1"), metadata.ve.track.id
      children[]           Clip.2 | Gap.1 | Transition.1 | Stack.1, in time order
```

A new project has two tracks, `V1` (video) and `A1` (audio); `track.add`
adds any number more ([../methods/tracks.md](../methods/tracks.md)). Video
tracks earlier in the list are composited **below** later ones; all audio
tracks mix.

### Where a clip is

OTIO stores **no start times** on items. An item's timeline start is the sum
of the durations of the items before it on its track; gaps (`Gap.1`) hold
empty time. Durations are `source_range.duration` (for a gap too).
Transitions take no time of their own. So to draw a track, walk its children
and keep a running position:

```js
let t = 0;
for (const item of track.children) {
  const d = seconds(item.source_range.duration);   // see time.md
  if (item.OTIO_SCHEMA.startsWith("Clip")) draw(item, t, t + d);
  if (!item.OTIO_SCHEMA.startsWith("Transition")) t += d;
}
```

Edit results also give you each touched clip's `start` and `end` directly
(`created` / `modified`), so you rarely need to recompute.

### Clip

```json
{
  "OTIO_SCHEMA": "Clip.2",
  "name": "tone.mkv",
  "source_range": {"start_time": {"value": 0.0, "rate": 25.0}, "duration": {"value": 100.0, "rate": 25.0}},
  "media_references": {"DEFAULT_MEDIA": {"OTIO_SCHEMA": "ExternalReference.1",
    "target_url": "file:///C:/media/tone.mkv",
    "available_range": {"start_time": {"value": 0.0, "rate": 25.0}, "duration": {"value": 100.0, "rate": 25.0}}}},
  "active_media_reference_key": "DEFAULT_MEDIA",
  "metadata": {"ve": {"clip": {"id": "clip_f4b280", "link": "lnk_ebb10d", "speed": [1, 1], "volume_db": 0.0}}},
  "enabled": true, "effects": [], "markers": []
}
```

| Field | Meaning |
| --- | --- |
| `source_range.start_time` | Media in-point (at the media's rate) |
| `source_range.duration` | **Timeline** duration of the clip (see speed in [time.md](time.md)) |
| `media_references[active_media_reference_key].target_url` | The media file. Match it to an asset by the asset's `url` |
| `available_range` | The whole media; a clip can never read outside it |
| `metadata.ve.clip.id` | Stable ID. Use it in every call |
| `metadata.ve.clip.link` | Clips sharing a link (a file's picture and sound) are edited together unless a call says `linked: false` |
| `metadata.ve.clip.speed` | Exact ratio `[num, den]`; `[3, 2]` is 1.5x. OTIO's `LinearTimeWarp` effect is written too, for other tools |
| `metadata.ve.clip.volume_db` | Audio gain in dB (audio clips) |
| `metadata.ve.effects[]` | Effect stack, applied in order: `{id, effect: "core.color/exposure@1", params, keyframes}` ([../methods/effects.md](../methods/effects.md)). Rendered by the GPU frame path |

A clip whose file is missing still loads; renders fail with
`render_failure` naming the asset.

### Marker

```json
{"OTIO_SCHEMA": "Marker.2", "name": "chapter", "color": "RED", "comment": "",
 "marked_range": {"start_time": {"value": 25.0, "rate": 25.0}, "duration": {"value": 10.0, "rate": 25.0}},
 "metadata": {"ve": {"marker": {"id": "mk_1b9862", "source": "user"}}}}
```

Timeline markers live on `tracks.markers` (timeline time). `source` is
`user`, `agent` or `analyzer`; analyzer markers (`silence`, `scene`) carry
a `payload` with the settings that made them. A marker's range can be fed
straight to `edit.ripple_delete` by name.

### Asset

`metadata.ve.assets[]`, also returned by `media.assets`:

```json
{"id": "asset_834f84", "kind": "source", "url": "file:///C:/media/tone.mkv",
 "hash": "ecc48741...",
 "media": {"duration": {"value": 100.0, "rate": 25.0}, "size_bytes": 836789,
   "video": {"codec": "h264", "width": 160, "height": 120, "rate": [25, 1],
     "color": {"pixel_format": "yuv420p", "bit_depth": 8, "chroma": "4:2:0", "range": "limited", "dynamic_range": "sdr"}},
   "audio": {"codec": "pcm_s16le", "sample_rate": 48000, "channels": 2}}}
```

| Field | Meaning |
| --- | --- |
| `url` | Absolute `file://` URL (other schemes later) |
| `hash` | blake3 of the file contents. Importing the same content again returns the existing asset |
| `media.duration` | Video duration if there is video, else audio |
| `media.video.rate` | Exact nominal frame rate `[num, den]` (`[30000, 1001]` is 29.97) |
| `media.video.rotation` | Display rotation (90/180/270, phone video); absent when 0 |
| `media.video.vfr` | Variable frame rate (common on phones); renders conform it |
| `media.video.color` | Pixel format, bit depth, chroma, range, primaries, transfer, matrix, `sdr`/`hdr10`/`hlg`. Fields the file does not state are absent |
| `media.audio` | First audio stream only |

### Project block

```json
"metadata": {"ve": {"project": {"schema_version": "0.1.0", "revision": 4,
  "output": {"width": 1920, "height": 1080, "rate": [30, 1], "sample_rate": 48000, "color_space": "bt709"}}}}
```

`revision` goes up by one per committed edit; it is the number every
mutating call's `base_rev` is checked against. `output` is what renders
produce by default (`master` preset).

## IDs

`<kind>_<6 hex>`: `clip_`, `trk_`, `mk_`, `asset_`, `lnk_` (link groups),
`grp_` (undo groups), `fx_`, `kf_`. IDs are stable across edits and saves.
A split keeps the ID on the left half and gives the right half a new one.
Files from other tools get IDs on open (`ids_added` in the result).
