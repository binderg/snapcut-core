---
name: ve-ui
description: ui.* methods for human editors (media panel thumbnails). Agents editing video do not need this; use ve-core.
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
