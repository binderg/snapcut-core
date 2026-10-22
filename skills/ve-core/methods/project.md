# project.*

A daemon serves exactly one project, opened when it starts. Over a daemon
connection you normally just call `project.get`; `project.open` with the
same folder is harmless (returns its info), a different folder is an error.

All of `init` / `open` / `save` return:

```json
{"path": "C:\\Users\\me\\Videos\\my-edit", "name": "my-edit", "revision": 4, "ids_added": 0}
```

`ids_added`: stable IDs given on open to items that had none (a file from
another tool). They are saved immediately.

## `project.init` (no project needed)

| Param | Type | Default | |
| --- | --- | --- | --- |
| `path` | string | required | Folder to create the project in. May exist, must not hold a project |
| `fps` | string | required | `30`, `29.97`, `23.976`, `25`, `60`, or exact `30000/1001` |
| `width`, `height` | int | required | Output size |
| `name` | string | folder name | |
| `sample_rate` | int | 48000 | |
| `color_space` | string | `bt709` | |

Creates `project.otio` with tracks `V1` (video) and `A1` (audio), and opens
it in the calling process. From the CLI: `ve init my-edit --fps 30 --size
1920x1080` (then any project command starts the daemon).

## `project.open`

`{"path": "<folder>"}`. Opens and locks it. `project_locked` (-31003) if
another engine process has it.

## `project.save`

No params. Writes `project.otio`. Rarely needed: every edit saves.

## `project.get`

No params. `{"revision": 4, "timeline": { ...OTIO Timeline.1... }}`. The
document is described in [../model/project.md](../model/project.md).

## `project.summary`

No params. `{"revision": 4, "text": "..."}`, a compact view for agents and
logs:

```
"demo"  rev 4  160x120  25fps  48000 Hz  bt709
duration 00:00:04:00 (100f)  2 tracks  4 clips  1 markers
assets:
  asset_834f84  source  tone.mkv  00:00:04:00  160x120 25fps h264  pcm_s16le 48000Hz 2ch
"V1" trk_a2027a video  ends 00:00:04:00
  00:00:00:00     50f  clip_f4b280 "tone.mkv"  src 00:00:00:00 asset_834f84
  00:00:02:00     50f  clip_938690 "tone.mkv"  src 00:00:02:00 asset_834f84
"A1" trk_a80c3e audio  ends 00:00:04:00
  00:00:00:00     50f  clip_fc7d84 "tone.mkv"  src 00:00:00:00 asset_834f84
  00:00:02:00     50f  clip_20c9b6 "tone.mkv"  src 00:00:02:00 asset_834f84
markers:
  00:00:01:00     10f  mk_1b9862 "chapter" red user
```

Not meant for parsing; use `project.get` in programs.
