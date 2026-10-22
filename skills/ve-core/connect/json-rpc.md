# Messages and errors

The protocol is [JSON-RPC 2.0](https://www.jsonrpc.org/specification).
Protocol major version: `1` (from `system.version`).

## Requests and responses

Params are always **by name** (an object), never positional. Omit `params`
or send `{}` for methods without any.

```json
{"jsonrpc": "2.0", "id": 12, "method": "render.frame", "params": {"at": "00:00:04:10", "width": 480}}
```

```json
{"jsonrpc": "2.0", "id": 12, "result": {"at": "00:00:04:10", "width": 480, "height": 270, "mime": "image/jpeg", "data": "<base64>", "warnings": []}}
```

- `id` can be a number or string; it comes back unchanged.
- Unknown params are **rejected** (`invalid_params`), so typos fail loudly.
- Optional params may be omitted or sent as `null`.
- A message with no `id` is a notification: it runs, and no response comes.
  (Rarely what you want for calls.)

## Server-to-client notifications

After `events.subscribe` the daemon also sends messages with a `method` and
no `id`. See [../editing/events.md](../editing/events.md).

| Method | Means |
| --- | --- |
| `project.changed` | A revision was committed: `{revision, actor, command, patch}` |
| `project.reloaded` | `project.otio` was changed outside the engine and re-read: refetch |
| `events.lagged` | You fell behind and missed `params.missed` events: refetch |

Tell them apart from responses by the missing `id`.

## Errors

```json
{"jsonrpc": "2.0", "id": 9, "error": {"code": -31000, "message": "stale_revision: the project is at rev 4; re-read it and retry", "data": {"current_rev": 4}}}
```

| Code | Name | When | CLI exit code |
| --- | --- | --- | --- |
| -32700 | parse error | Not valid JSON | 1 |
| -32601 | method not found | Unknown method name | 1 |
| -32602 | invalid_params | Bad or unknown params, unknown IDs, impossible edits ("no clip `clip_x`", "nothing loud enough to measure"). The message says what to fix | 2 |
| -32603 | internal | Engine bug or I/O failure | 1 |
| -31000 | stale_revision | `base_rev` is not the current revision. `data.current_rev` has the current one: refetch, re-plan, retry | 3 |
| -31001 | render_failure | ffmpeg failed, or media is missing or unreadable | 4 |
| -31002 | plugin_error | A plugin failed (no plugins exist yet) | 5 |
| -31003 | project_locked | Another engine process holds the project | 6 |
| -31004 | cancelled | A background job was cancelled (`jobs.cancel`, or Ctrl+C in the CLI) | 130 |

Messages are written for people and agents: show them as they are.

## Self-description: `system.describe`

Returns an [OpenRPC 1.3.2](https://spec.open-rpc.org/) document generated
from the code:

- `methods[]`: `name`, `summary`, `params[]` (one entry per field, each with
  a JSON Schema and `required`), `result.schema`.
- `x-mutates: true` on methods that change the project (they take
  `base_rev`, `dry_run`, `actor` and emit `project.changed`).

Use it to generate typed clients, validate requests, or build forms. Nested
types are inlined in each schema. CLI: `ve describe` prints it.
