---
name: ve-ui
description: ui.* methods for human editors (media panel thumbnails, drag-to-push moves). Agents editing video do not need this; use ve-core.
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
