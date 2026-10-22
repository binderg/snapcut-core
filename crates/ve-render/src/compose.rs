//! Timeline frames through the GPU compositor (Phase 2 spike).
//!
//! For a frame time: resolve which clip plays on each video track, decode
//! that clip's source frame with ffmpeg as RGBA, place it with the same fit
//! rules as the Phase 1 filtergraph, and stack the tracks on the GPU
//! ([`crate::gpu`]). Must match [`crate::frame`] (the ffmpeg path) up to
//! small resampling and chroma differences; `tests/gpu.rs` checks it.
//!
//! Decode is one ffmpeg run per layer for now. In-process decoders that stay
//! warm between frames (`ffmpeg-the-third`) are the next step for speed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ve_model::Timeline;
use ve_model::time::{Q, Rate};

use crate::ffmpeg::{self, Inputs};
use crate::gpu::{Gpu, Layer, Rect};
use crate::plan::{Segment, plan, timeline_end};
use crate::{Fit, Image, ImageFormat, RenderError, Span, local_path, project_canvas};

type Result<T> = std::result::Result<T, RenderError>;

/// Displayed size (after rotation) of each source file the project knows.
fn source_sizes(tl: &Timeline) -> HashMap<PathBuf, (u32, u32)> {
    tl.metadata
        .ve
        .assets
        .iter()
        .filter_map(|a| {
            let v = a.media.as_ref()?.video.as_ref()?;
            let size = if v.rotation % 180 == 90 { (v.height, v.width) } else { (v.width, v.height) };
            Some((local_path(&a.url)?, size))
        })
        .collect()
}

/// `av_rescale` with round-half-away-from-zero, as ffmpeg's scale filter uses.
fn rescale(a: u32, b: u32, c: u32) -> u32 {
    ((u64::from(a) * u64::from(b) * 2 + u64::from(c)) / (2 * u64::from(c))) as u32
}

/// Where a `w x h` source lands on a `cw x ch` canvas, matching the Phase 1
/// graph: `scale=force_original_aspect_ratio=decrease:force_divisible_by=2`
/// then centered `pad` (offsets aligned to even pixels), or for `Fill`,
/// scale to cover and crop the middle.
pub fn place(fit: Fit, w: u32, h: u32, cw: u32, ch: u32) -> Rect {
    match fit {
        Fit::Fit => {
            let sw = (rescale(ch, w, h).min(cw) / 2 * 2).max(2);
            let sh = (rescale(cw, h, w).min(ch) / 2 * 2).max(2);
            let x = ((cw - sw) / 2) & !1;
            let y = ((ch - sh) / 2) & !1;
            Rect { x: x as f32, y: y as f32, w: sw as f32, h: sh as f32 }
        }
        Fit::Fill => {
            let sw = rescale(ch, w, h).max(cw);
            let sh = rescale(cw, h, w).max(ch);
            Rect { x: (cw as f32 - sw as f32) / 2.0, y: (ch as f32 - sh as f32) / 2.0, w: sw as f32, h: sh as f32 }
        }
    }
}

/// The source frame a segment shows at timeline time `t`.
fn source_frame(seg: &Segment, t: Q) -> i64 {
    let media_t = seg.media_start + (t - seg.start) * seg.speed;
    let r = seg.media_rate.as_ratio();
    let mut n = (media_t * r).floor().to_integer();
    if let Some((_, end)) = seg.bounds {
        let last = (end * r).ceil().to_integer() - 1;
        n = n.min(last);
    }
    n.max(0)
}

fn seconds(q: Q) -> String {
    format!("{:.6}", *q.numer() as f64 / *q.denom() as f64)
}

/// Decode frame `n` of `path` (at `rate`) as `w x h` RGBA.
fn decode(path: &Path, n: i64, rate: Rate, w: u32, h: u32, work: &Path) -> Result<Vec<u8>> {
    // Seek half a frame early: the first frame at or after that point is `n`,
    // whatever rounding the container's timestamps have.
    let r = rate.as_ratio();
    let at = (Q::from_integer(n) - Q::new(1, 2)) / r;
    let at = if at < Q::from_integer(0) { Q::from_integer(0) } else { at };
    let args = vec!["-ss".to_owned(), seconds(at), "-i".to_owned(), path.to_string_lossy().into_owned()];
    let out: Vec<String> = ["-map", "0:v:0", "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgba", "-"].iter().map(|s| (*s).to_owned()).collect();
    let o = ffmpeg::run_inputs(&Inputs::Args(args), &out, work, false)?;
    let want = (w as usize) * (h as usize) * 4;
    if o.stdout.len() != want {
        return Err(RenderError::Media(format!(
            "{}: decoded {} bytes for frame {n}, expected {w}x{h} RGBA ({want})",
            path.display(),
            o.stdout.len()
        )));
    }
    Ok(o.stdout)
}

/// [`crate::frame`] through the GPU compositor: one frame at `at` (snapped
/// down to its frame), `width` pixels wide.
pub fn frame(tl: &Timeline, at: Q, width: u32, format: ImageFormat, work: &Path) -> Result<Image> {
    let canvas = project_canvas(tl).scaled_to_width(width);
    let r = canvas.rate.as_ratio();
    let at = (at * r).floor() / r;
    let end = timeline_end(tl);
    if at < Q::from_integer(0) || at >= end {
        return Err(RenderError::Invalid(format!("no frame there: the timeline has {} frames", (end * r).to_integer())));
    }
    let p = plan(tl, Span::new(at, at + Q::from_integer(1) / r))?;
    let sizes = source_sizes(tl);

    let mut pixels = Vec::new();
    for track in &p.video {
        let Some(seg) = track.segments.iter().find(|s| s.start <= at && at < s.end) else { continue };
        let &(w, h) = sizes
            .get(&seg.path)
            .ok_or_else(|| RenderError::Media(format!("{}: {} has no probed video size", seg.clip, seg.path.display())))?;
        let n = source_frame(seg, at);
        pixels.push((decode(&seg.path, n, seg.media_rate, w, h, work)?, w, h));
    }
    let layers: Vec<Layer<'_>> = pixels
        .iter()
        .map(|(rgba, w, h)| Layer { rgba, width: *w, height: *h, rect: place(canvas.fit, *w, *h, canvas.width, canvas.height) })
        .collect();

    let rgba = Gpu::shared()?.composite(canvas.width, canvas.height, &layers)?;
    let rgb = image::RgbImage::from_fn(canvas.width, canvas.height, |x, y| {
        let i = ((y * canvas.width + x) * 4) as usize;
        image::Rgb([rgba[i], rgba[i + 1], rgba[i + 2]])
    });
    let mut bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut bytes);
    let fmt = match format {
        ImageFormat::Png => image::ImageFormat::Png,
        ImageFormat::Jpeg => image::ImageFormat::Jpeg,
    };
    rgb.write_to(&mut cursor, fmt).map_err(|e| RenderError::Io(format!("encoding the frame: {e}")))?;
    Ok(Image { bytes, width: canvas.width, height: canvas.height, warnings: p.warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_matches_ffmpeg_rounding() {
        // 1920x1080 into 640x640: 640x360, centered.
        assert_eq!(place(Fit::Fit, 1920, 1080, 640, 640), Rect { x: 0.0, y: 140.0, w: 640.0, h: 360.0 });
        // Odd results round to even sizes and even offsets.
        let r = place(Fit::Fit, 1080, 1920, 640, 360);
        assert_eq!((r.w, r.h), (202.0, 360.0));
        assert_eq!(r.x, 218.0);
        // Same aspect: fills the canvas.
        assert_eq!(place(Fit::Fit, 64, 64, 64, 64), Rect { x: 0.0, y: 0.0, w: 64.0, h: 64.0 });
    }
}
