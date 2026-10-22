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

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
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
}

pub fn frame(p: &Snapshot, a: FrameParams) -> Result<ImageResult, RpcError> {
    let tl = &p.timeline;
    let at = parse_time("at", &a.at, tl.output_rate())?;
    // VE_RENDERER=gpu (set where the daemon starts) tries the Phase 2 GPU compositor.
    let gpu = std::env::var("VE_RENDERER").is_ok_and(|v| v.eq_ignore_ascii_case("gpu"));
    let img = if gpu {
        let fx = ve_render::fx::Registry::for_project(Some(p.root()));
        ve_render::compose::frame(tl, at, a.width, a.format.into(), &fx)
    } else {
        ve_render::frame(tl, at, a.width, a.format.into(), &work_dir(p))
    }
    .map_err(render_error)?;
    Ok(ImageResult {
        at: tc(at, tl.output_rate()),
        width: img.width,
        height: img.height,
        mime: ImageFormat::from(a.format).mime().into(),
        data: base64::engine::general_purpose::STANDARD.encode(img.bytes),
        warnings: img.warnings,
    })
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
    let e = ve_render::encode(tl, sp, &settings, &path, &work).map_err(render_error)?;
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
    })
}

pub fn range_preview(p: &Snapshot, a: PreviewParams) -> Result<VideoResult, RpcError> {
    let sp = span(&p.timeline, a.from.as_ref(), a.to.as_ref())?;
    let settings = ve_export::preview_settings(&p.timeline, a.width);
    encode(p, sp, settings, "preview")
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
