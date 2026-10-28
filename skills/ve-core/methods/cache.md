# cache.*

The engine keeps work it can redo in the OS cache folder, never in the
project folder: rendered frames, playback chunks, exports not collected yet,
measurement results and audio extracted for plugins. Frames and previews are capped (2 GB and 1 GB per
project, oldest dropped first), but a few projects add up. These methods show
and free that space. Everything listed here is rebuilt the next time it is
needed, so clearing never changes the project. Undo history, the project lock
and the daemon's own files are never touched.

Kinds:

| Kind | What | Where |
| --- | --- | --- |
| `frames` | Frames from `render.frame` and render-ahead | `<project cache>/frames/` |
| `previews` | Playback chunks from `render.range_preview` | `<project cache>/previews/` |
| `renders` | Exports waiting to be collected, and leftovers from failed or cancelled ones | `<project cache>/renders/` |
| `measure` | `measure.*` results for this project's media files (shared with other projects that use the same files) | `<cache>/measure/<file hash>/` |
| `audio` | Audio files made by `media.extract_audio` for plugins | `<project cache>/audio/` |
| `audio_fx` | Clips processed by plugin audio effects (`runtime: "process"`) | `<project cache>/audio_fx/` |

## `cache.status`

`{}`. How much each kind holds:

```json
{"entries": [
  {"kind": "frames", "bytes": 1412000000, "files": 8213, "paths": [".../frames"]},
  {"kind": "previews", "bytes": 310000000, "files": 96, "paths": [".../previews"]},
  {"kind": "renders", "bytes": 0, "files": 0, "paths": [".../renders"]},
  {"kind": "measure", "bytes": 48000, "files": 6, "paths": [".../measure/9f2c..."]}
 ],
 "total_bytes": 1722048000, "cleared": false}
```

## `cache.clear`

`{kinds?, dry_run?}`. Deletes the given kinds (all when `kinds` is empty or
absent) and answers like `cache.status` with the sizes that were freed and
`"cleared": true`. With `dry_run: true` nothing is deleted and `cleared` is
false.

- Refused (`invalid_params`) while any job is queued or running, since
  exports and previews write into these folders. Wait for it or
  `jobs.cancel` it.
- Clearing `renders` deletes a background export's file if you have not
  copied it out yet. The CLI's `ve export` moves its file to `-o` itself, so
  this only matters for `background: true` exports.
- Clearing `frames` also empties the daemon's in-memory frame cache, so the
  next frames render fresh (the first ones are slower).

CLI: `ve cache` shows sizes, `ve cache clear [frames,previews,...] [--dry-run]`.
