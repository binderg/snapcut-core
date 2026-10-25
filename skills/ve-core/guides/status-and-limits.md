# Status and limits

What the core does not do yet, as of protocol 1 / engine 0.1.0-rc.1, so
clients can plan around it. When something here gets built, this file
changes.

## Not built yet

| Missing | What to do meanwhile |
| --- | --- |
| **Imports as jobs** | `media.import` of big files still holds the project until done (renders, analysis and measurements run as jobs: [../methods/jobs.md](../methods/jobs.md)) |
| **Video playback stream** / serving media over HTTP | `render.frame` for stills (cached by content, ~10 ms on a hit); `render.range_preview` MP4 chunks; or a desktop shell plays source files directly |
| **Track lock** | Hide and mute exist (`track.set {enabled}`); nothing stops edits on a track yet |
| **Effects, transitions and transforms in contact sheets** | Frames, range previews and exports use the GPU automatically when the range needs it; `render.contact_sheet` still shows plain cuts and unprocessed clips, with a warning. Use `render.frame` at the times you want to see |
| **Anti-aliased edges** on rotated clips | Edges of a rotated picture are hard; fine for small angles |
| **Titles, text, captions** | Planned: text clips drawn by the core's GPU text renderer, captions on the same renderer. Meanwhile, make the title as a PNG and import it (stills keep transparency) |
| **Shapes, masks, LUTs, audio effects; `wasm` and `ofx` effect runtimes** | Write a `wgsl` effect ([writing-effects.md](writing-effects.md)); animated templates will come as generator plugins |
| **Video measurements** (brightness, saturation, black or frozen frames, blur) | Not available; only audio measurements |
| **Per-clip measurements** (`clip` param, a report of every clip) | Measure an asset (whole file) or a timeline range |
| **Proxies** for heavy footage | Frames come from originals, so long-GOP phone footage seeks slowly |
| **Plugins** (and anything AI: transcription, captions, keying) | No plugin host yet; the core will never bundle models |
| **More than one project per daemon** | One daemon per project folder; connect to each separately |
| **Remote access** | Loopback only, by design for now |

## Gotchas

- **Media paths** are absolute `file://` URLs. Moving or renaming a source
  file breaks clips that use it (`render_failure` naming the clip). There is
  no relink method yet.
- **Exports land in the cache** and are deleted after a day unless moved.
- **Handoff to other editors** (export the `.otio` to Resolve, Premiere,
  Avid): cuts, gaps, markers and speed (as `LinearTimeWarp`) survive; anything
  only in `metadata.ve` (volume, link groups, effects) is ignored by them.
- **Time** in the document is OTIO doubles; send times as timecode or
  frames, never as float seconds you computed.
- **External edits** to `project.otio` are picked up on the next call and
  drop the undo history.
- **Windows**: daemons run from a copy of `ve.exe` (in `<OS cache>/ve/bin/`)
  so rebuilding or upgrading `ve` is never blocked by a running daemon.
- **One undo stack** per project for all actors.
