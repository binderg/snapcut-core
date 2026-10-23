//! `ui.*` methods: helpers that exist only for human editors (media panel
//! thumbnails, the push that dragging a clip does; filmstrips and waveform
//! drawing later). Agents never need them.
//!
//! TEMPORARY HOME. These belong in a UI plugin, not the core. They live here,
//! behind the `ui-api` cargo feature (default on), only because `ve-plugin` has
//! no host yet. When the host exists, move this module into a `ui` plugin
//! under `plugins/` and delete the feature. Until then, an agent-only build can
//! use `--no-default-features`: the methods disappear from the registry and
//! from `describe`. Docs live in `skills/ve-ui/`, not `skills/ve-core/`, for
//! the same reason.

use std::path::Path;
use std::process::{Command, Stdio};

use base64::Engine;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ve_model::Id;
use ve_protocol::{ErrorCode, RpcError};

use crate::commit::{Committed, commit, mutation_params};
use crate::edit::{edit_err, time, yes};
use crate::project::OpenProject;
use ve_ops::EditReport;
use ve_ops::edit::{self, Move};

mutation_params! {
    /// `ui.move_push`: what a timeline drag does when a clip is let go in
    /// a spot that is too small: the clip (and its linked clips) moves to
    /// `to` and what is in the way slides right just far enough to fit,
    /// linked clips included. Leaves a gap where the clip was.
    pub struct MovePushParams {
        pub clip: Id,
        /// New timeline start of `clip`.
        pub to: String,
        /// Put `clip` (not its linked clips) on this track.
        #[serde(default)]
        pub track: Option<String>,
        #[serde(default = "yes")]
        pub linked: bool,
    }
}

pub fn move_push(p: &mut OpenProject, a: MovePushParams) -> Result<Committed<EditReport>, RpcError> {
    commit(p, a.mutation(), "ui.move_push", |tl| {
        let op = Move { clip: a.clip.clone(), to: time(tl, "to", &a.to)?, track: a.track.clone(), linked: a.linked, push: true, reorder: false };
        edit::move_clip(tl, &op).map_err(edit_err)
    })
}

/// Bump when the thumbnail's frame choice or encoding changes, so stale cache files are ignored.
const THUMB_VERSION: u32 = 1;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThumbnailParams {
    /// An asset ID from `media.assets`.
    pub asset: Id,
    /// Output width in pixels; height keeps the picture's aspect. Default 240.
    #[serde(default = "default_thumb_width")]
    pub width: u32,
}

fn default_thumb_width() -> u32 {
    240
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ThumbnailResult {
    pub asset: Id,
    pub mime: String,
    /// The JPEG, base64.
    pub data: String,
    /// Served from the cache.
    pub cached: bool,
}

/// A poster frame for an asset's file, so a media panel can show it before it is on the timeline.
/// Taken 10% in (at most 5 s), which skips most black openings. Cached by file hash.
pub fn thumbnail(project: &OpenProject, p: ThumbnailParams) -> Result<ThumbnailResult, RpcError> {
    if !(16..=1920).contains(&p.width) {
        return Err(RpcError::invalid_params("width must be 16 to 1920"));
    }
    let asset = project
        .timeline
        .metadata
        .ve
        .assets
        .iter()
        .find(|a| a.id == p.asset)
        .ok_or_else(|| RpcError::invalid_params(format!("no asset {}", p.asset)))?;
    let Some(media) = asset.media.as_ref().filter(|m| m.video.is_some()) else {
        return Err(RpcError::invalid_params(format!("{} has no picture", p.asset)));
    };
    let path = ve_render::local_path(&asset.url)
        .ok_or_else(|| RpcError::invalid_params(format!("{} is not a local file", asset.url)))?;
    let q = media.duration.seconds();
    let at = (*q.numer() as f64 / *q.denom() as f64 * 0.1).min(5.0);

    let cache = asset.hash.as_deref().and_then(|h| {
        let dir: String = h.chars().filter(char::is_ascii_alphanumeric).take(32).collect();
        crate::project::cache_base().ok().map(|b| b.join("thumbs").join(dir).join(format!("v{THUMB_VERSION}-w{}.jpg", p.width)))
    });
    let result = |bytes: &[u8], cached| ThumbnailResult {
        asset: p.asset.clone(),
        mime: "image/jpeg".into(),
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
        cached,
    };
    if let Some(bytes) = cache.as_deref().and_then(|c| std::fs::read(c).ok()) {
        return Ok(result(&bytes, true));
    }
    let bytes = extract_frame(&path, at, p.width)?;
    if let Some(c) = &cache {
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(c.parent().expect("cache path has a folder"))?;
            let tmp = c.with_extension("jpg.tmp");
            std::fs::write(&tmp, &bytes)?;
            std::fs::rename(tmp, c)
        };
        if let Err(e) = write() {
            tracing::warn!("could not cache {}: {e}", c.display());
        }
    }
    Ok(result(&bytes, false))
}

fn extract_frame(path: &Path, at: f64, width: u32) -> Result<Vec<u8>, RpcError> {
    let out = Command::new(ve_render::ffmpeg::ffmpeg_path())
        .args(["-hide_banner", "-nostdin", "-loglevel", "error", "-ss", &format!("{at:.3}")])
        .arg("-i")
        .arg(path)
        .args(["-frames:v", "1", "-vf", &format!("scale={width}:-2"), "-q:v", "4", "-f", "image2pipe", "-c:v", "mjpeg", "-"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| RpcError::new(ErrorCode::Internal, format!("could not run ffmpeg: {e}")))?;
    if !out.status.success() || out.stdout.is_empty() {
        let why = String::from_utf8_lossy(&out.stderr);
        let why = why.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no frame");
        return Err(RpcError::new(ErrorCode::Internal, format!("thumbnail failed for {}: {why}", path.display())));
    }
    Ok(out.stdout)
}

// --- render-ahead for the preview ---

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::render::Format;
use crate::{Session, Snapshot};
use ve_model::time::Q;

/// Seconds around the playhead rendered first (behind, ahead); the rest of the timeline follows.
const WINDOW: (i64, i64) = (10, 60);
/// A client asked for a frame this recently: wait, scrubbing comes first.
const YIELD_MS: u64 = 300;
/// Most often a `ui.cache` event goes out while rendering.
const STATUS_EVERY: Duration = Duration::from_millis(400);

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CacheHintParams {
    /// The playhead (`f300`, `10s`, timecode). Rendering starts here and works outward, ahead first.
    pub at: String,
    /// Width of the frames the preview asks `render.frame` for.
    pub width: u32,
    #[serde(default)]
    pub format: Format,
    /// False stops rendering ahead (the preview is hidden). Default true.
    #[serde(default = "yes")]
    pub active: bool,
    /// Frames per playback chunk, if the preview plays `render.range_preview`
    /// chunks on a grid of this size (0, `chunk`, `2 chunk`, ...). The
    /// chunks ahead of the playhead are then built too, so play starts at once.
    #[serde(default)]
    pub chunk_frames: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CacheStatusParams {}

/// What is cached for the hinted size; also sent as the `ui.cache` event.
#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
pub struct CacheStatus {
    /// Revision the ranges are for.
    pub revision: u64,
    pub width: u32,
    /// Timeline frames in the timeline.
    pub frames: i64,
    /// Cached frames as `[from, to)` frame ranges.
    pub cached: Vec<[i64; 2]>,
    /// Frames still to render.
    pub pending: usize,
}

#[derive(Debug, Clone, PartialEq)]
struct Hint {
    at: i64,
    width: u32,
    format: Format,
    active: bool,
    chunk: Option<i64>,
}

#[derive(Default)]
struct Shared {
    hint: Option<Hint>,
    generation: u64,
    status: CacheStatus,
}

/// The render-ahead worker: one thread per session, started by the first hint.
#[derive(Default)]
pub struct Prefetch {
    shared: Mutex<Shared>,
    wake: Condvar,
    started: AtomicBool,
}

/// `ui.cache_hint`: where the playhead is and what size the preview draws at.
pub fn cache_hint(s: &Arc<Session>, p: CacheHintParams) -> Result<CacheStatus, RpcError> {
    if !(16..=3840).contains(&p.width) {
        return Err(RpcError::invalid_params("width must be 16 to 3840"));
    }
    let rate = s.with_project(|proj| Ok(proj.timeline.output_rate()))?;
    let at = crate::commit::parse_time("at", &p.at, rate)?;
    let at = (at * rate.as_ratio()).floor().to_integer();
    let hint = Hint { at, width: p.width, format: p.format, active: p.active, chunk: p.chunk_frames.filter(|c| *c > 0) };
    let status = {
        let mut sh = s.prefetch.shared.lock().unwrap_or_else(|e| e.into_inner());
        if sh.hint.as_ref() != Some(&hint) {
            sh.hint = Some(hint);
            sh.generation += 1;
        }
        sh.status.clone()
    };
    s.prefetch.wake.notify_all();
    if !s.prefetch.started.swap(true, Ordering::SeqCst) {
        let s2 = s.clone();
        std::thread::Builder::new()
            .name("ve-prefetch".into())
            .spawn(move || worker(&s2))
            .map_err(|e| RpcError::new(ErrorCode::Internal, format!("could not start the render-ahead thread: {e}")))?;
    }
    Ok(status)
}

pub fn cache_status(s: &Session, _: CacheStatusParams) -> Result<CacheStatus, RpcError> {
    Ok(s.prefetch.shared.lock().unwrap_or_else(|e| e.into_inner()).status.clone())
}

fn ranges(cached: &[bool]) -> Vec<[i64; 2]> {
    let mut out: Vec<[i64; 2]> = Vec::new();
    for (i, c) in cached.iter().enumerate() {
        if !c {
            continue;
        }
        let i = i as i64;
        match out.last_mut() {
            Some(r) if r[1] == i => r[1] = i + 1,
            _ => out.push([i, i + 1]),
        }
    }
    out
}

/// What one pass works on: a snapshot, the hint, and keys per frame.
struct Pass {
    snap: Snapshot,
    hint: Hint,
    generation: u64,
    keys: Vec<Option<String>>,
    cached: Vec<bool>,
}

/// Most edits remembered with frames still to render; older ones are dropped
/// (their frames still get rendered, just not ahead of the playhead).
const MAX_EDITS: usize = 32;

fn worker(s: &Arc<Session>) {
    let gpu = ve_render::gpu::Gpu::shared().is_ok();
    let mut pool = ve_render::decoder::DecoderPool::default();
    // Keys of the last pass, by frame, at the width it ran at.
    let mut previous: Option<(u32, Vec<Option<String>>)> = None;
    // The frames each edit changed that are not rendered yet (by key), oldest
    // edit first. The newest edit is always worked on first.
    let mut edits: Vec<std::collections::HashSet<String>> = Vec::new();
    loop {
        let (hint, generation) = {
            let mut sh = s.prefetch.shared.lock().unwrap_or_else(|e| e.into_inner());
            while !sh.hint.as_ref().is_some_and(|h| h.active) {
                sh = s.prefetch.wake.wait(sh).unwrap_or_else(|e| e.into_inner());
            }
            (sh.hint.clone().expect("checked above"), sh.generation)
        };
        let Ok(snap) = s.snapshot() else {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        };
        let mut pass = keyed_pass(s, snap, hint, generation);
        let total = pass.keys.len() as i64;
        if let Some((w, prev)) = previous.take()
            && w == pass.hint.width
        {
            // What this edit changed: frames whose content differs from the last
            // pass and is not cached (a ripple that only moves frames changes nothing).
            let changed: std::collections::HashSet<String> = (0..pass.keys.len())
                .filter(|&i| !pass.cached[i] && prev.get(i).is_some_and(|old| *old != pass.keys[i]))
                .filter_map(|i| pass.keys[i].clone())
                .collect();
            if !changed.is_empty() {
                edits.push(changed);
            }
        }
        previous = Some((pass.hint.width, pass.keys.clone()));
        // Forget edits whose frames are all rendered (or no longer on the timeline).
        let waiting: std::collections::HashSet<&str> =
            (0..pass.keys.len()).filter(|&i| !pass.cached[i]).filter_map(|i| pass.keys[i].as_deref()).collect();
        edits.retain(|e| e.iter().any(|k| waiting.contains(k.as_str())));
        if edits.len() > MAX_EDITS {
            edits.drain(..edits.len() - MAX_EDITS);
        }

        let at = pass.hint.at.clamp(0, (total - 1).max(0));
        let fps = pass.snap.timeline.output_rate().as_ratio().ceil().to_integer().max(1);
        let near = |f: &i64| if *f >= at { f - at } else { (at - f) * 3 };
        publish(s, &pass);
        let mut finished = true;
        // 1. Edits, newest first: their frames nearest the playhead first, then their chunks.
        for e in edits.iter().rev() {
            let mut frames: Vec<i64> = (0..total).filter(|&f| !pass.cached[f as usize] && pass.keys[f as usize].as_ref().is_some_and(|k| e.contains(k))).collect();
            frames.sort_by_key(near);
            if !(render(s, &mut pass, &frames, gpu, &mut pool) && chunks(s, &pass, &frames)) {
                finished = false;
                break;
            }
        }
        // 2. Around the playhead, ahead first, then its chunks; 3. everything else.
        if finished {
            let window = (at - WINDOW.0 * fps).max(0)..(at + WINDOW.1 * fps).min(total);
            let mut around: Vec<i64> = window.clone().filter(|&f| !pass.cached[f as usize]).collect();
            around.sort_by_key(near);
            let ahead: Vec<i64> = (at..window.end).collect();
            let mut rest: Vec<i64> = (0..total).filter(|&f| !pass.cached[f as usize]).collect();
            rest.sort_by_key(near);
            finished = render(s, &mut pass, &around, gpu, &mut pool) && chunks(s, &pass, &ahead) && render(s, &mut pass, &rest, gpu, &mut pool);
        }
        if finished {
            // All done: sleep until the hint or the project changes.
            let rev = pass.snap.revision;
            let mut sh = s.prefetch.shared.lock().unwrap_or_else(|e| e.into_inner());
            while sh.generation == pass.generation && s.revision() == Some(rev) {
                sh = s.prefetch.wake.wait_timeout(sh, Duration::from_millis(500)).unwrap_or_else(|e| e.into_inner()).0;
            }
        }
    }
}

/// Key every frame of the timeline (one plan for all of it) and note which are cached.
fn keyed_pass(s: &Session, snap: Snapshot, hint: Hint, generation: u64) -> Pass {
    let tl = &snap.timeline;
    let r = tl.output_rate().as_ratio();
    let total = (ve_render::timeline_end(tl) * r).ceil().to_integer().max(0);
    let fx = ve_render::fx::Registry::for_project(Some(snap.root()));
    let format = hint.format.into();
    let keys: Vec<Option<String>> = match ve_render::compose::frame_keys(tl, 0, total, hint.width, format, &fx) {
        Ok(k) => k.into_iter().map(Some).collect(),
        // Something cannot render (missing media): key frame by frame, so the rest still can.
        Err(_) => (0..total).map(|f| ve_render::compose::frame_key(tl, Q::from_integer(f) / r, hint.width, format, &fx).ok()).collect(),
    };
    let dir = crate::render::frames_dir(&snap);
    let cached = keys.iter().map(|k| k.as_deref().is_none_or(|k| s.frames.contains(&dir, k))).collect();
    Pass { snap, hint, generation, keys, cached }
}

/// Build the playback chunks holding `frames`, in the order the frames come
/// (each is a quick encode from cached frames, or nothing at all when an
/// identical chunk was made before). Returns false if the pass went stale.
fn chunks(s: &Session, pass: &Pass, frames: &[i64]) -> bool {
    let Some(len) = pass.hint.chunk else { return true };
    let total = pass.cached.len() as i64;
    let mut seen = std::collections::HashSet::new();
    for from in frames.iter().map(|f| f / len * len) {
        if !seen.insert(from) {
            continue;
        }
        if !wait_turn(s, pass) {
            return false;
        }
        let to = (from + len).min(total);
        let params = serde_json::json!({ "from": format!("f{from}"), "to": format!("f{to}"), "width": pass.hint.width });
        match serde_json::from_value::<crate::render::PreviewParams>(params) {
            Ok(p) => {
                if let Err(e) = crate::render::range_preview(s, &pass.snap, p) {
                    tracing::debug!("render-ahead chunk f{from}: {}", e.message);
                }
            }
            Err(e) => tracing::warn!("render-ahead chunk params: {e}"),
        }
    }
    true
}

/// Wait while someone scrubs or a job runs. False if the pass went stale meanwhile.
fn wait_turn(s: &Session, pass: &Pass) -> bool {
    loop {
        if stale(s, pass) {
            return false;
        }
        let busy = s.frames.idle_ms() < YIELD_MS || s.jobs.list().iter().any(|j| matches!(j.state, crate::jobs::JobState::Running));
        if !busy {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Whether this pass is out of date (new hint or new revision).
fn stale(s: &Session, pass: &Pass) -> bool {
    s.prefetch.shared.lock().unwrap_or_else(|e| e.into_inner()).generation != pass.generation || s.revision() != Some(pass.snap.revision)
}

fn publish(s: &Session, pass: &Pass) {
    let status = CacheStatus {
        revision: pass.snap.revision,
        width: pass.hint.width,
        frames: pass.cached.len() as i64,
        cached: ranges(&pass.cached),
        pending: pass.cached.iter().filter(|c| !**c).count(),
    };
    let params = serde_json::to_value(&status).unwrap_or_default();
    s.prefetch.shared.lock().unwrap_or_else(|e| e.into_inner()).status = status;
    s.events.publish_job("ui.cache", &params);
}

/// Render the uncached ones of `frames`, in order. Returns false if the pass went stale.
fn render(s: &Session, pass: &mut Pass, frames: &[i64], gpu: bool, pool: &mut ve_render::decoder::DecoderPool) -> bool {
    let r = pass.snap.timeline.output_rate().as_ratio();
    let mut last = Instant::now();
    for &f in frames {
        if pass.cached[f as usize] {
            continue;
        }
        if !wait_turn(s, pass) {
            return false;
        }
        let rendered = crate::render::cached_frame(s, &pass.snap, Q::from_integer(f) / r, pass.hint.width, pass.hint.format, gpu.then_some(&mut *pool));
        if let Err(e) = &rendered {
            tracing::debug!("render-ahead frame {f}: {}", e.message);
        }
        pass.cached[f as usize] = true;
        if last.elapsed() >= STATUS_EVERY {
            publish(s, pass);
            last = Instant::now();
        }
    }
    publish(s, pass);
    true
}
