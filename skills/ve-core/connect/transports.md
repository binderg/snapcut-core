# Connecting: the daemon and its transports

## One daemon per project

A project is a folder holding `project.otio`. Exactly one engine process
(the daemon) has a project open at a time; it holds a lock on it and keeps
the document in memory. Every client of that project talks to that daemon.

| Command | What it does |
| --- | --- |
| `ve daemon start` | Start the project's daemon if it is not running, then show it |
| `ve daemon status` | Show the running daemon (pid, socket, WebSocket URL with token), or say none runs |
| `ve daemon stop` | Stop it. Nothing is lost: every edit was saved when it was made |
| `ve --json daemon status` | The same as JSON (the `daemon.info` result plus `"running"`) |

The project is the nearest folder at or above the current directory with a
`project.otio`, or `--project <folder>`.

- **Started on demand.** Any `ve` command on a project starts the daemon if
  needed. A frontend can run `ve daemon start` (or any project command) to
  make sure one is up, then read the URL from `ve --json daemon status`.
- **Idle exit.** The daemon exits after 10 minutes with no open connections.
  An open WebSocket keeps it alive. `VE_DAEMON_IDLE_SECS` changes the timeout
  (set it in the environment of the `ve` command that starts the daemon).
- **Fixed port.** The WebSocket port is random unless `VE_WS_PORT` is set
  when the daemon starts.
- **Upgrades.** If the `ve` binary changes (a rebuild or an update), the next
  CLI command stops the old daemon and starts a new one. Open connections to
  the old one drop; reconnect and refetch.
- **Logs.** The daemon writes to `daemon.log` in the project's cache folder
  (`<OS cache>/ve/projects/<key>/`).

## Two transports, one protocol

Both carry the same JSON-RPC 2.0 messages to the same method registry, so
anything works the same over either.

### WebSocket (for frontends and anything networked)

```
ws://127.0.0.1:<port>/rpc?token=<token>
```

- One JSON-RPC message per **text** frame. Binary frames are ignored.
- Listens on **127.0.0.1 only**; it is not reachable from other machines.
- The **token** is random per daemon run. Get the whole URL from
  `ve daemon status` or the `daemon.info` method (over the local socket).
  A wrong or missing token gets HTTP 401.
- **Origin check.** A browser sends an `Origin` header; it must be a loopback
  origin (`http://localhost:<any port>`, `http://127.0.0.1:...`,
  `http://[::1]:...`, http or https). Anything else, including `null` (a page
  opened from `file://`), gets HTTP 403. Non-browser clients send no Origin
  and only need the token. So a dev server on `localhost:5173` works; a page
  served from a real domain does not.
- The daemon restarts get a new port (unless `VE_WS_PORT`) and a new token:
  on disconnect, re-read the URL before reconnecting.

### Local socket (for the CLI and local tools)

- A Windows named pipe `\\.\pipe\ve-<key>`, or on Linux/macOS a Unix socket
  at `<OS cache>/ve/projects/<key>/daemon.sock` (only the user can reach it).
  `<key>` is a hash of the project path; `daemon.info` shows the exact name.
- **Newline-delimited JSON**: one message per line, `\n` terminated. JSON
  must not contain raw newlines (compact JSON never does).
- No token: access is limited by the OS to the current user.

## Calls without a project

`system.version`, `system.describe`, `media.probe`, `render.presets` and
`project.init` work without an open project. The CLI runs those in its own
process (`ve version`, `ve describe`, `ve probe`, `ve init`) without starting
a daemon.

## Concurrency

Requests on one connection may be sent without waiting; responses carry the
request `id` and can come back in any order. Slow calls (`render.*`,
`analyze.*`, `measure.*`, `edit.normalize`) work on a copy of the project, so
they do not make other calls wait; with `background: true` they run as jobs
with progress and cancel ([../methods/jobs.md](../methods/jobs.md)). At most
2 jobs run at once. `media.import` of large files still holds the project
until it is done.
