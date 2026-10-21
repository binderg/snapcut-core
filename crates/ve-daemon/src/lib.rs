//! Session server.
//!
//! One daemon per project folder (file lock). The CLI connects over a local
//! socket / Windows named pipe and spawns the daemon if none is running; the
//! daemon exits after an idle timeout. It owns the in-memory project, decoders,
//! frame cache, jobs and the `project.changed` event stream. Caches and proxies
//! live in the OS cache dir, not the (possibly OneDrive-synced) project folder.
//!
//! This crate also wires every method into the shared [`ve_protocol::Registry`].

pub mod media;
pub mod project;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ve_model::Rate;
use ve_model::meta::OutputSettings;
use ve_protocol::{ErrorCode, NoParams, PROTOCOL_VERSION, Registry, RpcError, VersionInfo};

use crate::project::{OpenProject, ProjectError};

pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// State shared by every method: the one open project, if any.
#[derive(Default)]
pub struct Session {
    project: Mutex<Option<OpenProject>>,
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
        f(p)
    }
}

/// Build the method registry used by every transport.
pub fn build_registry() -> Arc<Registry> {
    let session = Arc::new(Session::default());
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

    let s = session.clone();
    r.method(
        "media.import",
        "Probe, hash and add files to the asset table; same content is never added twice",
        true,
        move |p: media::ImportParams| s.with_project(|proj| media::import(proj, p)),
    );

    let s = session;
    r.method("media.assets", "The project's asset table", false, move |_: NoParams| {
        s.with_project(|p| Ok(media::assets(p)))
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
pub struct Summary {
    pub revision: u64,
    pub text: String,
}
