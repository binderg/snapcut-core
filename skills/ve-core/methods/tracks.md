# track.*

A project can have any number of video and audio tracks. A new project
starts with `V1` and `A1`. Tracks are addressed by ID (`trk_50b7f3`) or name
(`V2`).

**Positions.** `index` counts among tracks of the same kind: 0 is the first.
For video that is the stacking order: index 0 is the bottom layer and each
later video track draws on top of the ones before it. All audio tracks mix,
so an audio track's index only sets its place in lists. Adding an audio
track never changes a video track's index, and the other way round.

The edit methods (`base_rev`, `dry_run`, `actor`, a patch, a
`project.changed` event, undoable) return:

```json
{"track": {"id": "trk_50b7f3", "name": "V2", "kind": "video", "index": 1, "clips": 0, "enabled": true},
 "tracks": [{"id": "trk_ee8c2a", "name": "V1", "kind": "video", "index": 0, "clips": 1, "enabled": true},
            {"id": "trk_50b7f3", "name": "V2", "kind": "video", "index": 1, "clips": 0, "enabled": true},
            {"id": "trk_0b54dd", "name": "A1", "kind": "audio", "index": 0, "clips": 1, "enabled": true}],
 "revision": 3, "changed": true, "patch": [...]}
```

`track` is the track acted on (as it was, for a removal); `tracks` is every
track afterwards, in document order; `clips` counts the items on a track;
`enabled` is false for a hidden video track or a muted audio track.

## `track.add` (edit)

| Param | Default | |
| --- | --- | --- |
| `kind` | required | `video` or `audio` |
| `name` | next free `V<n>` / `A<n>` | Must not match another track's name or ID |
| `index` | the end (video: the top layer) | Position among tracks of its kind |

## `track.remove` (edit)

`{"track": "V2", "force": false}`. A track with clips is refused unless
`force: true`, which deletes its clips with it (listed in `removed`). Clips
linked to them on other tracks stay. Removing a project's last video or audio
track is allowed; inserts that need one then fail until you add one.

## `track.rename` (edit)

`{"track": "V2", "name": "Titles"}`. Names must be unique and not look like
another track's ID.

## `track.move` (edit)

`{"track": "V3", "index": 0}`. New position among tracks of its kind
(`index 0` on a video track sends it to the bottom).

## `track.set` (edit)

`{"track": "V2", "enabled": false}` hides a video track (renders do not
draw it) or mutes an audio track (renders do not mix it). `true` brings it
back. Edits still apply to its clips. This is OTIO's own `enabled` flag, so
other editors see it too.

## `track.list`

No params. `{"revision": 8, "tracks": [ ...as above... ]}`. The same data is
in the document (`tracks.children`); this is a shortcut for agents and tools.

**Layers.** Each upper video track draws over the ones below. Where an
upper clip covers the whole canvas the tracks below do not show; use
`edit.set_transform` ([edit.md](edit.md#editset_transform)) to shrink,
move, crop or fade it so they do.

Not yet: locking, solo, and track effects.
