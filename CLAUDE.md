# CLAUDE.md — Headless Video Engine

Headless, agent-friendly video editing engine in Rust (CapCut alternative). The
engine is the product; every UI, agent, script and plugin is a client of one
JSON-RPC 2.0 protocol. Primary use case: an agent editing real footage
(talking heads, podcasts, phone clips, screen recordings) alongside the user.

- Full design: `ARCHITECTURE.md` (exported copy). Live doc, which is ahead of the
  export and has a "Core crate stack" tab with researched crate picks:
  https://claude.ai/artifact/2wLqcMJu4wNw995uEt4exS
- `ve` is a placeholder name for the product, binary and `metadata.ve` namespace.

## Decisions already made (do not relitigate without asking)

- **AI-neutral core.** No models in core, ever: no bundled whisper, no STT. Core
  owns the data *shapes* (transcript asset with word timings/speakers/confidence,
  caption track, markers, jobs, provenance); every model lives in a plugin
  (local, BYOK cloud, or SaaS). Transcription is the first plugin, not built yet.
- **Agent interface is CLI + skills (SKILL.md), not MCP.** Don't build MCP. Don't
  make choices that would prevent someone else writing a thin MCP shim from `describe`.
- **Standards over invention:** OTIO JSON project, JSON-RPC 2.0 + OpenRPC,
  JSON Schema params, JSON Patch (RFC 6902) change events, WGSL effects.
- **OTIO stays the on-disk format (reconfirmed Oct 20 2026), with a checkpoint
  at edit ops (item 5).** Its main value is handoff to Resolve/Premiere/Avid; our
  speed/volume/effects live in `metadata.ve`, which other tools ignore. If the
  edit ops end up fighting OTIO's shape, switch to a native model with OTIO as
  import/export only. Either way, `set_speed` also writes OTIO's own
  `LinearTimeWarp` effect so speed survives the handoff.
- **Exact time only.** `Ratio<i64>` everywhere; no floats in model or protocol.
  OTIO stores doubles on the wire; `ve-model::time` snaps NTSC rates to n/1001.
- **Stable IDs** (`clip_7f3a`); commands address IDs, never indexes.
- **One run mode:** the CLI connects to a per-project daemon and spawns it if
  missing (idle timeout). No separate "direct mode". Only project-less
  commands (`init`, `probe`, `version`, `describe`) run in-process.
- **UI is deferred** (Oct 21 2026) until the core is further along. When it
  comes it is a separate client over the WebSocket (likely React), in a folder
  that is not git tracked; talk about it before building.
- **One method registry** (`ve-protocol::Registry` on jsonrpsee `RpcModule`);
  every transport calls `Registry::dispatch(raw_json)`. OpenRPC is generated
  from the same registrations, so it cannot drift.
- **Optimistic concurrency:** mutating calls carry `base_rev`; `stale_revision`
  on conflict. Every call carries an `actor` (user/agent/plugin). Dry runs on all
  mutations. Plugin callbacks run inside a transaction (`txn_id`) so a plugin's
  edits commit as one rev and one undo group.
- **Phase 1 renders with an FFmpeg `-filter_complex` graph via ffmpeg-sidecar**
  (trim/atrim, setpts, scale/crop, concat, afade). No wgpu until Phase 2. Phase 2
  adds the wgpu compositor and pipes composited frames into the FFmpeg encoder so
  preview and export share one graph again.
- **Default short audio crossfades on every cut**; edit ops must express J/L cuts.
- **Project = folder** (`project.otio` + `effects/` + `plugins/`). Caches and
  proxies go in the OS cache dir (etcetera), never the project folder.
- **SaaS seams kept open:** media addressed by URI (`file://` now), protocol
  returns bytes/asset refs (CLI writes files, protocol doesn't), BYOK secrets via
  OS keychain (keyring), auth hook on the WebSocket transport.
- **Dogfooding:** built-in analyzers/effects ship as plugins once the plugin host
  exists. If the API can't express a built-in, fix the API.

## License rules

- Never enable ffmpeg-sidecar's auto-download in release builds (it fetches GPL
  builds). It's behind `ve-media/download-ffmpeg`, dev only.
- Never add `dssim-core` (AGPL). Golden frames use `image-compare`.
- `symphonia` is MPL-2.0 (fine; file-level copyleft).
- gl-transitions importer must carry each shader's own license header.
- Engine license itself is still undecided (`license = "TBD"`).

## Workspace

`crates/`: ve-model, ve-ops, ve-history, ve-media, ve-render, ve-export,
ve-analyze, ve-plugin, ve-protocol, ve-daemon, ve-cli (`ve` binary).
All third-party versions are pinned once in root `[workspace.dependencies]`;
entries tagged `(Phase N)` are declared but unused. Add deps there, inherit with
`foo.workspace = true`. `unsafe_code` is forbidden workspace-wide, with one
approved exception: ve-cli is `deny` so `src/stdio.rs` can make its stdio
handles non-inheritable on Windows before spawning the daemon (otherwise the
daemon holds a caller's stdout pipe open and `ve ... | cat` hangs). ve-cli
copies the workspace lints by hand for that reason.

## Build

- Toolchain pinned in `rust-toolchain.toml` (1.97).
- This folder is inside OneDrive: set `CARGO_TARGET_DIR` to a path outside
  OneDrive before building, or `target/` syncs gigabytes.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets` must stay clean.
- Phase 1 needs `ffmpeg` and `ffprobe` on PATH.

## Status (Oct 2026)

Done: workspace scaffold; `RationalTime`/`TimeRange` with OTIO round-trip and
property tests; `Id`; `Registry` + OpenRPC generation + error/exit codes;
`ve version | describe | call`; `ve-ops::diff_patches` (forward + inverse);
OTIO schema types in `ve-model::otio` (Timeline, Stack, Track, Clip, Gap,
Transition, Marker, External/MissingReference) and typed `metadata.ve` blocks
in `ve-model::meta`, matching Python `opentimelineio` 0.18 output field for
field (`fixtures/otio/`, checked by `python scripts/otio_roundtrip.py`);
`ve-model::timecode` parses SMPTE (incl. drop-frame), `f2172` and `72.4s` and
formats SMPTE (matches OTIO's timecode output; we don't wrap at 24h).
Project folders (`ve-daemon::project`): `init`/`open`/`save` with an exclusive
lock file in the OS cache dir (`ve/locks/<hash>.lock`, not the project folder,
because of OneDrive), atomic saves, missing IDs assigned and saved on open.
Methods `project.init|open|save|get|summary`; CLI `ve init`, `ve summary`,
global `--project` (else nearest parent with `project.otio`). Error code
`project_locked` (-31003, exit 6).
Media (`ve-media`, `ve-daemon::media`): `media.probe` (no project needed),
`media.import` (base_rev + dry_run, all-or-nothing, dedup by blake3 hash,
bumps rev and saves), `media.assets`; CLI `ve probe`, `ve import`. Asset rows
carry a compact `media` block (exact duration, size, nominal rate, rotation,
vfr flag, first audio stream); full ffprobe JSON is not stored. Media URLs are
absolute `file://`. VFR footage keeps `r_frame_rate` as its rate. No proxies
yet (needs jobs).
Edit ops (`ve-ops`): insert, split, trim (ripple or roll), move (overwrite),
ripple_delete (ranges, marker names or clips), delete (lift), set_speed,
set_volume, marker add/remove, as pure fns over flattened tracks
(`ve-ops::lane`), with proptests (patches invert, linked clips stay in sync,
clips never read past their media). Clips inserted from an A/V asset share a
`metadata.ve.clip.link` and are edited together unless `linked: false` (how
J/L cuts are made: roll one side). Timeline times snap to the project frame
grid. Speed convention: `source_range.duration` is timeline time, media used
is `duration * speed` (OTIO's time-warp convention), and `LinearTimeWarp` is
written too. Patches come from our own prefix/suffix JSON diff
(`ve-ops::diff`) so a ripple delete is a few ops, not a rewrite of the track.
Every mutating method goes through `ve-daemon::commit` (base_rev, dry_run,
actor, save, history). History (`ve-history`): per-actor undo groups, undo
and redo are new revisions, persisted in the OS cache dir
(`ve/projects/<hash>/history.json`) so undo works across CLI calls; dropped if
the file's revision moved elsewhere. serde_json has `float_roundtrip` on and
`RationalTime` reads doubles back as the simplest exact fraction (needed once
speed changes make fractional media in-points).
**OTIO checkpoint (item 5): keep OTIO.** Flatten/edit/rebuild per command was
simple, implicit positions keep patches small, and Python OTIO reads edited
projects unchanged (positions and `LinearTimeWarp` agree).
Render (`ve-render`): resolver (`plan`) + filtergraph compiler (`graph`),
run through the ffmpeg binary with the graph in a script file
(`-/filter_complex`, or `-filter_complex_script` before 7.0). Frame-exact by
construction: fps-conform to the source rate, trim by frame/sample counts,
pad/cut every segment to the exact count its timeline span covers. Segments
reading one file forward share one input (`-ss` + split), so a jump-cut
timeline is one linear decode; geometry (scale/pad or crop) is applied once
per input before the split, and plain segments skip fps/tpad (each held a
full frame; a 160-cut 1080p export went from 1.6 GB to ~650 MB peak). Audio:
default 20 ms crossfade on every cut using media handles, on alternating
lanes summed with `amix normalize=0`; short fades at gap edges; J/L cuts fall
out of placing audio by its own position. Matroska/WebM audio is decoded from
the start (ms timestamps make seeks off by up to 0.5 ms). Methods
`render.frame`, `render.contact_sheet` (base64 images), `render.range_preview`,
`render.export` (MP4 in `<cache>/projects/<hash>/renders/`, CLI moves it to
`-o`), `render.presets` (`ve-export`: master, youtube-1080p/720p,
vertical-1080x1920 (crop-fill), draft-540p). Encoder: libx264 if the local
ffmpeg has it, else hardware, openh264, mpeg4. Synchronous (no jobs yet).
`VE_KEEP_GRAPH=1` keeps the generated graph for debugging. Tests check exact
frames and samples against sources whose luma/samples encode position.
Analyzers (`ve-analyze`, `ve-daemon::analyze`): `analyze.silence`
(silencedetect on the timeline mix; snapped inward to frames, optional pad)
and `analyze.scenes` (scdet at 320 px), writing analyzer markers that replace
earlier ones in the range; `ve ripple-delete --markers silence` cuts them.
Gate dry run (synthetic 10-min 1080p 29.97 talking head, 162 pauses):
analyze 2.5 s, ripple-delete 0.2 s, export 4.5 min (x264 medium, 4 cores).
Daemon (`ve-daemon::server`, `::client`, `::events`): one per project,
holding the project lock. Local socket with newline-delimited JSON-RPC
(`\\.\pipe\ve-<key>` on Windows, `<cache>/projects/<key>/daemon.sock`
elsewhere; key = the hash the lock uses, `project::locate`) and an axum
WebSocket at `ws://127.0.0.1:<port>/rpc?token=<random>`; a browser `Origin`
must be loopback. Both call `Registry::dispatch` on the blocking pool. The CLI
connects or spawns `ve daemon run` (detached, log in the project cache dir),
and replaces a daemon whose binary path or mtime differs from its own. Idle
exit after 10 min (`VE_DAEMON_IDLE_SECS`); `VE_WS_PORT` fixes the port. CLI
`ve daemon start|status|stop`; methods `daemon.info`, `daemon.stop`,
`events.subscribe`. `project.changed` {revision, actor, command, patch} is
queued by `commit_doc` and broadcast after each call under the session lock
(so in revision order; it can reach the caller before its own response).
`events.lagged` and `project.reloaded` mean refetch. The daemon re-reads
`project.otio` when its mtime/size changes under it (external edits), so it
never overwrites them. Calls still serialize on the session mutex: a long
export blocks other calls until jobs exist. Named pipe ACL is the Windows
default (not locked down further).
No CI yet; the Python check runs by hand.

Phase 1 gate: **an agent cuts a 10-minute talking head end to end via the CLI only.**

Next, in order:
1. (done) OTIO structs + `metadata.ve` blocks.
2. (done) Timecode parser.
3. (done) Project folder open/save, lock, `project.summary`.
4. (done) `media.import` / `media.probe`, asset table with blake3 hashes.
5. (done) edit ops (insert, split, trim, move, ripple_delete, set_speed) as pure
   fns with proptests; history with undo groups. Checkpoint: keep OTIO.
6. (done) Resolver + filtergraph compiler; `render.frame`, `render.contact_sheet`,
   `render.range_preview`, `render.export`.
7. (done) Analyzers: silencedetect, scdet. Then the `video-edit` skill
   (`skills/video-edit/SKILL.md`, every command in it run as written).

Not done yet, and what the gate still needs (suggested order, not decided):
- Run the gate for real: an agent with only the skill cutting real footage
  (so far only a synthetic 10-min talking head has been cut end to end).
- (done) Daemon socket + connect-or-spawn + WebSocket + `project.changed`.
- Jobs: export and analyze are synchronous and hold the session lock, so a
  UI or agent waits behind them.
- Proxies on import (needs jobs); faster seeks for long-GOP phone footage.
- `ve-plugin` host, then move the analyzers to `plugins/core` (dogfooding).

## Working style

The owner (Gershon) prefers blunt, substantive feedback over hedging. Push back
when a request conflicts with the decisions above.
