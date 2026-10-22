# Revisions, dry runs, actors and undo

## Every mutating call has the same three extra params

Methods marked `x-mutates` in `system.describe` (all `edit.*`, `marker.*`,
`media.import`, `analyze.*`, `history.undo` / `redo`) take:

| Param | Required | Meaning |
| --- | --- | --- |
| `base_rev` | yes | The revision you planned the edit against. If the project has moved on, the call fails with `stale_revision` (-31000) and `data.current_rev`; nothing changes |
| `dry_run` | no (false) | Compute the edit and return its patch without changing anything |
| `actor` | no (local user) | Who is editing: `{"kind": "user" \| "agent" \| "plugin", "id": "..."}`. Recorded in history and in `project.changed` |

This is optimistic concurrency: a user in a UI and an agent can edit the same
project at once. Whoever is second gets `stale_revision`, refetches, checks
the plan still makes sense, and retries with the new `base_rev`.

Typical loop:

```
rev = project.get().revision
r = edit.trim({clip, out: "-10f", base_rev: rev})
  ok        -> rev = r.revision
  -31000    -> refresh (or apply pending events), decide again, retry
```

A frontend that applies `project.changed` events always knows the current
revision, so stale errors only happen in a real race.

## What every mutating call returns

```json
{
  "revision": 5,
  "dry_run": false,
  "changed": true,
  "patch": [ {"op": "replace", "path": "/tracks/children/0/children/0/source_range/duration/value", "value": 50.0},
             {"op": "replace", "path": "/metadata/ve/project/revision", "value": 5} ],
  "group": "grp_93b149",
  "created":  [{"id": "clip_938690", "track": "V1", "start": "00:00:02:00", "end": "00:00:04:00"}],
  "modified": [{"id": "clip_f4b280", "track": "V1", "start": "00:00:00:00", "end": "00:00:02:00"}],
  "removed":  ["clip_1a2b3c"],
  "duration": "00:00:04:00"
}
```

| Field | Meaning |
| --- | --- |
| `revision` | Revision after the call. Unchanged on a dry run or when nothing changed |
| `changed` | Whether it changed (or on a dry run, would change) the project. A no-op edit is not an error |
| `patch` | JSON Patch (RFC 6902) from the old document to the new one, ending with the revision bump. Paths are into the `project.get` `timeline` |
| `group` | The undo group the edit joined, if the actor has one open |
| `created` / `modified` / `removed` | Clips touched, with their new places (edit methods) |
| `duration` | Timeline length afterwards |
| plus | The method's own fields (`markers`, `ranges`, `assets`, `gain_db`, ...) |

On a dry run, `patch` is what would happen; `revision` stays the same.

## Undo and redo

- `history.undo` / `history.redo` take `base_rev` like any edit, and each
  makes a **new revision** (history is never rewritten). They emit
  `project.changed` like any edit, with `command` `history.undo` / `history.redo`.
- There is **one undo stack per project**: undo removes the most recent step,
  whoever made it. `history.list` shows each step's `actor`, so a UI can
  label "Undo trim (agent:claude)".
- A new edit after an undo clears the redo stack.
- History survives daemon restarts (kept in the cache folder), up to 1000
  entries. It is dropped if `project.otio` was changed outside the engine.

### Undo groups

Wrap several edits so one undo removes them all:

```
history.begin_group {label: "remove silences", actor}
  ... edits by the same actor ...
history.end_group {actor}
```

- Groups are **per actor**: an agent's open group does not swallow the
  user's edits. Pass the same `actor` to the edits and to begin/end.
- The group's edits undo as one step when they are the most recent entries.
  If someone else edited in between, undo takes the group in parts.
- `begin_group` / `end_group` do not change the document, take no
  `base_rev`, and emit no event. `history.list` shows `open_groups`.
- `end_group` returns `{group, edits}`; `edits` is how many edits the group holds.
