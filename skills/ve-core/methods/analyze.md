# analyze.*

Analyzers look at what the timeline plays (after edits, over `from`/`to`)
and turn what they find into **timeline markers**, so the results are part
of the project, show up for every client, and can drive edits. They are edit
methods: `base_rev`, `dry_run`, `actor`, and a `project.changed` event when
markers are written. For numbers without markers, see
[measure.md](measure.md).

Both return the common edit result plus:

```json
{"ranges": [{"kind": "silence", "start": "00:00:01:15", "end": "00:00:02:10", "note": "20f"}],
 "markers": ["mk_1a2b3c"], "warnings": []}
```

Re-running an analyzer replaces its own earlier markers in the scanned range.
`markers: false` reports the ranges without writing anything.

## `analyze.silence`

| Param | Default | |
| --- | --- | --- |
| `from`, `to` | whole timeline | |
| `threshold` | `-35dB` | Below this counts as silence. `-30dB` for a noisy room, `-45dB` for a quiet one |
| `min_duration` | `0.5s` | Shortest silence to report |
| `pad` | `0s` | Shrink each silence at both ends; `0.1s` keeps a natural breath |
| `markers` | true | Write purple `silence` markers |

Ranges snap inward to whole frames, so a cut never removes a frame of
sound. Then `edit.ripple_delete {"markers": "silence"}` cuts them all in one
edit.

## `analyze.scenes`

| Param | Default | |
| --- | --- | --- |
| `from`, `to` | whole timeline | |
| `threshold` | `"10"` | 0-100; lower finds more cuts |
| `markers` | true | Write cyan point `scene` markers |

Finds hard cuts in the picture. `note` is the detector's score.
