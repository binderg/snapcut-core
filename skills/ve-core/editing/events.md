# Change events: keeping a live copy of the project

A UI, an agent and plugins can all edit one project at the same time. Each
client keeps its own copy of the document and applies the changes others
make, as they happen.

## Subscribe

```json
{"jsonrpc": "2.0", "id": 1, "method": "events.subscribe"}
-> {"jsonrpc": "2.0", "id": 1, "result": {"revision": 7}}
```

From then on the connection also receives notifications (messages with
`method` and no `id`). Subscriptions are per connection and last until it
closes. Calling it twice is harmless. Over the CLI's in-process mode it does
nothing; it is for daemon connections (WebSocket or local socket).

## `project.changed`

One per committed revision, from any client, including your own calls:

```json
{"jsonrpc": "2.0", "method": "project.changed", "params": {
  "revision": 8,
  "actor": {"kind": "agent", "id": "claude"},
  "command": "edit.ripple_delete",
  "patch": [ ... JSON Patch from revision 7 to 8 ... ]
}}
```

- `patch` is RFC 6902, against the `timeline` from `project.get`, and ends
  with `replace /metadata/ve/project/revision`. It is the same patch the
  editing call returned.
- Events arrive in revision order with no gaps (unless you lag, below).
- For your own edits, the event may arrive **before** the call's response.
  Apply by revision and ignore anything at or below your current revision.
- Dry runs, no-op edits, `begin_group` / `end_group` and read-only calls send
  nothing.

## `project.reloaded`

`{"revision": N}`: `project.otio` was changed outside the engine (a git
checkout, a script) and re-read. Your copy is invalid: call `project.get`.

## `events.lagged`

`{"missed": N}`: you read too slowly and missed events (the buffer holds
1024). Call `project.get`.

## Keeping a mirror, step by step

```js
let doc = null, rev = -1;
const pending = [];

async function sync() {
  const sub = await call("events.subscribe");       // 1. subscribe first: nothing is missed
  const got = await call("project.get");            // 2. then take the full document
  doc = got.timeline; rev = got.revision;
  for (const e of pending.splice(0)) apply(e);      // 3. apply anything that arrived meanwhile
}

function onNotification(msg) {
  if (msg.method === "project.changed") doc ? apply(msg.params) : pending.push(msg.params);
  if (msg.method === "project.reloaded" || msg.method === "events.lagged") sync();
}

function apply(e) {
  if (e.revision <= rev) return;                    // already have it
  if (e.revision !== rev + 1) return sync();        // a gap: refetch
  doc = jsonpatch.apply(doc, e.patch);              // any RFC 6902 library
  rev = e.revision;
  redraw(e);                                        // e.actor says who did it
}
```

After a reconnect (the daemon restarted, or the network blipped), run
`sync()` again; the token and port may have changed (see
[../connect/transports.md](../connect/transports.md)).

Patches touch only what changed: a ripple delete over 160 cuts is a few
operations, not a rewrite of the track. Re-rendering the whole timeline on
every event is still fine for most projects; diff by clip ID if not.
