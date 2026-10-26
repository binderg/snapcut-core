# Writing a plugin

A plugin adds methods (and `ve` commands) to the engine. It is a folder with
a `plugin.json` and a program in any language. The engine starts the program
on the first call, talks JSON-RPC with it over stdin/stdout, and stops it
after 5 idle minutes (`VE_PLUGIN_IDLE_SECS`). The program can call any
engine method back while it works. Anything with a model in it belongs in a
plugin; the core never bundles one.

A complete example: `plugins/deepgram/` in the engine repository (speech to
text, Python standard library only, about 250 lines). The test plugin in
`crates/ve-daemon/tests/fixtures/plugins/testplug/` shows every feature in
less.

## The folder

```
my-plugin/
  plugin.json        the manifest
  main.py            your program (any language)
  schemas/*.json     params schemas, if you keep them out of the manifest
  SKILL.md           how and when an agent should use it (recommended)
```

Put it in a project's `plugins/` folder to try it (the engine rescans on
`plugin.list` and on a call to a method it does not know), or install it for
every project with `ve plugin install my-plugin/`.

## `plugin.json`

```json
{
  "id": "deepgram",
  "version": "0.1.0",
  "engine": ">=0.1.0-rc.1, <0.2",
  "protocol": "1",
  "name": "Deepgram speech to text",
  "description": "One line for `ve plugin list`.",
  "contributes": {
    "commands": [
      {
        "method": "deepgram.transcribe",
        "summary": "Transcribe an asset's audio with Deepgram",
        "params": "schemas/transcribe.json",
        "result": { "type": "object" },
        "job": true,
        "mutates": false,
        "cli": { "name": "transcribe", "args": ["asset"] }
      }
    ]
  },
  "run": { "command": "python3", "args": ["main.py"], "windows": { "command": "python" } },
  "secrets": ["DEEPGRAM_API_KEY"],
  "skill": "SKILL.md"
}
```

| Field | |
| --- | --- |
| `id` | Lowercase letters, digits, `.`, `-`, `_`. Unique. |
| `engine` | Semver range of engine versions. A release candidate (`0.1.0-rc.1`) also counts as its release (`0.1.0`). |
| `protocol` | Protocol major version, `"1"`. |
| `commands[].method` | `namespace.name`, lowercase. The namespace must not be one the engine uses (`edit`, `media`, `transcript`, `plugin`, `system`, ...); a plugin claiming one is not loaded. |
| `commands[].params` / `result` | JSON Schema, inline or a path relative to the folder. `system.describe` shows them; the CLI builds flags from `params`. |
| `commands[].job` | Slow work: callers can pass `background: true`, and the CLI always runs it as a job with a progress line. |
| `commands[].mutates` | It edits the project. Clients send `base_rev`/`dry_run`/`actor`, and your edits during the call become one undo step. |
| `commands[].cli` | A `ve <name>` command. `args` are params filled from positional arguments, in order; every other param is a `--flag` (`max_speakers` answers to `--max-speakers`; booleans are bare flags; arrays repeat). |
| `run` | `command` and `args`, started in the plugin folder. A command with a `/` is relative to the folder; a bare name is looked up on PATH. `windows`, `macos`, `linux` override `command`/`args` per OS. |
| `secrets` | Environment variable names your program needs (API keys). |
| `requires` | `{"gpu": bool, "downloads": [...]}`: for the user to know (models fetched on first use). |
| `effects` | Effect and transition folders, relative to the plugin folder (see Effect packs). |
| `skill` | A SKILL.md shown in `plugin.list`. |

Unknown fields are refused, so a typo fails at load with a clear message
(see `issues` in `plugin.list`).

## The protocol

Newline-delimited JSON-RPC 2.0 on stdin/stdout, one message per line, UTF-8.
**Never print anything else on stdout**; log to stderr (it goes to
`<project cache>/plugins/<id>.log`) or send `$/log`.

Engine to plugin:

| Message | |
| --- | --- |
| `initialize` request | First, once. Params: `{"engine": "0.1.0-rc.1", "protocol": 1, "plugin": {"id", "version", "dir"}, "project": "<folder>"}`. Answer within 30 s with any object (`{}` is fine). |
| Your methods | Requests with the caller's params (minus `background`). Answer with `result` or `error`. Several can arrive at once. |
| `$/cancel` notification | `{"id": <the request id>}`: stop that call and answer it (any error will do). Not answering within 5 s gets the process killed. |
| `shutdown` notification | Exit. stdin closes right after; **exit when stdin closes**, whatever else happens. |

Plugin to engine:

| Message | |
| --- | --- |
| Engine method requests | Any engine method, with your own ids (strings are fine). The answer comes back on stdin. Methods that record an actor get `{"kind": "plugin", "id": "<your id>"}` whatever you send. |
| `$/progress` notification | `{"id": <request id>, "progress": 0.0-1.0, "message": "uploading"}` for a call you are working on; shows in `jobs.get` and the CLI. |
| `$/log` notification | `{"level": "info" \| "warn" \| "error", "message": "..."}` into the engine's log. |

Because answers to your calls arrive on stdin while you are handling a
request, read stdin on one thread and handle requests on others (or use
async). The example plugins do this in about 40 lines.

Environment: `VE_PLUGIN_ID`, `VE_PLUGIN_DIR`, `VE_ENGINE_VERSION`, your
secrets, and for Python `PYTHONIOENCODING=utf-8` and `PYTHONUNBUFFERED=1`.
The working directory is the plugin folder.

Errors: answer with a JSON-RPC error. Engine errors you pass on keep their
code (a `stale_revision` stays one, so the caller can retry); anything else
reaches the caller as `plugin_error` (-31002) with your message prefixed by
your id.

## Getting media and storing results

- **Audio**: `media.extract_audio {"asset", "sample_rate", "channels",
  "format": "wav" | "flac"}` gives a file path in the engine cache. You never
  need ffmpeg or a decoder.
- **Frames**: `render.frame {"at"}` gives a JPEG or PNG of the timeline.
- **The project**: `project.get` (OTIO), `project.summary` (text),
  `media.assets` (files, durations, hashes).
- **Transcripts**: `transcript.set {"asset", "words", "language",
  "speakers", "provenance"}` ([../methods/transcript.md](../methods/transcript.md)).
  Times are strings: `"%.3fs" % seconds`.
- **Markers**: `marker.add` (an edit: needs `base_rev`).
- **Edits**: any `edit.*`, `track.*`, `effect.*` method, with `base_rev`.
  Read the revision first (`project.summary`), and pass each result's
  `revision` to the next call.

Record provenance (plugin, version, model, parameters, request id) on
whatever you store, so results can be audited and regenerated.

## Secrets

Declare them in `secrets`. The user stores them once, in the OS keychain:

```bash
ve plugin secret set deepgram DEEPGRAM_API_KEY
```

A variable of the same name in the engine's environment wins (CI, shell
profiles). Your program reads it from its environment. If a secret is
missing, calls fail before your program starts, telling the user the
command to run. Secrets are never written to the project.

## Effect packs

A plugin can ship effects and transitions instead of (or as well as)
commands. A pack with only effects needs no `run` and no program:

```
glitch-pack/
  plugin.json
  effects/
    rgb-split/   effect.json + rgb-split.wgsl
    shake/       effect.json + shake.wgsl
```

```json
{
  "id": "glitch-pack",
  "version": "1.0.0",
  "engine": ">=0.1.0-rc.1",
  "protocol": "1",
  "name": "Glitch looks",
  "contributes": { "effects": ["effects"] }
}
```

Each `effects` entry is a folder holding an `effect.json`, or a folder of
such folders. The effect folders are the same as a project's `effects/`
ones (manifest, WGSL, images; contracts in
[../methods/effects.md](../methods/effects.md)). Ids are yours to choose
but should start with something of your own (`glitch/rgb-split`): built-in
ids and `project.` ids are refused, and so is an `id@major` another pack
already has.

Installed packs show in `fx.list` as `origin: "pack"`. A project never
renders from a pack: the first time it uses one of your effects, the
engine copies that effect's folder into the project's `effects/`. Bump the
major version for a change that would alter existing projects' look;
projects keep the copy they have until asked for the new major.

## Checklist

- [ ] Nothing but JSON-RPC on stdout; logs to stderr.
- [ ] Answer `initialize`; exit on `shutdown` or when stdin closes.
- [ ] Requests handled off the stdin-reading thread.
- [ ] Long calls send `$/progress` and honour `$/cancel`.
- [ ] Times as strings (`"12.345s"`, timecode), never numbers.
- [ ] Provenance on what you store.
- [ ] A SKILL.md saying when to use it and what it costs (money, time).
