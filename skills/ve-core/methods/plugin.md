# plugin.* and plugin methods

Plugins add methods to the engine: speech to text, beat detection, cut
helpers, anything with a model in it (the core has none). A plugin is a
folder with a `plugin.json`; its code runs as its own process, started on
the first call and stopped after 5 idle minutes. Writing one:
[../guides/writing-plugins.md](../guides/writing-plugins.md).

Plugins are found in two places, first match wins for an id:

1. The project's `plugins/` folder (travels with the project).
2. The user's plugin folder (`ve plugin install <folder>` copies there):
   `VE_PLUGINS_DIR`, else `<OS data dir>/ve/plugins`.

## Calling a plugin method

A plugin's methods are ordinary methods: same JSON-RPC, same errors, listed
in `system.describe` with `"x-plugin": "<id>"`. Their namespace is the
plugin's own (`deepgram.transcribe`); plugins can never take or shadow a
built-in method or namespace.

- Methods marked `job` take `background: true` and then answer `{job_id}`
  at once ([jobs.md](jobs.md)). Progress comes from the plugin; `jobs.cancel`
  asks it to stop and kills it if it has not after 5 seconds.
- Methods marked `mutates` take `base_rev`, `dry_run` and `actor` like any
  edit. The plugin makes its edits through the engine's own methods, as
  actor `plugin:<id>`, and every edit it makes during one call joins one
  undo group: one `history.undo` reverts the whole call.
- Errors from the plugin come back as `plugin_error` (-31002, exit code 5),
  except engine errors it passes on (a `stale_revision` stays one). A plugin
  that crashes mid-call fails the call with the last lines of its log; the
  next call starts it again.
- A plugin whose secret (API key) is missing fails with a message naming
  the command that sets it.

The CLI maps plugin methods to commands: a plugin's manifest names the
command (`ve transcribe <asset>`), positional args come from its `cli.args`,
and every other param is a `--flag`.

## `plugin.list`

`{}`. Rescans the plugin folders first, so a plugin installed while the
engine runs shows up. Works with no project open (user plugins only).

```json
{"plugins": [
   {"id": "deepgram", "name": "Deepgram speech to text", "version": "0.1.0",
    "description": "Transcribes an asset's audio with Deepgram ...",
    "source": "user", "dir": "C:\\Users\\me\\AppData\\Roaming\\ve\\plugins\\deepgram",
    "running": false,
    "commands": [{"method": "deepgram.transcribe", "summary": "...", "mutates": false, "job": true,
                  "cli": {"name": "transcribe", "args": ["asset"]},
                  "params": { "...JSON Schema...": "" }}],
    "effects": 0,
    "secrets": [{"name": "DEEPGRAM_API_KEY", "source": "keychain"}],
    "skill": "C:\\...\\deepgram\\SKILL.md"}
 ],
 "issues": [{"dir": "C:\\...\\plugins\\broken", "message": "plugin.json: expected value at line 1 column 3"}],
 "user_dir": "C:\\Users\\me\\AppData\\Roaming\\ve\\plugins"}
```

- `source`: `project` or `user`.
- `effects`: how many effects and transitions it contributes (an effect
  pack; `fx.list` lists them, [effects.md](effects.md)).
- `secrets[].source`: `environment` (a variable of that name is set where
  the engine runs), `keychain` (`ve plugin secret set`), or `missing`.
- `skill`: the plugin's SKILL.md, for agents to read before using it.
- `issues`: folders that were not loaded and why (bad manifest, wrong
  engine version, a method in an engine namespace, a duplicate id or method).
