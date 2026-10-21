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
`ve version | describe | call`; `ve-ops::diff_patches` (forward + inverse).
Everything else is documented stubs.

Phase 1 gate: **an agent cuts a 10-minute talking head end to end via the CLI only.**

Next, in order:
1. OTIO structs in ve-model (Timeline, Stack, Track, Clip, Gap, Transition,
   Marker, ExternalReference) + `metadata.ve` blocks; CI round-trip through
   Python `opentimelineio`.
2. Timecode parser (SMPTE incl. drop-frame, `f2172`, `72.4s`) -> RationalTime.
3. Project folder open/save with file lock; `project.summary` (compact text for agents).
4. `media.import` / `media.probe` via ffprobe JSON; asset table with blake3 hashes.
5. edit ops (insert, split, trim, move, ripple_delete, set_speed) as pure fns with
   proptests; history with undo groups.
6. Resolver + filtergraph compiler; `render.frame`, `render.contact_sheet`,
   `render.range_preview`, `export`.
7. Analyzers: silencedetect, scdet. Then the `video-edit` skill.

## Working style

The owner (Gershon) prefers blunt, substantive feedback over hedging. Push back
when a request conflicts with the decisions above.
