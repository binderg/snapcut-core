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
  missing (idle timeout). No separate "direct mode". Until the socket exists,
  the CLI dispatches in-process against `ve_daemon::build_registry()`.
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
`foo.workspace = true`. `unsafe_code` is forbidden workspace-wide.

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
yet (needs jobs). `actor` is not recorded yet (comes with history, item 5).
Everything else is documented stubs. No CI yet; the Python check runs by hand.

Phase 1 gate: **an agent cuts a 10-minute talking head end to end via the CLI only.**

Next, in order:
1. (done) OTIO structs + `metadata.ve` blocks.
2. (done) Timecode parser.
3. (done) Project folder open/save, lock, `project.summary`.
4. (done) `media.import` / `media.probe`, asset table with blake3 hashes.
5. edit ops (insert, split, trim, move, ripple_delete, set_speed) as pure fns with
   proptests; history with undo groups. `set_speed` writes `LinearTimeWarp` too.
   Checkpoint: is OTIO still the right in-memory model? (see Decisions).
6. Resolver + filtergraph compiler; `render.frame`, `render.contact_sheet`,
   `render.range_preview`, `export`.
7. Analyzers: silencedetect, scdet. Then the `video-edit` skill.

## Working style

The owner (Gershon) prefers blunt, substantive feedback over hedging. Push back
when a request conflicts with the decisions above.
