# Headless Video Engine: Architecture

Design document, first written Oct 20, 2026 by Gershon Binder. The status of each part is in the TODO list at the end and in the changelog.

## Overview and goals

Build a headless, agent-friendly video editing engine in Rust. The engine is the product; every UI, agent, script and plugin is a client of the same protocol.

**Goals**

- **Headless first.** Every capability is reachable without a UI. A UI never gets a private API.
- **Agents as first-class users.** An LLM can inspect footage, edit the timeline, check its work and render, through a CLI plus skills. No MCP required.
- **Standards over invention.** OTIO for the project, JSON-RPC 2.0 + OpenRPC for the protocol, JSON Schema for parameters, JSON Patch for change events, WGSL for effects, SKILL.md for agent docs.
- **Extensible by anyone.** Effects, transitions, commands, analyzers, importers and exporters are plugins. Built-in features ship as plugins too.
- **Bring your own frontend.** The protocol is small and self-describing, so a person (or an LLM reading `editor describe`) can build a simple UI, a pro UI, or an AI-native one.
- **AI-neutral core.** No model inside the core. AI arrives as a driving client or as plugins.

**Primary use case:** editing real footage (talking heads, podcasts, phone clips, screen recordings) with an agent alongside the user. Generated motion graphics are secondary and can come from tools like HyperFrames rendered as overlay clips.

**Non-goals for v1:** mobile apps, cloud sync, a template marketplace, real-time multi-user collaboration.

## System layers

```mermaid
flowchart TB
    subgraph Clients
        CLI["ve CLI<br/><small>scripts and CI</small>"]
        Agents["Agents<br/><small>via CLI + skills</small>"]
        UI["Reference UI<br/><small>Tauri, simple or pro</small>"]
        FE["Your frontend<br/><small>any stack, any style</small>"]
    end
    Agents --> CLI
    Protocol["Protocol: JSON-RPC 2.0<br/><small>OpenRPC describe · JSON Patch change events · jobs · revisions</small>"]
    CLI <--> Protocol
    UI <--> Protocol
    FE <--> Protocol
    subgraph Core["Core engine (Rust, headless)"]
        Model["Model + ops<br/><small>OTIO timeline, pure commands</small>"]
        History["History<br/><small>undo groups, actors, revs</small>"]
        Render["Render<br/><small>wgpu compositor, frame server</small>"]
        Media["Media I/O<br/><small>FFmpeg decode, proxies, export</small>"]
        Analyzers["Analyzers<br/><small>silence, scenes, loudness</small>"]
    end
    Protocol <--> Core
    subgraph Plugins["Plugins (built-ins ship this way too)"]
        Shader["Shader plugins<br/><small>WGSL effects and transitions, run inside the compositor</small>"]
        Process["Process plugins<br/><small>commands, analyzers, generators, import/export; JSON-RPC over stdio</small>"]
    end
    Shader --> Render
    Process <--> Protocol
```

Clients sit above one protocol; the core and plugins sit below it. Agents drive the engine through the CLI. Shader plugins run inside the compositor; process plugins call back through the protocol like any client.

**Two run modes, one code path**

- **Direct mode:** with no daemon running, each CLI call opens the project folder under a file lock, applies the command, saves and exits. Good for scripts, CI and one-shot agent tasks.
- **Session mode:** `ve daemon start` keeps the project in memory, owns the frame cache and decoders, and streams change events. The CLI detects the daemon and routes calls to it, so a UI watching the session sees agent edits land live.

Both modes run the same command functions from `ve-ops`; the daemon only adds caching, jobs and the event stream.

## Project format

The project file is an OpenTimelineIO (OTIO) JSON document. Anything OTIO cannot express lives in OTIO `metadata` under one namespaced key, so other OTIO tools read the edit and ignore our extensions.

**Rules**

- **Time is always `RationalTime`** (`value` + `rate`). No floats anywhere in the model or the protocol. UIs display SMPTE timecode.
- **Stable IDs.** Every track, clip, effect, keyframe and marker gets a short ID (e.g. `clip_7f3a`) stored in metadata. Commands address IDs, never indexes.
- **Media by reference.** Source files are `ExternalReference`s. Proxies, analysis output and AI-baked assets are tracked in an asset table with hashes.
- **Revision counter** on the document, incremented by every committed command.
- **Schema version** in metadata, with migrations from day one.

**Extension namespace (`ve` used as a placeholder name)**

| Where | Key | Holds |
| --- | --- | --- |
| Timeline | `metadata.ve.project` | schema version, revision, output settings (resolution, fps, sample rate, color space) |
| Timeline | `metadata.ve.assets` | asset table: id, path, hash, kind (source, proxy, analysis, baked), provenance |
| Clip | `metadata.ve.clip` | stable id, speed, volume, transform, effect stack |
| Clip effect | `metadata.ve.effects[]` | id, plugin effect id, params, keyframes |
| Transition | `metadata.ve.transition` | plugin transition id, params |
| Marker | `metadata.ve.marker` | stable id, source (user, agent, analyzer), payload |

**Example clip**

```json
{
  "OTIO_SCHEMA": "Clip.2",
  "name": "intro_take3",
  "source_range": {
    "OTIO_SCHEMA": "TimeRange.1",
    "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 312, "rate": 30 },
    "duration":   { "OTIO_SCHEMA": "RationalTime.1", "value": 450, "rate": 30 }
  },
  "media_references": {
    "DEFAULT_MEDIA": { "OTIO_SCHEMA": "ExternalReference.1", "target_url": "media/IMG_4410.MOV" }
  },
  "metadata": {
    "ve": {
      "clip": { "id": "clip_7f3a", "speed": [1, 1], "volume_db": 0 },
      "effects": [
        {
          "id": "fx_02",
          "effect": "core.color/exposure@1",
          "params": { "exposure": 0.3 },
          "keyframes": { "exposure": [
            { "t": { "value": 0, "rate": 30 }, "v": 0.0, "ease": "linear" },
            { "t": { "value": 30, "rate": 30 }, "v": 0.3, "ease": "ease-out" }
          ] }
        }
      ]
    }
  }
}
```

Keyframe times are relative to the clip's start on the timeline. Speed is a rational (`[num, den]`) for the same reason time is.

## Protocol

One protocol for every client: JSON-RPC 2.0 over a local socket (Unix socket, Windows named pipe) and optionally WebSocket. Commands change the project; queries read it; notifications stream changes out.

**Method families**

| Family | Examples | Notes |
| --- | --- | --- |
| `project.*` | `open`, `save`, `get`, `summary` | `summary` returns compact text for agents, not the full JSON |
| `media.*` | `import`, `probe`, `assets` | probe returns ffprobe-shaped JSON |
| `edit.*` | `insert`, `split`, `trim`, `move`, `ripple_delete`, `set_speed` | all mutate; all take `base_rev` |
| `fx.*` | `add`, `set_param`, `set_keyframe`, `remove`, `add_transition` | params validated against the effect's JSON Schema |
| `render.*` | `frame`, `range_preview`, `export` | `frame` and `range_preview` are the agent's eyes |
| `analyze.*` | `silence`, `scenes`, `loudness` | built-in analyzers; plugins add more |
| `history.*` | `undo`, `redo`, `list`, `begin_group`, `end_group` | groups collapse one agent action into one undo step |
| `jobs.*` | `list`, `get`, `cancel` | long-running work |
| `system.*` | `describe`, `version` | `describe` returns the OpenRPC document (plugin methods included) |
| `plugin.*` | `list` | installed plugins, their commands, secrets and load issues |
| `transcript.*` | `set`, `get`, `list`, `remove` | per-asset transcripts (made by plugins), words on the timeline |

**Self-description.** `system.describe` returns an OpenRPC document generated from the Rust code: every method, its params as JSON Schema, every installed plugin's effects and commands, and every event type. This is the contract agents and frontend builders read. It cannot drift because it is generated.

**Revisions and conflicts.** Every mutating call carries `base_rev`. If the project has moved on, the call fails with `stale_revision` and the current rev; the client re-reads and retries. Single writer, optimistic concurrency. No CRDTs.

**Actors.** Every call carries `actor`: `{ "kind": "user" | "agent" | "plugin", "id": "..." }`. History records it, so a UI can show and undo "the agent's last 12 edits" as one action.

**Change events.** Every committed command emits a notification:

```json
{
  "jsonrpc": "2.0",
  "method": "project.changed",
  "params": {
    "rev": 42,
    "actor": { "kind": "agent", "id": "claude" },
    "command": "edit.ripple_delete",
    "patch": [
      { "op": "remove", "path": "/tracks/children/0/children/4" },
      { "op": "replace", "path": "/metadata/ve/project/revision", "value": 42 }
    ]
  }
}
```

A frontend's whole sync story: load the document once, apply each `patch` (RFC 6902) in rev order, re-fetch if a rev is skipped. Undo is the stored inverse patch.

**Jobs.** Anything slower than ~200 ms (export, proxies, analysis, AI plugins) returns `{ "job_id": ... }` immediately and emits `job.progress` and `job.finished` notifications. Jobs are cancellable.

**Dry runs.** Every mutating method accepts `dry_run: true` and returns the patch it would apply without committing. Agents use it to preview destructive edits.

## Render pipeline

FFmpeg handles decode and encode; a wgpu compositor handles everything between. One render graph serves preview frames, range previews and final export, so they cannot disagree.

**Stages**

1. **Resolve.** For time `t`, walk the timeline and list active clips per track, their source times (after speed and trims), effects with keyframes evaluated at `t`, and any transition in progress.
2. **Decode.** Pull source frames through a decoder pool. Each source keeps a warm decoder with a GOP-aware seek (seek to the prior keyframe, decode forward). Proxies are used when the request is low-res.
3. **Upload.** Frames go to GPU textures in a linear working color space (convert from Rec.709/sRGB on the way in).
4. **Composite.** Per track: apply the clip's effect stack (shader passes), then the transform; blend tracks bottom to top; apply transitions where two clips overlap.
5. **Output.** Convert back to the output color space. Preview: read back to RGBA and encode as JPEG/PNG. Export: hand frames to the encoder.
6. **Audio.** Separate graph: decode, resample (`rubato`), apply gain and fades, mix tracks, measure loudness (`ebur128`). Export muxes audio with video; preview uses the audio clock as master.

**Frame server (what frontends and agents use)**

- `render.frame { t, width }` returns one composited frame. Target: under 150 ms at 640 px with proxies.
- `render.range_preview { in, out, width }` returns a low-res MP4 for a range. This is how an agent checks a cut.
- `render.contact_sheet { in, out, count }` returns a grid image of evenly spaced frames, the cheapest way for a model to see a range.
- **Playback stream (later, after jobs): HLS.** The daemon serves the timeline as an HLS playlist of short fMP4 segments (2 to 4 s) from its existing HTTP server, behind the same token. Browsers play it with hls.js (Safari natively), so a web frontend gets real playback with sound, and a hosted version later needs no new transport. Segments come from the same render graph as export, so playback cannot disagree with the final file. Each segment is cached by a hash of everything in its time range, so an edit re-renders only the segments it touches; the playlist is regenerated on every revision and the player reloads it. Jobs render segments ahead of the playhead. Scrubbing stays on `render.frame` (HLS is for playing, not frame-exact seeking). Generated clips with a live preview draw over the player, as described under generators. Known risks: start-up delay (one segment render, roughly 0.5 to 1.5 s), and AAC priming gaps at segment joins (render audio as one continuous stream, or use longer segments, if clicks show up). Considered and rejected for now: raw frames over WebSocket (no standard player, no audio sync for free) and MSE over WebSocket (same result as HLS with more custom code).

**Caches**

- **Proxies:** generated on import as a job (e.g. 540p, all-intra codec) so seeks are cheap.
- **Thumbnails and waveforms:** generated per asset, served to UIs as files.
- **Frame cache:** keyed by a hash of everything that affects the frame (clip, source time, effect params). A changed parameter invalidates exactly the frames it touches.

**Determinism rule.** A frame is a pure function of the project and `t`. Effects get time and params, nothing else: no wall clock, no random seeds that are not params, no state carried between frames. This is what makes any frame renderable on demand and caching safe.

**Encoding and licensing.** Ship an LGPL FFmpeg build and prefer hardware encoders (NVENC, QuickSync, AMF, VideoToolbox) plus `openh264`. Users can point the engine at their own FFmpeg if they want x264/x265. Shelling out to the `ffmpeg` binary for export is acceptable for v1; move to `ffmpeg-the-third` bindings where per-frame control is needed (preview decode).

**Decoding for the compositor, in two steps.**

1. **Now: warm ffmpeg processes** (`ve-render::decoder`). One long-lived `ffmpeg` per layer streams raw frames, already conformed to the source rate and scaled (in 4:2:0, like the Phase 1 graph) to the size the layer is drawn. The next frame, or a jump of up to 30 frames ahead, reads from the open pipe; a jump back or further ahead restarts that process (about 100 ms on Windows). Idle processes close after 30 s. Measured on 1080p H.264 (optimized build, RTX 4070): 9 ms per frame at 640 px and 54 ms at 1920 px when frames come in order, against 130 to 140 ms for a one-shot ffmpeg render. Random jumps stay near the cost of starting ffmpeg.
2. **Later: in-process decoders** with `ffmpeg-the-third`, kept open per source with GOP-aware seeks, so random scrubbing costs one seek (about 40 ms for 1080p long-GOP) instead of a process start. Needs, decided when we get there: LLVM/libclang on build machines (bindgen), an LGPL shared FFmpeg build with headers per platform, and shipping its DLLs/dylibs with the engine. `DecoderPool::frame` is the seam: same inputs, same RGBA out, so the compositor does not change.

## Effects and transitions

Effects, transitions and generators share one manifest format: an id, a `kind` (`effect`, `transition` or `generator`), params as JSON Schema (with `x-animatable` on params that take keyframes), and a `runtime` that says how the code runs. The compositor stays in the core; effects plug into it.

**Runtimes live in the core; effects are plugins.** The core ships four runtimes, each wired into how frames move (GPU textures, the frame cache, the determinism rule, sandboxing, timeouts). Runtimes are not pluggable: a fifth way to run code would gain little and could break every effect at once. A plugin package can hold any mix of manifests; the core hands each to the runtime it names.

| `runtime` | Author writes | Runs | Safety | For |
| --- | --- | --- | --- | --- |
| `wgsl` (default, first) | one WGSL function | on the GPU, inside the compositor; well under 1 ms per 1080p frame | naga validation plus a timed test render; cannot touch files | most transitions and looks; agent-written effects; gl-transitions import |
| `wasm` (second) | Rust, C++, Zig or anything that compiles to WebAssembly | CPU, in `wasmtime`; costs a GPU readback and upload per frame (a few ms at 1080p) | sandboxed: no files, no network, cannot crash the engine; one build runs on every OS | work that is not per-pixel: tracking, stabilization, complex logic |
| `ofx` (later) | an OpenFX plugin (C ABI; the standard Resolve, Nuke and Vegas host) | native CPU or CUDA, in-process | none: can crash the engine and reach the machine, so only installed plugins the user trusts, never project-local | running existing pro plugins; maximum CPU speed |
| `process` | anything: HTML, Blender, Python, a model | its own process, baked as a job (see Generated clips) | separate process, user permissions | titles, motion graphics, AI shots. Not for transitions |

Where it matters for speed, `wgsl` is the fast path: a GPU beats a CPU loop for per-pixel work, so a Rust or C++ author reaches for `wasm` or `ofx` only when the work is not shader-shaped. All four obey the determinism rule: output depends only on the input frames, time and params.

**Getting code into the core.** A clip lists effects in `metadata.ve.effects` by id with params and keyframes. Code comes from built-ins (`plugins/core`, shipped as plugins), installed plugins, and the project's `effects/` folder; an agent can also send source over the protocol (`fx.define`), which goes through validation before use. `ofx` plugins load from installed plugins only.

**The OpenFX host** implements the OpenFX C API against our frames and params. It needs `unsafe` FFI, so it lives in its own crate (`ve-ofx`) as a second approved exception to the workspace's `unsafe_code` rule, behind a cargo feature.

The `wgsl` contract is deliberately tiny so an agent can write a working one in a single attempt.

**Effect contract**

The engine wraps the author's function in a full shader that handles sampling, uniforms and output. The author writes only this:

```wgsl
// Inputs provided by the engine:
//   src(uv) -> vec4<f32>      sample the clip's frame (linear RGB, premultiplied alpha)
//   u.time: f32               seconds since clip start
//   u.progress: f32           0..1 across the clip
//   u.resolution: vec2<f32>   output size in pixels
//   p.<param>                 parameters from the manifest, keyframes already evaluated

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = src(uv);
    let gray = dot(c.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    return vec4<f32>(mix(c.rgb, vec3<f32>(gray), p.amount), c.a);
}
```

**Transition contract** (mirrors gl-transitions)

```wgsl
// outgoing(uv), incoming(uv) -> vec4<f32>   the two clips (`from` is reserved in WGSL)
// u.progress: f32                           0..1 across the transition

fn transition(uv: vec2<f32>) -> vec4<f32> {
    let edge = smoothstep(p.softness * -1.0, 0.0, uv.x - u.progress * (1.0 + p.softness));
    return mix(incoming(uv), outgoing(uv), edge);
}
```

**Manifest** (`effect.json`)

```json
{
  "id": "core.color/desaturate",
  "version": "1.0.0",
  "kind": "effect",
  "runtime": "wgsl",
  "name": "Desaturate",
  "shader": "desaturate.wgsl",
  "params": {
    "type": "object",
    "properties": {
      "amount": { "type": "number", "minimum": 0, "maximum": 1, "default": 1, "x-animatable": true }
    }
  },
  "skill": "SKILL.md"
}
```

`params` is plain JSON Schema. `x-animatable` marks params that accept keyframes. UIs build a settings panel straight from it.

**Importing gl-transitions.** The gl-transitions library is GLSL with the same shape (`from`, `to`, `progress`, uniforms). An importer translates GLSL to WGSL with `naga` and generates the manifest from its uniform declarations, giving a large transition library on day one. The library is not core: it is not compiled into the engine.

- *A pack (Oct 25 2026):* the converted set is the `gl-transitions` effect pack in `plugins/gl-transitions/` (125 transitions, ids `gl.transition/<name>`, each WGSL starting with its shader's own author and license header, plus the repository's LICENSE). The editor's `scripts/import_gl_transitions.py` writes it, and the editor's own copy keeps only the GLSL its preview cards run. The desktop editor bundles the pack and installs it on start when the installed version differs (it asks `ve plugin list` and runs `ve plugin install`); from the CLI, `ve plugin install plugins/gl-transitions`. Using one copies it into the project (copy on use), so a project renders anywhere. Projects from before the pack keep their `project.gl-<name>` copies. Still to do: the built-ins in `plugins/core` (that needs release builds to ship and install `plugins/core`).

**Agent-authored effects: the safety path**

1. Agent writes `effect.json` + `.wgsl` into the project's `effects/` folder (or calls `fx.define` with the source).
2. `naga` parses and validates the WGSL against the wrapper. Errors return to the agent with line numbers so it can fix them.
3. Engine compiles the pipeline and renders a 3-frame test at low res with a timeout. A hang or a NaN-filled output fails the test.
4. On pass, the effect registers as `project.<name>` and is usable immediately. Project-local effects travel with the project file.
5. A user can promote a project effect into an installed plugin with one command.

**Out of scope for v1:** multi-pass effects (blur pyramids, temporal effects that read neighboring frames). Design the manifest so a `passes` array can be added later without breaking single-pass effects.

## Plugin system

A plugin is a folder with a `plugin.json` manifest. It can contribute any mix of the five extension types below, and built-in features ship as plugins through the same API.

| Type | Contributes | Runs as | Example |
| --- | --- | --- | --- |
| Effect / transition | a manifest per effect with its `runtime`: WGSL shader, WASM module or OpenFX binary | inside the compositor (see Effects and transitions) | film grain, glitch wipe, a Rust stabilizer |
| Command | new protocol methods (macros over `edit.*`) | external process | `jumpcut.remove_silences`, `beatsync.cut_to_beats` |
| Analyzer | markers + analysis assets | external process, as a job | transcription, face tracking, beat detection |
| Generator | new media assets, or editable generated clips (template + params) | external process, as a job | TTS voiceover, background-removal matte, HyperFrames lower third, Blender scene |
| Import / export | project format adapters | external process | FCPXML, EDL, Premiere XML via OTIO adapters |

Every plugin can also ship a `SKILL.md` that teaches agents when and how to use it.

**Manifest** (`plugin.json`)

```json
{
  "id": "community.jumpcut",
  "version": "0.3.0",
  "engine": ">=0.4, <0.6",
  "protocol": "1",
  "name": "Jump Cut Tools",
  "contributes": {
    "commands": [
      { "method": "jumpcut.remove_silences", "summary": "Cut every pause", "params": "schemas/remove_silences.json",
        "mutates": true, "job": true, "cli": { "name": "jumpcut", "args": [] } }
    ]
  },
  "run": { "command": "python3", "args": ["main.py"], "windows": { "command": "python" } },
  "secrets": [],
  "requires": { "gpu": false, "downloads": [] },
  "skill": "SKILL.md"
}
```

**Effect packs (built).** `contributes.effects` lists effect folders
(each holds an `effect.json`, or is a folder of them); a plugin with only
effects needs no `run`. `fx.list` lists pack effects as `origin: "pack"`
(found with the same discovery as other plugins, `ve_plugin::discover`, no
host needed), and `effect.add`, `transition.add` and `transition.set` copy
a pack effect into the project's `effects/` before they commit (not on a
dry run; removed again if the edit fails). Rendering only reads built-ins
and `effects/`, so a project never depends on what is installed. The
project's copy wins over the pack (`id@major` resolves in the project
first); a new major is copied next to the old one. Built-in ids and
`project.` ids are refused in packs.

**Process plugins (built, `ve-plugin` + `ve-daemon::plugins`).** The engine starts the plugin's process on the first call to one of its methods and talks newline-delimited JSON-RPC over stdio, the same pattern as LSP: an `initialize` handshake, then calls; `$/progress` and `$/log` from the plugin, `$/cancel` and `shutdown` from the engine. The plugin can call back into any engine method (`media.extract_audio`, `transcript.set`, `edit.*`, `render.frame`) on the same pipe; methods that record an actor get `plugin:<id>` whatever the plugin sends. Any language works. Processes stop after 5 idle minutes, and on engine exit (they must also exit when stdin closes). Calls run on plain threads (the daemon's blocking pool or a job thread) and wait for the answer; the plugin's own calls are dispatched through the same `Registry`.

- **Registration.** The method registry is sealed before a project is open, so plugin methods are not registered: the registry has one `Extension` (the plugin host) that it asks for any method it lacks, and whose methods it adds to `system.describe` (`"x-plugin": "<id>"`). Built-in methods always win, and a plugin whose method uses an engine namespace is refused at load.
- **Jobs.** `job` commands take `background: true` and run on the jobs module; `$/progress` feeds the job's progress, `jobs.cancel` sends `$/cancel` and kills the process after 5 s.
- **Edits.** For `mutates` commands the host opens an undo group for `plugin:<id>` around the call, so all its edits undo as one step. Plugin edits still go through `base_rev` like anyone's. (A real transaction, one revision for the whole call, is still open; groups cover undo.)
- **Secrets.** Declared in the manifest, stored in the OS keychain (`keyring`, `ve plugin secret set`), passed as environment variables at spawn; an engine environment variable of the same name wins. Missing secrets fail the call before the process starts, naming the command to fix it.
- **CLI.** A command's `cli` gives it a `ve <name>` subcommand: unknown subcommands are looked up with `plugin.list` and their arguments mapped onto the params schema (positionals from `cli.args`, the rest as flags).
- **Errors.** Plugin errors are `plugin_error` (exit 5) unless they pass on an engine error code; a crash fails the call with the tail of the plugin's log (`<project cache>/plugins/<id>.log`), and the next call restarts it.

**Generated clips.** A generator can also make a clip that stays editable on the timeline: an animated lower third or title from HyperFrames, a 3D scene from Blender or three.js, an AI video shot, a caption style drawn in HTML. The clip keeps the recipe. While you edit it plays live; pixels are baked only when something needs them.

- **Recipe on the clip.** `metadata.ve.generated` holds the plugin id and version, the template and its params. Bakes are `baked` assets with provenance, cached by segment and params hash, so reopening a project never re-renders.
- **Live while editing.** A generator can ship a live preview: for HTML generators, the same page the bake renders, driven by the playhead time. A UI draws it over the player, so a changed param shows at once with no render. Generators without one (a Blender scene, an AI shot) show their last bake, or a placeholder until the first.
- **Bake only when pixels are needed.** The engine bakes through the plugin, as a job: the whole clip on export; only the frames asked for on `render.frame`, `render.contact_sheet` and `render.range_preview`, so agents and other clients see overlays too; and in the background when the daemon is idle, so export rarely waits. A param change invalidates only the segments it touches.
- **Output.** A bake is video with alpha (ProRes 4444, VP9 WebM with alpha, or a PNG sequence) at the project's size and frame rate. The compositor places it on an overlay track like any other clip.
- **Same look live and baked.** Frame N depends only on the template, the params and N: no wall clock, no randomness that is not a param, fonts and assets bundled with the template. For HTML, the UI's webview (WebView2 is Chromium) and the bake's headless Chrome are the same engine. Where they differ, the bake is the reference.
- **Plugins bring their own renderer** (headless Chrome, Blender, a model). The core never ships one, and anyone can write a different generator for the same job.
- **Needs from the core:** jobs, a compositor that stacks video tracks with alpha (also needed for text, masks and picture-in-picture), the `baked` asset kind (exists), and a small preview contract: a method that gives a UI the generator's preview page and the clip's params, which the UI then drives with the playhead time.

**Discovery.** Plugins load from the project's `plugins/` folder, then the user's plugin folder (`VE_PLUGINS_DIR`, else `<OS data dir>/ve/plugins`; `ve plugin install` copies there); a built-in set comes when the analyzers move to `plugins/core`. `plugin.list` lists them (rescanning first, with load problems as `issues`); `system.describe` merges their methods and schemas into the OpenRPC document.

**Transcripts (core data, plugin-made).** The core owns the transcript shape (`ve-model::transcript`): words with media-time start/end (exact decimal seconds to the millisecond), text, speaker, confidence, plus language, speakers and provenance. A transcript belongs to a source asset and lives in the project folder (`transcripts/<file hash>.json`), so it survives edits and re-imports. `transcript.set` stores one (from any plugin or tool), `transcript.get` returns it in media time or maps it onto the timeline through the audio clips (frame-floored start, frame-ceiled end, so `start..end` cuts exactly the word), `transcript.list|remove`. Vendor formats (Deepgram, WhisperX) are converted in their plugins, never in the core. `media.extract_audio` hands plugins a WAV or FLAC of an asset (16 kHz mono by default) from the cache, so no plugin needs a decoder. The first STT plugin, `plugins/deepgram` (Python standard library, the user's own key), is not compiled in or installed by default.

**Versioning.** Plugins declare the engine range and protocol major version they support. The engine refuses to load mismatches with a clear error. A prerelease engine (`0.1.0-rc.1`) also matches as its release (`0.1.0`), so `*` and `>=0.1` ranges work on release candidates. Protocol changes follow semver: additive in minors, breaking only in a new major with a deprecation window.

**Sandboxing.** Process plugins run with the user's permissions, like VS Code extensions. `wgsl` effects are sandboxed by the GPU validation path and `wasm` effects by `wasmtime`. If a public plugin registry happens, extend `wasmtime` (WASI) to commands and analyzers too, with explicit capabilities (read project, write edits, read media folder) for untrusted plugins. `ofx` plugins are never sandboxed and load only when installed by the user.

**Dogfooding rule.** Silence detection, scene detection, the default transitions and the color effects ship as plugins in `plugins/core`. If the plugin API cannot express a built-in feature, fix the API, not the feature.

## AI integration

The core contains no models. AI enters at three levels, and all three use mechanisms the engine needs anyway.

| Level | What it is | Needs from the core |
| --- | --- | --- |
| Driver | An agent edits through the CLI and skills (Claude Code, Codex, Cursor, Gemini CLI) | `describe`, compact `project.summary`, frame and range previews, dry runs, undo groups |
| Capability plugin | Analyzers and generators: transcription, background removal, upscaling, denoise, auto-reframe, TTS | jobs with progress and cancel, asset table, provenance, GPU and download declarations |
| AI-native frontend | A chat-first editor that embeds a model and talks to the protocol | the same protocol any frontend uses; nothing extra |

**Perception for agents.** Agents need cheap, precise ways to see footage. Video-capable models can read frames or clips directly, but they sample sparsely and cost many tokens, so the engine provides a ladder from cheapest to richest:

1. `project.summary`: tracks, clips, durations and markers as compact text.
2. Analyzer markers: silences, scene cuts, loudness, and transcript words when a transcription plugin is installed.
3. `render.contact_sheet`: one grid image for a range.
4. `render.frame`: exact frames at the cut points.
5. `render.range_preview`: a low-res clip for models that accept video.

**Bake, don't re-run.** AI output is non-deterministic, so generators write an asset (a matte video, an audio file, an upscaled clip) and the timeline references it. Rendering stays deterministic and re-renders never call the model again.

**Provenance.** Every baked asset records plugin id and version, model name, parameters, input asset hashes and timestamp in the asset table. That supports regeneration, auditing and honest labeling of AI-generated media.

**Agent edit loop (taught by the core skill)**

1. Read `project.summary` and the relevant markers.
2. Plan the edit; for destructive edits run it with `dry_run` first.
3. Wrap the edit in `history.begin_group` / `end_group`.
4. Check cut points with `render.frame` or a contact sheet.
5. Fix, or report what changed and how to undo it.

## CLI and agent skills

The CLI is a thin client: each subcommand maps to one protocol method. If a daemon is running for the project it sends the call there; otherwise it opens the project file directly under a lock, runs the call, saves and exits.

**Conventions**

- Non-interactive by default. Never prompts; destructive commands need `--yes` or run as `--dry-run`.
- `--json` on every command for machine-readable output; human-readable text otherwise.
- Times accept timecode (`00:01:12:12`), frames (`f2172`) or seconds with a rate (`72.4s`), and are normalized to `RationalTime`.
- Exit codes: 0 ok, 2 invalid params, 3 stale revision, 4 render failure, 5 plugin error.
- `--actor` defaults to `user`; the agent skill sets `--actor agent:<name>`.

**Initial command set**

```bash
ve init my-edit --fps 30 --size 1920x1080
ve import media/*.MOV                       # probes, creates proxies as a job
ve summary                                  # compact timeline text
ve describe --json                          # OpenRPC document

ve insert asset_03 --track V1 --at 00:00:10:00
ve split clip_7f3a --at 00:01:12:12
ve trim clip_7f3a --in +15f --out -1s
ve move clip_7f3a --to 00:00:42:00 --track V2
ve ripple-delete --from 00:02:10:00 --to 00:02:14:15 --dry-run
ve speed clip_7f3a 3/2

ve fx add clip_7f3a core.color/exposure --set exposure=0.3
ve fx key fx_02 exposure --at 0f=0 --at 30f=0.3 --ease ease-out
ve fx define ./effects/grain.json           # validate + test-render + register
ve transition add clip_7f3a clip_8b21 core.wipe/soft --duration 12f

ve analyze silence --threshold -40dB --min 0.4s
ve analyze scenes
ve frame --at 00:01:12:12 --width 640 -o cut.jpg
ve sheet --from 00:01:00:00 --to 00:01:30:00 --count 12 -o sheet.jpg
ve preview --from 00:01:00:00 --to 00:01:30:00 -o check.mp4
ve export --preset youtube-1080p -o final.mp4

ve undo --group
ve history --actor agent
ve daemon start                             # live session for UIs
ve plugin list | install <path> | secret set <plugin> <NAME>   # (`new` scaffolding: later)
ve transcribe asset_94b8                    # a plugin's command (cli name from its manifest)
ve transcript                               # the timeline's words as readable lines
```

**Skills shipped with the engine**

| Skill | Teaches |
| --- | --- |
| `video-edit` | the edit loop, time formats, IDs, undo groups, how to check cuts |
| `video-effects` | the effect and transition contracts, how to define, test and fix shaders |
| `video-plugin-dev` | scaffolding a process plugin, calling back into the engine, packaging (today: `skills/ve-core/guides/writing-plugins.md`) |
| `video-frontend-dev` | building a UI on the protocol: load, subscribe to patches, frame server, `describe` |

Plugins add their own skills next to these. `ve plugin new` scaffolds one.

## Rust workspace

One Cargo workspace. The model crate has no I/O and no GPU so it stays fast to test; everything heavy sits behind it.

```text
ve/
  crates/
    ve-model/      timeline types (OTIO serde), RationalTime, IDs, validation, migrations
    ve-ops/        commands as pure functions: (doc, params) -> (new doc, patch, inverse patch)
    ve-history/    undo/redo, groups, actors
    ve-media/      probing, decode pool, proxies, thumbnails, waveforms (FFmpeg)
    ve-render/     resolver, wgpu compositor, effect wrapper, frame cache, audio graph
    ve-export/     encode + mux, presets
    ve-analyze/    silence, scenes, loudness (built-in analyzers)
    ve-plugin/     manifest parsing, discovery, process host (JSON-RPC over stdio)
    ve-ofx/        OpenFX host (later; FFI, its own unsafe exception)
    ve-protocol/   method registry, JSON Schema + OpenRPC generation, error codes
    ve-daemon/     session server: socket/pipe + WebSocket, jobs, event stream
    ve-cli/        the `ve` binary
  plugins/core/    built-in effects, transitions, analyzers as plugins
  skills/          SKILL.md files shipped with the engine
  apps/desktop/    Tauri reference UI (optional client)
```

**Key dependencies**

| Need | Crate |
| --- | --- |
| Serialization | `serde`, `serde_json` |
| JSON Schema from Rust types | `schemars` |
| JSON Patch | `json-patch` |
| Exact rational time | `num-rational` |
| Async runtime | `tokio` |
| JSON-RPC server | `jsonrpsee` |
| CLI | `clap` |
| Decode / encode | `ffmpeg-the-third` (plus `ffmpeg` binary for export in v1) |
| GPU compositing | `wgpu` |
| Shader validation, GLSL to WGSL | `naga` |
| Text rendering | `cosmic-text` + `glyphon` |
| Audio output, resampling, loudness | `cpal`, `rubato`, `ebur128` |
| Image output | `image` |
| WASM effects (second runtime), later sandboxed plugins | `wasmtime` |
| OpenFX host (later runtime) | own crate `ve-ofx` over the OpenFX C headers (approved `unsafe` exception) |

Crate maintenance status changes; check recent release dates on crates.io before locking these in, especially the FFmpeg bindings.

**Testing strategy.** `ve-ops` gets property tests (every command's inverse patch restores the original document; total duration math holds after ripple edits). `ve-render` gets golden-frame tests: render known frames from fixture projects and compare against stored images with a small tolerance.

## Build order

Phase 1's gate is the real product test: if an agent can cut a 10-minute talking-head recording end to end through the CLI, the engine works and everything after it is extension.

| Phase | Weeks (solo, rough) | Deliverables | Gate to move on |
| --- | --- | --- | --- |
| 0 · Spike | 1–2 | decode a clip with FFmpeg; composite one frame in wgpu; OTIO round-trip with serde | one frame matches FFmpeg's own render |
| **1 · Headless core** | 3–8 | model, ops, history, method registry; CLI direct mode, export, frame/sheet/preview; silence + scene analyzers, `video-edit` skill | **an agent cuts a 10-min talking head end to end via the CLI only** |
| 2 · Effects | 9–12 | effect + transition contracts, `fx define`; gl-transitions import via naga; 10+ built-in effects and transitions | an agent writes a new effect that passes validation in 2 tries |
| 3 · Plugins | 13–16 | daemon, JSON-RPC, OpenRPC describe; patch events, jobs, process plugins; `video-plugin-dev` skill | a jump-cut plugin built from describe + skill with no engine changes |
| 4 · Frontends | 17+ | minimal Tauri reference UI; live playback driven by the audio clock; `video-frontend-dev` skill | an LLM builds a working frontend from describe + skill alone |

The method registry and JSON Schema generation start in Phase 1 even though the daemon arrives in Phase 3, so the CLI is built on the protocol from the first commit. The plugin API is frozen only after Phase 2's built-in effects prove it, per the dogfooding rule.

## Open decisions and risks

**Decisions**

- [ ] Product name and CLI binary name (`ve` is a placeholder; the metadata namespace follows it).
- [x] License: MIT OR Apache-2.0 (decided Oct 25, 2026), the Rust ecosystem default, so anyone can build on the engine, including commercial frontends.
- [x] The project is a folder (`project.otio` + `effects/` + `plugins/` + `transcripts/`); caches live in the OS cache dir.
- [x] `ve-model` implements the OTIO schema directly with `serde` (no binding), checked against Python `opentimelineio`.
- [ ] Target platforms for v1: Windows, macOS and Linux. Development so far is on Windows; macOS and Linux need a full test run before the first release.

**Risks**

| Risk | Why it bites | Mitigation |
| --- | --- | --- |
| Seek performance on phone and long-GOP footage | slow frames make the agent loop and any UI feel broken | proxies on import, warm decoder pool, frame cache |
| Variable frame rate sources | iPhone and screen recordings drift out of A/V sync | normalize to constant frame rate in proxies; map source times explicitly |
| Color handling | wrong conversions give washed-out or crushed exports | one linear working space, explicit input/output transforms, golden-frame tests |
| Protocol churn | third-party frontends and plugins break | version from day one, generate OpenRPC, deprecation windows |
| Plugin API designed too early | wrong abstractions get locked in | build 10+ built-in effects and two commands before freezing the API |
| Agent-written shaders | hangs, crashes, NaNs, driver resets | naga validation, timeout test render, project-local until promoted |
| FFmpeg licensing | GPL codecs force GPL on distribution | LGPL build, hardware encoders, user-supplied FFmpeg for x264/x265 |
| Scope creep toward a full NLE | months on UI polish instead of the engine | headless milestones first; the reference UI stays minimal until Phase 4 |

## TODO

**Real-time playback (the critical path for a CapCut-like feel).** Today
playback plays short MP4 chunks. They are kept by content and built ahead of
the playhead (and ahead of everything else where an edit just changed
something, newest edit first), so replaying or playing a prepared stretch
starts at once. But pressing play on a stretch the engine has not reached
yet, or right after an edit, still waits a couple of seconds for the first
chunk. Removing that wait for good needs two things:

- [ ] **Proxies on import:** all-intra, lower-resolution copies made in the
  background, so phone and other long-GOP footage decodes fast for preview
  (exports keep reading the originals). See the seek risk below.
- [ ] **A streaming playback path:** frames go to the player as they are
  composited (with audio kept in sync) instead of being encoded into a video
  file first. The planned HLS stream ("Frame server") does this for the web;
  the desktop app may stream frames more directly. Effects and transitions
  feel bad without it.
- [ ] **In-process decoders** (`ffmpeg-the-third`, step 2 of "Decoding for
  the compositor"): one seek (about 40 ms) per random jump instead of a
  process restart (100 to 200 ms), for scrubbing. Not decided yet: it needs
  LLVM on build machines, an LGPL shared FFmpeg build per platform, and
  shipping its DLLs. It is not the main cost of GPU export (that reads in
  order); profile export separately (RGBA pipes, synchronous readback, CPU
  RGBA to YUV).

Smaller gaps from the transform and preview work:

- [ ] Anti-aliased edges on rotated clips.
- [ ] Contact sheets (and the UI's timeline filmstrip) ignore transforms and
  effects: they still come from the ffmpeg path.
- [ ] Keyframes for transforms can only be set from the CLI; the UI shows them
  but cannot add or move them.
