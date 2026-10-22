# Building a frontend

A frontend is one more client of the protocol: it holds no state the engine
does not have, and an agent can make the same edits at the same time. This
guide is the order to build things in. Check
[status-and-limits.md](status-and-limits.md) first so you do not design
around features that do not exist yet (notably: no video playback stream,
no background jobs, no effects or transitions).

## 1. Pick a shell

| Shell | Fits when | Notes |
| --- | --- | --- |
| Desktop (Tauri, Electron) | A real editor | Can run `ve` to start the daemon and read the URL, open file pickers that give real paths (`media.import` needs paths), play local files, and move exports into place |
| Browser page from a local dev server (`localhost:<port>`) | Prototypes, dashboards | Allowed by the origin check. Cannot start the daemon or read files itself; get the URL from `ve daemon status` |

A page from a non-loopback origin or `file://` is refused (HTTP 403).

## 2. Connect

```
ve --json daemon start      # starts it if needed; prints daemon.info JSON
```

Take `websocket` from that JSON and open it. On close, run the command again
(the daemon may have restarted with a new port and token) and reconnect. An
open WebSocket keeps the daemon alive; it exits 10 minutes after the last
client leaves.

Write one small RPC helper: send `{jsonrpc, id, method, params}`, keep a map
of pending ids to promises, route messages without `id` to an event handler.
Generate types from `system.describe` if you use TypeScript.

## 3. Load and mirror the project

Follow [../editing/events.md](../editing/events.md): `events.subscribe`, then
`project.get`, then apply `project.changed` patches by revision. Keep `doc`
and `rev` in one store; everything below reads from it.

## 4. Draw the timeline

From `doc.tracks.children` (see [../model/project.md](../model/project.md)):

- One row per track; `kind` says video or audio, `name` is the label.
- Walk each track's children with a running time to place clips and gaps.
  Convert with `value / rate` for drawing; keep frame numbers for anything
  you send back ([../model/time.md](../model/time.md)).
- Label clips with `name`, key them by `metadata.ve.clip.id`.
- Clips sharing `metadata.ve.clip.link` are one A/V pair: select and drag
  them together, and offer an "unlink" modifier that sends `linked: false`.
- Markers from `doc.tracks.markers`, colored by `color`; analyzer markers
  (`metadata.ve.marker.source == "analyzer"`) can be styled as suggestions.
- Show who changed what: `project.changed` carries `actor`; flash clips an
  agent touched (the patch paths and the edit's `created` / `modified` say which).

## 5. Thumbnails and the viewer

- **Viewer:** `render.frame {at, width}` at the playhead. Debounce while
  scrubbing and keep only the latest request: each frame costs about 250 ms
  or more. Show the last frame until the new one arrives.
- **Filmstrips:** one `render.contact_sheet {from, to, count, width}` per
  visible range, then slice the grid image into cells (`times` gives each
  cell's timecode). Re-request after edits that touch that range.
- **Playback:** not available as a stream yet. Options today: a desktop shell
  can play the source file of the clip under the playhead (`target_url`, from
  `source_range.start_time` adjusted for speed) and jump at cuts; or render
  `render.range_preview` for the region around the playhead and play that MP4.

## 6. Waveforms

`measure.waveform {asset, buckets}` per asset is cached by the engine and does
not change with edits. Draw each audio clip by mapping its media range
(`source_range.start_time` to `+ duration x speed`) onto the asset's
buckets. Ask for more buckets when zoomed in. `measure.levels` /
`measure.loudness` with `resolution` give meters and loudness lanes.

## 7. Editing

Map gestures to methods ([../methods/edit.md](../methods/edit.md)):

| Gesture | Call |
| --- | --- |
| Blade tool / `S` key | `edit.split {at}` or `{clips: [id], at}` |
| Drag a clip edge | `edit.trim {clip, in or out: "+Nf"}`; with a modifier `roll: true` |
| Drag a clip | `edit.move {clip, to, track?}` |
| Delete / ripple delete | `edit.delete {clips}` / `edit.ripple_delete {clips}` |
| Select a range and delete | `edit.ripple_delete {ranges: [{from, to}]}` |
| Drop an asset from the bin | `edit.insert {asset, at, track?, overwrite?}` |
| Add / delete / rename / drag a track header | `track.add {kind}` / `track.remove {track}` / `track.rename` / `track.move {track, index}` |
| Speed / volume fields | `edit.set_speed {clip, speed: "3/2"}` / `edit.set_volume {clip, db}` |
| Undo / redo | `history.undo` / `history.redo` with `base_rev` |

- Send `base_rev: rev` (from your mirror) and your `actor`
  (`{"kind": "user", "id": "<user>"}`).
- Do not change your local copy first; apply the edit when its
  `project.changed` arrives (milliseconds later). Then UI and agent edits
  follow exactly one path.
- `stale_revision`: someone else edited first. Your mirror will catch up from
  events; re-evaluate the gesture against the new state and resend, or drop it.
- Previews: send the same call with `dry_run: true` to show what would
  happen (the patch, `created` / `modified` / `removed`) before committing,
  e.g. ghosting the clips a ripple delete would move.
- Compute times in frames on your side and send `f<n>` strings; avoid
  sending float seconds.

## 8. Import, analyze, export

- **Import:** `media.import {paths, base_rev}` with absolute paths from a
  file picker. The asset table (`doc.metadata.ve.assets`) is your media bin;
  asset rows have codec, size, rate, color and duration.
- **Silence removal:** `analyze.silence` (markers appear through events),
  show them, then `edit.ripple_delete {markers: "silence"}` inside a
  `history.begin_group` / `end_group` so it undoes in one step.
- **Export:** `render.export {preset}` blocks until done and other calls wait
  meanwhile, so show a blocking progress state (there is no progress
  percentage yet). Then move `result.path` where the user chose.

## 9. Working alongside an agent

The agent uses the same methods (usually through the `ve` CLI with
`--actor agent:<name>`). `history.list` shows each step's actor and
`open_groups` shows an agent in the middle of a multi-step change: show
"Agent is editing: remove silences" and avoid fighting it. Undo is one stack
for everyone; label undo menu items with the actor.
