//! `ve`: the CLI. Each subcommand maps to one protocol method.
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

// All command output goes through these instead of `out!`/`outln!`, which
// panic when the reader has gone away (`ve history | head`).
macro_rules! out {
    ($($arg:tt)*) => { write_stdout(format_args!($($arg)*), false) };
}
macro_rules! outln {
    ($($arg:tt)*) => { write_stdout(format_args!($($arg)*), true) };
}

/// Write to stdout. If the reader closed the pipe, stop quietly with success,
/// like a Unix tool killed by SIGPIPE but portable (Windows reports the same
/// `BrokenPipe` kind for a closed pipe).
fn write_stdout(args: std::fmt::Arguments<'_>, newline: bool) {
    use std::io::{ErrorKind, Write as _};
    let mut out = std::io::stdout().lock();
    let written = out.write_fmt(args).and_then(|()| if newline { out.write_all(b"\n") } else { Ok(()) });
    match written {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::BrokenPipe => std::process::exit(0),
        Err(e) => {
            eprintln!("error: writing to stdout: {e}");
            std::process::exit(ErrorCode::Internal.exit_code());
        }
    }
}

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
    Move {
        clip: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        track: Option<String>,
        #[arg(long)]
        unlinked: bool,
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
    /// .wgsl (naga, then a GPU test render) and save it to the project.
    Define {
        folder: PathBuf,
        #[arg(long)]
        dry_run: bool,
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
        /// `name@time=value` keyframes (time from the clip start). Replaces that param's keys.
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
            usage_error(format!("--key `{k}`: use NAME@TIME=VALUE, e.g. amount@1s=0.5"), json_output)
        };
        let value = serde_json::from_str(v.trim()).unwrap_or_else(|_| Value::String(v.trim().to_owned()));
        let list = m.entry(name.trim().to_owned()).or_insert_with(|| json!([]));
        if let Some(a) = list.as_array_mut() {
            a.push(json!({ "at": t.trim(), "value": value }));
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
        Command::Move { clip, to, track, unlinked } => {
            m("edit.move", json!({ "clip": clip, "to": to, "track": track, "linked": !unlinked }))
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
        Command::Fx(FxCmd::List) => q("fx.list", json!({})),
        Command::Fx(FxCmd::Define { folder, dry_run }) => {
            let mp = folder.join("effect.json");
            let text = std::fs::read_to_string(&mp).unwrap_or_else(|e| usage_error(format!("{}: {e}", mp.display()), json_output));
            let manifest: Value = serde_json::from_str(&text).unwrap_or_else(|e| usage_error(format!("{}: {e}", mp.display()), json_output));
            let shader = manifest["shader"].as_str().unwrap_or("effect.wgsl").to_owned();
            let sp = folder.join(&shader);
            let source = std::fs::read_to_string(&sp).unwrap_or_else(|e| usage_error(format!("{}: {e}", sp.display()), json_output));
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
        Command::Export { preset, span, .. } => {
            let mut p = span_params(span);
            p["preset"] = json!(preset);
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
        Command::Call { .. } | Command::Daemon(_) => unreachable!("planned in main"),
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
            outln!("{label:8} {}", place(c));
        }
    }
    for id in r["removed"].as_array().into_iter().flatten() {
        outln!("removed  {}", id.as_str().unwrap_or("?"));
    }
    for t in r["tracks"].as_array().into_iter().flatten() {
        let mark = if t["id"] == r["track"]["id"] { "*" } else { " " };
        outln!(
            "{mark} {:8} {:12} {} #{}  clips {}",
            t["name"].as_str().unwrap_or("?"),
            t["id"].as_str().unwrap_or("?"),
            t["kind"].as_str().unwrap_or("?"),
            t["index"],
            t["clips"]
        );
    }
    if let Some(list) = r["effects"].as_array() {
        outln!("effects on {}:", r["clip"].as_str().unwrap_or("?"));
        for (i, e) in list.iter().enumerate() {
            let mark = if e["id"] == r["effect"] { "*" } else { " " };
            let params = e.get("params").map_or(String::new(), |p| format!("  {p}"));
            let keyed = e["keyframed"]
                .as_array()
                .map_or(String::new(), |k| format!("  keyframed: {}", k.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")));
            outln!("{mark} {i}  {}  {}{params}{keyed}", e["id"].as_str().unwrap_or("?"), e["effect"].as_str().unwrap_or("?"));
        }
    }
    if let Some(cut) = r["cut"].as_str() {
        outln!(
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
        outln!("marker   {}", id.as_str().unwrap_or("?"));
    }
    if let Some(step) = r.get("step") {
        let revs: Vec<String> = step["revs"].as_array().into_iter().flatten().map(Value::to_string).collect();
        outln!("{} (by {}, rev {})", step["what"].as_str().unwrap_or("?"), step["actor"].as_str().unwrap_or("?"), revs.join(","));
    }
    if let Some(d) = r["duration"].as_str() {
        outln!("duration {d}");
    }
    let rev = &r["revision"];
    match (r["dry_run"].as_bool(), r["changed"].as_bool()) {
        (Some(true), Some(true)) => {
            outln!("dry run: would make rev {}; project unchanged (rev {rev})", rev.as_u64().unwrap_or(0) + 1);
            outln!("patch: {}", r["patch"]);
        }
        (_, Some(false)) => outln!("no change (rev {rev})"),
        _ => outln!("rev {rev}"),
    }
}

/// `-14.2` or `-` for a missing value.
fn num(v: &Value, unit: &str) -> String {
    v.as_f64().map_or_else(|| "-".to_owned(), |x| format!("{x:.1}{unit}"))
}

fn print_measure(cmd: &MeasureCmd, r: &Value) {
    let src = &r["source"];
    let what = src["asset"].as_str().map_or_else(|| "timeline".to_owned(), str::to_owned);
    let cached = if r["cached"] == true { "  (cached)" } else { "" };
    outln!("{what}  {}-{}{cached}", src["from"].as_str().unwrap_or("?"), src["to"].as_str().unwrap_or("?"));
    match cmd {
        MeasureCmd::Loudness { .. } => outln!(
            "integrated {}  range {}  true peak {}",
            num(&r["integrated_lufs"], " LUFS"),
            num(&r["range_lu"], " LU"),
            num(&r["true_peak_dbtp"], " dBTP")
        ),
        MeasureCmd::Levels { .. } => {
            for (i, c) in r["channels"].as_array().into_iter().flatten().enumerate() {
                outln!(
                    "ch{}  rms {}  peak {}  dc {}  clipped {}",
                    i + 1,
                    num(&c["rms_db"], " dB"),
                    num(&c["peak_db"], " dB"),
                    c["dc_offset"],
                    c["clipped_samples"]
                );
            }
            outln!("noise floor {}", num(&r["noise_floor_db"], " dB"));
            if r.get("balance_db").is_some() {
                outln!("balance (L-R) {}  correlation {}", num(&r["balance_db"], " dB"), r["correlation"]);
            }
        }
        MeasureCmd::Waveform { .. } => {
            let points = r["min"].as_array().map_or(0, Vec::len);
            outln!("{points} points over {} samples at {} Hz (--json for the arrays)", r["samples"], r["sample_rate"]);
        }
        MeasureCmd::Pitch { .. } => outln!(
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
        outln!("every {step}: {}", names.join("  "));
        let n = cols.iter().map(|c| c.1.len()).max().unwrap_or(0);
        for i in 0..n {
            let vals: Vec<String> = cols.iter().map(|c| num(c.1.get(i).unwrap_or(&Value::Null), "")).collect();
            outln!("  {i:5}  {}", vals.join("  "));
        }
    }
    for w in r["warnings"].as_array().into_iter().flatten() {
        outln!("warning: {}", w.as_str().unwrap_or("?"));
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
            outln!("{}", json!({ "running": false, "project": loc.root }));
        } else {
            outln!("no ve daemon is running for {}", loc.root.display());
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
        outln!("{}", serde_json::to_string_pretty(&out)?);
    } else if stop {
        outln!("stopped the ve daemon (pid {}) for {}", info["pid"], info["project"].as_str().unwrap_or("?"));
    } else {
        outln!("running    pid {}, ve {}", info["pid"], info["engine"].as_str().unwrap_or("?"));
        outln!("project    {}", info["project"].as_str().unwrap_or("?"));
        outln!("socket     {}", info["socket"].as_str().unwrap_or("?"));
        outln!("websocket  {}", info["websocket"].as_str().unwrap_or("?"));
        outln!("idle exit  {}s", info["idle_timeout_secs"]);
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
        outln!("{}", serde_json::to_string_pretty(&shown)?);
        return Ok(());
    }

    match &cli.command {
        Command::Version => outln!(
            "ve {}  protocol {}  schema {}",
            result["engine"].as_str().unwrap_or("?"),
            result["protocol"],
            result["schema"].as_str().unwrap_or("?")
        ),
        Command::Init { .. } => {
            outln!("created {} ({})", result["path"].as_str().unwrap_or("?"), result["name"].as_str().unwrap_or("?"))
        }
        Command::Summary => out!("{}", result["text"].as_str().unwrap_or_default()),
        Command::Probe { .. } => {
            outln!("{}", result["url"].as_str().unwrap_or("?"));
            outln!("{}", media_text(&result["media"]));
        }
        Command::Import { .. } => {
            for a in result["assets"].as_array().into_iter().flatten() {
                let file = a["url"].as_str().unwrap_or("?").rsplit('/').next().unwrap_or("?");
                let state = if a["existing"].as_bool() == Some(true) { "already imported" } else { "new" };
                outln!("{}  {file}  ({state})  {}", a["id"].as_str().unwrap_or("?"), media_text(&a["media"]));
            }
            print_commit(&result);
        }
        Command::History { .. } => {
            let line = |s: &Value| {
                let revs: Vec<String> = s["revs"].as_array().into_iter().flatten().map(Value::to_string).collect();
                format!("rev {:8}  {:14}  {}", revs.join(","), s["actor"].as_str().unwrap_or("?"), s["what"].as_str().unwrap_or("?"))
            };
            outln!("project at rev {}", result["revision"]);
            for s in result["undo"].as_array().into_iter().flatten() {
                outln!("undo  {}", line(s));
            }
            for s in result["redo"].as_array().into_iter().flatten() {
                outln!("redo  {}", line(s));
            }
            for g in result["open_groups"].as_array().into_iter().flatten() {
                outln!("open group  {}  {}  {:?}", g[0].as_str().unwrap_or("?"), g[1]["id"].as_str().unwrap_or("?"), g[1]["label"].as_str().unwrap_or(""));
            }
        }
        Command::Group(GroupCmd::Begin { .. }) => outln!("group {} open", result["id"].as_str().unwrap_or("?")),
        Command::Group(GroupCmd::End) => outln!(
            "group {} closed: {} edits undo as one step",
            result["group"]["id"].as_str().unwrap_or("?"),
            result["edits"]
        ),
        Command::Frame { output, .. } => outln!("{}  {}x{}  at {}", output.display(), result["width"], result["height"], result["at"].as_str().unwrap_or("?")),
        Command::Sheet { output, .. } => {
            let times: Vec<&str> = result["times"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            outln!("{}  {}x{} grid, row by row:", output.display(), result["columns"], result["rows"]);
            for (i, t) in times.iter().enumerate() {
                outln!("  {:2}  {t}", i + 1);
            }
        }
        Command::Preview { output, .. } | Command::Export { output: Some(output), list_presets: false, .. } => {
            outln!(
                "{}  {}  {} frames  {}x{}  {}",
                output.display(),
                result["duration"].as_str().unwrap_or("?"),
                result["frames"],
                result["width"],
                result["height"],
                result["encoder"].as_str().unwrap_or("?")
            );
            for w in result["warnings"].as_array().into_iter().flatten() {
                outln!("warning: {}", w.as_str().unwrap_or("?"));
            }
        }
        Command::Export { list_presets: true, .. } => {
            for p in result["presets"].as_array().into_iter().flatten() {
                outln!("{:16} {}", p["name"].as_str().unwrap_or("?"), p["description"].as_str().unwrap_or(""));
            }
        }
        Command::Analyze(_) => {
            for r in result["ranges"].as_array().into_iter().flatten() {
                outln!("{}  {}-{}  {}", r["kind"].as_str().unwrap_or("?"), r["start"].as_str().unwrap_or("?"), r["end"].as_str().unwrap_or("?"), r["note"].as_str().unwrap_or(""));
            }
            outln!("{} found", result["ranges"].as_array().map_or(0, Vec::len));
            print_commit(&result);
        }
        Command::Measure(cmd) => print_measure(cmd, &result),
        Command::Normalize { .. } => {
            let f = |v: &Value| v.as_f64().map_or("?".into(), |x| format!("{x:.1}"));
            outln!(
                "measured {} LUFS (true peak {} dBTP), gain {:+.1} dB -> {} LUFS (true peak {} dBTP)",
                f(&result["measured_lufs"]),
                f(&result["measured_true_peak_dbtp"]),
                result["gain_db"].as_f64().unwrap_or(0.0),
                f(&result["expected_lufs"]),
                f(&result["expected_true_peak_dbtp"])
            );
            for w in result["warnings"].as_array().into_iter().flatten() {
                outln!("warning: {}", w.as_str().unwrap_or("?"));
            }
            print_commit(&result);
        }
        Command::Fx(FxCmd::List) => {
            for e in result["effects"].as_array().into_iter().flatten() {
                let params: Vec<&str> = e["params"]["properties"].as_object().map(|o| o.keys().map(String::as_str).collect()).unwrap_or_default();
                outln!(
                    "{:<10} {:<32} {:<14} {}",
                    e["kind"].as_str().unwrap_or("?"),
                    e["reference"].as_str().unwrap_or("?"),
                    e["name"].as_str().unwrap_or("?"),
                    if params.is_empty() { "(no params)".to_owned() } else { params.join(", ") }
                );
            }
            for p in result["problems"].as_array().into_iter().flatten() {
                outln!("problem: {}", p.as_str().unwrap_or("?"));
            }
        }
        Command::Fx(FxCmd::Define { .. }) => {
            let e = &result["effect"];
            match result["path"].as_str() {
                Some(path) => outln!("defined {} ({}), saved to {path}", e["reference"].as_str().unwrap_or("?"), e["kind"].as_str().unwrap_or("?")),
                None => outln!("{} is valid (dry run; nothing saved)", e["reference"].as_str().unwrap_or("?")),
            }
            outln!("test render passed on {}", result["tested_on"].as_str().unwrap_or("?"));
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
        | Command::Marker(_)
        | Command::Track(_)
        | Command::Effect(_)
        | Command::Transition(_)
        | Command::Undo
        | Command::Redo => print_commit(&result),
        Command::Jobs { cmd: None } => {
            let jobs = result["jobs"].as_array().cloned().unwrap_or_default();
            if jobs.is_empty() {
                outln!("no jobs");
            }
            for j in &jobs {
                outln!("{}", job_line(j));
            }
        }
        Command::Jobs { cmd: Some(JobsCmd::Cancel { .. }) } => {
            let state = result["state"].as_str().unwrap_or("?");
            if ["done", "failed", "cancelled"].contains(&state) {
                outln!("{} already {state}", result["job_id"].as_str().unwrap_or("?"));
            } else {
                outln!("cancelling {}", result["job_id"].as_str().unwrap_or("?"));
            }
        }
        _ => outln!("{}", serde_json::to_string_pretty(&result)?),
    }
    Ok(())
}
