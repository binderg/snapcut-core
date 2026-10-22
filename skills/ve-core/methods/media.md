# media.*

## `media.probe` (no project needed)

`{"path": "<local file>"}`. Relative paths resolve against the engine's
working directory, so send absolute paths.

```json
{"url": "file:///C:/media/talk.mp4",
 "media": { ...same shape as an asset's "media"... },
 "ffprobe": { ...ffprobe's own -show_format -show_streams JSON... }}
```

`media` is the compact summary (duration, video size/rate/rotation/vfr/color,
first audio stream); see the asset fields in
[../model/project.md](../model/project.md#asset). `ffprobe` is everything
ffprobe said, for anything the summary leaves out. Errors: missing file or
not media -> `invalid_params` with the reason.

## `media.import` (edits)

| Param | Type | |
| --- | --- | --- |
| `paths` | [string] | Local files to add (absolute paths) |
| + `base_rev`, `dry_run`, `actor` | | |

All-or-nothing: if any file fails, nothing is added. Files are hashed
(blake3); content already in the table is not added again.

```json
{"revision": 1, "changed": true, "dry_run": false, "patch": [...],
 "assets": [{"id": "asset_834f84", "url": "file:///C:/media/tone.mkv", "existing": false, "media": {...}}]}
```

`assets` has one entry per input path, in order; `existing: true` means it
was already imported (same content, maybe a different path).

Importing does not put anything on the timeline; follow with `edit.insert`.
No proxies are made yet.

## `media.assets`

No params. `{"revision": 4, "assets": [ ...asset rows... ]}`, the same rows
as `metadata.ve.assets` in the document.
