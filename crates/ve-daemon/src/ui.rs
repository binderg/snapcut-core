//! `ui.*` methods: helpers that exist only for human editors (media panel
//! thumbnails today; filmstrips and waveform drawing later). Agents never
//! need them.
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

use crate::project::OpenProject;

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
