//! Session server.
//!
//! One daemon per project folder (file lock). The CLI connects over a local
//! socket / Windows named pipe and spawns the daemon if none is running; the
//! daemon exits after an idle timeout. It owns the in-memory project, decoders,
//! frame cache, jobs and the `project.changed` event stream. Caches and proxies
//! live in the OS cache dir, not the (possibly OneDrive-synced) project folder.
//!
//! This crate also wires every method into the shared [`ve_protocol::Registry`].

pub mod analyze;
pub mod cache;
pub mod client;
pub mod commit;
pub mod edit;
pub mod events;
pub mod frames;
pub mod fonts;
pub mod fx;
pub mod jobs;
pub mod layers;
pub mod measure;
pub mod media;
pub mod plugins;
pub mod project;
pub mod render;
pub mod server;
pub mod text;
pub mod transcript;
#[cfg(feature = "ui-api")]
pub mod ui;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ve_model::Rate;
use ve_model::meta::OutputSettings;
use ve_protocol::{ErrorCode, NoParams, PROTOCOL_VERSION, Registry, RpcError, VersionInfo};

use crate::project::{OpenProject, ProjectError};

pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// State shared by every method: the one open project, if any, the event
/// stream its changes go out on, and background jobs.
pub struct Session {
    project: Mutex<Option<OpenProject>>,
    pub events: events::Events,
    pub jobs: jobs::Jobs,
    /// Rendered frames by content key (`render.frame`).
    pub frames: frames::FrameCache,
    /// Background render-ahead for an editor's preview (`ui.cache_hint`).
    #[cfg(feature = "ui-api")]
    pub(crate) prefetch: ui::Prefetch,
    /// Set when this session is served by a daemon (`server::run`).
    daemon: OnceLock<DaemonInfo>,
    /// Signalled by `daemon.stop`.
    pub(crate) shutdown: tokio::sync::Notify,
    /// Plugin processes for the open project.
    pub plugins: plugins::PluginSlot,
}

impl Default for Session {
    fn default() -> Self {
        let events = events::Events::default();
        Self {
            project: Mutex::default(),
            jobs: jobs::Jobs::new(events.clone()),
            frames: frames::FrameCache::default(),
            #[cfg(feature = "ui-api")]
            prefetch: ui::Prefetch::default(),
            events,
            daemon: OnceLock::new(),
            shutdown: tokio::sync::Notify::new(),
            plugins: plugins::PluginSlot::default(),
        }
    }
}

/// A copy of the open project, taken under the lock, for slow work (renders,
/// analysis, measurements) that then runs without holding it. Work that ends
/// in an edit commits through the normal path, whose `base_rev` check fails
/// if someone else edited in the meantime.
pub struct Snapshot {
    pub timeline: ve_model::Timeline,
    pub revision: u64,
    root: PathBuf,
    cache: PathBuf,
}

impl Snapshot {
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    /// The project's folder in the OS cache dir.
    pub fn cache_dir(&self) -> &std::path::Path {
        &self.cache
    }
}

impl Session {
    /// Copy the open project for work that runs without the lock.
    pub fn snapshot(&self) -> Result<Snapshot, RpcError> {
        self.with_project(|p| {
            Ok(Snapshot {
                timeline: p.timeline.clone(),
                revision: p.revision(),
                root: p.root().to_owned(),
                cache: p.cache_dir().to_owned(),
            })
        })
    }

    fn lock(&self) -> MutexGuard<'_, Option<OpenProject>> {
        // A panic mid-call must not wedge the session; the data is still usable.
        self.project.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn with_project<R>(&self, f: impl FnOnce(&mut OpenProject) -> Result<R, RpcError>) -> Result<R, RpcError> {
        let mut guard = self.lock();
        let p = guard
            .as_mut()
            .ok_or_else(|| RpcError::invalid_params("no project is open; call project.open or project.init first"))?;
        if p.reload_if_changed().map_err(rpc_error)? {
            tracing::info!("{} changed on disk; reloaded at rev {}", p.root().display(), p.revision());
            self.events.publish_reloaded(p.revision());
        }
        let result = f(p);
        // Published under the lock, so events go out in revision order.
        for change in p.changes.drain(..) {
            self.events.publish(&change);
        }
        result
    }

    /// The open project's folder and cache folder, if one is open.
    pub fn paths(&self) -> Option<(PathBuf, PathBuf)> {
        self.lock().as_ref().map(|p| (p.root().to_owned(), p.cache_dir().to_owned()))
    }

    /// Revision of the open project, if one is open.
    pub fn revision(&self) -> Option<u64> {
        self.lock().as_ref().map(OpenProject::revision)
    }
}

/// Build the method registry used by every transport, with a fresh session.
pub fn build_registry() -> Arc<Registry> {
    registry(Arc::new(Session::default()))
}

/// Build the method registry over a given session (the daemon keeps a handle
/// to it for events and shutdown).
pub fn registry(session: Arc<Session>) -> Arc<Registry> {
    let mut r = Registry::new();
    r.method("system.version", "Engine, protocol and project schema versions", false, |_: NoParams| {
        Ok(VersionInfo {
            engine: ENGINE_VERSION.into(),
            protocol: PROTOCOL_VERSION,
            schema: ve_model::SCHEMA_VERSION.to_string(),
        })
    });

    let s = session.clone();
    r.method("project.init", "Create a project folder and open it", false, move |p: InitParams| {
        let rate = Rate::parse(&p.fps)
            .ok_or_else(|| RpcError::invalid_params(format!("fps `{}`: use 30, 29.97 or 30000/1001", p.fps)))?;
        let name = match p.name {
            Some(n) => n,
            None => p.path.file_name().map_or("untitled".into(), |n| n.to_string_lossy().into_owned()),
        };
        let output = OutputSettings {
            width: p.width,
            height: p.height,
            rate,
            sample_rate: p.sample_rate,
            color_space: p.color_space,
        };
        let mut guard = s.lock();
        if let Some(open) = guard.as_ref() {
            return Err(RpcError::invalid_params(format!(
                "{} is already open in this session",
                open.root().display()
            )));
        }
        let project = project::init(&p.path, &name, output).map_err(rpc_error)?;
        let info = ProjectInfo::of(&project);
        *guard = Some(project);
        Ok(info)
    });

    let s = session.clone();
    r.method("project.open", "Open and lock a project folder", false, move |p: OpenParams| {
        let mut guard = s.lock();
        if let Some(open) = guard.as_ref() {
            let same = std::fs::canonicalize(&p.path).ok() == std::fs::canonicalize(open.root()).ok();
            return if same {
                Ok(ProjectInfo::of(open))
            } else {
                Err(RpcError::invalid_params(format!("{} is already open in this session", open.root().display())))
            };
        }
        let project = project::open(&p.path).map_err(rpc_error)?;
        let info = ProjectInfo::of(&project);
        *guard = Some(project);
        Ok(info)
    });

    let s = session.clone();
    r.method("project.save", "Write the open project to disk", false, move |_: NoParams| {
        s.with_project(|p| {
            p.save().map_err(rpc_error)?;
            Ok(ProjectInfo::of(p))
        })
    });

    let s = session.clone();
    r.method("project.get", "The full project as OTIO JSON", false, move |_: NoParams| {
        s.with_project(|p| {
            let timeline = serde_json::to_value(&p.timeline)
                .map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string()))?;
            Ok(ProjectDoc { revision: p.revision(), timeline })
        })
    });

    let s = session.clone();
    r.method(
        "project.summary",
        "Compact text view of tracks, clips and markers with IDs, for agents",
        false,
        move |_: NoParams| s.with_project(|p| Ok(Summary { revision: p.revision(), text: ve_model::summary::summary(&p.timeline) })),
    );

    r.method("media.probe", "Probe a media file with ffprobe; no project needed", false, media::probe);

    // Methods that run against the open project. `$f(project, params)`.
    macro_rules! on_project {
        ($name:literal, $summary:literal, $mutates:expr, $f:expr) => {{
            let s = session.clone();
            r.method($name, $summary, $mutates, move |p| s.with_project(|proj| $f(proj, p)));
        }};
    }

    on_project!("media.import", "Probe, hash and add files to the asset table; same content is never added twice", true, media::import);
    on_project!("media.remove", "Remove assets from the asset table; refused while any clip uses them", true, media::remove);
    on_project!("media.relink", "Point assets at files that moved: one asset to a path, or search a folder for every missing asset", true, media::relink);
    on_project!("media.assets", "The project's asset table", false, |p: &mut OpenProject, _: NoParams| Ok(media::assets(p)));
    {
        let s = session.clone();
        r.method("media.extract_audio", "Decode an asset's audio to a WAV or FLAC file in the engine cache (16 kHz mono by default), for plugins", false, move |p: media::ExtractAudioParams| {
            media::extract_audio(&s.snapshot()?, p)
        });
    }
    on_project!("transcript.set", "Store an asset's transcript (words with media times, speakers); replaces any earlier one", false, transcript::set);
    on_project!("transcript.get", "An asset's transcript, or the words that play on the timeline in timeline time (words or readable text)", false, transcript::get);
    on_project!("font.add", "Add a font to the project's fonts/ folder from a file, a URL or Google Fonts", false, fonts::add);
    on_project!("font.list", "Fonts in the project (and with system: true, installed ones)", false, |p: &mut OpenProject, a: fonts::ListParams| Ok(fonts::list(p, a)));
    on_project!("font.remove", "Delete a font file from the project's fonts/ folder", false, fonts::remove);
    on_project!("transcript.list", "Assets that have a transcript", false, |p: &mut OpenProject, _: NoParams| transcript::list(p));
    on_project!("transcript.remove", "Delete an asset's transcript", false, transcript::remove);

    // TEMPORARY: UI-only helpers; they belong in a UI plugin (see ui.rs).
    #[cfg(feature = "ui-api")]
    on_project!("ui.thumbnail", "A poster frame (JPEG, base64) of a video asset, for media panels", false, ui::thumbnail);
    #[cfg(feature = "ui-api")]
    {
        let s = session.clone();
        r.method("ui.cache_hint", "Preview render-ahead: cache frames around the playhead (changed frames first) in the background", false, move |p: ui::CacheHintParams| ui::cache_hint(&s, p));
        let s = session.clone();
        r.method("ui.cache_status", "Which timeline frames are cached at the hinted preview size", false, move |p: ui::CacheStatusParams| ui::cache_status(&s, p));
    }
    #[cfg(feature = "ui-api")]
    on_project!("ui.move_push", "Timeline drag: move a clip and push what is in the way just far enough", true, ui::move_push);

    on_project!("edit.insert", "Put an asset on the timeline (video + linked audio); insert or overwrite", true, edit::insert);
    on_project!("edit.split", "Cut clips in two at a time (all clips under it, or the given ones)", true, edit::split);
    on_project!("edit.trim", "Move a clip's in/out point; ripple (default) or roll (J/L cuts with linked=false)", true, edit::trim);
    on_project!("edit.move", "Move a clip to a new time or track, overwriting what is there", true, edit::move_clip);
    on_project!("edit.ripple_delete", "Remove time ranges, marker ranges or clips and close the gap", true, edit::ripple_delete);
    on_project!("edit.delete", "Remove clips and leave gaps", true, edit::delete);
    on_project!("edit.set_speed", "Change a clip's speed (exact ratio); writes OTIO LinearTimeWarp too", true, edit::set_speed);
    on_project!("edit.set_volume", "Set a clip's audio gain in dB", true, edit::set_volume);
    r.method("text.presets", "Built-in title looks (style, place, animation) for text.add and text.captions", false, |_: NoParams| Ok(text::presets()));
    on_project!("text.add", "Put a text clip (title, label) on a video track; never pushes other clips", true, text::add);
    on_project!("text.set", "Change a text clip's words or look", true, text::set);
    on_project!("text.captions", "Caption clips from the transcript, timed to the edited timeline, on their own track", true, text::captions);
    on_project!("edit.set_transform", "Position, scale, rotate, fade (opacity) or crop a video clip's picture", true, edit::set_transform);
    on_project!("edit.unlink", "Break a clip's link with its partners so each is edited alone", true, edit::unlink);
    on_project!("track.add", "Add a video or audio track (any number of each)", true, edit::track_add);
    on_project!("track.remove", "Remove a track (with its clips only when forced)", true, edit::track_remove);
    on_project!("track.rename", "Rename a track", true, edit::track_rename);
    on_project!("track.move", "Change a track's position among tracks of its kind (video: stacking order)", true, edit::track_move);
    on_project!("track.set", "Hide or show a video track, mute or unmute an audio track", true, edit::track_set);
    on_project!("track.list", "Every track with its kind, position and clip count", false, |p: &mut OpenProject, _: NoParams| Ok(edit::track_list(p)));
    on_project!("fx.list", "Effects and transitions this project can use, with their param schemas", false, |p: &mut OpenProject, _: NoParams| Ok(fx::list(p)));
    on_project!("fx.define", "Validate a WGSL effect or transition (naga, then a GPU test render) and save it to the project", false, fx::define);
    on_project!("effect.add", "Put an effect on a video clip", true, fx::effect_add);
    on_project!("effect.set", "Change an effect's params, keyframes or place in the clip's stack", true, fx::effect_set);
    on_project!("effect.remove", "Remove an effect from its clip", true, fx::effect_remove);
    on_project!("transition.add", "Put a transition on a cut between two touching video clips", true, fx::transition_add);
    on_project!("transition.set", "Change a transition's length, alignment, kind or params", true, fx::transition_set);
    on_project!("transition.remove", "Remove a transition (back to a plain cut)", true, fx::transition_remove);
    on_project!("marker.add", "Add a timeline marker", true, edit::marker_add);
    on_project!("marker.remove", "Remove timeline markers by ID or name", true, edit::marker_remove);
    on_project!("history.undo", "Undo the most recent step (one edit or one group)", true, |p: &mut OpenProject, a| edit::undo(p, a, false));
    on_project!("history.redo", "Redo the most recently undone step", true, |p: &mut OpenProject, a| edit::undo(p, a, true));
    on_project!("history.list", "Undo and redo steps with their actors", false, |p: &mut OpenProject, a| Ok(edit::list(p, a)));
    on_project!("history.begin_group", "Start an undo group: the actor's next edits undo as one step", false, edit::begin_group);
    on_project!("history.end_group", "End the actor's undo group", false, edit::end_group);

    // Slow work runs on a snapshot, so it never holds the project lock: other
    // calls (scrubbing, edits) go on while an export runs. `long!` methods
    // also take `background: true` and then answer `{job_id}` at once.
    macro_rules! long {
        ($name:literal, $summary:literal, $mutates:expr, $p:ty, $f:expr) => {{
            let s = session.clone();
            let f = $f;
            r.method($name, $summary, $mutates, move |p: $p| {
                let snap = s.snapshot()?;
                if !jobs::Background::background(&p) {
                    return f(&s, &snap, p).map(jobs::MaybeJob::Done);
                }
                let expected = jobs::Background::expected_secs(&p, &snap);
                let s2 = s.clone();
                Ok(jobs::MaybeJob::Started(s.jobs.start($name, expected, move || {
                    let r = f(&s2, &snap, p)?;
                    serde_json::to_value(r).map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string()))
                })))
            });
        }};
    }

    let s = session.clone();
    r.method("render.frame", "One frame of the timeline as JPEG/PNG (base64)", false, move |p: render::FrameParams| {
        s.frames.touch_interactive();
        render::frame(&s, &s.snapshot()?, p)
    });
    long!("render.contact_sheet", "A grid of evenly spaced frames from a range (base64 image)", false, render::SheetParams,
        |_: &Session, snap: &Snapshot, p| render::contact_sheet(snap, p));
    long!("render.range_preview", "A low-res MP4 of a range, written to the engine cache", false, render::PreviewParams,
        |s: &Session, snap: &Snapshot, p| render::range_preview(s, snap, p));
    long!("render.export", "Render the timeline (or a range) with a preset and format (MP4, HEVC, ProRes, WebM, audio) to the engine cache", false, render::ExportParams,
        |_: &Session, snap: &Snapshot, p| render::export(snap, p));
    r.method("render.presets", "Export presets and formats", false, |_: NoParams| Ok(render::presets()));
    let s = session.clone();
    r.method(
        "render.layers",
        "What is on screen and heard: video layers at a frame (placement, effects, transitions, what shows) and audio clips with their gain, or which tracks show and are heard over a range",
        false,
        move |p: layers::LayersParams| layers::layers(&s.snapshot()?, p),
    );

    long!("analyze.silence", "Find quiet stretches in the timeline audio; writes `silence` markers", true, analyze::SilenceParams, analyze::silence);
    long!("measure.loudness", "EBU R128 loudness (LUFS, range, true peak) of an asset's file or the timeline mix", false, measure::MeasureParams,
        |_: &Session, snap: &Snapshot, p| measure::loudness(snap, p));
    long!("measure.levels", "RMS, peak, clipping, DC offset, noise floor, stereo balance and phase", false, measure::MeasureParams,
        |_: &Session, snap: &Snapshot, p| measure::levels(snap, p));
    long!("measure.waveform", "Waveform peaks (min, max, RMS per point) for drawing", false, measure::WaveformParams,
        |_: &Session, snap: &Snapshot, p| measure::waveform(snap, p));
    long!("measure.pitch", "Pitch (fundamental frequency) of voices and instruments: median, range, contour", false, measure::MeasureParams,
        |_: &Session, snap: &Snapshot, p| measure::pitch(snap, p));
    long!("analyze.scenes", "Find hard cuts in the timeline picture; writes `scene` markers", true, analyze::ScenesParams, analyze::scenes);
    long!("edit.normalize", "Measure the timeline's loudness and add one gain to its audio clips to hit a LUFS target", true, measure::NormalizeParams, measure::normalize);

    let s = session.clone();
    r.method("jobs.list", "Background jobs, oldest first, without their results", false, move |_: NoParams| {
        Ok(JobList { jobs: s.jobs.list() })
    });
    let s = session.clone();
    r.method("jobs.get", "One background job: state, progress, and its result or error once finished", false, move |p: JobParams| {
        s.jobs.get(&p.job_id)
    });
    let s = session.clone();
    r.method("jobs.cancel", "Stop a background job (queued: at once; running: its ffmpeg run is killed)", false, move |p: JobParams| {
        s.jobs.cancel(&p.job_id)
    });

    let s = session.clone();
    r.method("cache.status", "How much the project's rebuildable caches (frames, previews, renders, measurements) hold", false, move |_: NoParams| {
        cache::status(&s)
    });
    let s = session.clone();
    r.method("cache.clear", "Delete the project's rebuildable caches (all, or some kinds); refused while a job runs", false, move |p: cache::ClearParams| {
        cache::clear(&s, p)
    });

    let s = session.clone();
    r.method(
        "events.subscribe",
        "Get `project.changed` notifications on this connection (daemon transports only).          Each carries {revision, actor, command, patch}; `events.lagged` means refetch with project.get.          Background jobs send `job.progress` {job_id, state, progress} and `job.finished` {job_id, state, result | error}",
        false,
        move |_: NoParams| Ok(Subscribed { revision: s.revision() }),
    );
    let s = session.clone();
    r.method("daemon.info", "The running daemon: pid, binary, project, socket and WebSocket URL", false, move |_: NoParams| {
        s.daemon.get().cloned().ok_or_else(|| RpcError::invalid_params("not running as a daemon"))
    });
    let s = session.clone();
    r.method("daemon.stop", "Stop the daemon after answering (the project is already saved)", false, move |_: NoParams| {
        if s.daemon.get().is_none() {
            return Err(RpcError::invalid_params("not running as a daemon"));
        }
        s.shutdown.notify_one();
        Ok(Stopping { stopping: true })
    });

    let plugin_methods = plugins::PluginMethods::new(&session);
    {
        let pm = plugin_methods.clone();
        r.method("plugin.list", "Installed plugins: their commands, CLI names, secrets and load problems (rescans the plugin folders)", false, move |_: NoParams| pm.list());
    }

    let registry = r.seal("Headless Video Engine", ENGINE_VERSION);
    plugin_methods.attach(&registry);
    registry
}

pub(crate) fn rpc_error(e: ProjectError) -> RpcError {
    let code = match e {
        ProjectError::Locked(_) => ErrorCode::ProjectLocked,
        ProjectError::Io { .. } => ErrorCode::Internal,
        _ => ErrorCode::InvalidParams,
    };
    RpcError::new(code, e.to_string())
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InitParams {
    /// Folder to create the project in. It may exist but must not hold a project.
    pub path: PathBuf,
    /// Defaults to the folder name.
    #[serde(default)]
    pub name: Option<String>,
    /// Frame rate: `30`, `29.97`, `23.976` or an exact `30000/1001`.
    pub fps: String,
    pub width: u32,
    pub height: u32,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    #[serde(default = "default_color_space")]
    pub color_space: String,
}

fn default_sample_rate() -> u32 {
    48_000
}

fn default_color_space() -> String {
    "bt709".into()
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenParams {
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProjectInfo {
    pub path: PathBuf,
    pub name: String,
    pub revision: u64,
    /// Stable IDs given on open to items that had none (files from other tools).
    pub ids_added: usize,
    /// Schema version the file had when this engine upgraded it on open
    /// (the next save writes the current one, which older engines refuse).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upgraded_from: Option<String>,
    /// Missing media found at its place relative to the project folder (the
    /// folder moved with its media) and relinked on open.
    #[serde(default)]
    pub relinked: usize,
}

impl ProjectInfo {
    fn of(p: &OpenProject) -> Self {
        Self {
            path: p.root().to_owned(),
            name: p.timeline.name.clone(),
            revision: p.revision(),
            ids_added: p.ids_added,
            upgraded_from: p.upgraded_from.as_ref().map(ToString::to_string),
            relinked: p.relinked,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProjectDoc {
    pub revision: u64,
    /// The `Timeline.1` OTIO document.
    pub timeline: serde_json::Value,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JobParams {
    /// The `job_id` a `background: true` call answered with.
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct JobList {
    pub jobs: Vec<jobs::JobInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Subscribed {
    /// Current revision (none if no project is open). Events newer than this follow.
    pub revision: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Stopping {
    pub stopping: bool,
}

/// Result of `daemon.info`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DaemonInfo {
    pub pid: u32,
    pub engine: String,
    /// The daemon's binary and its modification time; the CLI restarts a
    /// daemon whose binary differs from its own.
    pub exe: ExeIdentity,
    pub project: PathBuf,
    /// Local socket (named pipe on Windows) name.
    pub socket: String,
    /// `ws://127.0.0.1:<port>/rpc?token=...`, loopback only.
    pub websocket: String,
    /// Seconds without connections before the daemon exits.
    pub idle_timeout_secs: u64,
}

/// Which binary a process runs, to spot a daemon left over from an older build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExeIdentity {
    pub path: PathBuf,
    /// Modification time in nanoseconds since the Unix epoch, as text.
    pub modified: String,
}

impl ExeIdentity {
    pub fn current() -> Self {
        Self::of(&std::env::current_exe().unwrap_or_default())
    }

    pub fn of(path: &std::path::Path) -> Self {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
        let modified = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or_else(String::new, |d| d.as_nanos().to_string());
        Self { path, modified }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Summary {
    pub revision: u64,
    pub text: String,
}
