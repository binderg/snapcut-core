# history.*

How undo works overall: [../editing/revisions-and-undo.md](../editing/revisions-and-undo.md#undo-and-redo).

## `history.undo` / `history.redo` (edit)

Params: `base_rev` (required), `dry_run`, `actor`. Result: the common edit
result plus `step`, what was undone or redone:

```json
{"revision": 6, "changed": true, "patch": [...],
 "step": {"what": "edit.split", "actor": "user:local", "revs": [4]}}
```

`invalid_params` "nothing to undo" / "nothing to redo" when the stack is empty.

## `history.list`

`{"limit": 20}` (optional). Read-only.

```json
{"revision": 4,
 "undo": [{"what": "edit.split", "actor": "user:local", "revs": [4]},
          {"what": "remove silences", "actor": "agent:claude", "revs": [2, 3], "group": "grp_93b149"}],
 "redo": [],
 "open_groups": [["agent:claude", {"id": "grp_93b149", "label": "remove silences"}]]}
```

- `undo`: what `history.undo` would undo, most recent first. A group is one
  step: `what` is its label, `revs` the revisions it made.
- `redo`: what `history.redo` would bring back, next first.
- `actor` here is a string `kind:id` (`user:local`, `agent:claude`).
- `open_groups`: `[actor, group]` pairs.

## `history.begin_group`

`{"label": "remove silences", "actor": {...}}`. Returns `{"id": "grp_...", "label": ...}`.
The actor's next edits join the group until `end_group`. One open group per
actor; beginning a second one is an error. Does not change the document.

## `history.end_group`

`{"actor": {...}}`. Returns `{"group": {"id", "label"}, "edits": 3}`. Error if
the actor has no open group.
