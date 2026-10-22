//! Timeline frames through the GPU compositor (Phase 2 spike).
//!
//! For a frame time: resolve which clip plays on each video track, decode
//! that clip's source frame with ffmpeg as RGBA, place it with the same fit
//! rules as the Phase 1 filtergraph, and stack the tracks on the GPU
//! ([`crate::gpu`]). Must match [`crate::frame`] (the ffmpeg path) up to
//! small resampling and chroma differences; `tests/gpu.rs` checks it.
//!
//! Decode goes through warm ffmpeg processes ([`crate::decoder`]), one per
//! layer, at the size each layer is drawn.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ve_model::Timeline;
use ve_model::time::Q;

use crate::decoder;
use crate::ffmpeg::{self, Inputs};
use crate::fx::{Kind, Registry};
use crate::gpu::{Gpu, Item, Pass, Picture, Rect};
use crate::plan::{GPU_ONLY, Plan, Segment, plan, timeline_end};
use crate::{Canvas, EncodeSettings, Encoded, Fit, Graph, Image, ImageFormat, RenderError, Span, encode_args, local_path, project_canvas};

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
            // `crop` takes the middle, offsets rounded down to whole pixels.
            let x = -(((sw - cw) / 2) as f32);
            let y = -(((sh - ch) / 2) as f32);
            Rect { x, y, w: sw as f32, h: sh as f32 }
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

/// The effect passes of a segment's clip at timeline time `t`.
fn passes(seg: &Segment, t: Q, fx: &Registry) -> Result<Vec<Pass>> {
    seg.effects
        .iter()
        .map(|e| {
            let f = fx.require(&e.effect, Kind::Effect).map_err(|err| RenderError::Invalid(format!("{} on {}: {err}", e.id, seg.clip)))?;
            let local = t - seg.clip_start;
            let progress = if seg.clip_duration > Q::from_integer(0) { seconds(local / seg.clip_duration) } else { 0.0 };
            Ok(Pass { params: f.pack(&e.params, &e.keyframes, local), fx: f, time: seconds(local), progress })
        })
        .collect()
}

fn seconds(q: Q) -> f32 {
    (*q.numer() as f64 / *q.denom() as f64) as f32
}

/// A segment's frame at `t`, decoded at its drawn size, with its effects.
struct Decoded {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    rect: Rect,
    effects: Vec<Pass>,
}

fn decoded(seg: &Segment, t: Q, slot: usize, sizes: &HashMap<PathBuf, (u32, u32)>, fit: Fit, canvas: (u32, u32), fx: &Registry) -> Result<Decoded> {
    let &(w, h) = sizes
        .get(&seg.path)
        .ok_or_else(|| RenderError::Media(format!("{}: {} has no probed video size", seg.clip, seg.path.display())))?;
    let rect = place(fit, w, h, canvas.0, canvas.1);
    let (dw, dh) = (rect.w as u32, rect.h as u32);
    let key = decoder::key(&seg.path, dw, dh, seg.media_rate, slot);
    let rgba = decoder::frame(&key, source_frame(seg, t))?;
    Ok(Decoded { rgba, width: dw, height: dh, rect, effects: passes(seg, t, fx)? })
}

impl Decoded {
    fn picture(&self) -> Picture<'_> {
        Picture { rgba: &self.rgba, width: self.width, height: self.height, rect: self.rect, effects: self.effects.clone() }
    }
}

/// What one track shows at a frame.
enum Shown {
    Plain(Decoded),
    Transition(Decoded, Decoded, Pass),
}

/// The composited RGBA of the frame at `at` from a plan that covers it.
fn render_rgba(p: &Plan, sizes: &HashMap<PathBuf, (u32, u32)>, canvas: &Canvas, at: Q, fx: &Registry) -> Result<Vec<u8>> {
    let size = (canvas.width, canvas.height);
    // Each layer is decoded at the size it is drawn, by a warm decoder: one
    // stream per track, a second for the incoming clip of a transition.
    let mut shown = Vec::new();
    for (k, track) in p.video.iter().enumerate() {
        if let Some(tr) = track.transitions.iter().find(|t| t.start <= at && at < t.end) {
            let f = fx.require(&tr.meta.transition, Kind::Transition).map_err(|e| RenderError::Invalid(format!("{}: {e}", tr.meta.id)))?;
            let local = at - tr.start;
            let pass = Pass {
                params: f.pack(&tr.meta.params, &indexmap::IndexMap::new(), local),
                fx: f,
                time: seconds(local),
                progress: seconds(local / (tr.end - tr.start)),
            };
            let a = decoded(&tr.outgoing, at, 2 * k, sizes, canvas.fit, size, fx)?;
            let b = decoded(&tr.incoming, at, 2 * k + 1, sizes, canvas.fit, size, fx)?;
            shown.push(Shown::Transition(a, b, pass));
        } else if let Some(seg) = track.segments.iter().find(|s| s.start <= at && at < s.end) {
            shown.push(Shown::Plain(decoded(seg, at, 2 * k, sizes, canvas.fit, size, fx)?));
        }
    }
    let items: Vec<Item<'_>> = shown
        .iter()
        .map(|s| match s {
            Shown::Plain(d) => Item::Picture(d.picture()),
            Shown::Transition(a, b, pass) => Item::Transition { outgoing: a.picture(), incoming: b.picture(), pass: pass.clone() },
        })
        .collect();
    Gpu::shared()?.render(canvas.width, canvas.height, &items)
}

/// [`crate::frame`] through the GPU compositor: one frame at `at` (snapped
/// down to its frame), `width` pixels wide, with clip effects and
/// transitions from `fx`.
pub fn frame(tl: &Timeline, at: Q, width: u32, format: ImageFormat, fx: &Registry) -> Result<Image> {
    let canvas = project_canvas(tl).scaled_to_width(width);
    let r = canvas.rate.as_ratio();
    let at = (at * r).floor() / r;
    let end = timeline_end(tl);
    if at < Q::from_integer(0) || at >= end {
        return Err(RenderError::Invalid(format!("no frame there: the timeline has {} frames", (end * r).to_integer())));
    }
    let p = plan(tl, Span::new(at, at + Q::from_integer(1) / r))?;
    let rgba = render_rgba(&p, &source_sizes(tl), &canvas, at, fx)?;
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
    let warnings = p.warnings.into_iter().filter(|w| !w.contains(GPU_ONLY)).collect();
    Ok(Image { bytes, width: canvas.width, height: canvas.height, warnings })
}

/// [`crate::encode`] through the GPU compositor: every frame of `span` is
/// composited here and piped as raw RGBA into one ffmpeg run, which also
/// builds the audio from the Phase 1 graph (crossfades, J/L cuts) and encodes
/// both with the same settings. Reports progress (seconds of output) and stops
/// on cancel through the thread's [`crate::control::Control`]; a failed or
/// cancelled run leaves no file at `out`.
pub fn encode(tl: &Timeline, span: Span, s: &EncodeSettings, out: &Path, work: &Path, fx: &Registry) -> Result<Encoded> {
    let p = plan(tl, span)?;
    let c = &s.canvas;
    let r = c.rate.as_ratio();
    let total = ((span.end - span.start) * r).round().to_integer();
    let encoder = match &s.encoder {
        Some(e) => e.clone(),
        None => ffmpeg::h264_encoder()?.to_owned(),
    };
    // Fail on a missing effect or GPU before an encoder is started.
    Gpu::shared()?;
    let sizes = source_sizes(tl);

    let mut g = Graph::new();
    let a = g.audio(&p, c, s.crossfade);
    let pipe_index = g.input_count();
    g.chain(format!("[{pipe_index}:v]format=yuv420p,setsar=1[gv]"));
    let pipe = ["-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{}x{}", c.width, c.height), "-framerate", &c.rate.to_string(), "-i", "-"]
        .iter()
        .map(|a| (*a).to_owned())
        .collect();
    let args = encode_args(s, &encoder, "[gv]", &format!("[{a}]"), out);

    let ctrl = crate::control::current();
    let mut produce = |send: &mut ffmpeg::Send| -> Result<()> {
        for k in 0..total {
            let at = span.start + Q::from_integer(k) / r;
            send(render_rgba(&p, &sizes, c, at, fx)?)?;
            if let Some(ctrl) = &ctrl {
                ctrl.progress(((k + 1) as f64) / c.rate.to_f64());
            }
        }
        Ok(())
    };
    let run = ffmpeg::feed(&Inputs::GraphPipe(&g, pipe), &args, work, &mut produce);
    if let Err(e) = run {
        let _ = std::fs::remove_file(out);
        return Err(e);
    }
    let warnings = p.warnings.into_iter().filter(|w| !w.contains(GPU_ONLY)).collect();
    Ok(Encoded { frames: total, encoder, width: c.width, height: c.height, warnings })
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
