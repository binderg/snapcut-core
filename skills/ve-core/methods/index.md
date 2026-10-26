# Methods

Every method the engine has. "Edits" means it takes `base_rev` / `dry_run` /
`actor`, returns a patch and emits `project.changed`
([../editing/revisions-and-undo.md](../editing/revisions-and-undo.md)).
"No project" means it works before any project is open. "Job" means it can
run in the background with `background: true` ([jobs.md](jobs.md)). Exact
schemas: `system.describe`.

| Method | Edits | Does | Details |
| --- | --- | --- | --- |
| `system.version` | | Engine, protocol and schema versions (no project) | [system.md](system.md) |
| `system.describe` | | OpenRPC document for every method (no project) | [system.md](system.md) |
| `daemon.info` | | Running daemon: pid, project, socket, WebSocket URL | [system.md](system.md) |
| `daemon.stop` | | Stop the daemon | [system.md](system.md) |
| `events.subscribe` | | Receive `project.changed` and job events on this connection | [system.md](system.md) |
| `jobs.list` | | Background jobs, without results | [jobs.md](jobs.md) |
| `jobs.get` | | One job: state, progress, result or error | [jobs.md](jobs.md) |
| `jobs.cancel` | | Stop a job | [jobs.md](jobs.md) |
| `cache.status` | | Size of the project's rebuildable caches | [cache.md](cache.md) |
| `cache.clear` | | Delete them (all, or some kinds) | [cache.md](cache.md) |
| `project.init` | | Create a project folder and open it (no project) | [project.md](project.md) |
| `project.open` | | Open (and lock) a project folder | [project.md](project.md) |
| `project.save` | | Write the project to disk (edits already save) | [project.md](project.md) |
| `project.get` | | The whole project as OTIO JSON, with its revision | [project.md](project.md) |
| `project.summary` | | Compact text of tracks, clips, markers, assets | [project.md](project.md) |
| `media.probe` | | What a media file contains (no project) | [media.md](media.md) |
| `media.import` | yes | Add files to the asset table | [media.md](media.md) |
| `media.remove` | yes | Remove unused assets from the table | [media.md](media.md) |
| `media.assets` | | The asset table | [media.md](media.md) |
| `media.extract_audio` | | An asset's audio as a WAV or FLAC file in the engine cache, for plugins | [media.md](media.md) |
| `transcript.set` | | Store an asset's transcript (words, speakers, provenance) | [transcript.md](transcript.md) |
| `transcript.get` | | An asset's transcript, or the words on the timeline (words or text) | [transcript.md](transcript.md) |
| `transcript.list` | | Assets that have a transcript | [transcript.md](transcript.md) |
| `transcript.remove` | | Delete an asset's transcript | [transcript.md](transcript.md) |
| `plugin.list` | | Installed plugins, their commands and secrets (no project) | [plugin.md](plugin.md) |
| `edit.insert` | yes | Put an asset (or part of it) on the timeline | [edit.md](edit.md) |
| `edit.split` | yes | Cut clips in two at a time | [edit.md](edit.md) |
| `edit.trim` | yes | Move a clip's in or out point (ripple or roll) | [edit.md](edit.md) |
| `edit.move` | yes | Move a clip in time or to another track | [edit.md](edit.md) |
| `edit.ripple_delete` | yes | Remove ranges, marker ranges or clips and close the gap | [edit.md](edit.md) |
| `edit.delete` | yes | Remove clips, leave gaps | [edit.md](edit.md) |
| `edit.set_speed` | yes | Change a clip's speed (exact ratio) | [edit.md](edit.md) |
| `edit.set_volume` | yes | Set a clip's audio gain | [edit.md](edit.md) |
| `edit.set_transform` | yes | Position, scale, rotate, fade or crop a video clip's picture (keyframes) | [edit.md](edit.md) |
| `edit.unlink` | yes | Break a clip's link with its partners | [edit.md](edit.md) |
| `edit.normalize` | yes | Hit a loudness target with one gain on the audio clips (job) | [measure.md](measure.md) |
| `track.add` | yes | Add a video or audio track (any number of each) | [tracks.md](tracks.md) |
| `track.remove` | yes | Remove a track (with its clips only when forced) | [tracks.md](tracks.md) |
| `track.rename` | yes | Rename a track | [tracks.md](tracks.md) |
| `track.move` | yes | Change a track's position (video: stacking order) | [tracks.md](tracks.md) |
| `track.set` | yes | Hide a video track or mute an audio track | [tracks.md](tracks.md) |
| `track.list` | | Every track with kind, position, clip count | [tracks.md](tracks.md) |
| `fx.list` | | Effects and transitions available, with param schemas | [effects.md](effects.md) |
| `fx.define` | | Validate (naga + GPU test render) and save a project effect | [effects.md](effects.md) |
| `effect.add` | yes | Put an effect on a video clip | [effects.md](effects.md) |
| `effect.set` | yes | Change params, keyframes or stack position | [effects.md](effects.md) |
| `effect.remove` | yes | Remove an effect | [effects.md](effects.md) |
| `transition.add` | yes | Put a transition on a cut | [effects.md](effects.md) |
| `transition.set` | yes | Change a transition's length, alignment, kind or params | [effects.md](effects.md) |
| `transition.remove` | yes | Back to a plain cut | [effects.md](effects.md) |
| `marker.add` | yes | Add a timeline marker | [markers.md](markers.md) |
| `marker.remove` | yes | Remove markers by ID or name | [markers.md](markers.md) |
| `history.undo` | yes | Undo the last step | [history.md](history.md) |
| `history.redo` | yes | Redo the last undone step | [history.md](history.md) |
| `history.list` | | Undo and redo steps with their actors | [history.md](history.md) |
| `history.begin_group` | | Start an undo group for an actor | [history.md](history.md) |
| `history.end_group` | | End it | [history.md](history.md) |
| `render.frame` | | One frame as JPEG/PNG (base64) | [render.md](render.md) |
| `render.layers` | | What is on screen and heard: layers and audio at a frame, or which tracks show and are heard over a range | [render.md](render.md) |
| `render.contact_sheet` | | A grid of frames from a range (job) | [render.md](render.md) |
| `render.range_preview` | | Low-res MP4 of a range, in the engine cache (job) | [render.md](render.md) |
| `render.export` | | Final MP4 with a preset, in the engine cache (job) | [render.md](render.md) |
| `render.presets` | | Export presets (no project) | [render.md](render.md) |
| `analyze.silence` | yes | Find quiet stretches; writes `silence` markers (job) | [analyze.md](analyze.md) |
| `analyze.scenes` | yes | Find hard picture cuts; writes `scene` markers (job) | [analyze.md](analyze.md) |
| `measure.loudness` | | LUFS, loudness range, true peak (job) | [measure.md](measure.md) |
| `measure.levels` | | RMS, peak, clipping, DC, noise floor, balance, phase (job) | [measure.md](measure.md) |
| `measure.waveform` | | Min/max/RMS peaks for drawing (job) | [measure.md](measure.md) |
| `measure.pitch` | | Pitch: median, range, note, contour (job) | [measure.md](measure.md) |

Plugins add methods of their own (`deepgram.transcribe`); they are listed by
`plugin.list` and in `system.describe` with `"x-plugin"`, not here
([plugin.md](plugin.md)).

The `ve` CLI has a subcommand for most of these (`ve trim`, `ve measure
loudness`, ...) and `ve call <method> '<json>'` for any of them; `--json`
gives the raw result. See the `video-edit` skill for the CLI.
