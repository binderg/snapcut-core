# system.*, daemon.*, events.subscribe

## `system.version`

No params. `{"engine": "0.1.0-rc.1", "protocol": 1, "schema": "0.1.0"}`.
`protocol` changes only on breaking protocol changes; `schema` is the project
file schema this engine writes.

## `system.describe`

No params. The OpenRPC document for every method. See
[../connect/json-rpc.md](../connect/json-rpc.md#self-description-systemdescribe).

## `daemon.info`

No params. Only on a daemon (errors with `invalid_params` in-process).

```json
{"pid": 32752, "engine": "0.1.0-rc.1",
 "exe": {"path": "C:\\...\\ve.exe", "modified": "1761090000000000000"},
 "project": "C:\\Users\\me\\Videos\\my-edit",
 "socket": "ve-6b692bae2d27fa88",
 "websocket": "ws://127.0.0.1:52898/rpc?token=DCVgBKymcJ_ueXjllkVs9Wu2k6YUSH3x",
 "idle_timeout_secs": 600}
```

`websocket` is the full URL a frontend connects to, token included. Treat
the token like a password: anyone with it can edit the project.

## `daemon.stop`

No params. Answers `{"stopping": true}`, then the daemon exits. Everything is
already saved. Open connections close.

## `events.subscribe`

No params. `{"revision": 7}` (or `null` with no project open). Afterwards
this connection receives `project.changed`, `project.reloaded` and
`events.lagged` notifications. See [../editing/events.md](../editing/events.md).
