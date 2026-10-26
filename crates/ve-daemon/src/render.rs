//! `render.*` methods: the agent's eyes (frame, contact sheet, range
//! preview) and export.
//!
//! The protocol never writes where a client asked: images come back as
//! base64 bytes, videos as a file in the project's cache folder that the
//! client moves (the CLI puts it at `-o`). Each works from a [`Snapshot`],
//! so a long render never holds the project lock, and the slow ones can run
//! as background jobs (`background: true`).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ve_model::Timeline;
use ve_ops::edit::tc;
use ve_protocol::{ErrorCode, RpcError};
use ve_render::{ImageFormat, RenderError, Span};

use crate::Snapshot;
use crate::commit::parse_time;

pub(crate) fn render_error(e: RenderError) -> RpcError {
    let code = match &e {
        RenderError::Invalid(_) => ErrorCode::InvalidParams,
        RenderError::Media(_) | RenderError::Ffmpeg { .. } => ErrorCode::RenderFailure,
        RenderError::Io(_) => ErrorCode::Internal,
        RenderError::Cancelled => ErrorCode::Cancelled,
    };
    RpcError::new(code, e.to_string())
}

/// `<cache>/renders`, with files older than a day cleared out.
pub(crate) fn work_dir(s: &Snapshot) -> PathBuf {
    let dir = s.cache_dir().join("renders");
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let old = SystemTime::now() - Duration::from_secs(24 * 3600);
        for e in entries.flatten() {
            if e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t < old) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    dir
}

pub(crate) fn span(tl: &Timeline, from: Option<&String>, to: Option<&String>) -> Result<Span, RpcError> {
    let rate = tl.output_rate();
    let from = from.map(|s| parse_time("from", s, rate)).transpose()?;
    let to = to.map(|s| parse_time("to", s, rate)).transpose()?;
    ve_render::span_or_all(tl, from, to).map_err(render_error)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Jpeg,
    Png,
}

impl From<Format> for ImageFormat {
    fn from(f: Format) -> Self {
        match f {
            Format::Jpeg => ImageFormat::Jpeg,
            Format::Png => ImageFormat::Png,
        }
    }
}

fn default_width() -> u32 {
    640
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FrameParams {
    /// Timeline time (`00:01:12:12`, `f2172`, `72.4s`).
    pub at: String,
    /// Output width in pixels (height keeps the project aspect). Default 640.
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default)]
    pub format: Format,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ImageResult {
    /// Timecode of the frame (or the first cell).
    pub at: String,
    pub width: u32,
    pub height: u32,
    pub mime: String,
    /// The image, base64.
    pub data: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// Served from the frame cache (nothing in the frame changed since it was drawn).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cached: bool,
}

/// Where a project's cached frames live.
pub(crate) fn frames_dir(p: &Snapshot) -> PathBuf {
    p.cache_dir().join("frames")
}

/// One frame through the frame cache: `(frame, served from the cache)`.
/// `pool` renders on the GPU with the caller's decoders (background work);
/// `None` picks the renderer the usual way.
pub(crate) fn cached_frame(
    s: &crate::Session,
    p: &Snapshot,
    at: ve_model::time::Q,
    width: u32,
    format: Format,
    pool: Option<&mut ve_render::decoder::DecoderPool>,
) -> Result<(crate::frames::CachedFrame, bool), RpcError> {
    let tl = &p.timeline;
    let fx = ve_render::fx::Registry::for_project(Some(p.root()));
    let forced = renderer_override();
    // A forced backend is for tests and debugging: skip the cache.
    let key = if forced.is_none() { Some(ve_render::compose::frame_key(tl, at, width, format.into(), &fx).map_err(render_error)?) } else { None };
    let dir = frames_dir(p);
    if let Some(hit) = key.as_deref().and_then(|k| s.frames.get(&dir, k)) {
        return Ok((hit, true));
    }
    let img = match (pool, forced) {
        (Some(pool), None) => ve_render::compose::frame_in(tl, at, width, format.into(), &fx, pool),
        _ => {
            let gpu = match forced {
                Some(f) => f,
                None => ve_render::needs_gpu(tl, ve_render::Span::new(at, at + tl.output_rate().as_ratio().recip())).map_err(render_error)?,
            };
            if gpu { ve_render::compose::frame(tl, at, width, format.into(), &fx) } else { ve_render::frame(tl, at, width, format.into(), &work_dir(p)) }
        }
    }
    .map_err(render_error)?;
    let f = crate::frames::CachedFrame { width: img.width, height: img.height, warnings: img.warnings, bytes: std::sync::Arc::new(img.bytes) };
    if let Some(k) = &key {
        s.frames.put(&dir, k, f.clone());
    }
    Ok((f, false))
}

pub fn frame(s: &crate::Session, p: &Snapshot, a: FrameParams) -> Result<ImageResult, RpcError> {
    let tl = &p.timeline;
    let at = parse_time("at", &a.at, tl.output_rate())?;
    let (f, cached) = cached_frame(s, p, at, a.width, a.format, None)?;
    Ok(ImageResult {
        at: tc(at, tl.output_rate()),
        width: f.width,
        height: f.height,
        mime: ImageFormat::from(a.format).mime().into(),
        data: base64::engine::general_purpose::STANDARD.encode(f.bytes.as_slice()),
        warnings: f.warnings,
        cached,
    })
}

/// `VE_RENDERER=gpu|ffmpeg` (set where the daemon starts) forces a backend;
/// this is a test and debugging switch. Otherwise a range with effects or
/// transitions renders on the GPU and everything else through ffmpeg.
fn renderer_override() -> Option<bool> {
    match std::env::var("VE_RENDERER") {
        Ok(v) if v.eq_ignore_ascii_case("gpu") => Some(true),
        Ok(v) if v.eq_ignore_ascii_case("ffmpeg") => Some(false),
        _ => None,
    }
}

fn default_count() -> u32 {
    12
}

fn default_cell() -> u32 {
    320
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetParams {
    /// Range start; default the timeline start.
    #[serde(default)]
    pub from: Option<String>,
    /// Range end (exclusive); default the timeline end.
    #[serde(default)]
    pub to: Option<String>,
    /// Frames to show, evenly spaced (1-100). Default 12.
    #[serde(default = "default_count")]
    pub count: u32,
    /// Default: a near-square grid.
    #[serde(default)]
    pub columns: Option<u32>,
    /// Width of each cell. Default 320.
    #[serde(default = "default_cell")]
    pub width: u32,
    #[serde(default)]
    pub format: Format,
    /// Run as a background job: answer `{job_id}` at once (see `jobs.*`).
    #[serde(default)]
    pub background: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SheetResult {
    #[serde(flatten)]
    pub image: ImageResult,
    pub columns: u32,
    pub rows: u32,
    /// Timecode of each cell, row by row.
    pub times: Vec<String>,
}

pub fn contact_sheet(p: &Snapshot, a: SheetParams) -> Result<SheetResult, RpcError> {
    let tl = &p.timeline;
    let rate = tl.output_rate();
    let sp = span(tl, a.from.as_ref(), a.to.as_ref())?;
    let s = ve_render::contact_sheet(tl, sp, a.count, a.columns, a.width, a.format.into(), &work_dir(p)).map_err(render_error)?;
    let times: Vec<String> = s.times.iter().map(|&t| tc(t, rate)).collect();
    Ok(SheetResult {
        image: ImageResult {
            at: times.first().cloned().unwrap_or_default(),
            width: s.image.width,
            height: s.image.height,
            mime: ImageFormat::from(a.format).mime().into(),
            data: base64::engine::general_purpose::STANDARD.encode(s.image.bytes),
            warnings: s.image.warnings,
            cached: false,
        },
        columns: s.columns,
        rows: s.rows,
        times,
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreviewParams {
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    /// Width in pixels. Default 640.
    #[serde(default = "default_width")]
    pub width: u32,
    /// Run as a background job: answer `{job_id}` at once (see `jobs.*`).
    #[serde(default)]
    pub background: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportParams {
    /// Preset name (`render.presets` lists them). Default `master`.
    #[serde(default)]
    pub preset: Option<String>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    /// Run as a background job: answer `{job_id}` at once (see `jobs.*`).
    #[serde(default)]
    pub background: bool,
}

/// Seconds of output a range covers, for job progress.
pub(crate) fn span_secs(tl: &Timeline, from: Option<&String>, to: Option<&String>) -> Option<f64> {
    let d = span(tl, from, to).ok()?.duration();
    Some(*d.numer() as f64 / *d.denom() as f64)
}

impl crate::jobs::Background for SheetParams {
    fn background(&self) -> bool {
        self.background
    }
}

impl crate::jobs::Background for PreviewParams {
    fn background(&self) -> bool {
        self.background
    }
    fn expected_secs(&self, s: &Snapshot) -> Option<f64> {
        span_secs(&s.timeline, self.from.as_ref(), self.to.as_ref())
    }
}

impl crate::jobs::Background for ExportParams {
    fn background(&self) -> bool {
        self.background
    }
    fn expected_secs(&self, s: &Snapshot) -> Option<f64> {
        span_secs(&s.timeline, self.from.as_ref(), self.to.as_ref())
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct VideoResult {
    /// The rendered file, in the engine's cache. Move it where it belongs;
    /// files left there are deleted after a day.
    pub path: PathBuf,
    pub url: String,
    pub mime: String,
    pub size_bytes: u64,
    /// Range rendered, as timecode.
    pub from: String,
    pub to: String,
    pub duration: String,
    pub frames: i64,
    pub width: u32,
    pub height: u32,
    pub encoder: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// A preview whose content had not changed since it was last made: the same file, at once.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cached: bool,
}

fn out_file(work: &Path, kind: &str, rev: u64) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    work.join(format!("{kind}-rev{rev}-{nanos}.mp4"))
}

fn encode(p: &Snapshot, sp: Span, settings: ve_render::EncodeSettings, kind: &str) -> Result<VideoResult, RpcError> {
    let work = work_dir(p);
    let path = out_file(&work, kind, p.revision);
    let tl = &p.timeline;
    let rate = tl.output_rate();
    let gpu = match renderer_override() {
        Some(forced) => forced,
        None => ve_render::needs_gpu(tl, sp).map_err(render_error)?,
    };
    let fx = ve_render::fx::Registry::for_project(Some(p.root()));
    let e = if gpu {
        ve_render::compose::encode(tl, sp, &settings, &path, &work, &fx)
    } else {
        ve_render::encode(tl, sp, &settings, &path, &work, &fx)
    }
    .map_err(render_error)?;
    let size_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let url = ve_media::file_url(&path).unwrap_or_default();
    Ok(VideoResult {
        path,
        url,
        mime: "video/mp4".into(),
        size_bytes,
        from: tc(sp.start, rate),
        to: tc(sp.end, rate),
        duration: tc(sp.duration(), rate),
        frames: e.frames,
        width: e.width,
        height: e.height,
        encoder: e.encoder,
        warnings: e.warnings,
        cached: false,
    })
}

/// Playback previews kept by content, per project.
const PREVIEW_BYTES: u64 = 1 << 30;

/// A playback preview of a range. Kept by content ([`ve_render::preview_key`]):
/// asking again for a range nothing changed in returns the same file at once.
/// Otherwise its frames come from the frame cache when the preview has them
/// all (render-ahead usually has), or are rendered into it when the range
/// needs the GPU; a plain range nobody has looked at goes through the
/// ffmpeg graph, which is fastest for that.
pub fn range_preview(s: &crate::Session, p: &Snapshot, a: PreviewParams) -> Result<VideoResult, RpcError> {
    let tl = &p.timeline;
    let sp = span(tl, a.from.as_ref(), a.to.as_ref())?;
    let settings = ve_export::preview_settings(tl, a.width);
    if renderer_override().is_some() {
        return encode(p, sp, settings, "preview");
    }
    let t0 = std::time::Instant::now();
    let fx = ve_render::fx::Registry::for_project(Some(p.root()));
    let frame_keys = ve_render::preview_frame_keys(tl, sp, &settings, &fx).map_err(render_error)?;
    let key = ve_render::preview_key_from(tl, sp, &settings, &frame_keys, &fx).map_err(render_error)?;
    let dir = p.cache_dir().join("previews");
    let path = dir.join(format!("{key}.mp4"));
    let meta = dir.join(format!("{key}.json"));
    // One build per preview at a time: a second asker (the render-ahead
    // worker and a pressed play) waits for the first, then finds its file.
    let lock = building(&key);
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    if path.is_file()
        && let Some(mut r) = std::fs::read(&meta).ok().and_then(|b| serde_json::from_slice::<StoredPreview>(&b).ok()).map(StoredPreview::into_result)
    {
        r.path.clone_from(&path);
        r.url = ve_media::file_url(&path).unwrap_or_default();
        r.cached = true;
        return Ok(r);
    }
    std::fs::create_dir_all(&dir).map_err(|e| RpcError::new(ErrorCode::Internal, format!("{}: {e}", dir.display())))?;
    let tmp = dir.join(format!("{key}.part.mp4"));
    let c = settings.canvas;
    let r = c.rate.as_ratio();
    let frames_dir = frames_dir(p);
    let all_cached = frame_keys.iter().all(|key| s.frames.contains(&frames_dir, key));
    let gpu = ve_render::needs_gpu(tl, sp).map_err(render_error)?;
    let work = work_dir(p);
    let e = if all_cached || gpu {
        let mut pool = ve_render::decoder::DecoderPool::default();
        let have_gpu = ve_render::gpu::Gpu::shared().is_ok();
        let mut frame = |t: ve_model::time::Q| -> Result<Vec<u8>, RenderError> {
            // Frames the cache has come straight from it, by the keys worked out above.
            let k = ((t - sp.start) * r).round().to_integer();
            if let Some(f) = usize::try_from(k).ok().and_then(|k| frame_keys.get(k)).and_then(|key| s.frames.get(&frames_dir, key)) {
                return Ok(f.bytes.as_ref().clone());
            }
            let pool = have_gpu.then_some(&mut pool);
            cached_frame(s, p, t, c.width, Format::Jpeg, pool).map(|(f, _)| f.bytes.as_ref().clone()).map_err(|e| RenderError::Invalid(e.message))
        };
        ve_render::encode_frames(tl, sp, &settings, &tmp, &work, &fx, &mut frame)
    } else {
        ve_render::encode(tl, sp, &settings, &tmp, &work, &fx)
    }
    .map_err(render_error)?;
    tracing::info!(
        "range_preview {} frames: {:?} ({})",
        frame_keys.len(),
        t0.elapsed(),
        if all_cached { "from cached frames" } else if gpu { "rendering frames" } else { "ffmpeg graph" }
    );
    std::fs::rename(&tmp, &path).map_err(|e| RpcError::new(ErrorCode::Internal, format!("{}: {e}", path.display())))?;
    let rate = tl.output_rate();
    let result = VideoResult {
        url: ve_media::file_url(&path).unwrap_or_default(),
        size_bytes: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
        path,
        mime: "video/mp4".into(),
        from: tc(sp.start, rate),
        to: tc(sp.end, rate),
        duration: tc(sp.duration(), rate),
        frames: e.frames,
        width: e.width,
        height: e.height,
        encoder: e.encoder,
        warnings: e.warnings,
        cached: false,
    };
    if let Ok(b) = serde_json::to_vec(&StoredPreview::from(&result)) {
        let _ = std::fs::write(&meta, b);
    }
    prune_previews(&dir);
    Ok(result)
}

/// The lock for building the preview with this key.
fn building(key: &str) -> std::sync::Arc<std::sync::Mutex<()>> {
    type Locks = std::sync::Mutex<std::collections::HashMap<String, std::sync::Weak<std::sync::Mutex<()>>>>;
    static LOCKS: std::sync::OnceLock<Locks> = std::sync::OnceLock::new();
    let mut m = LOCKS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    m.retain(|_, w| w.strong_count() > 0);
    if let Some(l) = m.get(key).and_then(std::sync::Weak::upgrade) {
        return l;
    }
    let l = std::sync::Arc::new(std::sync::Mutex::new(()));
    m.insert(key.to_owned(), std::sync::Arc::downgrade(&l));
    l
}

/// What is kept next to a preview file to answer for it again.
#[derive(Serialize, Deserialize)]
struct StoredPreview {
    from: String,
    to: String,
    duration: String,
    frames: i64,
    width: u32,
    height: u32,
    encoder: String,
    size_bytes: u64,
    warnings: Vec<String>,
}

impl From<&VideoResult> for StoredPreview {
    fn from(r: &VideoResult) -> Self {
        Self {
            from: r.from.clone(),
            to: r.to.clone(),
            duration: r.duration.clone(),
            frames: r.frames,
            width: r.width,
            height: r.height,
            encoder: r.encoder.clone(),
            size_bytes: r.size_bytes,
            warnings: r.warnings.clone(),
        }
    }
}

impl StoredPreview {
    fn into_result(self) -> VideoResult {
        VideoResult {
            path: PathBuf::new(),
            url: String::new(),
            mime: "video/mp4".into(),
            size_bytes: self.size_bytes,
            from: self.from,
            to: self.to,
            duration: self.duration,
            frames: self.frames,
            width: self.width,
            height: self.height,
            encoder: self.encoder,
            warnings: self.warnings,
            cached: true,
        }
    }
}

/// Keep the preview folder under its budget, oldest first.
fn prune_previews(dir: &Path) {
    let mut files: Vec<(SystemTime, u64, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((m.modified().unwrap_or(UNIX_EPOCH), m.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    if total <= PREVIEW_BYTES {
        return;
    }
    files.sort();
    for (_, len, path) in files {
        if total <= PREVIEW_BYTES / 4 * 3 {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= len;
        }
    }
}

pub fn export(p: &Snapshot, a: ExportParams) -> Result<VideoResult, RpcError> {
    let name = a.preset.as_deref().unwrap_or("master");
    let preset = ve_export::preset(name).ok_or_else(|| {
        let names: Vec<&str> = ve_export::PRESETS.iter().map(|p| p.name).collect();
        RpcError::invalid_params(format!("no preset `{name}`; presets are {}", names.join(", ")))
    })?;
    let sp = span(&p.timeline, a.from.as_ref(), a.to.as_ref())?;
    let settings = ve_export::settings(&p.timeline, preset);
    encode(p, sp, settings, name)
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PresetInfo {
    pub name: String,
    pub description: String,
    /// `[width, height]`, or absent for the project's size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<(u32, u32)>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PresetList {
    pub presets: Vec<PresetInfo>,
}

pub fn presets() -> PresetList {
    PresetList {
        presets: ve_export::PRESETS
            .iter()
            .map(|p| PresetInfo { name: p.name.into(), description: p.description.into(), size: p.size })
            .collect(),
    }
}
