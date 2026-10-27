//! `ve`: the CLI. Each subcommand maps to one protocol method.
//!
//! Plugins add commands of their own (`ve transcribe ...`): any subcommand
//! clap does not know is looked up in the project's plugins (`plugin.list`)
//! and its arguments are mapped onto the method's params schema.
//!
//! Commands on a project go to that project's daemon over its local socket
//! (a named pipe on Windows); if none is running the CLI starts one, which
//! exits after 10 idle minutes (`VE_DAEMON_IDLE_SECS`). Commands that need no
//! project (`init`, `probe`, `version`, `describe`) run in-process against the
//! same registry.
//!
//! Mutating commands fill in `base_rev` (the project's revision when this
//! call started, unless `--base-rev` says otherwise), `dry_run` and `actor`
//! for you. Files are written here, never by the protocol: render
//! methods return bytes or a file in the engine's cache, and the CLI puts the
//! result at `-o`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};
use ve_daemon::client::{self, Client};
use ve_protocol::{ErrorCode, Registry};

mod stdio;

#[derive(Parser)]
#[command(name = "ve", version, about = "Headless, agent-friendly video engine")]
struct Cli {
    /// Machine-readable JSON output.
    #[arg(long, global = true)]
    json: bool,
    /// Project folder. Defaults to the nearest folder at or above the current
    /// directory that holds a project.otio.
    #[arg(long, global = true)]
    project: Option<PathBuf>,
    /// Who is editing: `user`, `agent:<name>` or `plugin:<id>`. Recorded in history.
    #[arg(long, global = true, default_value = "user")]
    actor: String,
    /// Show what a mutating command would do (with its JSON Patch) without changing anything.
    #[arg(long, global = true)]
    dry_run: bool,
    /// Revision you planned against; defaults to the project's current one.
    #[arg(long, global = true)]
    base_rev: Option<u64>,
    #[command(subcommand)]
    command: Command,
}

/// A timeline range.
#[derive(Args, Clone)]
struct Span {
    /// Start (timecode `00:01:00:00`, frames `f1800` or seconds `60s`). Default: timeline start.
    #[arg(long)]
    from: Option<String>,
    /// End, exclusive. Default: timeline end.
    #[arg(long)]
    to: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Engine, protocol and schema versions.
    Version,
    /// Print the OpenRPC document describing every method.
    Describe,
    /// Create a project folder: `ve init my-edit --fps 30 --size 1920x1080`.
    Init {
        path: PathBuf,
        /// Frame rate: 30, 29.97, 23.976 or an exact 30000/1001.
        #[arg(long)]
        fps: String,
        /// Output size, WIDTHxHEIGHT.
        #[arg(long)]
        size: String,
        /// Defaults to the folder name.
        #[arg(long)]
        name: Option<String>,
        #[arg(long, default_value_t = 48_000)]
        sample_rate: u32,
    },
    /// Compact timeline text: tracks, clips and markers with their IDs.
    Summary,
    /// Show what a media file contains (no project needed).
    Probe { path: PathBuf },
    /// Probe, hash and add media files to the project's asset table.
    Import {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Fix media that moved: `ve relink --folder D:/footage` finds every missing
    /// asset there (by content); `ve relink asset_3f2a D:/new/take3.mp4` points one asset.
    /// With no arguments, lists missing assets.
    Relink {
        asset: Option<String>,
        path: Option<PathBuf>,
        /// Search this folder and its subfolders for every missing asset.
        #[arg(long)]
        folder: Option<PathBuf>,
        /// Use the file even if its content differs from the asset's.
        #[arg(long)]
        force: bool,
    },

    /// Put an asset on the timeline: `ve insert asset_3f2a --at 00:00:10:00`.
    Insert {
        asset: String,
        /// Track (name or ID) for the video. Default: first video track.
        #[arg(long)]
        track: Option<String>,
        /// Track for the audio. Default: first audio track.
        #[arg(long)]
        audio_track: Option<String>,
        /// Timeline position. Default: end of the target tracks.
        #[arg(long)]
        at: Option<String>,
        /// Media in-point (at the media's frame rate).
        #[arg(long = "in")]
        source_in: Option<String>,
        /// Media out-point, exclusive.
        #[arg(long = "out")]
        source_out: Option<String>,
        /// Length instead of --out.
        #[arg(long)]
        duration: Option<String>,
        /// Overwrite instead of pushing later content right.
        #[arg(long)]
        overwrite: bool,
        #[arg(long)]
        no_video: bool,
        #[arg(long)]
        no_audio: bool,
    },
    /// Cut clips at a time: `ve split --at 00:01:12:12` (every clip there) or `ve split clip_7f3a --at ...`.
    Split {
        clips: Vec<String>,
        #[arg(long)]
        at: String,
        /// Limit an all-clips split to these tracks.
        #[arg(long = "track")]
        tracks: Vec<String>,
        /// Leave linked clips alone.
        #[arg(long)]
        unlinked: bool,
    },
    /// Move a clip's edges: `ve trim clip_7f3a --in +15f --out -1s`.
    Trim {
        clip: String,
        /// In-point change: +15f starts later, -15f earlier.
        #[arg(long = "in", allow_hyphen_values = true)]
        in_delta: Option<String>,
        /// Out-point change: -1s ends earlier, +10f later.
        #[arg(long = "out", allow_hyphen_values = true)]
        out_delta: Option<String>,
        /// Roll the edit point (the neighbour gives/takes the time) instead of rippling.
        /// With --unlinked on an audio clip this makes a J or L cut.
        #[arg(long)]
        roll: bool,
        #[arg(long)]
        unlinked: bool,
    },
    /// Move a clip (overwrites the destination, leaves a gap): `ve move clip_7f3a --to 00:00:42:00 --track V2`.
    /// `--reorder` swaps places: what lies between slides over to fill the old place.
    Move {
        clip: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        track: Option<String>,
        #[arg(long)]
        unlinked: bool,
        #[arg(long)]
        reorder: bool,
    },
    /// Remove time and close the gap on every track (or --track):
    /// `ve ripple-delete --from 00:02:10:00 --to 00:02:14:15`, `--range A..B` (repeatable),
    /// `--markers silence`, or clip IDs.
    RippleDelete {
        clips: Vec<String>,
        #[command(flatten)]
        span: Span,
        /// FROM..TO, repeatable.
        #[arg(long = "range")]
        ranges: Vec<String>,
        /// Delete the ranges of every timeline marker with this name.
        #[arg(long)]
        markers: Option<String>,
        #[arg(long = "track")]
        tracks: Vec<String>,
        #[arg(long)]
        unlinked: bool,
    },
    /// Remove clips and leave gaps.
    Delete {
        #[arg(required = true)]
        clips: Vec<String>,
        #[arg(long)]
        unlinked: bool,
    },
    /// Change speed: `ve speed clip_7f3a 3/2`.
    Speed {
        clip: String,
        speed: String,
        #[arg(long)]
        unlinked: bool,
        /// Keep later clips in place (a slower clip overwrites what follows).
        #[arg(long)]
        no_ripple: bool,
    },
    /// Audio gain in dB: `ve volume clip_7f3a -6`.
    Volume {
        clip: String,
        #[arg(allow_hyphen_values = true)]
        db: f64,
        #[arg(long)]
        unlinked: bool,
    },
    /// Break a clip's link with its partners (video and its audio): `ve unlink clip_7f3a`.
    Unlink { clip: String },
    /// Place a video clip's picture: `ve transform clip_7f3a --scale 0.4 --x 0.3 --y 0.3`
    /// (picture in picture, bottom right). `--reset` puts it back to fitted and centered.
    Transform {
        clip: String,
        /// Offset of the picture's center from the canvas center, in canvas widths.
        #[arg(long, allow_hyphen_values = true)]
        x: Option<f64>,
        /// Offset in canvas heights; positive is down.
        #[arg(long, allow_hyphen_values = true)]
        y: Option<f64>,
        /// Size relative to the fitted picture.
        #[arg(long)]
        scale: Option<f64>,
        /// Degrees clockwise.
        #[arg(long, allow_hyphen_values = true)]
        rotation: Option<f64>,
        /// 0 (invisible) to 1 (opaque).
        #[arg(long)]
        opacity: Option<f64>,
        /// Crop each side, as a fraction of the picture.
        #[arg(long)]
        crop_left: Option<f64>,
        #[arg(long)]
        crop_right: Option<f64>,
        #[arg(long)]
        crop_top: Option<f64>,
        #[arg(long)]
        crop_bottom: Option<f64>,
        /// `field@time=value[:ease]` keyframes (time from the clip start): `--key opacity@0s=0:ease-out --key opacity@1s=1`.
        /// Eases: linear, hold, ease, ease-in, ease-out, ease-in-out, cubic-bezier(x1,y1,x2,y2).
        #[arg(long = "key", value_name = "FIELD@TIME=VALUE")]
        keys: Vec<String>,
        /// Remove a field's keyframes: `--clear-keys opacity`.
        #[arg(long = "clear-keys")]
        clear: Vec<String>,
        /// How it mixes with the tracks below: normal, multiply, screen, overlay, darken, lighten,
        /// color-dodge, color-burn, hard-light, soft-light, difference, exclusion, hue,
        /// saturation, color, luminosity, add, subtract, linear-burn.
        #[arg(long)]
        blend: Option<String>,
        /// Start from the default before applying the rest.
        #[arg(long)]
        reset: bool,
    },
    /// Timeline markers.
    #[command(subcommand)]
    Marker(MarkerCmd),
    /// Tracks: any number of video and audio tracks.
    #[command(subcommand)]
    Track(TrackCmd),
    /// Available effects and transitions; define new ones.
    #[command(subcommand)]
    Fx(FxCmd),
    /// Effects on clips.
    #[command(subcommand)]
    Effect(EffectCmd),
    /// Transitions on cuts.
    #[command(subcommand)]
    Transition(TransitionCmd),
    /// Text clips (titles, labels): `ve text add "Hello" --at 5s --duration 3s`.
    #[command(subcommand)]
    Text(TextCmd),
    /// The project's fonts: `ve font add Brand.ttf`, `ve font add --google "Bebas Neue"`, `ve font list`.
    #[command(subcommand)]
    Font(FontCmd),
    /// Caption clips from the transcript, timed to the edited timeline:
    /// `ve captions` (again after editing: `ve captions --track Captions`).
    Captions {
        #[command(flatten)]
        span: Span,
        /// Replace the text clips on this track instead of making a new `Captions` track.
        #[arg(long)]
        track: Option<String>,
        /// Vertical position, canvas heights from the center (default 0.36, near the bottom).
        #[arg(long, allow_hyphen_values = true)]
        y: Option<f64>,
        /// Look over the caption default: `--style size=0.07 --style color=[1,1,0]`.
        #[arg(long = "style", value_name = "FIELD=VALUE")]
        style: Vec<String>,
    },
    /// Undo the last step (one edit, or a whole group).
    Undo,
    /// Redo the last undone step.
    Redo,
    /// Undo and redo steps, with who made them.
    History {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Undo groups: `ve group begin "remove silences"` ... `ve group end`.
    #[command(subcommand)]
    Group(GroupCmd),

    /// One frame as JPEG/PNG: `ve frame --at 00:01:12:12 --width 640 -o cut.jpg`.
    Frame {
        #[arg(long)]
        at: String,
        #[arg(long, default_value_t = 640)]
        width: u32,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// What is on screen and heard at a frame: `ve layers --at 00:00:42:00` (each
    /// layer, its placement, effects, transition, how much shows; each audio clip
    /// and its gain). Without `--at`, which tracks show and are heard over a range:
    /// `ve layers --from 00:01:00:00 --to 00:02:00:00`.
    Layers {
        #[arg(long, conflicts_with_all = ["from", "to"])]
        at: Option<String>,
        #[command(flatten)]
        span: Span,
        /// Only the picture (`video`) or only the sound (`audio`).
        #[arg(long, value_parser = ["video", "audio"])]
        only: Option<String>,
    },
    /// Grid of evenly spaced frames: `ve sheet --from 00:01:00:00 --to 00:01:30:00 --count 12 -o sheet.jpg`.
    Sheet {
        #[command(flatten)]
        span: Span,
        #[arg(long, default_value_t = 12)]
        count: u32,
        #[arg(long)]
        columns: Option<u32>,
        /// Width of each cell.
        #[arg(long, default_value_t = 320)]
        width: u32,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Low-res MP4 of a range: `ve preview --from 00:01:00:00 --to 00:01:30:00 -o check.mp4`.
    Preview {
        #[command(flatten)]
        span: Span,
        #[arg(long, default_value_t = 640)]
        width: u32,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Render the timeline: `ve export --preset youtube-1080p -o final.mp4`.
    Export {
        /// `ve export --list-presets` shows them. Default: `master` (project settings).
        #[arg(long)]
        preset: Option<String>,
        /// mp4 (H.264), hevc, prores, prores-4444 (alpha), webm, or audio only: wav, m4a, mp3, flac.
        /// Default: from the `-o` extension (.mov is prores), else mp4.
        #[arg(long)]
        format: Option<String>,
        #[arg(long)]
        list_presets: bool,
        #[command(flatten)]
        span: Span,
        #[arg(short, long, required_unless_present = "list_presets")]
        output: Option<PathBuf>,
    },
    /// Built-in analyzers. Results become timeline markers.
    #[command(subcommand)]
    Analyze(AnalyzeCmd),
    /// Measure audio without changing anything: `ve measure loudness`, `ve measure levels --asset asset_3f2a`.
    #[command(subcommand)]
    Measure(MeasureCmd),
    /// Set the mix to a loudness target: `ve normalize --target -14` (one gain on every audio clip).
    Normalize {
        /// Integrated loudness to hit, LUFS: -14 (YouTube), -16 (podcasts), -23 (broadcast).
        #[arg(long, default_value_t = -14.0, allow_hyphen_values = true)]
        target: f64,
        /// Highest true peak allowed after the gain, dBTP.
        #[arg(long, default_value_t = -1.0, allow_hyphen_values = true)]
        ceiling: f64,
        #[command(flatten)]
        span: Span,
    },

    /// What was said, as readable lines with timecodes: the words that play on
    /// the timeline (`ve transcript`), or one asset's whole transcript
    /// (`--asset`). Making a transcript is a plugin's job (`ve plugin list`).
    Transcript {
        /// One asset's whole transcript, in its own media time.
        #[arg(long)]
        asset: Option<String>,
        /// `text` (default), or captions: `srt` or `vtt` (`ve transcript --format srt > captions.srt`).
        #[arg(long)]
        format: Option<String>,
        #[command(flatten)]
        span: Span,
    },

    /// Plugins: `ve plugin list`, `ve plugin install <folder>`, `ve plugin secret set <plugin> <NAME>`.
    #[command(subcommand)]
    Plugin(PluginCmd),

    /// Send a raw JSON-RPC method call, e.g. `ve call project.get '{}'`.
    Call { method: String, params: Option<String> },

    /// Background jobs (exports, previews, analysis): list them, or `ve jobs cancel <id>`.
    Jobs {
        #[command(subcommand)]
        cmd: Option<JobsCmd>,
    },

    /// The project's background engine process (started on demand by other commands).
    #[command(subcommand)]
    Daemon(DaemonCmd),

    /// Cached frames, previews, renders and measurements: show their size, or `ve cache clear`.
    Cache {
        #[command(subcommand)]
        cmd: Option<CacheCmd>,
    },

    /// A command a plugin adds (`ve plugin list` shows them).
    #[command(external_subcommand)]
    External(Vec<String>),
}

#[derive(Subcommand)]
enum PluginCmd {
    /// Installed plugins, their commands, and whether their secrets are set.
    List,
    /// Copy a plugin folder into your plugin folder (replacing an older copy of it).
    /// Refused when its `engine` range or `protocol` does not match this engine.
    Install {
        folder: PathBuf,
        /// Install even if it does not match this engine.
        #[arg(long)]
        force: bool,
    },
    /// API keys plugins need, kept in the OS keychain (never in the project).
    #[command(subcommand)]
    Secret(SecretCmd),
}

#[derive(Subcommand)]
enum SecretCmd {
    /// Store a key: `ve plugin secret set deepgram DEEPGRAM_API_KEY`, then paste it.
    /// Reads one line from stdin, so `echo $KEY | ve plugin secret set ...` works too.
    Set { plugin: String, name: String },
    /// Remove a stored key.
    Rm { plugin: String, name: String },
}

#[derive(Subcommand)]
enum CacheCmd {
    /// Delete the project's caches (everything is rebuilt when needed). Undo history is kept.
    Clear {
        /// Only these: frames, previews, renders, measure, audio. Default: all.
        #[arg(value_delimiter = ',')]
        kinds: Vec<String>,
        /// Show what would be freed without deleting anything.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum JobsCmd {
    /// One job: state, progress, and its result or error.
    Get { job_id: String },
    /// Stop a job (a running export stops within a second).
    Cancel { job_id: String },
}

#[derive(Subcommand)]
enum DaemonCmd {
    /// Start the daemon if it is not running, and show it.
    Start,
    /// Show the running daemon: pid, socket and WebSocket URL (with its token).
    Status,
    /// Stop the daemon. The project is saved after every edit, so nothing is lost.
    Stop,
    /// Serve the project in the foreground (what `start` runs in the background).
    /// `VE_DAEMON_IDLE_SECS` sets the idle timeout, `VE_WS_PORT` a fixed WebSocket port.
    #[command(hide = true)]
    Run,
}

#[derive(Subcommand)]
enum FxCmd {
    /// Every effect and transition with its params.
    List,
    /// `ve fx define my_glow/`: validate a folder with effect.json and its
    /// .wgsl (naga, then a GPU test render) and save it to the project. An
    /// audio effect (`"kind": "audio"`) is the manifest alone, with its ffmpeg
    /// chain in `filter`; it is test-run through ffmpeg.
    Define {
        folder: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum FontCmd {
    /// Copy a font into the project's fonts/ folder, from a file, `--url`, or `--google "Family"`.
    Add {
        /// A .ttf/.otf/.ttc file.
        path: Option<PathBuf>,
        #[arg(long)]
        url: Option<String>,
        /// A Google Fonts family name: every weight and italic it has.
        #[arg(long)]
        google: Option<String>,
        /// Overwrite a font file of the same name.
        #[arg(long)]
        replace: bool,
    },
    /// Fonts in the project; `--system` also lists installed families.
    List {
        #[arg(long)]
        system: bool,
    },
    /// Delete a font file from the project's fonts/ folder.
    Rm { file: String },
}

#[derive(Subcommand)]
enum TextCmd {
    /// `ve text add "Big news" --at 00:00:05:00 --duration 3s --style size=0.1 --style weight=700 --y -0.3`.
    /// Goes on a free video track (a new `Text` track if none); never pushes other clips.
    Add {
        text: String,
        #[arg(long)]
        at: Option<String>,
        #[arg(long)]
        duration: Option<String>,
        /// A video track by name or ID (default: a free one).
        #[arg(long)]
        track: Option<String>,
        /// On `--track`, replace what is under the text.
        #[arg(long)]
        overwrite: bool,
        /// `FIELD=VALUE`: font, size, weight, italic, color, align, width, line_height,
        /// outline, outline_color, shadow, shadow_color, background, padding.
        #[arg(long = "style", value_name = "FIELD=VALUE")]
        style: Vec<String>,
        /// Center position, canvas widths from the center.
        #[arg(long, allow_hyphen_values = true)]
        x: Option<f64>,
        /// Center position, canvas heights from the center; positive is down.
        #[arg(long, allow_hyphen_values = true)]
        y: Option<f64>,
    },
    /// `ve text set clip_7f3a --text "New words" --style color=[1,0.8,0]`. Move or fade it with `ve transform`.
    Set {
        clip: String,
        #[arg(long)]
        text: Option<String>,
        #[arg(long = "style", value_name = "FIELD=VALUE")]
        style: Vec<String>,
        /// Start from the default look before applying `--style`.
        #[arg(long)]
        reset_style: bool,
    },
}

#[derive(Subcommand)]
enum EffectCmd {
    /// `ve effect add clip_7f3a core.color/exposure --param stops=1`.
    Add {
        clip: String,
        effect: String,
        /// `name=value` (value as JSON: `0.5`, `true`, `[1,0.5,0]`). Repeatable.
        #[arg(long = "param", value_name = "NAME=VALUE")]
        params: Vec<String>,
        /// Position in the stack (0 runs first).
        #[arg(long)]
        index: Option<usize>,
    },
    /// `ve effect set fx_1a2b --param amount=0.3 --key amount@0s=0 --key amount@2s=1`.
    Set {
        id: String,
        /// `name=value`; `name=null` resets to the default.
        #[arg(long = "param", value_name = "NAME=VALUE")]
        params: Vec<String>,
        /// `name@time=value[:ease]` keyframes (time from the clip start). Replaces that param's keys.
        #[arg(long = "key", value_name = "NAME@TIME=VALUE")]
        keys: Vec<String>,
        /// Remove a param's keyframes.
        #[arg(long = "clear-keys", value_name = "NAME")]
        clear_keys: Vec<String>,
        #[arg(long)]
        index: Option<usize>,
    },
    /// `ve effect rm fx_1a2b`.
    Rm { id: String },
}

#[derive(Subcommand)]
enum TransitionCmd {
    /// `ve transition add --at 00:00:10:00 [--type core.transition/wipe] [--duration 15f]`.
    Add {
        /// The outgoing clip (the transition goes on its end).
        #[arg(long)]
        clip: Option<String>,
        /// Or the time of the cut.
        #[arg(long)]
        at: Option<String>,
        #[arg(long)]
        track: Option<String>,
        /// Default core.transition/dissolve.
        #[arg(long = "type")]
        kind: Option<String>,
        /// Default 0.5s.
        #[arg(long)]
        duration: Option<String>,
        /// center (default), start or end.
        #[arg(long)]
        align: Option<String>,
        #[arg(long = "param", value_name = "NAME=VALUE")]
        params: Vec<String>,
    },
    /// `ve transition set tr_1a2b --duration 1s --type core.transition/slide`.
    Set {
        id: String,
        #[arg(long)]
        duration: Option<String>,
        #[arg(long)]
        align: Option<String>,
        #[arg(long = "type")]
        kind: Option<String>,
        #[arg(long = "param", value_name = "NAME=VALUE")]
        params: Vec<String>,
    },
    /// `ve transition rm tr_1a2b`.
    Rm { id: String },
}

/// `name=value` pairs to a JSON object; values parse as JSON, else strings.
fn param_map(pairs: &[String], json_output: bool) -> Value {
    let mut m = serde_json::Map::new();
    for p in pairs {
        let Some((k, v)) = p.split_once('=') else {
            usage_error(format!("--param `{p}`: use NAME=VALUE"), json_output)
        };
        m.insert(k.trim().to_owned(), serde_json::from_str(v.trim()).unwrap_or_else(|_| Value::String(v.trim().to_owned())));
    }
    Value::Object(m)
}

/// `name@time=value` keyframes grouped by param.
fn key_map(keys: &[String], clear: &[String], json_output: bool) -> Value {
    let mut m = serde_json::Map::new();
    for k in keys {
        let parsed = k.split_once('@').and_then(|(name, rest)| rest.split_once('=').map(|(t, v)| (name, t, v)));
        let Some((name, t, v)) = parsed else {
            usage_error(format!("--key `{k}`: use NAME@TIME=VALUE[:EASE], e.g. amount@1s=0.5 or amount@0s=0:ease-out"), json_output)
        };
        // An ease may follow the value: `opacity@0s=0:ease-out`.
        let (v, ease) = match v.split_once(':') {
            Some((v, e)) => (v, Some(e.trim())),
            None => (v, None),
        };
        let value = serde_json::from_str(v.trim()).unwrap_or_else(|_| Value::String(v.trim().to_owned()));
        let list = m.entry(name.trim().to_owned()).or_insert_with(|| json!([]));
        if let Some(a) = list.as_array_mut() {
            let mut key = json!({ "at": t.trim(), "value": value });
            if let Some(e) = ease {
                key["ease"] = json!(e);
            }
            a.push(key);
        }
    }
    for c in clear {
        m.insert(c.clone(), json!([]));
    }
    Value::Object(m)
}

#[derive(Subcommand)]
enum TrackCmd {
    /// Every track with its kind, position and clip count.
    List,
    /// `ve track add video [--name Titles] [--index 0]`. Video tracks added
    /// later draw on top; `--index 0` puts one at the bottom.
    Add {
        /// `video` or `audio`.
        kind: String,
        #[arg(long)]
        name: Option<String>,
        /// Position among tracks of the same kind (0 is the first).
        #[arg(long)]
        index: Option<usize>,
    },
    /// `ve track rm V2`. `--force` also deletes the track's clips.
    Rm {
        track: String,
        #[arg(long)]
        force: bool,
    },
    /// `ve track rename V2 Titles`.
    Rename { track: String, name: String },
    /// `ve track move V3 0`: new position among tracks of its kind.
    Move { track: String, index: usize },
    /// `ve track hide V2` (or `mute A2`): renders skip the track; its clips stay.
    #[command(alias = "mute")]
    Hide { track: String },
    /// `ve track show V2` (or `unmute A2`).
    #[command(alias = "unmute")]
    Show { track: String },
}

#[derive(Subcommand)]
enum MarkerCmd {
    /// `ve marker add --at 00:00:10:00 --name chapter`.
    Add {
        #[arg(long)]
        at: String,
        #[arg(long)]
        duration: Option<String>,
        #[arg(long, default_value = "")]
        name: String,
        #[arg(long)]
        color: Option<String>,
        #[arg(long, default_value = "")]
        comment: String,
    },
    /// `ve marker rm mk_1a2b` or `ve marker rm --name silence`.
    Rm {
        ids: Vec<String>,
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Subcommand)]
enum GroupCmd {
    Begin { label: String },
    End,
}

/// What to measure: an asset's whole file, or the timeline (optionally a range).
#[derive(Args, Clone)]
struct MeasureSource {
    /// Measure this asset's file instead of the timeline.
    #[arg(long)]
    asset: Option<String>,
    #[command(flatten)]
    span: Span,
}

#[derive(Subcommand)]
enum MeasureCmd {
    /// EBU R128 loudness: integrated LUFS, loudness range, true peak.
    Loudness {
        #[command(flatten)]
        source: MeasureSource,
        /// Also list loudness per step, e.g. `1s`.
        #[arg(long)]
        resolution: Option<String>,
    },
    /// Levels per channel, clipping, DC offset, noise floor, stereo balance and phase.
    Levels {
        #[command(flatten)]
        source: MeasureSource,
        #[arg(long)]
        resolution: Option<String>,
    },
    /// Waveform peaks for drawing (use --json to get the arrays).
    Waveform {
        #[command(flatten)]
        source: MeasureSource,
        #[arg(long, default_value_t = 1000)]
        buckets: usize,
    },
    /// Pitch of voices and instruments: median, range, nearest note.
    Pitch {
        #[command(flatten)]
        source: MeasureSource,
        #[arg(long)]
        resolution: Option<String>,
    },
}

#[derive(Subcommand)]
enum AnalyzeCmd {
    /// Quiet stretches of the timeline's audio, as `silence` markers.
    Silence {
        #[command(flatten)]
        span: Span,
        /// Level below which audio counts as silence, e.g. -35dB.
        #[arg(long, default_value = "-35dB", allow_hyphen_values = true)]
        threshold: String,
        /// Shortest silence to report.
        #[arg(long, default_value = "0.5s")]
        min: String,
        /// Shrink each silence by this much at both ends (keeps breaths natural).
        #[arg(long, default_value = "0s")]
        pad: String,
        /// Report only; do not write markers.
        #[arg(long)]
        no_markers: bool,
    },
    /// Hard cuts in the timeline's picture, as `scene` markers.
    Scenes {
        #[command(flatten)]
        span: Span,
        /// scdet threshold, 0-100 (lower finds more cuts).
        #[arg(long, default_value = "10")]
        threshold: String,
        #[arg(long)]
        no_markers: bool,
    },
}

/// Paths go to the engine absolute, since a daemon's working directory is not ours.
fn absolute(p: &Path) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| p.to_owned())
}

fn media_text(media: &Value) -> String {
    serde_json::from_value::<ve_model::meta::MediaInfo>(media.clone())
        .map(|m| ve_model::summary::media_line(&m))
        .unwrap_or_default()
}

/// A failed call: the JSON-RPC error object.
struct CallError(Value);

/// Where calls go: the project's daemon, or this process for project-less commands.
enum Engine {
    Local(Arc<Registry>),
    Daemon(Client),
}

impl Engine {
    async fn call(&mut self, method: &str, params: Value) -> Result<Value, CallError> {
        match self {
            Self::Local(registry) => {
                let req = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
                let resp: Value = serde_json::from_str(&registry.dispatch(&req).await)
                    .unwrap_or_else(|e| json!({ "error": { "code": -32603, "message": e.to_string() } }));
                match resp.get("error") {
                    Some(err) => Err(CallError(err.clone())),
                    None => Ok(resp["result"].clone()),
                }
            }
            Self::Daemon(client) => match client.call(method, params).await {
                Ok(r) => r.map_err(CallError),
                Err(e) => Err(CallError(json!({
                    "code": ErrorCode::Internal.rpc_code(),
                    "message": format!("lost the connection to the ve daemon: {e}"),
                }))),
            },
        }
    }
}

fn internal_error(message: String, json_output: bool) -> ! {
    fail(&json!({ "code": ErrorCode::Internal.rpc_code(), "message": message }), json_output)
}

/// The project for this command: `--project`, else the nearest folder with a project.otio.
fn project_root(cli: &Cli) -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    cli.project.clone().or_else(|| ve_daemon::project::find_root(&cwd)).map(|p| absolute(&p))
}

fn require_project(cli: &Cli) -> PathBuf {
    project_root(cli).unwrap_or_else(|| {
        let cwd = std::env::current_dir().unwrap_or_default();
        usage_error(
            format!("no project.otio in {} or any parent folder; pass --project or run `ve init`", cwd.display()),
            cli.json,
        )
    })
}

async fn connect(root: &Path, json_output: bool) -> Client {
    let exe = std::env::current_exe().unwrap_or_else(|e| internal_error(format!("finding the ve binary: {e}"), json_output));
    stdio::keep_from_children();
    client::connect_or_spawn(root, &exe).await.unwrap_or_else(|e| internal_error(e.to_string(), json_output))
}

fn fail(err: &Value, json_output: bool) -> ! {
    if json_output {
        eprintln!("{}", serde_json::to_string_pretty(err).unwrap_or_default());
    } else {
        eprintln!("error: {}", err["message"].as_str().unwrap_or("unknown error"));
    }
    let code = err["code"].as_i64().unwrap_or(0);
    std::process::exit(ErrorCode::from_rpc_code(code).exit_code());
}

fn usage_error(message: String, json_output: bool) -> ! {
    fail(&json!({ "code": ErrorCode::InvalidParams.rpc_code(), "message": message }), json_output)
}

/// `agent:claude` -> `{"kind": "agent", "id": "claude"}`.
fn parse_actor(s: &str) -> Option<Value> {
    let (kind, id) = s.split_once(':').unwrap_or((s, if s == "user" { "local" } else { "" }));
    (["user", "agent", "plugin"].contains(&kind) && !id.is_empty()).then(|| json!({"kind": kind, "id": id}))
}

fn span_params(span: &Span) -> Value {
    json!({ "from": span.from, "to": span.to })
}

/// A call to make: method, params, and whether it mutates (gets base_rev etc.).
struct Plan {
    method: &'static str,
    params: Value,
    mutates: bool,
}

fn plan(cmd: &Command, json_output: bool) -> Plan {
    let m = |method, params| Plan { method, params, mutates: true };
    let q = |method, params| Plan { method, params, mutates: false };
    match cmd {
        Command::Version => q("system.version", json!({})),
        Command::Describe => q("system.describe", json!({})),
        Command::Init { path, fps, size, name, sample_rate } => {
            let Some((w, h)) = size.split_once(['x', 'X']).and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
            else {
                usage_error(format!("--size `{size}`: use WIDTHxHEIGHT, e.g. 1920x1080"), json_output)
            };
            q("project.init", json!({ "path": absolute(path), "name": name, "fps": fps, "width": w, "height": h, "sample_rate": sample_rate }))
        }
        Command::Summary => q("project.summary", json!({})),
        Command::Probe { path } => q("media.probe", json!({ "path": absolute(path) })),
        Command::Import { paths } => {
            let paths: Vec<PathBuf> = paths.iter().map(|p| absolute(p)).collect();
            m("media.import", json!({ "paths": paths }))
        }
        Command::Relink { asset: None, path: None, folder: None, .. } => q("media.assets", json!({})),
        Command::Relink { asset, path, folder, force } => {
            m("media.relink", json!({ "asset": asset, "path": path.as_deref().map(absolute), "folder": folder.as_deref().map(absolute), "force": force }))
        }
        Command::Insert { asset, track, audio_track, at, source_in, source_out, duration, overwrite, no_video, no_audio } => m(
            "edit.insert",
            json!({
                "asset": asset, "track": track, "audio_track": audio_track, "at": at,
                "source_in": source_in, "source_out": source_out, "duration": duration,
                "overwrite": overwrite, "video": !no_video, "audio": !no_audio,
            }),
        ),
        Command::Split { clips, at, tracks, unlinked } => {
            m("edit.split", json!({ "clips": clips, "at": at, "tracks": tracks, "linked": !unlinked }))
        }
        Command::Trim { clip, in_delta, out_delta, roll, unlinked } => {
            m("edit.trim", json!({ "clip": clip, "in": in_delta, "out": out_delta, "roll": roll, "linked": !unlinked }))
        }
        Command::Move { clip, to, track, unlinked, reorder } => {
            m("edit.move", json!({ "clip": clip, "to": to, "track": track, "linked": !unlinked, "reorder": reorder }))
        }
        Command::RippleDelete { clips, span, ranges, markers, tracks, unlinked } => {
            let mut rs: Vec<Value> = Vec::new();
            match (&span.from, &span.to) {
                (Some(f), Some(t)) => rs.push(json!({"from": f, "to": t})),
                (None, None) => {}
                _ => usage_error("give both --from and --to".into(), json_output),
            }
            for r in ranges {
                let Some((f, t)) = r.split_once("..") else {
                    usage_error(format!("--range `{r}`: use FROM..TO, e.g. 00:01:00:00..00:01:02:15"), json_output)
                };
                rs.push(json!({"from": f, "to": t}));
            }
            m(
                "edit.ripple_delete",
                json!({ "clips": clips, "ranges": rs, "markers": markers, "tracks": tracks, "linked": !unlinked }),
            )
        }
        Command::Delete { clips, unlinked } => m("edit.delete", json!({ "clips": clips, "linked": !unlinked })),
        Command::Speed { clip, speed, unlinked, no_ripple } => {
            m("edit.set_speed", json!({ "clip": clip, "speed": speed, "linked": !unlinked, "ripple": !no_ripple }))
        }
        Command::Volume { clip, db, unlinked } => m("edit.set_volume", json!({ "clip": clip, "db": db, "linked": !unlinked })),
        Command::Unlink { clip } => m("edit.unlink", json!({ "clip": clip })),
        Command::Transform { clip, x, y, scale, rotation, opacity, crop_left, crop_right, crop_top, crop_bottom, keys, clear, blend, reset } => {
            let mut p = json!({ "clip": clip, "reset": reset });
            if let Some(b) = blend {
                p["blend"] = json!(b);
            }
            for (k, v) in [
                ("x", x),
                ("y", y),
                ("scale", scale),
                ("rotation", rotation),
                ("opacity", opacity),
                ("crop_left", crop_left),
                ("crop_right", crop_right),
                ("crop_top", crop_top),
                ("crop_bottom", crop_bottom),
            ] {
                if let Some(v) = v {
                    p[k] = json!(v);
                }
            }
            if !keys.is_empty() || !clear.is_empty() {
                p["keyframes"] = key_map(keys, clear, json_output);
            }
            m("edit.set_transform", p)
        }
        Command::Marker(MarkerCmd::Add { at, duration, name, color, comment }) => {
            m("marker.add", json!({ "at": at, "duration": duration, "name": name, "color": color, "comment": comment }))
        }
        Command::Marker(MarkerCmd::Rm { ids, name }) => m("marker.remove", json!({ "ids": ids, "name": name })),
        Command::Track(TrackCmd::List) => q("track.list", json!({})),
        Command::Track(TrackCmd::Add { kind, name, index }) => {
            m("track.add", json!({ "kind": kind.to_lowercase(), "name": name, "index": index }))
        }
        Command::Track(TrackCmd::Rm { track, force }) => m("track.remove", json!({ "track": track, "force": force })),
        Command::Track(TrackCmd::Rename { track, name }) => m("track.rename", json!({ "track": track, "name": name })),
        Command::Track(TrackCmd::Move { track, index }) => m("track.move", json!({ "track": track, "index": index })),
        Command::Track(TrackCmd::Hide { track }) => m("track.set", json!({ "track": track, "enabled": false })),
        Command::Track(TrackCmd::Show { track }) => m("track.set", json!({ "track": track, "enabled": true })),
        Command::Fx(FxCmd::List) => q("fx.list", json!({})),
        Command::Fx(FxCmd::Define { folder, dry_run }) => {
            let mp = folder.join("effect.json");
            let text = std::fs::read_to_string(&mp).unwrap_or_else(|e| usage_error(format!("{}: {e}", mp.display()), json_output));
            let manifest: Value = serde_json::from_str(&text).unwrap_or_else(|e| usage_error(format!("{}: {e}", mp.display()), json_output));
            // Audio effects carry their code (the filter chain) in the manifest.
            let source = if manifest["kind"] == "audio" {
                String::new()
            } else {
                let shader = manifest["shader"].as_str().unwrap_or("effect.wgsl").to_owned();
                let sp = folder.join(&shader);
                std::fs::read_to_string(&sp).unwrap_or_else(|e| usage_error(format!("{}: {e}", sp.display()), json_output))
            };
            // Images for image params travel with the shader.
            let mut files = serde_json::Map::new();
            for e in std::fs::read_dir(folder).into_iter().flatten().flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if [".png", ".jpg", ".jpeg"].iter().any(|x| name.to_ascii_lowercase().ends_with(x)) {
                    let bytes = std::fs::read(e.path()).unwrap_or_else(|err| usage_error(format!("{name}: {err}"), json_output));
                    files.insert(name, Value::from(base64::engine::general_purpose::STANDARD.encode(bytes)));
                }
            }
            q("fx.define", json!({ "manifest": manifest, "source": source, "files": files, "dry_run": dry_run }))
        }
        Command::Effect(EffectCmd::Add { clip, effect, params, index }) => {
            m("effect.add", json!({ "clip": clip, "effect": effect, "params": param_map(params, json_output), "index": index }))
        }
        Command::Effect(EffectCmd::Set { id, params, keys, clear_keys, index }) => {
            let mut p = json!({ "id": id, "index": index });
            if !params.is_empty() {
                p["params"] = param_map(params, json_output);
            }
            if !keys.is_empty() || !clear_keys.is_empty() {
                p["keyframes"] = key_map(keys, clear_keys, json_output);
            }
            m("effect.set", p)
        }
        Command::Effect(EffectCmd::Rm { id }) => m("effect.remove", json!({ "id": id })),
        Command::Font(FontCmd::Add { path, url, google, replace }) => {
            q("font.add", json!({ "path": path.as_deref().map(absolute), "url": url, "google": google, "replace": replace }))
        }
        Command::Font(FontCmd::List { system }) => q("font.list", json!({ "system": system })),
        Command::Font(FontCmd::Rm { file }) => q("font.remove", json!({ "file": file })),
        Command::Text(TextCmd::Add { text, at, duration, track, overwrite, style, x, y }) => m(
            "text.add",
            json!({ "text": text, "at": at, "duration": duration, "track": track, "overwrite": overwrite, "style": param_map(style, json_output), "x": x, "y": y }),
        ),
        Command::Text(TextCmd::Set { clip, text, style, reset_style }) => {
            m("text.set", json!({ "clip": clip, "text": text, "style": param_map(style, json_output), "reset_style": reset_style }))
        }
        Command::Captions { span, track, y, style } => {
            let mut p = span_params(span);
            p["track"] = json!(track);
            p["y"] = json!(y);
            p["style"] = param_map(style, json_output);
            m("text.captions", p)
        }
        Command::Transition(TransitionCmd::Add { clip, at, track, kind, duration, align, params }) => {
            let mut p = json!({ "clip": clip, "at": at, "track": track, "params": param_map(params, json_output) });
            if let Some(k) = kind {
                p["transition"] = json!(k);
            }
            if let Some(d) = duration {
                p["duration"] = json!(d);
            }
            if let Some(a) = align {
                p["align"] = json!(a);
            }
            m("transition.add", p)
        }
        Command::Transition(TransitionCmd::Set { id, duration, align, kind, params }) => {
            let mut p = json!({ "id": id, "duration": duration, "align": align, "transition": kind });
            if !params.is_empty() {
                p["params"] = param_map(params, json_output);
            }
            m("transition.set", p)
        }
        Command::Transition(TransitionCmd::Rm { id }) => m("transition.remove", json!({ "id": id })),
        Command::Undo => m("history.undo", json!({})),
        Command::Redo => m("history.redo", json!({})),
        Command::History { limit } => q("history.list", json!({ "limit": limit })),
        Command::Group(GroupCmd::Begin { label }) => q("history.begin_group", json!({ "label": label })),
        Command::Group(GroupCmd::End) => q("history.end_group", json!({})),
        Command::Frame { at, width, output } => {
            let format = if ext(output) == "png" { "png" } else { "jpeg" };
            q("render.frame", json!({ "at": at, "width": width, "format": format }))
        }
        Command::Layers { at: Some(at), only, .. } => q("render.layers", json!({ "at": at, "only": only })),
        Command::Layers { span, only, .. } => {
            let mut p = span_params(span);
            p["only"] = json!(only);
            q("render.layers", p)
        }
        Command::Sheet { span, count, columns, width, output } => {
            let mut p = span_params(span);
            p["count"] = json!(count);
            p["columns"] = json!(columns);
            p["width"] = json!(width);
            p["format"] = json!(if ext(output) == "png" { "png" } else { "jpeg" });
            q("render.contact_sheet", p)
        }
        Command::Preview { span, width, .. } => {
            let mut p = span_params(span);
            p["width"] = json!(width);
            q("render.range_preview", p)
        }
        Command::Export { list_presets: true, .. } => q("render.presets", json!({})),
        Command::Export { preset, format, span, output, .. } => {
            let mut p = span_params(span);
            p["preset"] = json!(preset);
            let ext = output.as_deref().and_then(|o| o.extension()).map(|e| e.to_string_lossy().to_lowercase());
            let from_ext = match ext.as_deref() {
                Some("mov") => Some("prores"),
                Some(e @ ("mp4" | "webm" | "wav" | "m4a" | "mp3" | "flac")) => Some(e),
                _ => None,
            };
            p["format"] = json!(format.as_deref().or(from_ext));
            q("render.export", p)
        }
        Command::Analyze(AnalyzeCmd::Silence { span, threshold, min, pad, no_markers }) => {
            let mut p = span_params(span);
            p["threshold"] = json!(threshold);
            p["min_duration"] = json!(min);
            p["pad"] = json!(pad);
            p["markers"] = json!(!no_markers);
            m("analyze.silence", p)
        }
        Command::Analyze(AnalyzeCmd::Scenes { span, threshold, no_markers }) => {
            let mut p = span_params(span);
            p["threshold"] = json!(threshold);
            p["markers"] = json!(!no_markers);
            m("analyze.scenes", p)
        }
        Command::Measure(cmd) => {
            let (method, source, extra) = match cmd {
                MeasureCmd::Loudness { source, resolution } => ("measure.loudness", source, json!({ "resolution": resolution })),
                MeasureCmd::Levels { source, resolution } => ("measure.levels", source, json!({ "resolution": resolution })),
                MeasureCmd::Waveform { source, buckets } => ("measure.waveform", source, json!({ "buckets": buckets })),
                MeasureCmd::Pitch { source, resolution } => ("measure.pitch", source, json!({ "resolution": resolution })),
            };
            let mut p = span_params(&source.span);
            p["asset"] = json!(source.asset);
            for (k, v) in extra.as_object().into_iter().flatten() {
                p[k] = v.clone();
            }
            q(method, p)
        }
        Command::Normalize { target, ceiling, span } => {
            let mut p = span_params(span);
            p["target"] = json!(target);
            p["ceiling"] = json!(ceiling);
            m("edit.normalize", p)
        }
        Command::Jobs { cmd: None } => q("jobs.list", json!({})),
        Command::Jobs { cmd: Some(JobsCmd::Get { job_id }) } => q("jobs.get", json!({ "job_id": job_id })),
        Command::Jobs { cmd: Some(JobsCmd::Cancel { job_id }) } => q("jobs.cancel", json!({ "job_id": job_id })),
        Command::Cache { cmd: None } => q("cache.status", json!({})),
        Command::Cache { cmd: Some(CacheCmd::Clear { kinds, dry_run }) } => {
            q("cache.clear", json!({ "kinds": kinds, "dry_run": dry_run }))
        }
        Command::Transcript { asset, format, span } => {
            let default = if json_output { "words" } else { "text" };
            let mut p = json!({ "format": format.as_deref().unwrap_or(default) });
            if let Some(a) = asset {
                p["asset"] = json!(a);
            }
            if let Some(f) = &span.from {
                p["from"] = json!(f);
            }
            if let Some(t) = &span.to {
                p["to"] = json!(t);
            }
            q("transcript.get", p)
        }
        Command::Call { .. } | Command::Daemon(_) | Command::Plugin(_) | Command::External(_) => unreachable!("planned in main"),
    }
}

/// `job_3f2a  render.export  running  42%`
fn job_line(j: &Value) -> String {
    let progress = match (j["state"].as_str(), j["progress"].as_f64()) {
        (Some("running"), Some(p)) => format!("  {:.0}%", p * 100.0),
        _ => String::new(),
    };
    let error = j["error"]["message"].as_str().map_or(String::new(), |m| format!("  ({m})"));
    format!(
        "{}  {:22} {}{progress}{error}",
        j["job_id"].as_str().unwrap_or("?"),
        j["method"].as_str().unwrap_or("?"),
        j["state"].as_str().unwrap_or("?")
    )
}

/// Commands whose work can take minutes, run as background jobs.
fn runs_as_job(cmd: &Command) -> bool {
    matches!(
        cmd,
        Command::Export { list_presets: false, .. }
            | Command::Preview { .. }
            | Command::Sheet { .. }
            | Command::Analyze(_)
            | Command::Measure(_)
            | Command::Normalize { .. }
    )
}

/// Wait for a job, showing its progress on stderr when that is a terminal.
/// Ctrl+C cancels the job (the CLI then exits with the `cancelled` code).
async fn wait_for_job(engine: &mut Engine, job_id: &Value, method: &str, json_output: bool) -> Value {
    use std::io::{IsTerminal, Write};
    let show = std::io::stderr().is_terminal();
    let label = method.rsplit('.').next().unwrap_or(method).replace('_', " ");
    let clear = || {
        if show {
            eprint!("\r{:40}\r", "");
        }
    };
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);
    let mut cancelling = false;
    loop {
        let job = match engine.call("jobs.get", json!({ "job_id": job_id })).await {
            Ok(j) => j,
            Err(CallError(e)) => {
                clear();
                fail(&e, json_output)
            }
        };
        match job["state"].as_str() {
            Some("done") => {
                clear();
                return job["result"].clone();
            }
            Some("failed" | "cancelled") => {
                clear();
                fail(&job["error"], json_output)
            }
            _ => {}
        }
        if show {
            let state = if cancelling {
                "cancelling".to_owned()
            } else {
                match job["progress"].as_f64() {
                    Some(p) => format!("{:3.0}%", p * 100.0),
                    None if job["state"] == "queued" => "waiting for another job".to_owned(),
                    None => "running".to_owned(),
                }
            };
            eprint!("\r{label}  {state}   ");
            let _ = std::io::stderr().flush();
        }
        tokio::select! {
            () = tokio::time::sleep(Duration::from_millis(200)) => {}
            r = &mut ctrl_c, if !cancelling => {
                if r.is_ok() {
                    cancelling = true;
                    let _ = engine.call("jobs.cancel", json!({ "job_id": job_id })).await;
                }
            }
        }
    }
}

fn ext(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Lines for an edit report: created/modified/removed clips and the duration.
fn print_commit(r: &Value) {
    let place = |c: &Value| {
        format!(
            "{}  {}  {}-{}",
            c["id"].as_str().unwrap_or("?"),
            c["track"].as_str().unwrap_or("?"),
            c["start"].as_str().unwrap_or("?"),
            c["end"].as_str().unwrap_or("?")
        )
    };
    for (key, label) in [("created", "new"), ("modified", "changed")] {
        for c in r[key].as_array().into_iter().flatten() {
            println!("{label:8} {}", place(c));
        }
    }
    for id in r["removed"].as_array().into_iter().flatten() {
        println!("removed  {}", id.as_str().unwrap_or("?"));
    }
    if r["opacity"].is_number() && r["clip"].is_string() {
        let mut line = format!("transform of {}:", r["clip"].as_str().unwrap_or("?"));
        for f in ["x", "y", "scale", "rotation", "opacity", "crop_left", "crop_right", "crop_top", "crop_bottom"] {
            line.push_str(&format!(" {f}={}", r[f]));
        }
        if let Some(k) = r["keyframed"].as_array().filter(|k| !k.is_empty()) {
            line.push_str(&format!("  keyframed: {}", k.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")));
        }
        println!("{line}");
    }
    for t in r["tracks"].as_array().into_iter().flatten() {
        let mark = if t["id"] == r["track"]["id"] { "*" } else { " " };
        println!(
            "{mark} {:8} {:12} {} #{}  clips {}{}",
            t["name"].as_str().unwrap_or("?"),
            t["id"].as_str().unwrap_or("?"),
            t["kind"].as_str().unwrap_or("?"),
            t["index"],
            t["clips"],
            match (t["enabled"].as_bool(), t["kind"].as_str()) {
                (Some(false), Some("audio")) => "  (muted)",
                (Some(false), _) => "  (hidden)",
                _ => "",
            }
        );
    }
    if let Some(list) = r["effects"].as_array() {
        println!("effects on {}:", r["clip"].as_str().unwrap_or("?"));
        for (i, e) in list.iter().enumerate() {
            let mark = if e["id"] == r["effect"] { "*" } else { " " };
            let params = e.get("params").map_or(String::new(), |p| format!("  {p}"));
            let keyed = e["keyframed"]
                .as_array()
                .map_or(String::new(), |k| format!("  keyframed: {}", k.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")));
            println!("{mark} {i}  {}  {}{params}{keyed}", e["id"].as_str().unwrap_or("?"), e["effect"].as_str().unwrap_or("?"));
        }
    }
    if let Some(cut) = r["cut"].as_str() {
        println!(
            "transition {} {}  {}  {}-{} (cut {cut})  {} -> {}",
            r["id"].as_str().unwrap_or("?"),
            r["transition"].as_str().unwrap_or("?"),
            r["track"].as_str().unwrap_or("?"),
            r["start"].as_str().unwrap_or("?"),
            r["end"].as_str().unwrap_or("?"),
            r["outgoing"].as_str().unwrap_or("?"),
            r["incoming"].as_str().unwrap_or("?")
        );
    }
    for id in r["markers"].as_array().into_iter().flatten() {
        println!("marker   {}", id.as_str().unwrap_or("?"));
    }
    if let Some(step) = r.get("step") {
        let revs: Vec<String> = step["revs"].as_array().into_iter().flatten().map(Value::to_string).collect();
        println!("{} (by {}, rev {})", step["what"].as_str().unwrap_or("?"), step["actor"].as_str().unwrap_or("?"), revs.join(","));
    }
    if let Some(d) = r["duration"].as_str() {
        println!("duration {d}");
    }
    let rev = &r["revision"];
    match (r["dry_run"].as_bool(), r["changed"].as_bool()) {
        (Some(true), Some(true)) => {
            println!("dry run: would make rev {}; project unchanged (rev {rev})", rev.as_u64().unwrap_or(0) + 1);
            println!("patch: {}", r["patch"]);
        }
        (_, Some(false)) => println!("no change (rev {rev})"),
        _ => println!("rev {rev}"),
    }
}

/// `-14.2` or `-` for a missing value.
fn num(v: &Value, unit: &str) -> String {
    v.as_f64().map_or_else(|| "-".to_owned(), |x| format!("{x:.1}{unit}"))
}

/// `render.layers`: a frame's layers top to bottom (as the eye reads a
/// stack), or a range's spans.
fn print_layers(r: &Value) {
    let s = |v: &Value| v.as_str().unwrap_or("?").to_owned();
    let list = |v: &Value| v.as_array().into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>().join(", ");
    let effect_lines = |fx: &Value| {
        for e in fx.as_array().into_iter().flatten() {
            let params: Vec<String> = e["params"].as_object().into_iter().flatten().map(|(k, v)| format!("{k}={v}")).collect();
            let missing = if e["missing"].as_bool() == Some(true) { " (not installed)" } else { "" };
            println!("       effect {} {}{missing}  {}", s(&e["id"]), s(&e["effect"]), params.join(" "));
        }
    };
    if !r["at"].is_string() {
        for sp in r["spans"].as_array().into_iter().flatten() {
            let hidden = if sp["hidden"].is_array() { format!("  (hidden: {})", list(&sp["hidden"])) } else { String::new() };
            let pic = match sp["visible"].as_array() {
                Some(v) if !v.is_empty() => format!("  {}{hidden}", list(&sp["visible"])),
                Some(_) => "  no picture".to_owned(),
                None => String::new(),
            };
            let snd = match sp["audible"].as_array() {
                Some(v) if !v.is_empty() => list(&sp["audible"]),
                Some(_) => "silent".to_owned(),
                None => String::new(),
            };
            let sep = if pic.is_empty() || snd.is_empty() { "  " } else { "  |  " };
            println!("{}-{}{pic}{sep}{snd}", s(&sp["from"]), s(&sp["to"]));
        }
        return;
    }
    let layers = r["layers"].as_array().cloned().unwrap_or_default();
    if r["layers"].is_array() {
        let disabled = if r["disabled"].is_array() { format!("  (off: {})", list(&r["disabled"])) } else { String::new() };
        println!("at {}  visible: {}{disabled}", s(&r["at"]), list(&r["visible"]));
    } else {
        println!("at {}", s(&r["at"]));
    }
    for l in layers.iter().rev() {
        let mut what = s(&l["visibility"]);
        if what == "partial" {
            what = format!("partial {}%", l["visible_percent"]);
        }
        if l["covered_by"].is_array() {
            what += &format!(", covered by {}", list(&l["covered_by"]));
        }
        if l["overlapped_by"].is_array() {
            what += &format!(", under {}", list(&l["overlapped_by"]));
        }
        if let Some(n) = l["note"].as_str() {
            what += &format!(" ({n})");
        }
        let src = l["source_time"].as_str().map_or("still".to_owned(), |t| format!("at {t}"));
        println!("{:4} {}  {} {}  {what}", s(&l["track"]), s(&l["clip"]), l["asset"].as_str().unwrap_or("-"), src);
        let rect = &l["rect"];
        let mut place = format!("{}x{} at {},{}", rect["w"], rect["h"], rect["x"], rect["y"]);
        if let Some(d) = l["rotation"].as_f64() {
            place += &format!(", turned {d}");
        }
        if l["opacity"].as_f64().is_some_and(|o| o < 1.0) {
            place += &format!(", opacity {}", l["opacity"]);
        }
        println!("       {place}, alpha {}{}", s(&l["alpha"]), if l["solid"].as_bool() == Some(true) { ", solid" } else { "" });
        if let Some(t) = l["transition"].as_object() {
            println!("       transition {} {} ({} with {}, {:.0}%)", s(&t["id"]), s(&t["transition"]), s(&t["role"]), s(&t["with"]), t["progress"].as_f64().unwrap_or(0.0) * 100.0);
        }
        effect_lines(&l["effects"]);
    }
    if !r["audio"].is_array() {
        return;
    }
    let muted = if r["muted"].is_array() { format!("  (off: {})", list(&r["muted"])) } else { String::new() };
    let heard = if r["audible"].as_array().is_some_and(|v| !v.is_empty()) { list(&r["audible"]) } else { "nothing".into() };
    println!("heard: {heard}{muted}");
    for a in r["audio"].as_array().into_iter().flatten() {
        let gain = a["gain_db"].as_f64().unwrap_or(0.0);
        let mut what = if a["audible"].as_bool() == Some(true) { format!("{gain:+.1} dB") } else { "silenced (-100 dB)".into() };
        if let Some(sp) = a["speed"].as_f64() {
            what += &format!(", speed {sp}");
        }
        if let Some(link) = a["link"].as_str() {
            let with: Vec<String> = layers.iter().filter(|l| l["link"].as_str() == Some(link)).map(|l| s(&l["track"])).collect();
            if !with.is_empty() {
                what += &format!(", with picture on {}", with.join(", "));
            }
        }
        println!("{:4} {}  {} at {}  {what}", s(&a["track"]), s(&a["clip"]), a["asset"].as_str().unwrap_or("-"), s(&a["source_time"]));
        effect_lines(&a["effects"]);
    }
}

fn print_measure(cmd: &MeasureCmd, r: &Value) {
    let src = &r["source"];
    let what = src["asset"].as_str().map_or_else(|| "timeline".to_owned(), str::to_owned);
    let cached = if r["cached"] == true { "  (cached)" } else { "" };
    println!("{what}  {}-{}{cached}", src["from"].as_str().unwrap_or("?"), src["to"].as_str().unwrap_or("?"));
    match cmd {
        MeasureCmd::Loudness { .. } => println!(
            "integrated {}  range {}  true peak {}",
            num(&r["integrated_lufs"], " LUFS"),
            num(&r["range_lu"], " LU"),
            num(&r["true_peak_dbtp"], " dBTP")
        ),
        MeasureCmd::Levels { .. } => {
            for (i, c) in r["channels"].as_array().into_iter().flatten().enumerate() {
                println!(
                    "ch{}  rms {}  peak {}  dc {}  clipped {}",
                    i + 1,
                    num(&c["rms_db"], " dB"),
                    num(&c["peak_db"], " dB"),
                    c["dc_offset"],
                    c["clipped_samples"]
                );
            }
            println!("noise floor {}", num(&r["noise_floor_db"], " dB"));
            if r.get("balance_db").is_some() {
                println!("balance (L-R) {}  correlation {}", num(&r["balance_db"], " dB"), r["correlation"]);
            }
        }
        MeasureCmd::Waveform { .. } => {
            let points = r["min"].as_array().map_or(0, Vec::len);
            println!("{points} points over {} samples at {} Hz (--json for the arrays)", r["samples"], r["sample_rate"]);
        }
        MeasureCmd::Pitch { .. } => println!(
            "median {} ({})  range {}-{}  voiced {:.0}%",
            num(&r["median_hz"], " Hz"),
            r["note"].as_str().unwrap_or("-"),
            num(&r["low_hz"], ""),
            num(&r["high_hz"], " Hz"),
            r["voiced"].as_f64().unwrap_or(0.0) * 100.0
        ),
    }
    if let Some(s) = r.get("series").and_then(Value::as_object) {
        let step = s.get("step").and_then(Value::as_str).unwrap_or("?");
        let cols: Vec<(&String, &Vec<Value>)> = s.iter().filter_map(|(k, v)| v.as_array().map(|a| (k, a))).collect();
        let names: Vec<&str> = cols.iter().map(|c| c.0.as_str()).collect();
        println!("every {step}: {}", names.join("  "));
        let n = cols.iter().map(|c| c.1.len()).max().unwrap_or(0);
        for i in 0..n {
            let vals: Vec<String> = cols.iter().map(|c| num(c.1.get(i).unwrap_or(&Value::Null), "")).collect();
            println!("  {i:5}  {}", vals.join("  "));
        }
    }
    for w in r["warnings"].as_array().into_iter().flatten() {
        println!("warning: {}", w.as_str().unwrap_or("?"));
    }
}

fn write_output(result: &Value, output: &Path, json_output: bool) {
    let res = if let Some(data) = result["data"].as_str() {
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| e.to_string())
            .and_then(|bytes| std::fs::write(output, bytes).map_err(|e| e.to_string()))
    } else if let Some(path) = result["path"].as_str() {
        // Rename if on the same filesystem; copy otherwise.
        std::fs::rename(path, output).or_else(|_| std::fs::copy(path, output).map(|_| ()).and_then(|()| std::fs::remove_file(path))).map_err(|e| e.to_string())
    } else {
        Err("the engine returned no file".into())
    };
    if let Err(e) = res {
        fail(&json!({"code": ErrorCode::Internal.rpc_code(), "message": format!("writing {}: {e}", output.display())}), json_output);
    }
}

fn main() -> anyhow::Result<()> {
    // Debug builds of the argument parser need more stack than the 1 MiB
    // Windows gives the main thread; 8 MiB is the usual Linux default.
    match std::thread::Builder::new().name("ve".into()).stack_size(8 << 20).spawn(real_main)?.join() {
        Ok(result) => result,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

fn real_main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    if let Command::Daemon(DaemonCmd::Run) = cli.command {
        return run_daemon(&cli);
    }
    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(run(cli))
}

/// `ve daemon run`: serve the project until idle or stopped.
fn run_daemon(cli: &Cli) -> anyhow::Result<()> {
    let project = require_project(cli);
    let env = |name: &str| std::env::var(name).ok().and_then(|v| v.parse::<u64>().ok());
    let opts = ve_daemon::server::Options {
        project,
        idle: env("VE_DAEMON_IDLE_SECS").map_or(ve_daemon::server::DEFAULT_IDLE, Duration::from_secs),
        ws_port: env("VE_WS_PORT").and_then(|p| u16::try_from(p).ok()).unwrap_or(0),
    };
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let result = rt.block_on(ve_daemon::server::run(opts));
    // Do not wait on a call still running (an export nobody is waiting for).
    rt.shutdown_timeout(Duration::from_secs(1));
    result.map_err(|e| {
        eprintln!("error: {e}");
        e.into()
    })
}

/// `ve daemon start|status|stop`.
async fn daemon_command(cli: &Cli, cmd: &DaemonCmd) -> anyhow::Result<()> {
    let root = require_project(cli);
    let loc = ve_daemon::project::locate(&root).unwrap_or_else(|e| usage_error(e.to_string(), cli.json));
    let mut client = match cmd {
        DaemonCmd::Start => Some(connect(&root, cli.json).await),
        _ => Client::connect(&loc).await.ok(),
    };
    let Some(c) = client.as_mut() else {
        if cli.json {
            println!("{}", json!({ "running": false, "project": loc.root }));
        } else {
            println!("no ve daemon is running for {}", loc.root.display());
        }
        return Ok(());
    };
    let info = match c.call("daemon.info", json!({})).await? {
        Ok(i) => i,
        Err(e) => fail(&e, cli.json),
    };
    let stop = matches!(cmd, DaemonCmd::Stop);
    if stop && let Err(e) = c.call("daemon.stop", json!({})).await? {
        fail(&e, cli.json);
    }
    if cli.json {
        let mut out = info.clone();
        out["running"] = json!(!stop);
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else if stop {
        println!("stopped the ve daemon (pid {}) for {}", info["pid"], info["project"].as_str().unwrap_or("?"));
    } else {
        println!("running    pid {}, ve {}", info["pid"], info["engine"].as_str().unwrap_or("?"));
        println!("project    {}", info["project"].as_str().unwrap_or("?"));
        println!("socket     {}", info["socket"].as_str().unwrap_or("?"));
        println!("websocket  {}", info["websocket"].as_str().unwrap_or("?"));
        println!("idle exit  {}s", info["idle_timeout_secs"]);
    }
    Ok(())
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    if let Command::Daemon(cmd) = &cli.command {
        return daemon_command(&cli, cmd).await;
    }
    let Some(actor) = parse_actor(&cli.actor) else {
        usage_error(format!("--actor `{}`: use user, agent:<name> or plugin:<id>", cli.actor), cli.json)
    };
    if let Command::Plugin(cmd) = &cli.command {
        return plugin_command(&cli, cmd).await;
    }
    if let Command::External(args) = &cli.command {
        return plugin_cli(&cli, args, actor).await;
    }

    let (method, mut params, mutates) = match &cli.command {
        Command::Call { method, params } => {
            let p = match params.as_deref().map(serde_json::from_str).transpose() {
                Ok(p) => p.unwrap_or(json!({})),
                Err(e) => usage_error(format!("params are not valid JSON: {e}"), cli.json),
            };
            (method.clone(), p, false)
        }
        cmd => {
            let p = plan(cmd, cli.json);
            (p.method.to_owned(), p.params, p.mutates)
        }
    };

    // Methods on a project go to its daemon; the rest run here.
    let standalone = method.starts_with("system.") || ["project.init", "project.open", "media.probe", "render.presets"].contains(&method.as_str());
    let mut opened_rev = None;
    let mut engine = match (standalone, project_root(&cli)) {
        (false, Some(root)) => {
            let mut engine = Engine::Daemon(connect(&root, cli.json).await);
            match engine.call("project.open", json!({ "path": root })).await {
                Ok(info) => opened_rev = info["revision"].as_u64(),
                Err(CallError(e)) => fail(&e, cli.json),
            }
            engine
        }
        // A raw call with no project runs here (it fails cleanly if it needs one).
        (false, None) if !matches!(cli.command, Command::Call { .. }) => {
            require_project(&cli);
            unreachable!("require_project exits when there is no project")
        }
        _ => Engine::Local(ve_daemon::build_registry()),
    };

    if mutates {
        params["base_rev"] = json!(cli.base_rev.or(opened_rev).unwrap_or(0));
        params["dry_run"] = json!(cli.dry_run);
    }
    if mutates || method.ends_with("_group") {
        params["actor"] = actor;
    }

    // Long work runs as a daemon job: a progress line, and Ctrl+C cancels it.
    let as_job = matches!(engine, Engine::Daemon(_)) && runs_as_job(&cli.command);
    if as_job {
        params["background"] = json!(true);
    }
    let result = match engine.call(&method, params).await {
        Ok(r) if as_job => wait_for_job(&mut engine, &r["job_id"], &method, cli.json).await,
        Ok(r) => r,
        Err(CallError(e)) => fail(&e, cli.json),
    };

    // Render commands put their file at -o (also with --json).
    let output = match &cli.command {
        Command::Frame { output, .. } | Command::Sheet { output, .. } | Command::Preview { output, .. } => Some(output.clone()),
        Command::Export { output, list_presets: false, .. } => output.clone(),
        _ => None,
    };
    if let Some(out) = &output {
        write_output(&result, out, cli.json);
    }
    if cli.json {
        let mut shown = result.clone();
        if let Some(o) = shown.as_object_mut() {
            // Bytes went to the file; keep the JSON readable.
            if o.remove("data").is_some()
                && let Some(out) = &output
            {
                o.insert("output".into(), json!(absolute(out)));
            }
            if output.is_some() && o.remove("path").is_some() {
                o.insert("output".into(), json!(output.as_deref().map(absolute)));
            }
        }
        println!("{}", serde_json::to_string_pretty(&shown)?);
        return Ok(());
    }

    match &cli.command {
        Command::Version => println!(
            "ve {}  protocol {}  schema {}",
            result["engine"].as_str().unwrap_or("?"),
            result["protocol"],
            result["schema"].as_str().unwrap_or("?")
        ),
        Command::Init { .. } => {
            println!("created {} ({})", result["path"].as_str().unwrap_or("?"), result["name"].as_str().unwrap_or("?"))
        }
        Command::Summary => print!("{}", result["text"].as_str().unwrap_or_default()),
        Command::Probe { .. } => {
            println!("{}", result["url"].as_str().unwrap_or("?"));
            println!("{}", media_text(&result["media"]));
        }
        Command::Relink { .. } => {
            let file = |u: &Value| u.as_str().unwrap_or("?").to_owned();
            for r in result["relinked"].as_array().into_iter().flatten() {
                let note = if r["changed_content"].as_bool() == Some(true) { "  (different content, probed again)" } else { "" };
                println!("{}  {} -> {}{note}", r["asset"].as_str().unwrap_or("?"), file(&r["from"]), file(&r["url"]));
            }
            let missing: Vec<&str> = result["missing"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            if missing.is_empty() {
                println!("no missing media");
            } else {
                let urls: std::collections::HashMap<&str, &str> = result["assets"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|a| Some((a["id"].as_str()?, a["url"].as_str()?)))
                    .collect();
                for id in &missing {
                    println!("missing  {id}  {}", urls.get(id).copied().unwrap_or(""));
                }
                println!("fix with: ve relink --folder <where the files are now>");
            }
            if result.get("patch").is_some() {
                print_commit(&result);
            }
        }
        Command::Import { .. } => {
            for a in result["assets"].as_array().into_iter().flatten() {
                let file = a["url"].as_str().unwrap_or("?").rsplit('/').next().unwrap_or("?");
                let state = if a["existing"].as_bool() == Some(true) { "already imported" } else { "new" };
                println!("{}  {file}  ({state})  {}", a["id"].as_str().unwrap_or("?"), media_text(&a["media"]));
            }
            print_commit(&result);
        }
        Command::History { .. } => {
            let line = |s: &Value| {
                let revs: Vec<String> = s["revs"].as_array().into_iter().flatten().map(Value::to_string).collect();
                format!("rev {:8}  {:14}  {}", revs.join(","), s["actor"].as_str().unwrap_or("?"), s["what"].as_str().unwrap_or("?"))
            };
            println!("project at rev {}", result["revision"]);
            for s in result["undo"].as_array().into_iter().flatten() {
                println!("undo  {}", line(s));
            }
            for s in result["redo"].as_array().into_iter().flatten() {
                println!("redo  {}", line(s));
            }
            for g in result["open_groups"].as_array().into_iter().flatten() {
                println!("open group  {}  {}  {:?}", g[0].as_str().unwrap_or("?"), g[1]["id"].as_str().unwrap_or("?"), g[1]["label"].as_str().unwrap_or(""));
            }
        }
        Command::Group(GroupCmd::Begin { .. }) => println!("group {} open", result["id"].as_str().unwrap_or("?")),
        Command::Group(GroupCmd::End) => println!(
            "group {} closed: {} edits undo as one step",
            result["group"]["id"].as_str().unwrap_or("?"),
            result["edits"]
        ),
        Command::Frame { output, .. } => println!("{}  {}x{}  at {}", output.display(), result["width"], result["height"], result["at"].as_str().unwrap_or("?")),
        Command::Layers { .. } => print_layers(&result),
        Command::Sheet { output, .. } => {
            let times: Vec<&str> = result["times"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            println!("{}  {}x{} grid, row by row:", output.display(), result["columns"], result["rows"]);
            for (i, t) in times.iter().enumerate() {
                println!("  {:2}  {t}", i + 1);
            }
        }
        Command::Preview { output, .. } | Command::Export { output: Some(output), list_presets: false, .. } => {
            println!(
                "{}  {}  {} frames  {}x{}  {}",
                output.display(),
                result["duration"].as_str().unwrap_or("?"),
                result["frames"],
                result["width"],
                result["height"],
                result["encoder"].as_str().unwrap_or("?")
            );
            for w in result["warnings"].as_array().into_iter().flatten() {
                println!("warning: {}", w.as_str().unwrap_or("?"));
            }
        }
        Command::Export { list_presets: true, .. } => {
            for p in result["presets"].as_array().into_iter().flatten() {
                println!("{:16} {}", p["name"].as_str().unwrap_or("?"), p["description"].as_str().unwrap_or(""));
            }
        }
        Command::Analyze(_) => {
            for r in result["ranges"].as_array().into_iter().flatten() {
                println!("{}  {}-{}  {}", r["kind"].as_str().unwrap_or("?"), r["start"].as_str().unwrap_or("?"), r["end"].as_str().unwrap_or("?"), r["note"].as_str().unwrap_or(""));
            }
            println!("{} found", result["ranges"].as_array().map_or(0, Vec::len));
            print_commit(&result);
        }
        Command::Measure(cmd) => print_measure(cmd, &result),
        Command::Normalize { .. } => {
            let f = |v: &Value| v.as_f64().map_or("?".into(), |x| format!("{x:.1}"));
            println!(
                "measured {} LUFS (true peak {} dBTP), gain {:+.1} dB -> {} LUFS (true peak {} dBTP)",
                f(&result["measured_lufs"]),
                f(&result["measured_true_peak_dbtp"]),
                result["gain_db"].as_f64().unwrap_or(0.0),
                f(&result["expected_lufs"]),
                f(&result["expected_true_peak_dbtp"])
            );
            for w in result["warnings"].as_array().into_iter().flatten() {
                println!("warning: {}", w.as_str().unwrap_or("?"));
            }
            print_commit(&result);
        }
        Command::Fx(FxCmd::List) => {
            for e in result["effects"].as_array().into_iter().flatten() {
                let params: Vec<&str> = e["params"]["properties"].as_object().map(|o| o.keys().map(String::as_str).collect()).unwrap_or_default();
                println!(
                    "{:<10} {:<32} {:<14} {}",
                    e["kind"].as_str().unwrap_or("?"),
                    e["reference"].as_str().unwrap_or("?"),
                    e["name"].as_str().unwrap_or("?"),
                    if params.is_empty() { "(no params)".to_owned() } else { params.join(", ") }
                );
            }
            for p in result["problems"].as_array().into_iter().flatten() {
                println!("problem: {}", p.as_str().unwrap_or("?"));
            }
        }
        Command::Fx(FxCmd::Define { .. }) => {
            let e = &result["effect"];
            match result["path"].as_str() {
                Some(path) => println!("defined {} ({}), saved to {path}", e["reference"].as_str().unwrap_or("?"), e["kind"].as_str().unwrap_or("?")),
                None => println!("{} is valid (dry run; nothing saved)", e["reference"].as_str().unwrap_or("?")),
            }
            println!("test render passed on {}", result["tested_on"].as_str().unwrap_or("?"));
        }
        Command::Insert { .. }
        | Command::Split { .. }
        | Command::Trim { .. }
        | Command::Move { .. }
        | Command::RippleDelete { .. }
        | Command::Delete { .. }
        | Command::Speed { .. }
        | Command::Volume { .. }
        | Command::Unlink { .. }
        | Command::Transform { .. }
        | Command::Marker(_)
        | Command::Track(_)
        | Command::Effect(_)
        | Command::Transition(_)
        | Command::Text(_)
        | Command::Undo
        | Command::Redo => print_commit(&result),
        Command::Font(FontCmd::Add { .. }) => {
            for f in result["files"].as_array().into_iter().flatten() {
                let faces: Vec<String> = f["faces"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|x| format!("{} {}{}", x["family"].as_str().unwrap_or("?"), x["weight"], if x["italic"] == true { " italic" } else { "" }))
                    .collect();
                let note = if f["existing"] == true { "  (already there)" } else { "" };
                println!("fonts/{}  {}{note}", f["file"].as_str().unwrap_or("?"), faces.join(", "));
            }
            let fams: Vec<&str> = result["families"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            println!("use with: --style font=\"{}\"", fams.first().copied().unwrap_or("?"));
        }
        Command::Font(FontCmd::List { .. }) => {
            let project = result["project"].as_array().cloned().unwrap_or_default();
            if project.is_empty() {
                println!("no project fonts (ve font add adds one)");
            }
            for f in &project {
                let fam = f["faces"][0]["family"].as_str().unwrap_or("?");
                println!("fonts/{}  {fam}", f["file"].as_str().unwrap_or("?"));
            }
            for fam in result["system"].as_array().into_iter().flatten() {
                println!("system  {}", fam.as_str().unwrap_or("?"));
            }
        }
        Command::Font(FontCmd::Rm { .. }) => {
            let state = if result["removed"] == true { "removed" } else { "not found:" };
            println!("{state} fonts/{}", result["file"].as_str().unwrap_or("?"));
        }
        Command::Captions { .. } => {
            print_commit(&result);
            println!("{} captions", result["captions"]);
            for a in result["missing"].as_array().into_iter().flatten() {
                eprintln!("{} has no transcript yet, so it has no captions (a speech-to-text plugin makes one: `ve plugin list`)", a.as_str().unwrap_or("?"));
            }
        }
        Command::Jobs { cmd: None } => {
            let jobs = result["jobs"].as_array().cloned().unwrap_or_default();
            if jobs.is_empty() {
                println!("no jobs");
            }
            for j in &jobs {
                println!("{}", job_line(j));
            }
        }
        Command::Jobs { cmd: Some(JobsCmd::Cancel { .. }) } => {
            let state = result["state"].as_str().unwrap_or("?");
            if ["done", "failed", "cancelled"].contains(&state) {
                println!("{} already {state}", result["job_id"].as_str().unwrap_or("?"));
            } else {
                println!("cancelling {}", result["job_id"].as_str().unwrap_or("?"));
            }
        }
        Command::Cache { cmd } => {
            for e in result["entries"].as_array().into_iter().flatten() {
                let (bytes, files) = (e["bytes"].as_u64().unwrap_or(0), e["files"].as_u64().unwrap_or(0));
                let noun = if files == 1 { "file" } else { "files" };
                println!("{:<9} {:>10}  {files} {noun}", e["kind"].as_str().unwrap_or("?"), size_text(bytes));
            }
            let total = size_text(result["total_bytes"].as_u64().unwrap_or(0));
            match (cmd, result["cleared"].as_bool()) {
                (_, Some(true)) => println!("freed {total}"),
                (Some(_), _) => println!("would free {total}"),
                (None, _) => println!("total {total}"),
            }
        }
        Command::Transcript { .. } => {
            print!("{}", result["text"].as_str().unwrap_or_default());
            for a in result["missing"].as_array().into_iter().flatten() {
                eprintln!("{} has no transcript yet (a speech-to-text plugin makes one: `ve plugin list`)", a.as_str().unwrap_or("?"));
            }
        }
        _ => println!("{}", serde_json::to_string_pretty(&result)?),
    }
    Ok(())
}

// --- plugins ---

/// `ve plugin list|install|secret`.
async fn plugin_command(cli: &Cli, cmd: &PluginCmd) -> anyhow::Result<()> {
    match cmd {
        PluginCmd::List => {
            // Inside a project its daemon answers (project plugins too); elsewhere this process does.
            let mut engine = match project_root(cli) {
                Some(root) => {
                    let mut e = Engine::Daemon(connect(&root, cli.json).await);
                    if let Err(CallError(err)) = e.call("project.open", json!({ "path": root })).await {
                        fail(&err, cli.json);
                    }
                    e
                }
                None => Engine::Local(ve_daemon::build_registry()),
            };
            let list = match engine.call("plugin.list", json!({})).await {
                Ok(l) => l,
                Err(CallError(e)) => fail(&e, cli.json),
            };
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&list)?);
                return Ok(());
            }
            print_plugins(&list);
        }
        PluginCmd::Install { folder, force } => match ve_plugin::install(&absolute(folder), &ve_daemon::ENGINE_VERSION.parse().expect("engine version is semver"), *force) {
            Ok((m, dest)) => {
                if cli.json {
                    println!("{}", json!({ "id": m.id, "version": m.version.to_string(), "dir": dest }));
                } else {
                    println!("installed {} {} in {}", m.id, m.version, dest.display());
                    for s in &m.secrets {
                        if ve_plugin::secrets::source(&m.id, s) == ve_plugin::secrets::SecretSource::Missing {
                            println!("it needs {s}: ve plugin secret set {} {s}", m.id);
                        }
                    }
                }
            }
            Err(e) => usage_error(e.to_string(), cli.json),
        },
        PluginCmd::Secret(SecretCmd::Set { plugin, name }) => {
            use std::io::{BufRead, IsTerminal, Write};
            if std::io::stdin().is_terminal() {
                eprint!("{name} for {plugin} (shown as you type): ");
                let _ = std::io::stderr().flush();
            }
            let mut value = String::new();
            std::io::stdin().lock().read_line(&mut value)?;
            let value = value.trim();
            if value.is_empty() {
                usage_error("no value given".into(), cli.json);
            }
            if let Err(e) = ve_plugin::secrets::set(plugin, name, value) {
                internal_error(e, cli.json);
            }
            if cli.json {
                println!("{}", json!({ "plugin": plugin, "name": name, "stored": true }));
            } else {
                println!("stored {name} for {plugin} in the OS keychain");
            }
        }
        PluginCmd::Secret(SecretCmd::Rm { plugin, name }) => {
            let removed = ve_plugin::secrets::delete(plugin, name).unwrap_or_else(|e| internal_error(e, cli.json));
            if cli.json {
                println!("{}", json!({ "plugin": plugin, "name": name, "removed": removed }));
            } else if removed {
                println!("removed {name} for {plugin}");
            } else {
                println!("no {name} stored for {plugin}");
            }
        }
    }
    Ok(())
}

fn print_plugins(list: &Value) {
    let plugins = list["plugins"].as_array().cloned().unwrap_or_default();
    if plugins.is_empty() {
        println!("no plugins installed");
    }
    for p in &plugins {
        let running = if p["running"] == true { "  (running)" } else { "" };
        println!(
            "{} {}  {}  [{}]{running}",
            p["id"].as_str().unwrap_or("?"),
            p["version"].as_str().unwrap_or("?"),
            p["name"].as_str().unwrap_or(""),
            p["source"].as_str().unwrap_or("?")
        );
        for c in p["commands"].as_array().into_iter().flatten() {
            let usage = match c["cli"]["name"].as_str() {
                Some(name) => {
                    let args: Vec<String> = c["cli"]["args"].as_array().into_iter().flatten().filter_map(|a| a.as_str().map(|a| format!("<{a}>"))).collect();
                    format!("ve {name} {}", args.join(" "))
                }
                None => format!("ve {}", c["method"].as_str().unwrap_or("?")),
            };
            println!("  {:36} {}", usage.trim_end(), c["summary"].as_str().unwrap_or(""));
        }
        for s in p["secrets"].as_array().into_iter().flatten() {
            let name = s["name"].as_str().unwrap_or("?");
            match s["source"].as_str() {
                Some("missing") => println!("  needs {name}: ve plugin secret set {} {name}", p["id"].as_str().unwrap_or("?")),
                Some(src) => println!("  {name} set ({src})"),
                None => {}
            }
        }
        if let Some(skill) = p["skill"].as_str() {
            println!("  skill: {skill}");
        }
    }
    for i in list["issues"].as_array().into_iter().flatten() {
        eprintln!("not loaded: {}", i["message"].as_str().unwrap_or("?"));
    }
    if let Some(dir) = list["user_dir"].as_str() {
        println!("user plugins: {dir}");
    }
}

/// `ve <plugin command> ...`: find the command among the project's plugins,
/// map the arguments onto its params, call it (as a job if it is one).
async fn plugin_cli(cli: &Cli, args: &[String], actor: Value) -> anyhow::Result<()> {
    let Some((name, rest)) = args.split_first() else { usage_error("no command".into(), cli.json) };
    // Global flags written after the subcommand land here; honour them.
    let json_output = cli.json || rest.iter().any(|a| a == "--json");
    let dry_run = cli.dry_run || rest.iter().any(|a| a == "--dry-run");
    let rest: Vec<String> = rest.iter().filter(|a| *a != "--json" && *a != "--dry-run").cloned().collect();
    let Some(root) = project_root(cli) else {
        usage_error(format!("unknown command `{name}`; `ve --help` lists the built-in ones (plugin commands need a project)"), json_output)
    };
    let mut engine = Engine::Daemon(connect(&root, json_output).await);
    let opened_rev = match engine.call("project.open", json!({ "path": root })).await {
        Ok(info) => info["revision"].as_u64(),
        Err(CallError(e)) => fail(&e, json_output),
    };
    let list = match engine.call("plugin.list", json!({})).await {
        Ok(l) => l,
        Err(CallError(e)) => fail(&e, json_output),
    };
    let found = list["plugins"].as_array().into_iter().flatten().find_map(|p| {
        p["commands"].as_array().into_iter().flatten().find(|c| c["cli"]["name"] == name.as_str() || c["method"] == name.as_str()).cloned()
    });
    let Some(cmd) = found else {
        usage_error(format!("unknown command `{name}`; `ve --help` lists the built-in ones, `ve plugin list` the plugin ones"), json_output)
    };
    let method = cmd["method"].as_str().unwrap_or_default().to_owned();
    if rest.iter().any(|a| a == "--help" || a == "-h") {
        print_plugin_help(name, &cmd);
        return Ok(());
    }
    let mut params = plugin_params(&cmd, &rest).unwrap_or_else(|e| usage_error(format!("{name}: {e} (see `ve {name} --help`)"), json_output));
    let props = cmd["params"]["properties"].as_object().cloned().unwrap_or_default();
    if props.contains_key("base_rev") && params.get("base_rev").is_none() {
        params["base_rev"] = json!(cli.base_rev.or(opened_rev).unwrap_or(0));
    }
    if props.contains_key("dry_run") && dry_run {
        params["dry_run"] = json!(true);
    }
    if props.contains_key("actor") {
        params["actor"] = actor;
    }
    let as_job = cmd["job"] == true;
    if as_job {
        params["background"] = json!(true);
    }
    let result = match engine.call(&method, params).await {
        Ok(r) if as_job => wait_for_job(&mut engine, &r["job_id"], &method, json_output).await,
        Ok(r) => r,
        Err(CallError(e)) => fail(&e, json_output),
    };
    match result["text"].as_str() {
        Some(text) if !json_output => {
            print!("{text}");
            if !text.ends_with('\n') {
                println!();
            }
        }
        _ => println!("{}", serde_json::to_string_pretty(&result)?),
    }
    Ok(())
}

fn print_plugin_help(name: &str, cmd: &Value) {
    let positional: Vec<&str> = cmd["cli"]["args"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    println!("{}", cmd["summary"].as_str().unwrap_or(""));
    println!();
    println!("Usage: ve {name} {}", positional.iter().map(|a| format!("<{a}>")).collect::<Vec<_>>().join(" "));
    println!("Method: {}", cmd["method"].as_str().unwrap_or("?"));
    let required: Vec<&str> = cmd["params"]["required"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    if let Some(props) = cmd["params"]["properties"].as_object() {
        println!();
        for (k, v) in props {
            if ["base_rev", "dry_run", "actor", "background"].contains(&k.as_str()) {
                continue;
            }
            let flag = if positional.contains(&k.as_str()) { format!("<{k}>") } else { format!("--{}", k.replace('_', "-")) };
            let ty = schema_type(v).unwrap_or("string");
            let req = if required.contains(&k.as_str()) { " (required)" } else { "" };
            println!("  {flag:24} {ty}{req}  {}", v["description"].as_str().unwrap_or(""));
        }
    }
}

/// The JSON Schema `type` of a property (the first non-null one of a list).
fn schema_type(prop: &Value) -> Option<&str> {
    match &prop["type"] {
        Value::String(s) => Some(s.as_str()),
        Value::Array(a) => a.iter().filter_map(Value::as_str).find(|t| *t != "null"),
        _ => None,
    }
}

/// Map `ve <cmd>` arguments onto a command's params: positionals in the
/// order the manifest's `cli.args` gives, everything else `--flag value`
/// (or `--flag=value`; a boolean flag alone means true; an array flag can repeat).
fn plugin_params(cmd: &Value, args: &[String]) -> Result<Value, String> {
    let props = cmd["params"]["properties"].as_object().cloned().unwrap_or_default();
    let positional: Vec<String> = cmd["cli"]["args"].as_array().into_iter().flatten().filter_map(|a| a.as_str().map(String::from)).collect();
    let mut out = serde_json::Map::new();
    let mut next_pos = 0;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        i += 1;
        if let Some(flag) = a.strip_prefix("--") {
            let (key, inline) = match flag.split_once('=') {
                Some((k, v)) => (k.replace('-', "_"), Some(v.to_owned())),
                None => (flag.replace('-', "_"), None),
            };
            let Some(prop) = props.get(&key) else {
                let known: Vec<String> = props
                    .keys()
                    .filter(|k| !["base_rev", "dry_run", "actor", "background"].contains(&k.as_str()))
                    .map(|k| format!("--{}", k.replace('_', "-")))
                    .collect();
                return Err(format!("unknown flag --{flag}; it takes {}", if known.is_empty() { "none".into() } else { known.join(", ") }));
            };
            let ty = schema_type(prop).unwrap_or("string");
            let raw = match inline {
                Some(v) => v,
                None if ty == "boolean" => "true".into(),
                None => {
                    let v = args.get(i).cloned().ok_or_else(|| format!("--{flag} needs a value"))?;
                    i += 1;
                    v
                }
            };
            if ty == "array" {
                let item_ty = schema_type(&prop["items"]).unwrap_or("string");
                let v = convert(item_ty, &raw).map_err(|e| format!("--{flag}: {e}"))?;
                let entry = out.entry(key).or_insert_with(|| json!([]));
                if let Some(arr) = entry.as_array_mut() {
                    arr.push(v);
                }
            } else {
                out.insert(key, convert(ty, &raw).map_err(|e| format!("--{flag}: {e}"))?);
            }
        } else {
            let Some(key) = positional.get(next_pos) else {
                return Err(format!("unexpected argument `{a}`"));
            };
            next_pos += 1;
            let ty = props.get(key).and_then(schema_type).unwrap_or("string");
            out.insert(key.clone(), convert(ty, a).map_err(|e| format!("<{key}>: {e}"))?);
        }
    }
    for r in cmd["params"]["required"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        if !out.contains_key(r) && !["base_rev", "actor"].contains(&r) {
            let shown = if positional.iter().any(|p| p == r) { format!("<{r}>") } else { format!("--{}", r.replace('_', "-")) };
            return Err(format!("missing {shown}"));
        }
    }
    Ok(Value::Object(out))
}

fn convert(ty: &str, raw: &str) -> Result<Value, String> {
    match ty {
        "integer" => raw.parse::<i64>().map(Value::from).map_err(|_| format!("`{raw}` is not a whole number")),
        "number" => raw.parse::<f64>().ok().filter(|f| f.is_finite()).map(Value::from).ok_or_else(|| format!("`{raw}` is not a number")),
        "boolean" => match raw {
            "true" | "yes" | "1" => Ok(Value::Bool(true)),
            "false" | "no" | "0" => Ok(Value::Bool(false)),
            _ => Err(format!("`{raw}`: use true or false")),
        },
        "object" | "array" => serde_json::from_str(raw).map_err(|e| format!("not valid JSON: {e}")),
        _ => Ok(Value::String(raw.to_owned())),
    }
}

/// `1.4 GB`, `320 MB`, `12 KB`.
fn size_text(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1000.0 && u < UNITS.len() - 1 {
        v /= 1000.0;
        u += 1;
    }
    if u == 0 || v >= 100.0 { format!("{v:.0} {}", UNITS[u]) } else { format!("{v:.1} {}", UNITS[u]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_args_map_onto_params() {
        let cmd = json!({
            "method": "acme.transcribe",
            "cli": { "name": "transcribe", "args": ["asset"] },
            "params": {
                "type": "object",
                "properties": {
                    "asset": { "type": "string" },
                    "language": { "type": ["string", "null"] },
                    "max_speakers": { "type": "integer" },
                    "diarize": { "type": "boolean" },
                    "keywords": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["asset"]
            }
        });
        let args: Vec<String> = ["asset_1", "--language", "he", "--max-speakers=3", "--diarize", "--keywords", "a", "--keywords", "b"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let p = plugin_params(&cmd, &args).unwrap();
        assert_eq!(p, json!({ "asset": "asset_1", "language": "he", "max_speakers": 3, "diarize": true, "keywords": ["a", "b"] }));
        assert!(plugin_params(&cmd, &[]).unwrap_err().contains("missing <asset>"));
        assert!(plugin_params(&cmd, &["a".into(), "b".into()]).unwrap_err().contains("unexpected"));
        assert!(plugin_params(&cmd, &["a".into(), "--nope".into()]).unwrap_err().contains("unknown flag"));
        assert!(plugin_params(&cmd, &["a".into(), "--max-speakers".into(), "x".into()]).unwrap_err().contains("whole number"));
    }
}
