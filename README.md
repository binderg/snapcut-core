# Headless Video Engine

A video editor with no window. You (or an AI agent) edit real footage from
the command line or over a JSON-RPC API: cut silences out of a talking head,
trim, reorder, change speed, make J and L cuts, add effects and transitions,
check the result with frames and contact sheets, and export.

It is built for agents first. Every command is undoable, can run as a dry
run, and names who made it, so an agent and a person can edit the same
project side by side. The project is an
[OpenTimelineIO](https://opentimeline.io) file, so a cut made here opens in
DaVinci Resolve, Premiere or Avid.

> **Status: 0.1.0-rc.1, alpha.** The editing core works and is heavily
> tested, but expect rough edges and protocol changes before 1.0. The
> product name is not final; the command is `ve` for now.

## What it does

- **Edit**: insert, split, trim (ripple or roll), move, reorder, ripple
  delete, lift, speed (exact ratios), volume, any number of video and audio
  tracks, hide and mute. Video and its sound stay linked; unlink one side
  for J/L cuts.
- **Find what to cut**: silence and scene-cut detection write markers;
  `ve ripple-delete --markers silence` removes every pause in one step.
- **Measure sound**: loudness (LUFS, true peak), levels, noise floor, stereo
  balance, pitch, waveforms. `ve normalize --target -14` hits a loudness
  target.
- **Look at it**: exact frames, contact sheets, short preview clips.
- **Picture**: position, scale, rotation, opacity and crop per clip, with
  keyframes. Still images (logos, title cards, photos) with transparency. GPU effects and transitions (WGSL shaders), with an API for
  writing your own.
- **Export**: H.264 MP4 presets (master, YouTube 1080p/720p, vertical
  1080x1920), frame- and sample-exact, with a short crossfade on every audio
  cut so nothing clicks.
- **Undo** across sessions, undo groups, and a full history of who did what.

It does not bundle any AI models. Transcription, captions and the like are
meant to come from plugins (the plugin host is not built yet).

## Install

### 1. FFmpeg

The engine runs `ffmpeg` and `ffprobe`, so they must be on your `PATH`.

| System | Command |
| --- | --- |
| Windows | `winget install Gyan.FFmpeg` |
| macOS | `brew install ffmpeg` |
| Debian, Ubuntu | `sudo apt install ffmpeg` |

Check with `ffmpeg -version`.

### 2. The engine

**Prebuilt binaries** for Windows, macOS (Apple silicon and Intel) and Linux
(x86_64 and arm64) are on the [Releases](../../releases) page. Or install
the newest one with a single command:

```bash
# macOS, Linux (installs to ~/.local/bin)
curl -fsSL https://raw.githubusercontent.com/binderg/snapcut-core/main/install.sh | sh
```

```powershell
# Windows (installs to %LOCALAPPDATA%\Programs\ve and adds it to PATH)
irm https://raw.githubusercontent.com/binderg/snapcut-core/main/install.ps1 | iex
```

The binaries are not code-signed yet. On Windows, SmartScreen may warn the
first time; choose "More info", then "Run anyway". The macOS install script
is not affected (only browser downloads are quarantined); for a downloaded
archive, run `xattr -d com.apple.quarantine ve` once.

**From source**, with Rust from [rustup.rs](https://rustup.rs) (the pinned
toolchain installs itself):

```bash
cargo install --path crates/ve-cli
```

Then `ve version` should print the engine version.

Effects, transitions and transforms use the GPU (Vulkan, Metal or
DirectX 12). Without a usable GPU the engine falls back to a software
renderer, which works but is slow.

## Quick start

```bash
ve init my-edit --fps 30 --size 1920x1080   # a project is a folder
cd my-edit
ve import ../talk.mp4                       # prints the asset ID
ve insert asset_94b8                        # video on V1, linked audio on A1
ve summary                                  # tracks and clips with IDs and timecodes
ve analyze silence --min 0.5s --pad 0.1s    # marks the pauses
ve ripple-delete --markers silence          # removes them
ve sheet --count 12 -o sheet.jpg            # look at the result
ve export --preset youtube-1080p -o final.mp4
```

Changed your mind? `ve undo`. Every command takes `--dry-run` to show what
it would change first, and `--help` for its options.

## Editing with an AI agent

The agent interface is the CLI plus a skill file, not a plugin protocol.
[`skills/video-edit/SKILL.md`](skills/video-edit/SKILL.md) teaches an agent
the edit loop, time formats, IDs, undo groups, how to check its cuts and a
talking-head recipe. For Claude Code, copy the folder into your skills:

```bash
cp -r skills/video-edit ~/.claude/skills/
```

Then ask it to tighten a video in a folder with a `project.otio`. Agents
pass `--actor agent:<name>`, so `ve history` shows which edits were theirs.

## Building on the engine

Every UI, script and plugin talks to one JSON-RPC 2.0 protocol, over a
local socket or a WebSocket. A background process per project (started on
demand by the CLI) serves it, and broadcasts every change as a JSON Patch so
several clients stay in sync. `ve describe` prints the whole API as an
OpenRPC document generated from the code.

The API reference is [`skills/ve-core/`](skills/ve-core/SKILL.md): how to
connect, the project model, revisions and undo, events, and every method.

## Development

```bash
cargo build
cargo test --workspace
cargo clippy --workspace --all-targets
```

- If the checkout is in a synced folder (OneDrive, Dropbox), set
  `CARGO_TARGET_DIR` to a path outside it.
- `--features ve-media/download-ffmpeg` fetches an FFmpeg build for local
  development. Those builds are GPL, so never ship with that feature.
- `python scripts/otio_roundtrip.py fixtures/otio` checks the OTIO files
  against Python `opentimelineio`.
- Design: [`ARCHITECTURE.md`](ARCHITECTURE.md). Changes: [`CHANGELOG.md`](CHANGELOG.md).

| Crate | Job |
| --- | --- |
| `ve-model` | OTIO timeline types, exact rational time, stable IDs. No I/O. |
| `ve-ops` | Edit commands as pure functions returning forward and inverse JSON Patch |
| `ve-history` | Undo and redo, undo groups, actors, revisions |
| `ve-media` | Probing and importing media |
| `ve-render` | Resolver, FFmpeg filtergraph renderer, wgpu compositor, effects |
| `ve-export` | Export presets and encoder choice |
| `ve-analyze` | Silence, scenes, loudness, levels, pitch |
| `ve-plugin` | Plugin manifests (the host is not built yet) |
| `ve-protocol` | Method registry, OpenRPC generation, error codes |
| `ve-daemon` | The per-project server; wires every method |
| `ve-cli` | The `ve` command |

## License

Dual licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at
your option. FFmpeg is not bundled; you install it yourself under its own
license.
