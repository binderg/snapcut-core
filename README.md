<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/assets/snapcut-core-logo-dark-mode.svg">
    <img alt="SnapCut Core" src="docs/assets/snapcut-core-logo.svg" width="520">
  </picture>
</p>

<p align="center">
  <strong>The engine under a video editor. Bring the GUI, the AI, or both.</strong>
</p>

<p align="center">
  <a href="https://github.com/binderg/snapcut-core/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/binderg/snapcut-core/actions/workflows/ci.yml/badge.svg"></a>
  <img alt="Status: alpha" src="https://img.shields.io/badge/status-alpha-d10000">
  <img alt="License: MIT or Apache-2.0" src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue">
  <img alt="Platforms" src="https://img.shields.io/badge/platforms-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey">
</p>

<p align="center">
  <a href="#quick-start">Quick start</a> ·
  <a href="#install">Install</a> ·
  <a href="#editing-with-an-ai-agent">AI agents</a> ·
  <a href="#building-your-own-editor">Build an editor</a> ·
  <a href="#plugins">Plugins</a> ·
  <a href="ARCHITECTURE.md">Architecture</a>
</p>

---

SnapCut Core is not a video editor. It is the part underneath one: the
timeline, cuts, audio, titles, effects, rendering and export, with no window
of its own. Everything it can do goes through one self-describing API, so
whatever sits on top is up to you:

- **A GUI.** A simple CapCut-style app for vertical clips, a dense
  Resolve-style editor, or editing built into your own product.
- **An AI.** An agent that edits from the command line or a chat, sees and
  measures the footage, and checks its own work.
- **Both at once.** A person in an editor and an agent in a terminal on the
  same project, live. The agent's cuts appear on the timeline as it makes
  them, and undo knows whose edit is whose.

The bet behind it: AI is making interfaces cheap to build, so the part worth
building well is the engine. Think of it as an open-source, self-hostable
Shotstack you can build your own editor, automation or video service on.

> [!NOTE]
> **Status: 0.1.0-rc.1, alpha.** The editing core works end to end and is
> heavily tested, but expect rough edges and protocol changes before 1.0.
> The command is `ve` for now. See [What is not done yet](#what-is-not-done-yet).

## Why SnapCut Core

| | |
| --- | --- |
| **Built for agents** | Every edit can run as a dry run first, can be undone, and records who made it (`ve history`). Agents read the whole timeline, transcripts and measurements, so they edit from facts rather than guesses. |
| **Bring your own frontend** | Every UI, script and plugin is a client of one JSON-RPC API over a local socket or WebSocket. Changes stream to every client as JSON Patch. `ve describe` prints the whole API as OpenRPC, generated from the code. |
| **Lean core, open edges** | No AI models, voices or 3D in the core, ever. They come as plugins, in any language, local or cloud. |
| **Open formats** | Projects are [OpenTimelineIO](https://opentimeline.io) files, so a cut made here opens in DaVinci Resolve, Premiere or Avid. Effects are WGSL shaders with a JSON Schema for their parameters. |
| **Exact** | Time is stored as exact fractions, never floats. Renders are frame- and sample-exact, with a short crossfade on every audio cut so nothing clicks. |

## What it does

**Edit.** Insert, split, trim (ripple or roll), move, reorder, ripple delete,
lift, speed (exact ratios), volume, any number of video and audio tracks,
hide and mute. Video and its sound stay linked; unlink one side for J and L
cuts.

**Find what to cut.** Silence and scene-cut detection write markers, and
`ve ripple-delete --markers silence` removes every pause in one step.

**Measure.** Loudness (LUFS, true peak), levels, noise floor, stereo balance,
pitch and waveforms. `ve normalize --target -14` hits a loudness target.

**Look at it.** Exact frames, contact sheets and short preview clips, so an
agent (or you) can check a cut without opening an editor.

**Picture.** Position, scale, rotation, opacity, crop and blend modes per
clip, with keyframes and easing curves. Still images with transparency for
logos, title cards and photos.

**Text.** Titles, lower thirds and labels with presets and in/out
animations, any installed font, a font file or Google Fonts. Burned-in
captions from a transcript, with the spoken word highlighted as it is said.

**Effects and transitions.** GPU shaders (WGSL) that are validated and
test-rendered before use, with an API for writing your own. A few are built
in, and an optional pack adds 125 transitions from gl-transitions.

**Audio effects.** FFmpeg filter chains on audio clips, with keyframes.

**Export.** MP4 (H.264 or HEVC), ProRes 422 HQ, ProRes 4444 with alpha,
WebM, or audio only (WAV, M4A, MP3, FLAC). Presets for master, YouTube and
vertical 1080x1920. Captions as SRT or VTT.

**Undo** across sessions, undo groups per actor, and a full history of who
did what.

## Install

### 1. FFmpeg

The engine runs `ffmpeg` and `ffprobe`, so they must be on your `PATH`.

| System | Command |
| --- | --- |
| Windows | `winget install Gyan.FFmpeg` |
| macOS | `brew install ffmpeg` |
| Debian, Ubuntu | `sudo apt install ffmpeg` |

Check with `ffmpeg -version`.

### 2. SnapCut Core

Each release publishes prebuilt binaries for Windows, macOS (Apple silicon
and Intel) and Linux (x86_64 and arm64) on the [Releases](../../releases)
page. To install the newest one:

```bash
# macOS, Linux (installs to ~/.local/bin)
curl -fsSL https://raw.githubusercontent.com/binderg/snapcut-core/main/install.sh | sh
```

```powershell
# Windows (installs to %LOCALAPPDATA%\Programs\ve and adds it to PATH)
irm https://raw.githubusercontent.com/binderg/snapcut-core/main/install.ps1 | iex
```

Then check it:

```bash
ve version
```

<details>
<summary>Unsigned binaries, GPU, and building from source</summary>

**Unsigned binaries.** The binaries are not code-signed yet. On Windows,
SmartScreen may warn the first time; choose "More info", then "Run anyway".
For a macOS archive downloaded in a browser, run
`xattr -d com.apple.quarantine ve` once (the install script is not
affected).

**GPU.** Effects, transitions, transforms and text use the GPU (Vulkan,
Metal or DirectX 12). Without a usable GPU the engine falls back to a
software renderer, which works but is slow.

**From source**, with Rust from [rustup.rs](https://rustup.rs) (the pinned
toolchain installs itself):

```bash
git clone https://github.com/binderg/snapcut-core
cd snapcut-core
cargo install --path crates/ve-cli
```

</details>

## Quick start

Cut the pauses out of a talking head and export it for YouTube:

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

A few more things to try:

```bash
ve text add "The Big Launch" --preset title --in pop --out fade:0.6s
ve font add --google "Bebas Neue"
ve transform clip_7f3a --scale 0.2 --x 0.4 --y -0.4   # logo in a corner
ve export --format prores-4444 -o overlay.mov          # keeps transparency
ve history                                             # who did what
```

## Editing with an AI agent

The agent interface is the CLI plus a skill file. No MCP server or special
protocol is needed.
[`skills/video-edit/SKILL.md`](skills/video-edit/SKILL.md) teaches an agent
the edit loop, time formats, IDs, undo groups, how to check its cuts, and a
talking-head recipe. For Claude Code, copy the folder into your skills:

```bash
cp -r skills/video-edit ~/.claude/skills/
```

Then open a folder with a `project.otio` and ask it to "tighten this video."
Agents pass `--actor agent:<name>`, so `ve history` shows which edits were
theirs, and you can undo an agent's edit without losing your own.

## Building your own editor

Every frontend talks to the same JSON-RPC 2.0 API:

```
  Your editor (any stack)      AI agent (CLI + skill)      Scripts, CI
            \                          |                       /
             '--------- JSON-RPC 2.0 over socket or WebSocket -'
                                       |
              SnapCut Core: timeline, render, analysis, history
                                       |
                Plugins: AI models, voices, effect packs, tools
```

- A background process per project (started on demand by the CLI) serves
  the API and broadcasts every change, so a UI watching the project sees an
  agent's edits land as they happen.
- Mutating calls carry the revision they were based on, so two clients never
  silently overwrite each other.
- `ve describe` prints the full API as an OpenRPC document. Hand it to an AI
  and ask it to scaffold an editor.

The API reference is [`skills/ve-core/`](skills/ve-core/SKILL.md): how to
connect, the project model, revisions and undo, events, and every method.

## Plugins

The core does the editing and nothing else. Everything beyond it is a
plugin, kept apart from the engine, so the core stays small and stable while
what you can build on it keeps growing.

A plugin is a folder with a `plugin.json` and a program in any language. It
can add commands (which become `ve` subcommands), analyzers, generators and
packs of effects, run as a background job with progress and cancel, and call
back into the engine like any client, with its edits undone as one step.
API keys live in the OS keychain, never in the project. A plugin written
once works in every editor built on the engine.

**Available now**

| Plugin | What it does |
| --- | --- |
| [`plugins/deepgram`](plugins/deepgram) | Speech to text with your own Deepgram key: words with exact times, speakers and confidence, for captions and cutting by text. |
| [`plugins/gl-transitions`](plugins/gl-transitions) | 125 transitions from gl-transitions.com as an effect pack, each with its author's license. |

```bash
ve plugin install plugins/deepgram
ve plugin secret set deepgram DEEPGRAM_API_KEY   # then paste your key
ve transcribe asset_94b8
ve captions --preset caption-highlight
```

**What plugins make possible.** Speech to text and text to speech, local or
cloud. AI green screen and background removal. Depth maps for effects that
know what is near and far. Noise removal, upscaling and stabilization.
Generated b-roll, music and sound effects. A bridge to Blender for 3D, or
motion graphics written in HTML. Pulling footage from stock libraries and
cloud drives, and publishing to YouTube or TikTok. Each can run on a local
model or call a cloud API, chosen per plugin.

## What is not done yet

- Tested mostly on Windows. CI runs the tests on macOS and Linux, but real
  footage has mostly been cut on Windows.
- No proxies yet, so long-GOP phone footage seeks slowly.
- Playback in an editor plays short rendered chunks. A stretch the engine
  has not rendered yet waits a second or two before it plays.
- Exports with effects take several times longer than plain cuts.
- Contact sheets ignore effects and transforms.
- Plugins run with your user's permissions (no sandbox), so review a plugin
  before installing it.
- No typed JS/TS client package yet; frontends call the JSON-RPC API
  directly.

The full plan is the TODO list at the end of
[`ARCHITECTURE.md`](ARCHITECTURE.md).

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

<details>
<summary>Crates</summary>

| Crate | Job |
| --- | --- |
| `ve-model` | OTIO timeline types, exact rational time, stable IDs. No I/O. |
| `ve-ops` | Edit commands as pure functions returning forward and inverse JSON Patch |
| `ve-history` | Undo and redo, undo groups, actors, revisions |
| `ve-media` | Probing and importing media |
| `ve-render` | Resolver, FFmpeg filtergraph renderer, wgpu compositor, effects, text |
| `ve-export` | Export presets and encoder choice |
| `ve-analyze` | Silence, scenes, loudness, levels, pitch |
| `ve-plugin` | Plugin manifests, discovery and the plugin host |
| `ve-protocol` | Method registry, OpenRPC generation, error codes |
| `ve-daemon` | The per-project server; wires every method |
| `ve-cli` | The `ve` command |

</details>

## License

Dual licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE), at
your option. FFmpeg is not bundled; you install it yourself under its own
license.
