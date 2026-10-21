//! `ve`: the CLI. Each subcommand maps to one protocol method.
//!
//! For now calls are dispatched in-process against the same registry the daemon
//! serves: the CLI opens the project (taking its lock), runs the call and
//! exits. Once ve-daemon has its socket, this becomes connect-or-spawn.
//!
//! Mutating commands fill in `base_rev` (the revision the project had when
//! this call opened it, unless `--base-rev` says otherwise), `dry_run` and
//! `actor` for you. Files are written here, never by the protocol: render
//! methods return bytes or a file in the engine's cache, and the CLI puts the
//! result at `-o`.

use std::path::{Path, PathBuf};

use base64::Engine;
use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};
use ve_protocol::{ErrorCode, Registry};

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
    /// Timeline markers.
    #[command(subcommand)]
    Marker(MarkerCmd),
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

    /// Send a raw JSON-RPC method call, e.g. `ve call project.get '{}'`.
    Call { method: String, params: Option<String> },
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

async fn call(registry: &Registry, method: &str, params: Value) -> Result<Value, CallError> {
    let req = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
    let resp: Value = serde_json::from_str(&registry.dispatch(&req).await)
        .unwrap_or_else(|e| json!({ "error": { "code": -32603, "message": e.to_string() } }));
    match resp.get("error") {
        Some(err) => Err(CallError(err.clone())),
        None => Ok(resp["result"].clone()),
    }
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
        Command::Marker(MarkerCmd::Add { at, duration, name, color, comment }) => {
            m("marker.add", json!({ "at": at, "duration": duration, "name": name, "color": color, "comment": comment }))
        }
        Command::Marker(MarkerCmd::Rm { ids, name }) => m("marker.remove", json!({ "ids": ids, "name": name })),
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
        Command::Call { .. } => unreachable!("raw calls are planned in main"),
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

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let registry = ve_daemon::build_registry();
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

    // Open the project first for methods that work on one.
    let standalone = method.starts_with("system.") || ["project.init", "project.open", "media.probe", "render.presets"].contains(&method.as_str());
    let mut opened_rev = None;
    if !standalone {
        let cwd = std::env::current_dir()?;
        match cli.project.clone().or_else(|| ve_daemon::project::find_root(&cwd)) {
            Some(path) => match call(&registry, "project.open", json!({ "path": absolute(&path) })).await {
                Ok(info) => opened_rev = info["revision"].as_u64(),
                Err(CallError(e)) => fail(&e, cli.json),
            },
            None if !matches!(cli.command, Command::Call { .. }) => usage_error(
                format!("no project.otio in {} or any parent folder; pass --project or run `ve init`", cwd.display()),
                cli.json,
            ),
            None => {}
        }
    }

    if mutates {
        params["base_rev"] = json!(cli.base_rev.or(opened_rev).unwrap_or(0));
        params["dry_run"] = json!(cli.dry_run);
    }
    if mutates || method.ends_with("_group") {
        params["actor"] = actor;
    }

    let result = match call(&registry, &method, params).await {
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
        Command::Insert { .. }
        | Command::Split { .. }
        | Command::Trim { .. }
        | Command::Move { .. }
        | Command::RippleDelete { .. }
        | Command::Delete { .. }
        | Command::Speed { .. }
        | Command::Volume { .. }
        | Command::Marker(_)
        | Command::Undo
        | Command::Redo => print_commit(&result),
        _ => println!("{}", serde_json::to_string_pretty(&result)?),
    }
    Ok(())
}
