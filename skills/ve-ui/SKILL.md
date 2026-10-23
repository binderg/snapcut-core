---
name: ve-ui
description: ui.* methods for human editors (media panel thumbnails, drag-to-push moves, preview render-ahead). Agents editing video do not need this; use ve-core.
---

# ve-ui

**Temporary home.** These methods exist for human UIs only and are meant to
move into a UI plugin once the plugin host (`ve-plugin`) exists. Today they
live in `ve-daemon::ui` behind the `ui-api` cargo feature (default on); an
agent-only build can drop them with `--no-default-features`. Do not build
agent workflows on them.

## `ui.thumbnail`

| Param | Default | |
| --- | --- | --- |
| `asset` | required | An asset ID from `media.assets` |
| `width` | 240 | 16 to 1920; height keeps the picture's aspect |

A poster frame of a video asset, for a media panel that shows files before
they are on the timeline. Taken 10% into the file (at most 5 s), which skips
most black openings. Audio-only assets fail with `invalid_params`.

```json
{"asset": "asset_4f1c", "mime": "image/jpeg", "data": "<base64>", "cached": false}
```

JPEGs are cached by file hash in `<cache>/ve/thumbs/`, so repeat calls are
instant and survive daemon restarts. Read-only; works on any open project.

## `ui.move_push`

What dropping a dragged clip does when the spot is too small. Takes
`base_rev`, `dry_run` and `actor` and returns the common edit result, like
every `edit.*` method.

| Param | Default | |
| --- | --- | --- |
| `clip` | required | |
| `to` | required | New timeline start |
| `track` | same track | Put this clip (not its linked clips) on another track of the same kind |
| `linked` | true | |

The clip and its linked clips move to `to`, leaving a gap where they were.
Content on the destination tracks from `to` moves right just far enough for
the clip to fit (not at all if it fits in a gap), and linked clips on other
tracks move with it, so sound stays in sync. A clip under `to` is cut there.
Agents use `edit.move` (overwrite, or `reorder` to swap places) instead.

## `ui.cache_hint`

Tell the engine where the preview's playhead is and what size it draws, so
it renders frames ahead into the frame cache (see `render.frame` in
ve-core) while the editor is idle. Send it when the playhead stops or jumps
and when the preview size changes; it is cheap to repeat.

| Param | Default | |
| --- | --- | --- |
| `at` | required | The playhead |
| `width` | required | The `width` the preview passes to `render.frame` |
| `format` | `jpeg` | The `format` it passes |
| `active` | true | False stops rendering ahead (preview hidden) |
| `chunk_frames` | | If the preview plays `render.range_preview` chunks on a grid of this many frames, build the chunks ahead too |

Order, anywhere on the timeline: first what edits changed, **newest edit
first** (a transition added a minute away still goes before the playhead
area): that edit's frames nearest the playhead first, then (with
`chunk_frames`) its playback chunks; then older edits the same way. Then the
10 s behind / 60 s ahead of the playhead, ahead first, and the chunks from
the one holding the playhead through that window; then the rest of the
timeline. An edit made while it works jumps to the front. Chunks
come from the cached frames and are kept by content (see `render.range_preview`),
so pressing play over them starts at once. It pauses while `render.frame` calls
are coming in (scrubbing) and while a job runs. A new hint or a new
revision restarts the walk; frames already cached are skipped. Returns the
current [status](#uicache_status).

## `ui.cache_status`

No params. What is cached at the hinted size:

```json
{"revision": 12, "width": 640, "frames": 2400, "cached": [[0, 310], [900, 1200]], "pending": 1790}
```

`cached` holds `[from, to)` frame ranges, for a render bar on the ruler.
The same object arrives as the `ui.cache` notification (after
`events.subscribe`) while rendering ahead, at most a few times a second.
