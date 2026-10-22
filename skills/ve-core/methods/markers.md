# marker.*

Timeline markers: named, colored points or ranges on the timeline
(`tracks.markers` in the document, see
[../model/project.md](../model/project.md#marker)). Both methods edit the
project (`base_rev`, `dry_run`, `actor`) and return `markers`: the IDs added
or removed.

## `marker.add`

| Param | Default | |
| --- | --- | --- |
| `at` | required | Start |
| `duration` | 0 (a point) | Length, making it a range |
| `name` | `""` | `chapter`, `todo`, ... Names are not unique; several markers can share one |
| `color` | red | OTIO colors: red, pink, orange, yellow, green, cyan, blue, purple, magenta, black, white |
| `comment` | `""` | |

The marker's `source` is `agent` when the actor is an agent, `analyzer` for a
plugin, else `user`.

## `marker.remove`

`{"ids": [...]}` or `{"name": "silence"}` (every marker with that name).

Analyzers (`analyze.silence`, `analyze.scenes`) write markers named
`silence` / `scene`, replacing their own earlier markers in the range they
scan, so re-running one does not pile up duplicates.
