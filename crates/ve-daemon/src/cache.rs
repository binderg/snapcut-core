//! `cache.status` and `cache.clear`: what a project keeps in the OS cache
//! folder, and a way to free it. Only things the engine rebuilds on demand
//! are listed. Undo history, the project lock and the daemon's own files are
//! never touched.

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ve_protocol::RpcError;

use crate::Session;
use crate::project::{OpenProject, cache_base};

/// One kind of rebuildable cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CacheKind {
    /// Rendered frames (`render.frame`, render-ahead).
    Frames,
    /// Playback chunks (`render.range_preview`).
    Previews,
    /// Exports waiting to be collected, and leftovers from failed or cancelled ones.
    Renders,
    /// Measurement results for this project's media files.
    Measure,
    /// Audio extracted for plugins (`media.extract_audio`).
    Audio,
}

const ALL: [CacheKind; 5] = [CacheKind::Frames, CacheKind::Previews, CacheKind::Renders, CacheKind::Measure, CacheKind::Audio];

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClearParams {
    /// Which caches to clear. Empty or absent: all of them.
    #[serde(default)]
    pub kinds: Vec<CacheKind>,
    /// Report what would be freed without deleting anything.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CacheEntry {
    pub kind: CacheKind,
    pub bytes: u64,
    pub files: u64,
    /// Folders this kind lives in (measure has one per media file).
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CacheReport {
    pub entries: Vec<CacheEntry>,
    pub total_bytes: u64,
    /// True when `cache.clear` deleted the files (false for status and dry runs).
    pub cleared: bool,
}

fn folders(p: &OpenProject, kind: CacheKind) -> Vec<PathBuf> {
    match kind {
        CacheKind::Frames => vec![p.cache_dir().join("frames")],
        CacheKind::Previews => vec![p.cache_dir().join("previews")],
        CacheKind::Renders => vec![p.cache_dir().join("renders")],
        CacheKind::Audio => vec![p.cache_dir().join("audio")],
        CacheKind::Measure => {
            let Ok(base) = cache_base() else { return Vec::new() };
            p.timeline
                .metadata
                .ve
                .assets
                .iter()
                .filter_map(|a| a.hash.as_deref())
                .map(|h| base.join("measure").join(crate::measure::cache_dir_name(h)))
                .collect()
        }
    }
}

/// Bytes and files under `dir` (0 if it does not exist).
fn usage(dir: &Path) -> (u64, u64) {
    let Ok(entries) = std::fs::read_dir(dir) else { return (0, 0) };
    let mut total = (0, 0);
    for e in entries.flatten() {
        let Ok(meta) = e.metadata() else { continue };
        if meta.is_dir() {
            let (b, f) = usage(&e.path());
            total.0 += b;
            total.1 += f;
        } else {
            total.0 += meta.len();
            total.1 += 1;
        }
    }
    total
}

fn report(dirs: &[(CacheKind, Vec<PathBuf>)], cleared: bool) -> CacheReport {
    let entries: Vec<CacheEntry> = dirs
        .iter()
        .map(|(kind, paths)| {
            let (bytes, files) = paths.iter().map(|d| usage(d)).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
            CacheEntry { kind: *kind, bytes, files, paths: paths.iter().map(|d| d.display().to_string()).collect() }
        })
        .collect();
    CacheReport { total_bytes: entries.iter().map(|e| e.bytes).sum(), entries, cleared }
}

fn kinds_of(p: &OpenProject, kinds: &[CacheKind]) -> Vec<(CacheKind, Vec<PathBuf>)> {
    let kinds = if kinds.is_empty() { &ALL[..] } else { kinds };
    kinds.iter().map(|&k| (k, folders(p, k))).collect()
}

/// `cache.status`: how much each cache holds.
pub fn status(s: &Session) -> Result<CacheReport, RpcError> {
    let dirs = s.with_project(|p| Ok(kinds_of(p, &[])))?;
    Ok(report(&dirs, false))
}

/// `cache.clear`: delete caches (all, or the given kinds). Refused while a
/// job runs, since exports and previews write into these folders.
pub fn clear(s: &Session, p: ClearParams) -> Result<CacheReport, RpcError> {
    let dirs = s.with_project(|proj| Ok(kinds_of(proj, &p.kinds)))?;
    let before = report(&dirs, false);
    if p.dry_run {
        return Ok(before);
    }
    if let Some(job) = s.jobs.list().into_iter().find(|j| !j.state.finished()) {
        return Err(RpcError::invalid_params(format!(
            "{} ({}) is still {}; wait for it or cancel it (`jobs.cancel`) before clearing the cache",
            job.job_id,
            job.method,
            serde_json::to_value(job.state).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
        )));
    }
    if dirs.iter().any(|(k, _)| *k == CacheKind::Frames) {
        s.frames.forget_memory();
    }
    for (_, paths) in &dirs {
        for d in paths {
            match std::fs::remove_dir_all(d) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(RpcError::invalid_params(format!("could not delete {}: {e}", d.display()))),
            }
        }
    }
    Ok(CacheReport { cleared: true, ..before })
}
