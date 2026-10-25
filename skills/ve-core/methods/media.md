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

**Still images** (PNG, JPEG, WebP, BMP, TIFF, single-frame GIF) import like
any file. Their `media.video.still` is true and `media.duration` is zero:
an image has no length of its own, its clip decides how long it shows (see
`edit.insert`). Transparency (PNG, WebP) is kept, so a logo on an upper
video track shows the tracks below through its clear parts. Animated GIFs
are video.

## `media.remove` (edits)

| Param | Type | |
| --- | --- | --- |
| `assets` | [string] | Asset IDs to remove |
| + `base_rev`, `dry_run`, `actor` | | |

All-or-nothing. Refused (`invalid_params`) for an unknown ID, or for an
asset any clip still uses; the message lists those clip IDs, so delete the
clips first (`edit.delete` or `edit.ripple_delete`). Only the table row
goes; the media file on disk is never touched. Undo brings the row back.

```json
{"revision": 5, "changed": true, "dry_run": false, "patch": [...],
 "removed": ["asset_834f84"]}
```

## `media.assets`

No params. `{"revision": 4, "assets": [ ...asset rows... ]}`, the same rows
as `metadata.ve.assets` in the document.

## `media.extract_audio`

Decode an asset's first audio stream, whole, to a plain file in the
engine's cache, so plugins (speech to text, beat detection) never need a
decoder of their own.

| Param | Type | |
| --- | --- | --- |
| `asset` | string | The asset |
| `sample_rate` | integer | Default 16000 (what speech models expect); 8000 to 192000 |
| `channels` | integer | 1 (default, a mono downmix) or 2 |
| `format` | `wav` \| `flac` | Default `wav` (16-bit PCM). `flac` is lossless and about a third the size, for uploads |

```json
{"path": "C:\Users\me\AppData\Local\ve\cache\ve\projects\6b69...\audio\ba7c...-16000-1.flac",
 "format": "flac", "sample_rate": 16000, "channels": 1, "duration": "602.433s", "cached": false}
```

The file is kept by the file's content hash and the settings, so a second
call answers at once (`cached: true`). Read it; do not move or change it.
`cache.clear` with kind `audio` deletes these files. The asset must have
audio and be a local file (`invalid_params` otherwise).
