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
pub mod client;
pub mod commit;
pub mod edit;
pub mod events;
pub mod media;
pub mod project;
pub mod render;
pub mod server;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ve_model::Rate;
use ve_model::meta::OutputSettings;
use ve_protocol::{ErrorCode, NoParams, PROTOCOL_VERSION, Registry, RpcError, VersionInfo};

use crate::project::{OpenProject, ProjectError};

pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// State shared by every method: the one open project, if any, and the
/// event stream its changes go out on.
#[derive(Default)]
pub struct Session {
    project: Mutex<Option<OpenProject>>,
    pub events: events::Events,
    /// Set when this session is served by a daemon (`server::run`).
    daemon: OnceLock<DaemonInfo>,
    /// Signalled by `daemon.stop`.
    pub(crate) shutdown: tokio::sync::Notify,
}

impl Session {
    fn lock(&self) -> MutexGuard<'_, Option<OpenProject>> {
        // A panic mid-call must not wedge the session; the data is still usable.
        self.project.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn with_project<R>(&self, f: impl FnOnce(&mut OpenProject) -> Result<R, RpcError>) -> Result<R, RpcError> {
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
    on_project!("media.assets", "The project's asset table", false, |p: &mut OpenProject, _: NoParams| Ok(media::assets(p)));

    on_project!("edit.insert", "Put an asset on the timeline (video + linked audio); insert or overwrite", true, edit::insert);
    on_project!("edit.split", "Cut clips in two at a time (all clips under it, or the given ones)", true, edit::split);
    on_project!("edit.trim", "Move a clip's in/out point; ripple (default) or roll (J/L cuts with linked=false)", true, edit::trim);
    on_project!("edit.move", "Move a clip to a new time or track, overwriting what is there", true, edit::move_clip);
    on_project!("edit.ripple_delete", "Remove time ranges, marker ranges or clips and close the gap", true, edit::ripple_delete);
    on_project!("edit.delete", "Remove clips and leave gaps", true, edit::delete);
    on_project!("edit.set_speed", "Change a clip's speed (exact ratio); writes OTIO LinearTimeWarp too", true, edit::set_speed);
    on_project!("edit.set_volume", "Set a clip's audio gain in dB", true, edit::set_volume);
    on_project!("marker.add", "Add a timeline marker", true, edit::marker_add);
    on_project!("marker.remove", "Remove timeline markers by ID or name", true, edit::marker_remove);
    on_project!("history.undo", "Undo the most recent step (one edit or one group)", true, |p: &mut OpenProject, a| edit::undo(p, a, false));
    on_project!("history.redo", "Redo the most recently undone step", true, |p: &mut OpenProject, a| edit::undo(p, a, true));
    on_project!("history.list", "Undo and redo steps with their actors", false, |p: &mut OpenProject, a| Ok(edit::list(p, a)));
    on_project!("history.begin_group", "Start an undo group: the actor's next edits undo as one step", false, edit::begin_group);
    on_project!("history.end_group", "End the actor's undo group", false, edit::end_group);

    on_project!("render.frame", "One frame of the timeline as JPEG/PNG (base64)", false, render::frame);
    on_project!("render.contact_sheet", "A grid of evenly spaced frames from a range (base64 image)", false, render::contact_sheet);
    on_project!("render.range_preview", "A low-res MP4 of a range, written to the engine cache", false, render::range_preview);
    on_project!("render.export", "Render the timeline (or a range) with a preset to an MP4 in the engine cache", false, render::export);
    r.method("render.presets", "Export presets", false, |_: NoParams| Ok(render::presets()));

    on_project!("analyze.silence", "Find quiet stretches in the timeline audio; writes `silence` markers", true, analyze::silence);
    on_project!("analyze.scenes", "Find hard cuts in the timeline picture; writes `scene` markers", true, analyze::scenes);

    let s = session.clone();
    r.method(
        "events.subscribe",
        "Get `project.changed` notifications on this connection (daemon transports only).          Each carries {revision, actor, command, patch}; `events.lagged` means refetch with project.get",
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

    r.seal("Headless Video Engine", ENGINE_VERSION)
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
}

impl ProjectInfo {
    fn of(p: &OpenProject) -> Self {
        Self {
            path: p.root().to_owned(),
            name: p.timeline.name.clone(),
            revision: p.revision(),
            ids_added: p.ids_added,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProjectDoc {
    pub revision: u64,
    /// The `Timeline.1` OTIO document.
    pub timeline: serde_json::Value,
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
