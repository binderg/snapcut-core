# edit.*

All of these edit the project: each takes `base_rev` (required), `dry_run`
and `actor`, and returns the common edit result (`revision`, `changed`,
`patch`, `created`, `modified`, `removed`, `duration`); see
[../editing/revisions-and-undo.md](../editing/revisions-and-undo.md). Times
are strings ([../model/time.md](../model/time.md)); clips and tracks are
addressed by ID (tracks also by name, `V1`).

**Linked clips.** A file with picture and sound is inserted as a video clip
and an audio clip sharing `metadata.ve.clip.link`. `split`, `trim`, `move`,
`delete`, `ripple_delete` (by clip), `set_speed` act on the whole link group
unless `linked: false`. That keeps sound in sync; turn it off for J/L cuts or
to change one side.

**Audio crossfades.** Every audio cut gets a 20 ms crossfade at render time.
Nothing to do in the edit.

## `edit.insert`

Put an asset (or part of it) on the timeline.

| Param | Default | |
| --- | --- | --- |
| `asset` | required | Asset ID |
| `at` | end of the target tracks | Timeline position |
| `source_in` | start of file | Media in-point, at the media's rate |
| `source_out` | end of file | Media out-point (exclusive) |
| `duration` | | Length instead of `source_out` |
| `track` | first video track (first audio track for audio-only assets) | Track for the picture |
| `audio_track` | first audio track | Track for the sound of an A/V asset |
| `video`, `audio` | true, true | Leave one out (`audio: false` for B-roll without its sound) |
| `overwrite` | false | Replace what is there instead of pushing it later |

Without `overwrite`, content after `at` on the **target tracks** moves right
by the clip's length (a clip under `at` is split first, together with its
linked clips). Linked clips stay in sync: any other track holding a partner
of a clip that moves is pushed too, from that partner's start (so J/L
offsets are kept). Tracks with nothing linked to the moving clips do not
move. Timeline markers move too when every track with later content moved.
`created` lists the new clip(s).

## `edit.split`

| Param | Default | |
| --- | --- | --- |
| `at` | required | Time to cut at |
| `clips` | all clips under `at` | Cut only these (and their linked clips) |
| `tracks` | all | With no `clips`, cut only on these tracks |
| `linked` | true | |

The left part keeps the clip's ID (`modified`); the right part is new
(`created`). With no `clips`, a time where no clip spans (exactly on an
existing cut, or in a gap) fails with `invalid_params` "no clip spans ...".

## `edit.trim`

| Param | Default | |
| --- | --- | --- |
| `clip` | required | |
| `in` | | Signed change of the in-point: `+15f` starts later (shorter), `-15f` earlier (longer) |
| `out` | | Signed change of the out-point: `-1s` ends earlier, `+10f` later |
| `roll` | false | Move the edit point instead of rippling |
| `linked` | true | |

- **Ripple** (default): the clip gets shorter or longer and everything after
  it on its tracks moves to match.
- **Roll**: the neighbour gives or takes the time, nothing else moves.
- **J/L cut**: roll the audio side only: `{"clip": <audio clip>, "out": "+10f", "roll": true, "linked": false}`
  lets the outgoing sound run 10 frames over the next shot (L cut).

A clip can never extend past its media (`available_range`); trying fails
with `invalid_params`.

## `edit.move`

| Param | Default | |
| --- | --- | --- |
| `clip` | required | |
| `to` | required | New timeline start |
| `track` | same track | Move this clip (not its linked clips) to another track of the same kind |
| `linked` | true | |

Overwrites whatever is at the destination and leaves a gap where the clip was.

## `edit.ripple_delete`

Remove time and close the gap. Give any of:

| Param | | |
| --- | --- | --- |
| `ranges` | `[{"from": "...", "to": "..."}]` | Timeline ranges; overlapping ones merge |
| `markers` | `"silence"` | The ranges of every timeline marker with this name |
| `clips` | `[ids]` | These clips (and linked ones), each gap closed on its own track |
| `tracks` | all tracks | Limit ranges to these tracks (all tracks keeps A/V in sync) |
| `linked` | true | |

Clips partly inside a range are trimmed. Timeline markers inside a range are
removed and later ones move left, but only when the cut covers every track
that has content after it (otherwise markers stay where they are). The usual talking-head cut is `analyze.silence` then
`edit.ripple_delete {"markers": "silence"}`.

## `edit.delete`

`{"clips": [...], "linked": true}`. Removes clips and leaves gaps; nothing moves.

## `edit.set_speed`

| Param | Default | |
| --- | --- | --- |
| `clip` | required | |
| `speed` | required | Exact: `3/2`, `1.5`, `0.5`, `2x` |
| `ripple` | true | Later content moves to fit. False: a slower clip overwrites what follows |
| `linked` | true | |

Keeps the same media, so the clip's timeline length changes (`duration /
speed`). Writes OTIO's `LinearTimeWarp` too. Audio is pitch-corrected
(`atempo`).

## `edit.set_volume`

`{"clip": id, "db": -6, "linked": true}`. Sets (not adds) the gain of the
clip's audio, -100 to +30 dB. On a video clip it applies to its linked audio.

## `edit.unlink`

`{"clip": id}`. Breaks the link between a clip and its partners (a video clip
and its audio), so later edits touch each alone. Nothing moves. Fails if the
clip is not linked. Use it before an edit that should hit only one side; for
a single J/L cut, `linked: false` on that edit is enough. There is no way to
re-link yet.

## `edit.normalize`

Loudness target for the whole mix. Documented with the measurements:
[measure.md](measure.md#editnormalize).
