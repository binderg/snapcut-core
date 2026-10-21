# Headless Video Engine

A headless, agent-friendly video editing engine in Rust. The engine is the
product; every UI, agent, script and plugin is a client of one JSON-RPC protocol.
See `ARCHITECTURE.md` and the live architecture doc (Core crate stack tab).

`ve` is a placeholder name.

## Build

Install Rust from https://rustup.rs (the pinned toolchain in
`rust-toolchain.toml` installs itself on first build), then:

```
cargo build
cargo test
cargo run -p ve-cli -- version
cargo run -p ve-cli -- describe        # OpenRPC document
cargo run -p ve-cli -- call system.version '{}'
```

FFmpeg: Phase 1 drives the `ffmpeg`/`ffprobe` binaries, so put them on PATH.
For local development only, `--features ve-media/download-ffmpeg` lets the
engine fetch a build; those builds are GPL, so never ship with that feature.

## Layout

| Crate | Job |
| --- | --- |
| `ve-model` | OTIO timeline types, exact rational time, stable IDs. No I/O. |
| `ve-ops` | Edit commands as pure functions returning forward + inverse JSON Patch |
| `ve-history` | Undo/redo, undo groups, actors, revisions |
| `ve-media` | Probe, proxies, thumbnails, waveforms (FFmpeg binary in Phase 1) |
| `ve-render` | Resolver, FFmpeg filtergraph compiler (Phase 1), wgpu compositor (Phase 2) |
| `ve-export` | Encode + mux, presets |
| `ve-analyze` | Silence, scenes, loudness via FFmpeg filters (moves to `plugins/core` later) |
| `ve-plugin` | Plugin manifests, discovery, stdio process host |
| `ve-protocol` | Method registry, OpenRPC generation, actors, error codes |
| `ve-daemon` | Session server spawned on demand by the CLI; wires all methods |
| `ve-cli` | The `ve` binary |

All third-party versions are pinned once in the root `Cargo.toml`
(`[workspace.dependencies]`); entries tagged `(Phase N)` are declared but unused.

## What works today

- `RationalTime` / `TimeRange` with OTIO JSON round-trip and exact NTSC rates
- OTIO timeline types with typed `metadata.ve` blocks; reads and writes what
  Python `opentimelineio` writes (`python scripts/otio_roundtrip.py fixtures/otio`)
- Method registry on jsonrpsee `RpcModule`; one dispatch path for every transport
- Generated OpenRPC from `schemars` schemas (`ve describe`)
- CLI exit codes mapped from protocol errors
- `ve init my-edit --fps 29.97 --size 1920x1080` and `ve summary` (compact timeline text with IDs)
