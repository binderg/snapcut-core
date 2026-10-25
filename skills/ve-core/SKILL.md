---
name: ve-core
description: Reference for the video engine core (`ve`) and its JSON-RPC API. Read it before building anything that talks to the engine (a frontend, a plugin, a script, an integration): how to connect, the project data model, revisions and undo, change events, and every method with its parameters and results. For editing video from a shell as an agent, use the `video-edit` skill instead.
---

# ve core

`ve` is a headless video editing engine. The engine is the product; every
UI, agent, script and plugin is a client of one JSON-RPC 2.0 protocol. There
is no private API: anything a frontend can do, an agent can do, and the
other way round.

- One background process (the **daemon**) per project holds the project in
  memory and serves the protocol over a local socket and a WebSocket.
- The project is an **OpenTimelineIO** (OTIO) document plus a `metadata.ve`
  namespace for engine data. Everything has a **stable ID** (`clip_7f3a`).
- Every edit carries the **revision** it was planned against, can be a
  **dry run**, names its **actor**, and comes back as a **JSON Patch**. The
  same patch goes to every subscriber as a `project.changed` event.
- Time is **exact**: no floats for time in the protocol. You send timecode
  strings and get timecode strings back.

## Files in this skill

Read in this order the first time; afterwards jump to what you need.

| File | Read it for |
| --- | --- |
| [connect/transports.md](connect/transports.md) | Starting or finding the daemon; local socket vs WebSocket; the token |
| [connect/json-rpc.md](connect/json-rpc.md) | Message format, errors, error codes, `system.describe` (OpenRPC) |
| [model/project.md](model/project.md) | The project document: tracks, clips, gaps, markers, assets, IDs, linked clips |
| [model/time.md](model/time.md) | Time formats you can send, what comes back, rates, speed |
| [editing/revisions-and-undo.md](editing/revisions-and-undo.md) | `base_rev`, `dry_run`, `actor`, the result of every edit, undo groups |
| [editing/events.md](editing/events.md) | `events.subscribe`, `project.changed`, keeping a live copy of the project |
| [methods/index.md](methods/index.md) | Every method, one line each, linking to the details |
| [methods/](methods/) | One file per method family: `project`, `media`, `edit`, `tracks`, `effects`, `markers`, `history`, `render`, `analyze`, `measure`, `transcript`, `plugin`, `jobs`, `cache`, `system` |
| [guides/frontend.md](guides/frontend.md) | Building a UI on the engine, step by step |
| [guides/writing-effects.md](guides/writing-effects.md) | The contract for writing your own effect or transition (WGSL) |
| [guides/writing-plugins.md](guides/writing-plugins.md) | Writing a plugin: manifest, the stdio protocol, calling back into the engine, progress, cancel, secrets, CLI commands |
| [guides/status-and-limits.md](guides/status-and-limits.md) | What is not built yet and the gotchas, so you do not design around things that do not exist |

## Thirty-second start

```bash
ve init my-edit --fps 30 --size 1920x1080   # create a project folder
cd my-edit
ve daemon start                             # prints the WebSocket URL with its token
```

```js
const ws = new WebSocket(URL_FROM_DAEMON_STATUS);   // ws://127.0.0.1:<port>/rpc?token=...
ws.onopen = () => {
  ws.send(JSON.stringify({ jsonrpc: "2.0", id: 1, method: "events.subscribe" }));
  ws.send(JSON.stringify({ jsonrpc: "2.0", id: 2, method: "project.get" }));
};
ws.onmessage = (m) => console.log(JSON.parse(m.data));
```

Every edit then looks like this (the revision comes from `project.get`):

```json
{"jsonrpc": "2.0", "id": 3, "method": "edit.split",
 "params": {"at": "00:00:12:15", "base_rev": 7, "actor": {"kind": "user", "id": "local"}}}
```

## The exact contract

`system.describe` (or `ve describe`) returns the OpenRPC document generated
from the engine's own code: every method, every parameter as JSON Schema,
every result. It cannot drift from the code. These docs explain meaning,
defaults and how things fit together; when they and `describe` disagree on a
field, `describe` wins (and these docs have a bug).

## Rules that never change

- **AI-neutral core.** No models in the engine (no speech recognition, no
  segmentation). Models live in plugins; the core owns the data shapes.
- **Standards first:** OTIO for the project, JSON-RPC 2.0 + OpenRPC for the
  protocol, JSON Schema for params, JSON Patch (RFC 6902) for changes.
- **IDs, never indexes.** Address clips, tracks, markers and assets by ID.
- **Files are the client's job.** Renders land in the engine's cache and the
  result gives you the path; moving them where the user wants is up to you.
