---
name: video-edit
description: Edit video with the `ve` CLI (headless video engine) - cut talking heads, podcasts, phone clips and screen recordings - remove silences, trim, split, reorder, change speed, make J/L cuts, check cuts with frames and contact sheets, and export. Use when asked to edit, cut, tighten, trim or export a video project, or when a folder has a project.otio.
---

# Editing video with `ve`

`ve` edits a project folder (`project.otio`, OpenTimelineIO) through one
command per operation. You never touch the file yourself. Every edit is
undoable, and you can preview any edit with `--dry-run` first.

Always pass `--actor agent:<your-name>` on commands that change the project
so the user can see and undo your edits (`ve history`).

## 1. Look before you cut

```bash
ve summary                  # tracks, clips, markers, with IDs and timecodes
ve summary --json           # same, plus the revision, for scripts
```

A summary line reads `00:00:12:10   97f  clip_53a9ee "talk.mp4"  src 00:00:13:05 asset_94b8`:
timeline position, length in frames, clip ID, name, where in the source it
starts, and the asset it plays. Everything is addressed by ID (`clip_…`,
`asset_…`, `trk_…`, `mk_…`), never by position in a list. Tracks can also be
named (`V1`, `A1`).

New project and media:

```bash
ve init my-edit --fps 29.97 --size 1920x1080   # match the footage's rate
cd my-edit
ve probe ../IMG_4410.MOV                        # what a file contains
ve import ../IMG_4410.MOV                       # adds asset_…; same file twice is a no-op
ve --actor agent:claude insert asset_94b8       # video on V1 + linked audio on A1, appended
```

## 2. Times

Every time argument takes any of:

| Form | Example | Meaning |
| --- | --- | --- |
| SMPTE | `00:01:12:12` | hh:mm:ss:frames at the project rate |
| Drop-frame | `"00:01:12;12"` | 29.97/59.94 only. **Quote it** (`;` ends a shell command). `00:01:00;00` and `;01` don't exist (dropped labels). |
| Frames | `f2172` or `2172f` | |
| Seconds | `72.4s` | exact decimal, snapped to the nearest frame |
| Offsets | `+15f`, `-1s` | for `trim --in/--out` |

All edits snap to whole project frames.

## 3. The edit loop

1. `ve summary` (and markers from analyzers) to see what's there.
2. Plan. For anything destructive, run it with `--dry-run` first: it prints
   what would change and the JSON Patch, and changes nothing.
3. Wrap a multi-step change in an undo group so the user can undo it in one go:
   `ve --actor agent:claude group begin "remove silences"` … `ve --actor agent:claude group end`.
4. Check your cuts (section 6). Don't report success without looking.
5. Report what changed (durations before/after, what was removed) and how
   to undo it: `ve undo` removes the last step or the whole group.

If a command fails with exit code 3 (`stale_revision`), the project changed
under you (the user edited it). Run `ve summary` again, re-plan, retry.
Exit codes: 0 ok, 2 bad arguments (the message says what), 3 stale revision,
4 render failure (often missing media), 6 project open in another process.

## 4. Commands

| Task | Command |
| --- | --- |
| Remove a time range on all tracks, closing the gap | `ve ripple-delete --from 00:02:10:00 --to 00:02:14:15` |
| Remove several ranges in one step | `ve ripple-delete --range 00:02:10:00..00:02:14:15 --range f4000..f4100` |
| Remove clips and close the gap | `ve ripple-delete clip_7f3a` |
| Remove clips, leave a gap | `ve delete clip_7f3a` |
| Cut at a time (every clip there) | `ve split --at 00:01:12:12` |
| Cut one clip (and its linked audio) | `ve split clip_7f3a --at 00:01:12:12` |
| Shorten / lengthen a clip, shifting what follows | `ve trim clip_7f3a --in +15f --out -1s` |
| Move a cut point without shifting anything | `ve trim clip_7f3a --out +10f --roll` |
| Move a clip (overwrites the destination) | `ve move clip_7f3a --to 00:00:42:00 [--track V2]` |
| Swap a clip with the one before it | `ve move clip_7f3a --to <other clip's start> --reorder` |
| Speed | `ve speed clip_7f3a 3/2` (exact ratios; `1/2` is slow motion) |
| Gain | `ve volume clip_7f3a -6` (dB) |
| Unlink video from its audio (later edits touch one side) | `ve unlink clip_7f3a` |
| Loudness to a target (one gain on every audio clip) | `ve normalize --target -14` (-16 podcasts, -23 broadcast) |
| Insert part of a source | `ve insert asset_94b8 --in 00:03:10:00 --out 00:03:25:00 --at 00:01:00:00` |
| Insert over existing material | `… --overwrite` |
| Add a track (any number of each kind) | `ve track add video` (makes `V2`, drawn on top of `V1`), `ve track add audio --name Music` |
| B-roll without its sound, over the main shot | `ve track add video`, then `ve insert asset_b --no-audio --track V2 --at 00:00:30:00 --overwrite` |
| Tracks: list, rename, reorder, remove | `ve track list`, `ve track rename V2 Broll`, `ve track move V2 0`, `ve track rm V2 [--force]` |
| Effects available | `ve fx list` |
| Effect on a clip | `ve effect add clip_7f3a core.color/exposure --param stops=0.5`; change with `ve effect set fx_1a2b --param stops=1`, animate with `--key stops@0s=0 --key stops@2s=1`; `ve effect rm fx_1a2b` |
| Transition on a cut | `ve transition add --at 00:00:42:10 [--type core.transition/wipe] [--duration 15f]`; `ve transition rm tr_1a2b` |
| Markers | `ve marker add --at 00:01:00:00 --name chapter`, `ve marker rm --name silence` |
| Undo / redo / what happened | `ve undo`, `ve redo`, `ve history` |

**Linked clips.** Inserting a file with picture and sound makes a video clip
and an audio clip that share a link. `split`, `trim`, `move`, `speed`,
`delete` and `ripple-delete clip_…` act on both, which keeps them in sync.
Add `--unlinked` to act on one clip only.

**J and L cuts.** Roll one side of the cut with `--unlinked`:

```bash
# L cut: the outgoing shot's sound runs 10 frames over the next picture.
ve trim <outgoing audio clip> --out +10f --roll --unlinked
# J cut: the incoming shot's sound starts 10 frames before its picture.
ve trim <outgoing audio clip> --out -10f --roll --unlinked
```

Every audio cut gets a 20 ms crossfade automatically, so cuts don't click.

## 5. Recipe: tighten a talking head

```bash
ve summary
ve --actor agent:claude analyze silence --min 0.5s --pad 0.1s
#   -35dB default; use --threshold -30dB for a noisy room, -45dB for a quiet one.
#   --pad keeps 0.1 s of air at each side so speech doesn't sound clipped.
ve summary                                  # silences appear as purple `silence` markers
ve --actor agent:claude ripple-delete --markers silence --dry-run
ve --actor agent:claude group begin "remove silences"
ve --actor agent:claude ripple-delete --markers silence
ve --actor agent:claude group end
ve sheet --count 12 -o sheet.jpg            # look at it
```

Then deal with what silence detection can't see: false starts, repeated
takes ("let me say that again"), long ums. Find them by looking at frames
around suspicious spots, or with markers from a transcription plugin if one
is installed (core ships no speech recognition). Remove each with
`ve ripple-delete --from … --to …`.

Too choppy? Undo the group and re-run with a longer `--min` (0.8s) or more
`--pad`. Re-running an analyzer replaces its old markers.

`ve analyze scenes` marks hard cuts in the picture (`scene` markers) - useful
for finding where a screen recording changes or where clips were joined.

## 5b. Measure the sound

Measurements change nothing. Each works on the timeline mix (optionally
`--from/--to`) or on one source file (`--asset asset_…`, cached after the
first run). `--resolution 1s` adds a value per second, to find where
something changes.

```bash
ve measure loudness                      # integrated LUFS, loudness range, true peak
ve measure levels --asset asset_94b8     # per channel: RMS, peak, clipping, DC; noise floor;
                                         # stereo balance and correlation
ve measure pitch --asset asset_94b8      # median pitch, range, nearest note
ve measure waveform --buckets 800 --json # min/max/RMS arrays for drawing
```

What the numbers mean:
- **Loudness**: online video and podcasts sit near -14 to -16 LUFS. Fix it
  with `ve normalize --target -14`, which measures, then adds one gain to the
  audio clips and keeps the true peak under -1 dBTP. If it says the gain was
  limited, the audio is too peaky to reach the target without a limiter: say so.
- **Clipped samples** above zero mean the recording distorted; gain can't fix that, tell the user.
- **Balance** far from 0 dB or **correlation** near 0 on a voice usually means
  one channel is dead or a lav mic was on one side only. Negative correlation
  means one side is polarity-flipped.
- **DC offset** above about 0.001 points at faulty gear.
- **Noise floor** above about -50 dB is audible hiss or room noise.

## 6. Check your work

Cheapest first:

```bash
ve summary                                         # structure, durations
ve sheet --from 00:01:00:00 --to 00:01:30:00 --count 12 -o sheet.jpg
ve frame --at 00:01:12:12 --width 640 -o cut.jpg   # exact frame at a cut
ve preview --from 00:01:05:00 --to 00:01:20:00 -o check.mp4   # if you can watch video
```

Open the images and look. They show the finished picture, as the export
will: every track, speed changes, effects and transitions. At a cut, check
the frame just before and just after (`--at` the cut, and one frame
earlier). `ve sheet` prints the timecode of every cell so you can map what
you see back to the timeline. For full size or fine detail, ask for a wider
PNG (a `.png` name gives PNG):

```bash
ve frame --at 00:01:12:12 --width 1920 -o cut.png
```

## 7. Export

```bash
ve export --list-presets
ve export -o final.mp4                          # project size, high quality
ve export --preset youtube-1080p -o final.mp4
ve export --preset vertical-1080x1920 -o short.mp4   # landscape sources are center-cropped
ve export --from 00:01:00:00 --to 00:02:00:00 -o clip.mp4
```

A 1080p export runs at about twice real time on a 4-core machine; run it last.

## Notes

- `--json` on any command gives machine-readable output (`ve --json summary`).
- `ve probe <file>` shows codec, size, frame rate and, when unusual, bit depth,
  HDR (`hdr10`, `hlg`) and full-range video. HDR or 10-bit sources exported
  to an 8-bit preset may look washed out: mention it.
- `ve describe` prints the full protocol (every method and parameter); `ve call <method> '<json>'` sends any of them directly.
- Transitions and effects don't render yet (cuts only); captions and
  transcription come from plugins, not the core.
- If a render says media is missing, the source file moved; tell the user.
- Commands go through a background `ve` process for the project. It starts on
  its own, exits after 10 idle minutes, and restarts itself after a rebuild.
  `ve daemon status` shows it; `ve daemon stop` ends it (nothing is lost, every
  edit is saved). The user may be editing the same project at the same time,
  which is what exit code 3 catches.
