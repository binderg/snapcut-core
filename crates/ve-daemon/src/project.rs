//! Project folders on disk: `project.otio` + `effects/` + `plugins/`.
//!
//! Opening takes an exclusive lock so two engine processes never write the
//! same project. The lock file lives in the OS cache dir, not the project
//! folder, because a held-open file inside OneDrive/Dropbox causes sync
//! errors. It is named by a hash of the canonical project path and holds that
//! path for debugging. The same hash names the daemon's socket (see [`locate`]).
//!
//! Saves write a temp file next to `project.otio` and rename it over the old
//! one, so a crash mid-save never leaves a half-written project.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use etcetera::BaseStrategy;
use fs4::FileExt;
use ve_history::History;
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
    #[error("project {} is open in another ve process (a running daemon stops with `ve daemon stop`)", .0.display())]
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
    /// Undo history. Persisted in the OS cache dir (not the project folder)
    /// so undo works across CLI calls and daemon restarts.
    pub history: History,
    /// IDs added on open to items that had none (files from other tools).
    pub ids_added: usize,
    /// Per-project folder in the OS cache dir (renders, history).
    cache: PathBuf,
    /// Changes committed during the current call, waiting to go out as
    /// `project.changed` events (the session drains them after each call).
    pub(crate) changes: Vec<crate::events::Change>,
    /// `project.otio` as this process last read or wrote it, to spot edits
    /// made outside the engine while a daemon holds the project.
    disk: Option<Stamp>,
    _lock: File,
}

/// Modification time and size of `project.otio`.
type Stamp = (std::time::SystemTime, u64);

fn stamp(root: &Path) -> Option<Stamp> {
    let m = fs::metadata(root.join(PROJECT_FILE)).ok()?;
    Some((m.modified().ok()?, m.len()))
}

impl OpenProject {
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// This project's folder in the OS cache dir: `ve/projects/<hash>/`.
    pub fn cache_dir(&self) -> &Path {
        &self.cache
    }

    fn history_path(&self) -> PathBuf {
        self.cache.join("history.json")
    }

    /// Write the history next to the lock. Failure only costs undo across
    /// processes, so it is logged rather than failing the edit.
    pub fn save_history(&self) {
        let result = fs::create_dir_all(&self.cache)
            .and_then(|()| serde_json::to_vec(&self.history).map_err(io::Error::other))
            .and_then(|bytes| {
                let tmp = self.cache.join("history.json.tmp");
                fs::write(&tmp, bytes)?;
                rename_with_retry(&tmp, &self.history_path()).map_err(io::Error::other)
            });
        if let Err(e) = result {
            tracing::warn!("could not save undo history to {}: {e}", self.history_path().display());
        }
    }

    /// History saved for this project, if it ends at the project's revision.
    /// A file edited outside the engine makes the old history meaningless.
    fn load_history(&mut self) {
        let rev = self.revision();
        self.history = fs::read(self.history_path())
            .ok()
            .and_then(|b| serde_json::from_slice::<History>(&b).ok())
            .filter(|h| h.head_rev == rev)
            .unwrap_or_else(|| History::new(rev));
    }

    pub fn revision(&self) -> u64 {
        self.timeline.project().map_or(0, |p| p.revision)
    }

    /// Write `project.otio` atomically.
    pub fn save(&mut self) -> Result<()> {
        write_atomic(&self.root, &self.timeline)?;
        self.disk = stamp(&self.root);
        Ok(())
    }

    /// Replace the document with `next`, saving it first so a failed write
    /// leaves both the file and the in-memory project unchanged.
    pub fn commit(&mut self, next: Timeline) -> Result<()> {
        write_atomic(&self.root, &next)?;
        self.disk = stamp(&self.root);
        self.timeline = next;
        Ok(())
    }

    /// Re-read `project.otio` if something outside the engine changed it
    /// (a git checkout, a script, another editor). Undo history survives only
    /// if the file's revision still matches it. Returns whether it reloaded.
    pub fn reload_if_changed(&mut self) -> Result<bool> {
        let now = stamp(&self.root);
        if now.is_none() || now == self.disk {
            return Ok(false);
        }
        let (timeline, ids_added) = load(&self.root)?;
        self.timeline = timeline;
        self.ids_added = ids_added;
        self.disk = now;
        self.load_history();
        if ids_added > 0 {
            self.save()?;
        }
        Ok(true)
    }
}

fn write_atomic(root: &Path, timeline: &Timeline) -> Result<()> {
    let path = root.join(PROJECT_FILE);
    let tmp = root.join(format!("{PROJECT_FILE}.tmp"));
    let mut text = to_pretty_json(timeline)
        .map_err(|e| ProjectError::Invalid { path: path.clone(), message: e.to_string() })?;
    text.push('\n');
    {
        let mut f = File::create(&tmp).map_err(io_err(&tmp))?;
        f.write_all(text.as_bytes()).map_err(io_err(&tmp))?;
        f.sync_all().map_err(io_err(&tmp))?;
    }
    rename_with_retry(&tmp, &path)
}

/// Write a file inside the project folder so it is either the old or the new
/// content, never half written (OneDrive may sync it at any moment).
pub(crate) fn write_file_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(io_err(dir))?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut f = File::create(&tmp).map_err(io_err(&tmp))?;
        f.write_all(bytes).map_err(io_err(&tmp))?;
        f.sync_all().map_err(io_err(&tmp))?;
    }
    rename_with_retry(&tmp, path)
}

/// Create a new project folder (the folder itself may already exist, empty or not).
pub fn init(root: &Path, name: &str, output: OutputSettings) -> Result<OpenProject> {
    fs::create_dir_all(root).map_err(io_err(root))?;
    let root = canonical(root)?;
    if root.join(PROJECT_FILE).exists() {
        return Err(ProjectError::AlreadyExists(root));
    }
    let (lock, cache) = lock(&root)?;
    for dir in SUBDIRS {
        let d = root.join(dir);
        fs::create_dir_all(&d).map_err(io_err(&d))?;
    }
    let mut project = OpenProject {
        timeline: ve_model::doc::new_project(name, output),
        root,
        history: History::new(0),
        ids_added: 0,
        cache,
        changes: Vec::new(),
        disk: None,
        _lock: lock,
    };
    project.save()?;
    project.save_history();
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
    let (lock, cache) = lock(&root)?;
    let disk = stamp(&root);
    let (timeline, ids_added) = load(&root)?;
    let mut project =
        OpenProject { root, timeline, history: History::default(), ids_added, cache, changes: Vec::new(), disk, _lock: lock };
    project.load_history();
    if ids_added > 0 {
        project.save()?;
    }
    Ok(project)
}

/// Read and check `project.otio`, giving IDs to items that have none.
fn load(root: &Path) -> Result<(Timeline, usize)> {
    let path = root.join(PROJECT_FILE);
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
    Ok((timeline, ids_added))
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

/// Where a project's engine state lives, keyed by its canonical path.
#[derive(Debug, Clone)]
pub struct Located {
    /// Canonical project folder.
    pub root: PathBuf,
    /// 16 hex chars of blake3 over the root path (case-folded on Windows).
    pub key: String,
    /// `<OS cache dir>/ve/projects/<key>/`: renders, history, daemon log and socket.
    pub cache: PathBuf,
}

/// Canonicalize a project folder and work out its key and cache folder.
pub fn locate(root: &Path) -> Result<Located> {
    let root = canonical(root).map_err(|_| ProjectError::NotFound(root.to_owned()))?;
    // Case-fold on Windows, where C:\A and c:\a are the same folder.
    let folded = if cfg!(windows) { root.to_string_lossy().to_lowercase() } else { root.to_string_lossy().into_owned() };
    let key = blake3::hash(folded.as_bytes()).to_hex()[..16].to_owned();
    let cache = cache_base()?.join("projects").join(&key);
    Ok(Located { root, key, cache })
}

/// Take the project's lock. Returns the lock file and the project's cache folder.
fn lock(root: &Path) -> Result<(File, PathBuf)> {
    let Located { key, cache, .. } = locate(root)?;
    let dir = cache_base()?.join("locks");
    fs::create_dir_all(&dir).map_err(io_err(&dir))?;
    let path = dir.join(format!("{key}.lock"));
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
    Ok((file, cache))
}

/// `<OS cache dir>/ve`.
pub fn cache_base() -> Result<PathBuf> {
    let base = etcetera::choose_base_strategy().map_err(|e| ProjectError::Io {
        path: PathBuf::from("~"),
        source: io::Error::other(e.to_string()),
    })?;
    Ok(base.cache_dir().join("ve"))
}

/// OneDrive, antivirus scanners and the search indexer briefly hold files open
/// on Windows, which makes a rename fail with "access denied" or "used by
/// another process". A few short retries (about 6 s at most) ride it out.
fn rename_with_retry(from: &Path, to: &Path) -> Result<()> {
    let mut last = None;
    for attempt in 0..7 {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if is_transient(&e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(50 << attempt));
            }
            Err(e) => return Err(ProjectError::Io { path: to.to_owned(), source: e }),
        }
    }
    Err(ProjectError::Io { path: to.to_owned(), source: last.expect("loop ran") })
}

/// Errors that mean "someone else has the file open right now". Windows
/// sharing and lock violations (os errors 32 and 33) have no `ErrorKind` of
/// their own; std reports them as uncategorized.
fn is_transient(e: &io::Error) -> bool {
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const ERROR_LOCK_VIOLATION: i32 = 33;
    e.kind() == io::ErrorKind::PermissionDenied
        || (cfg!(windows) && matches!(e.raw_os_error(), Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)))
}

/// Two-space indented JSON, the same layout Python OTIO writes.
fn to_pretty_json(tl: &Timeline) -> serde_json::Result<String> {
    let mut buf = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    serde::Serialize::serialize(tl, &mut ser)?;
    Ok(String::from_utf8(buf).expect("serde_json writes UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sharing_violations_are_transient() {
        assert!(is_transient(&io::Error::from(io::ErrorKind::PermissionDenied)));
        assert!(!is_transient(&io::Error::from(io::ErrorKind::NotFound)));
        assert_eq!(is_transient(&io::Error::from_raw_os_error(32)), cfg!(windows));
        assert_eq!(is_transient(&io::Error::from_raw_os_error(33)), cfg!(windows));
    }

    /// Antivirus scans the temp file right after it is written, holding it
    /// with no sharing. That must delay the save, not fail it.
    #[cfg(windows)]
    #[test]
    fn rename_waits_out_a_file_held_open() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = std::env::temp_dir().join(format!("ve-rename-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let to = dir.join(PROJECT_FILE);
        let from = dir.join("project.otio.tmp");
        fs::write(&to, "old").unwrap();
        fs::write(&from, "new").unwrap();
        let held = File::options().read(true).share_mode(0).open(&from).unwrap();
        let err = fs::rename(&from, &to).unwrap_err();
        assert_eq!(err.raw_os_error(), Some(32), "the test should reproduce a sharing violation");
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            drop(held);
        });
        rename_with_retry(&from, &to).unwrap();
        release.join().unwrap();
        assert_eq!(fs::read_to_string(&to).unwrap(), "new");
        fs::remove_dir_all(&dir).ok();
    }
}
