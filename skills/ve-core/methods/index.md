# Methods

Every method the engine has. "Edits" means it takes `base_rev` / `dry_run` /
`actor`, returns a patch and emits `project.changed`
([../editing/revisions-and-undo.md](../editing/revisions-and-undo.md)).
"No project" means it works before any project is open. Exact schemas:
`system.describe`.

| Method | Edits | Does | Details |
| --- | --- | --- | --- |
| `system.version` | | Engine, protocol and schema versions (no project) | [system.md](system.md) |
| `system.describe` | | OpenRPC document for every method (no project) | [system.md](system.md) |
| `daemon.info` | | Running daemon: pid, project, socket, WebSocket URL | [system.md](system.md) |
| `daemon.stop` | | Stop the daemon | [system.md](system.md) |
| `events.subscribe` | | Receive `project.changed` on this connection | [system.md](system.md) |
| `project.init` | | Create a project folder and open it (no project) | [project.md](project.md) |
| `project.open` | | Open (and lock) a project folder | [project.md](project.md) |
| `project.save` | | Write the project to disk (edits already save) | [project.md](project.md) |
| `project.get` | | The whole project as OTIO JSON, with its revision | [project.md](project.md) |
| `project.summary` | | Compact text of tracks, clips, markers, assets | [project.md](project.md) |
| `media.probe` | | What a media file contains (no project) | [media.md](media.md) |
| `media.import` | yes | Add files to the asset table | [media.md](media.md) |
| `media.assets` | | The asset table | [media.md](media.md) |
| `edit.insert` | yes | Put an asset (or part of it) on the timeline | [edit.md](edit.md) |
| `edit.split` | yes | Cut clips in two at a time | [edit.md](edit.md) |
| `edit.trim` | yes | Move a clip's in or out point (ripple or roll) | [edit.md](edit.md) |
| `edit.move` | yes | Move a clip in time or to another track | [edit.md](edit.md) |
| `edit.ripple_delete` | yes | Remove ranges, marker ranges or clips and close the gap | [edit.md](edit.md) |
| `edit.delete` | yes | Remove clips, leave gaps | [edit.md](edit.md) |
| `edit.set_speed` | yes | Change a clip's speed (exact ratio) | [edit.md](edit.md) |
| `edit.set_volume` | yes | Set a clip's audio gain | [edit.md](edit.md) |
| `edit.normalize` | yes | Hit a loudness target with one gain on the audio clips | [measure.md](measure.md) |
| `marker.add` | yes | Add a timeline marker | [markers.md](markers.md) |
| `marker.remove` | yes | Remove markers by ID or name | [markers.md](markers.md) |
| `history.undo` | yes | Undo the last step | [history.md](history.md) |
| `history.redo` | yes | Redo the last undone step | [history.md](history.md) |
| `history.list` | | Undo and redo steps with their actors | [history.md](history.md) |
| `history.begin_group` | | Start an undo group for an actor | [history.md](history.md) |
| `history.end_group` | | End it | [history.md](history.md) |
| `render.frame` | | One frame as JPEG/PNG (base64) | [render.md](render.md) |
| `render.contact_sheet` | | A grid of frames from a range | [render.md](render.md) |
| `render.range_preview` | | Low-res MP4 of a range, in the engine cache | [render.md](render.md) |
| `render.export` | | Final MP4 with a preset, in the engine cache | [render.md](render.md) |
| `render.presets` | | Export presets (no project) | [render.md](render.md) |
| `analyze.silence` | yes | Find quiet stretches; writes `silence` markers | [analyze.md](analyze.md) |
| `analyze.scenes` | yes | Find hard picture cuts; writes `scene` markers | [analyze.md](analyze.md) |
| `measure.loudness` | | LUFS, loudness range, true peak | [measure.md](measure.md) |
| `measure.levels` | | RMS, peak, clipping, DC, noise floor, balance, phase | [measure.md](measure.md) |
| `measure.waveform` | | Min/max/RMS peaks for drawing | [measure.md](measure.md) |
| `measure.pitch` | | Pitch: median, range, note, contour | [measure.md](measure.md) |

The `ve` CLI has a subcommand for most of these (`ve trim`, `ve measure
loudness`, ...) and `ve call <method> '<json>'` for any of them; `--json`
gives the raw result. See the `video-edit` skill for the CLI.
