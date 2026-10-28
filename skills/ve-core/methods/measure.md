# measure.* and edit.normalize

Measurements are read-only: no `base_rev`, no markers, no events.

## Choosing what to measure

Every `measure.*` method takes one of:

| Params | Measures |
| --- | --- |
| `asset: "asset_..."` | That asset's file, whole, as it is on disk, with its own channels (no downmix). Cached (see below). Add `stream` for an audio stream other than the first |
| `from`, `to` (or nothing) | The timeline mix over that range: what plays after cuts, speed and volume, all tracks mixed |

Sending `asset` together with `from`/`to` is an error. Times in results
count from the start of the source (`source.from`).

`resolution` (`"1s"`, `"10f"`, `"0.5s"`) adds a `series`: one value per step,
at most 100000 points. Point `i` covers `[i * step, (i + 1) * step)`; `null`
where there was nothing to measure (silence).

```json
"series": {"step": "00:00:01:00", "lufs": [-14.0, -14.2, null, -13.8]}
```

Every result has `source` (`{asset?, stream?, from, to}`), `cached` and `warnings`.

`measure.levels` on one stream shows which channels carry sound: a channel
far below the others (or at the noise floor) is an unused input, and
`edit.set_audio` with `channels` picks the ones that matter.

**Cache.** Asset results are stored in `<OS cache>/ve/measure/<file hash>/`,
keyed by method and settings, so the same file is measured once however many
projects or calls ask; `cached: true` says it came from there. Timeline
results are always computed fresh.

**Cost.** Audio measurements of a 10-minute file take a few seconds; other
calls keep working meanwhile. Every `measure.*` and `edit.normalize` takes
`background: true` to run as a job ([jobs.md](jobs.md)).

## `measure.loudness`

EBU R128 via ffmpeg's `ebur128`.

```json
{"source": {"from": "00:00:00:00", "to": "00:00:04:00"},
 "integrated_lufs": -14.0, "range_lu": 0.0, "true_peak_dbtp": -13.1,
 "series": {"step": "00:00:01:00", "lufs": [-14.0, -14.0, -14.0, -14.0]}, "cached": false}
```

| Field | Meaning |
| --- | --- |
| `integrated_lufs` | Whole-range loudness. Online video and podcasts aim at -14 to -16, EBU broadcast -23. `null` if everything is below -70 |
| `range_lu` | How much loud and quiet parts differ. Speech is usually 3-8 |
| `true_peak_dbtp` | Highest peak including between samples. Above -1 risks distortion after encoding |
| `series.lufs` | Momentary loudness (400 ms windows) averaged per step |

## `measure.levels`

Sample statistics, per channel and overall.

```json
{"sample_rate": 48000,
 "channels": [{"rms_db": -23.0, "peak_db": -20.0, "dc_offset": 0.0, "clipped_samples": 0},
              {"rms_db": -23.0, "peak_db": -20.0, "dc_offset": 0.0, "clipped_samples": 0}],
 "noise_floor_db": -61.5, "balance_db": 0.0, "correlation": 1.0,
 "series": {"step": "00:00:01:00", "rms_db": [...], "peak_db": [...]}}
```

| Field | Meaning |
| --- | --- |
| `rms_db`, `peak_db` | dBFS. `null` for a silent channel |
| `clipped_samples` | Samples at full scale. Above 0 means the recording distorted; gain cannot fix it |
| `dc_offset` | Mean sample value. Above about 0.001 points at faulty gear |
| `noise_floor_db` | The quietest stretches that are not digital silence (room tone, hiss): 5th percentile of 50 ms windows. Above about -50 is audible |
| `balance_db` | Stereo only: left minus right. Far from 0 (up to ±120 for a dead side) means one channel is weak or empty, e.g. a lav mic on one side |
| `correlation` | Stereo only, -1 to 1. Near 1 is mono-like, near 0 unrelated, negative means one side is polarity-flipped |

The timeline is measured as stereo at the project sample rate; a file at its
own rate and channel count (up to 8).

## `measure.waveform`

| Param | Default | |
| --- | --- | --- |
| `buckets` | 1000 | Number of points (max 100000), e.g. the pixel width you draw |
| `asset` or `from`/`to` | timeline | |

```json
{"sample_rate": 48000, "samples": 192000,
 "min": [-0.1, -0.1, ...], "max": [0.1, 0.1, ...], "rms": [0.0707, ...]}
```

Per point: the lowest and highest sample on any channel (-1 to 1) and the RMS
over all channels; no downmix, so levels read as recorded. Draw a vertical
line from `min` to `max` per pixel, and a darker one for `±rms`. `samples` is
frames per channel. Buckets are exact to 64 samples.

For a zoomable timeline, fetch per asset (cached) at a few bucket counts and
map clips onto them using each clip's media in-point and speed, rather than
re-measuring the timeline after every edit.

## `measure.pitch`

Fundamental frequency with the YIN algorithm, 60-800 Hz (voices and most
instruments), a value every 20 ms.

```json
{"median_hz": 220.1, "low_hz": 220.0, "high_hz": 220.1, "note": "A3", "voiced": 1.0,
 "series": {"step": "00:00:01:00", "hz": [220.1, 220.0, null, 220.1]}}
```

| Field | Meaning |
| --- | --- |
| `median_hz` | Median pitch of voiced frames |
| `low_hz`, `high_hz` | 10th and 90th percentile: the usual range, ignoring octave slips |
| `note` | Nearest note to the median (`A3`) |
| `voiced` | Share of frames with a pitch, 0 to 1 (low for silence, noise or music with no clear pitch) |
| `series.hz` | Median pitch per step |

Typical speaking voices: about 85-180 Hz (lower voices) and 165-255 Hz
(higher voices). The tracker reads the fundamental only; it does not
separate speakers.

## `edit.normalize`

An edit (`base_rev`, `dry_run`, `actor`): measures the timeline mix, then
adds one gain to every audio clip so the integrated loudness hits the target.

| Param | Default | |
| --- | --- | --- |
| `target` | -14 | LUFS: -14 (YouTube), -16 (podcasts), -23 (EBU broadcast) |
| `ceiling` | -1 | Highest true peak allowed after the gain, dBTP |
| `from`, `to` | whole timeline | Measure only this range and change only clips overlapping it (whole clips, so audio outside the range changes too) |

Result: the common edit result plus

```json
{"measured_lufs": -20.7, "measured_true_peak_dbtp": -20.0, "gain_db": 6.7,
 "expected_lufs": -14.0, "expected_true_peak_dbtp": -13.3, "clips": ["clip_fc7d84"], "warnings": []}
```

The gain is added to each clip's `volume_db` (rounded to 0.1 dB, kept within
-100..+30). If reaching the target would push the true peak over `ceiling`,
the gain is limited and a warning says so: the audio is too peaky for that
target without a limiter (the engine has none yet). Undo removes it like any
edit. Try `dry_run: true` first to show the user the gain.
