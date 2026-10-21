//! Project folders on disk: `project.otio` + `effects/` + `plugins/`.
//!
//! Opening takes an exclusive lock so two engine processes never write the
//! same project. The lock file lives in the OS cache dir, not the project
//! folder, because a held-open file inside OneDrive/Dropbox causes sync
//! errors. It is named by a hash of the canonical project path and holds that
//! path (and later the daemon's socket address) for debugging.
//!
//! Saves write a temp file next to `project.otio` and rename it over the old
//! one, so a crash mid-save never leaves a half-written project.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use etcetera::BaseStrategy;
use fs4::FileExt;
use ve_model::Timeline;
use ve_model::meta::OutputSettings;

pub const PROJECT_FILE: &str = "project.otio";
const SUBDIRS: [&str; 2] = ["effects", "plugins"];

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("no project in {} (no {PROJECT_FILE}); create one with `ve init`", .0.display())]
    NotFound(PathBuf),
    #[error("{} already has a {PROJECT_FILE}", .0.display())]
    AlreadyExists(PathBuf),
    #[error("project {} is open in another ve process", .0.display())]
    Locked(PathBuf),
    #[error("{} is not a ve project: {message}", .path.display())]
    Invalid { path: PathBuf, message: String },
    #[error("{} was saved by a newer engine (schema {found}; this engine reads up to {supported})", .path.display())]
    TooNew { path: PathBuf, found: semver::Version, supported: semver::Version },
    #[error("{}: {source}", .path.display())]
    Io { path: PathBuf, source: io::Error },
}

type Result<T> = std::result::Result<T, ProjectError>;

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> ProjectError + '_ {
    move |source| ProjectError::Io { path: path.to_owned(), source }
}

/// A project held open (and locked) by this process. Dropping it releases the lock.
#[derive(Debug)]
pub struct OpenProject {
    root: PathBuf,
    pub timeline: Timeline,
    /// IDs added on open to items that had none (files from other tools).
    pub ids_added: usize,
    _lock: File,
}

impl OpenProject {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn revision(&self) -> u64 {
        self.timeline.project().map_or(0, |p| p.revision)
    }

    /// Write `project.otio` atomically.
    pub fn save(&self) -> Result<()> {
        let path = self.root.join(PROJECT_FILE);
        let tmp = self.root.join(format!("{PROJECT_FILE}.tmp"));
        let mut text = to_pretty_json(&self.timeline).map_err(|e| ProjectError::Invalid {
            path: path.clone(),
            message: e.to_string(),
        })?;
        text.push('\n');
        {
            let mut f = File::create(&tmp).map_err(io_err(&tmp))?;
            f.write_all(text.as_bytes()).map_err(io_err(&tmp))?;
            f.sync_all().map_err(io_err(&tmp))?;
        }
        rename_with_retry(&tmp, &path)
    }
}

/// Create a new project folder (the folder itself may already exist, empty or not).
pub fn init(root: &Path, name: &str, output: OutputSettings) -> Result<OpenProject> {
    fs::create_dir_all(root).map_err(io_err(root))?;
    let root = canonical(root)?;
    if root.join(PROJECT_FILE).exists() {
        return Err(ProjectError::AlreadyExists(root));
    }
    let lock = lock(&root)?;
    for dir in SUBDIRS {
        let d = root.join(dir);
        fs::create_dir_all(&d).map_err(io_err(&d))?;
    }
    let project =
        OpenProject { timeline: ve_model::doc::new_project(name, output), root, ids_added: 0, _lock: lock };
    project.save()?;
    Ok(project)
}

/// Open and lock an existing project folder. Items without stable IDs get
/// them now, and the file is saved right away so the IDs an agent reads stay
/// valid on the next call.
pub fn open(root: &Path) -> Result<OpenProject> {
    let root = canonical(root).map_err(|_| ProjectError::NotFound(root.to_owned()))?;
    let path = root.join(PROJECT_FILE);
    if !path.is_file() {
        return Err(ProjectError::NotFound(root));
    }
    let lock = lock(&root)?;
    let text = fs::read_to_string(&path).map_err(io_err(&path))?;
    let mut timeline: Timeline = serde_json::from_str(&text)
        .map_err(|e| ProjectError::Invalid { path: path.clone(), message: e.to_string() })?;
    let Some(meta) = timeline.project() else {
        return Err(ProjectError::Invalid {
            path,
            message: "no metadata.ve.project block (plain OTIO files need importing, not opening)".into(),
        });
    };
    let supported = ve_model::SCHEMA_VERSION;
    let found = &meta.schema_version;
    // 0.x: a newer minor may be breaking. 1.x+: a newer major is.
    let too_new = if supported.major == 0 {
        found.major > 0 || found.minor > supported.minor
    } else {
        found.major > supported.major
    };
    if too_new {
        return Err(ProjectError::TooNew { path, found: found.clone(), supported });
    }
    let ids_added = timeline.ensure_ids();
    let project = OpenProject { root, timeline, ids_added, _lock: lock };
    if ids_added > 0 {
        project.save()?;
    }
    Ok(project)
}

/// Walk up from `start` to the nearest folder holding a `project.otio`.
pub fn find_root(start: &Path) -> Option<PathBuf> {
    start.ancestors().find(|d| d.join(PROJECT_FILE).is_file()).map(Path::to_path_buf)
}

fn canonical(p: &Path) -> Result<PathBuf> {
    let c = fs::canonicalize(p).map_err(io_err(p))?;
    Ok(strip_verbatim(c))
}

/// Windows `canonicalize` returns `\\?\C:\...`; strip that so paths read normally.
fn strip_verbatim(p: PathBuf) -> PathBuf {
    match p.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => p,
    }
}

fn lock(root: &Path) -> Result<File> {
    let dir = lock_dir()?;
    fs::create_dir_all(&dir).map_err(io_err(&dir))?;
    // Case-fold on Windows, where C:\A and c:\a are the same folder.
    let key = if cfg!(windows) { root.to_string_lossy().to_lowercase() } else { root.to_string_lossy().into_owned() };
    let hash = blake3::hash(key.as_bytes()).to_hex();
    let path = dir.join(format!("{}.lock", &hash[..16]));
    let mut file = File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(io_err(&path))?;
    // fs4's, not std's: std `File::try_lock` needs Rust 1.89 and our MSRV is 1.88.
    match FileExt::try_lock(&file) {
        Ok(()) => {}
        Err(fs4::TryLockError::WouldBlock) => return Err(ProjectError::Locked(root.to_owned())),
        Err(fs4::TryLockError::Error(e)) => return Err(ProjectError::Io { path, source: e }),
    }
    file.set_len(0).map_err(io_err(&path))?;
    writeln!(file, "{}", root.display()).map_err(io_err(&path))?;
    Ok(file)
}

fn lock_dir() -> Result<PathBuf> {
    let base = etcetera::choose_base_strategy().map_err(|e| ProjectError::Io {
        path: PathBuf::from("~"),
        source: io::Error::other(e.to_string()),
    })?;
    Ok(base.cache_dir().join("ve").join("locks"))
}

/// OneDrive and antivirus scanners briefly hold files open on Windows, which
/// makes a rename fail with "access denied". A few short retries ride it out.
fn rename_with_retry(from: &Path, to: &Path) -> Result<()> {
    let mut last = None;
    for attempt in 0..5 {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(50 << attempt));
            }
            Err(e) => return Err(ProjectError::Io { path: to.to_owned(), source: e }),
        }
    }
    Err(ProjectError::Io { path: to.to_owned(), source: last.expect("loop ran") })
}

/// Two-space indented JSON, the same layout Python OTIO writes.
fn to_pretty_json(tl: &Timeline) -> serde_json::Result<String> {
    let mut buf = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    serde::Serialize::serialize(tl, &mut ser)?;
    Ok(String::from_utf8(buf).expect("serde_json writes UTF-8"))
}
