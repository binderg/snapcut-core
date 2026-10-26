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
use ve_model::meta::Transform;
use ve_model::time::Q;

use crate::decoder::{self, DecoderPool};
use crate::ffmpeg::{self, Inputs};
use crate::fx::{Kind, Registry};
use crate::gpu::{Gpu, Item, Pass, Picture, Rect, Xf};
use crate::plan::{GPU_ONLY, Plan, Segment, plan, timeline_end};
use crate::{Canvas, EncodeSettings, Encoded, Fit, Graph, Image, ImageFormat, RenderError, Span, encode_args, local_path, project_canvas};

type Result<T> = std::result::Result<T, RenderError>;

/// The matrix GPU exports are encoded with and tagged. Decoders read each
/// source by its own tag (untagged: ffmpeg's BT.601 default, as the ffmpeg
/// path's stills do), so the RGB in between is the same either way. The
/// compositor works in 8-bit RGBA: the output is SDR Rec.709.
pub const OUTPUT_MATRIX: &str = "bt709";

/// What the compositor needs to know about a source file.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Source {
    /// Displayed size, after rotation.
    width: u32,
    height: u32,
}

/// A text clip's picture at `t`, drawn sharp for the largest scale the clip
/// reaches, with its native size (at scale 1) on this canvas.
pub(crate) fn text_picture(seg: &Segment, canvas: &Canvas) -> Result<Option<(std::sync::Arc<crate::text::TextImage>, f32, f32)>> {
    let Some(t) = &seg.text else { return Ok(None) };
    let most = seg
        .transform
        .as_ref()
        .map_or(1.0, |tr| tr.keyframes.get("scale").into_iter().flatten().filter_map(|k| k.v.as_f64()).fold(tr.scale, f64::max))
        .clamp(1.0, 8.0) as f32;
    let img = crate::text::render(t, canvas.width, canvas.height, most)?;
    Ok(Some((std::sync::Arc::clone(&img), img.width as f32 / most, img.height as f32 / most)))
}

/// Where a text clip's picture goes at `t`: its native size, centered,
/// then the transform (scale around its own center, x/y in canvas fractions).
pub(crate) fn text_layout(seg: &Segment, t: Q, (w, h): (f32, f32), canvas: &Canvas) -> (Rect, Xf) {
    let (cw, ch) = (canvas.width as f32, canvas.height as f32);
    let base = Rect { x: (cw - w) / 2.0, y: (ch - h) / 2.0, w, h };
    let Some(tf) = transform_at(seg, t) else { return (base, Xf::default()) };
    let scale = tf.scale as f32;
    let (sw, sh) = (w * scale, h * scale);
    let cx = cw / 2.0 + tf.x as f32 * cw;
    let cy = ch / 2.0 + tf.y as f32 * ch;
    let xf = Xf {
        crop: [tf.crop_left as f32, tf.crop_top as f32, tf.crop_right as f32, tf.crop_bottom as f32],
        rotation: (tf.rotation as f32).to_radians(),
        opacity: tf.opacity as f32,
        blend: ve_model::meta::BLEND_MODES.iter().position(|m| *m == tf.blend).unwrap_or(0) as u32,
    };
    (Rect { x: cx - sw / 2.0, y: cy - sh / 2.0, w: sw, h: sh }, xf)
}

/// Every source file the project knows.
pub(crate) fn sources(tl: &Timeline) -> HashMap<PathBuf, Source> {
    tl.metadata
        .ve
        .assets
        .iter()
        .filter_map(|a| {
            let v = a.media.as_ref()?.video.as_ref()?;
            let (width, height) = if v.rotation % 180 == 90 { (v.height, v.width) } else { (v.width, v.height) };
            Some((local_path(&a.url)?, Source { width, height }))
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
    // A still shows the same picture throughout, so every frame keys the same.
    if seg.still {
        return 0;
    }
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
            Ok(Pass { params: f.pack(&e.params, &e.keyframes, local), images: f.images_for(&e.params), fx: f, time: seconds(local), progress })
        })
        .collect()
}

pub(crate) fn seconds(q: Q) -> f32 {
    (*q.numer() as f64 / *q.denom() as f64) as f32
}

/// A segment's transform at timeline time `t`, keyframes evaluated, or
/// `None` for the default placement.
pub(crate) fn transform_at(seg: &Segment, t: Q) -> Option<Transform> {
    let tr = seg.transform.as_ref()?;
    let mut v = tr.clone();
    let local = t - seg.clip_start;
    for (field, keys) in tr.keyframes.iter().filter(|(_, k)| !k.is_empty()) {
        if let Some(x) = crate::fx::interpolate(keys, local).as_f64() {
            v.set(field, ve_model::meta::Transform::clamp(field, x));
        }
    }
    Some(v)
}

/// Where a segment's picture goes at `t`, how it is drawn, and the size to
/// decode it at. The default placement is [`place`] unchanged (so it still
/// matches the ffmpeg path); a transform scales the fitted rectangle around
/// its center, moves it by canvas fractions, and crops, turns and fades it
/// on the GPU.
pub(crate) fn layout(seg: &Segment, t: Q, src: Source, canvas: &Canvas) -> (Rect, Xf, (u32, u32)) {
    let base = place(canvas.fit, src.width, src.height, canvas.width, canvas.height);
    let Some(tf) = transform_at(seg, t) else {
        return (base, Xf::default(), (base.w as u32, base.h as u32));
    };
    let (cw, ch) = (canvas.width as f32, canvas.height as f32);
    let scale = tf.scale as f32;
    let (w, h) = (base.w * scale, base.h * scale);
    let cx = base.x + base.w / 2.0 + tf.x as f32 * cw;
    let cy = base.y + base.h / 2.0 + tf.y as f32 * ch;
    let rect = Rect { x: cx - w / 2.0, y: cy - h / 2.0, w, h };
    let xf = Xf {
        crop: [tf.crop_left as f32, tf.crop_top as f32, tf.crop_right as f32, tf.crop_bottom as f32],
        rotation: (tf.rotation as f32).to_radians(),
        opacity: tf.opacity as f32,
        blend: ve_model::meta::BLEND_MODES.iter().position(|m| *m == tf.blend).unwrap_or(0) as u32,
    };
    // Decode at the largest size the clip reaches (so animated scale keeps one
    // warm decoder), never above the source's own size.
    let tr = seg.transform.as_ref().expect("transform_at found one");
    let most = tr.keyframes.get("scale").into_iter().flatten().filter_map(|k| k.v.as_f64()).fold(tr.scale, f64::max) as f32;
    let k = (most.min(src.width as f32 / base.w).min(src.height as f32 / base.h)).max(most.min(1.0));
    let even = |v: f32| ((v / 2.0).round() as u32 * 2).max(2);
    (rect, xf, (even(base.w * k), even(base.h * k)))
}

/// A segment's frame at `t`, decoded at its drawn size, with its effects.
struct Decoded {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    rect: Rect,
    xf: Xf,
    effects: Vec<Pass>,
}

/// How [`render_rgba`] decodes: the sources it reads and the decoders it reads with.
struct Ctx<'a> {
    sources: &'a HashMap<PathBuf, Source>,
    pool: &'a mut DecoderPool,
    canvas: &'a Canvas,
    fx: &'a Registry,
    /// Draw on a transparent background and return straight alpha (exports with alpha).
    transparent: bool,
}

fn decoded(ctx: &mut Ctx<'_>, seg: &Segment, t: Q, slot: usize) -> Result<Decoded> {
    if let Some((img, w, h)) = text_picture(seg, ctx.canvas)? {
        let (rect, xf) = text_layout(seg, t, (w, h), ctx.canvas);
        return Ok(Decoded { rgba: img.rgba.clone(), width: img.width, height: img.height, rect, xf, effects: passes(seg, t, ctx.fx)? });
    }
    let src = *ctx
        .sources
        .get(&seg.path)
        .ok_or_else(|| RenderError::Media(format!("{}: {} has no probed video size", seg.clip, seg.path.display())))?;
    let (rect, xf, (dw, dh)) = layout(seg, t, src, ctx.canvas);
    let key = decoder::Key { still: seg.still, ..decoder::key(&seg.path, dw, dh, seg.media_rate, slot) };
    let fx = ctx.fx;
    let rgba = ctx.pool.frame(&key, source_frame(seg, t))?;
    Ok(Decoded { rgba, width: dw, height: dh, rect, xf, effects: passes(seg, t, fx)? })
}

impl Decoded {
    fn picture(&self) -> Picture<'_> {
        Picture { rgba: &self.rgba, width: self.width, height: self.height, rect: self.rect, xf: self.xf, effects: self.effects.clone() }
    }
}

/// What one track shows at a frame.
enum Shown {
    Plain(Decoded),
    Transition(Decoded, Decoded, Pass),
}

/// The composited RGBA of the frame at `at` from a plan that covers it.
fn render_rgba(ctx: &mut Ctx<'_>, p: &Plan, at: Q) -> Result<Vec<u8>> {
    let fx = ctx.fx;
    // Each layer is decoded at the size it is drawn, by a warm decoder: one
    // stream per track, a second for the incoming clip of a transition.
    let mut shown = Vec::new();
    for (k, track) in p.video.iter().enumerate() {
        if let Some(tr) = track.transitions.iter().find(|t| t.start <= at && at < t.end) {
            let f = fx.require(&tr.meta.transition, Kind::Transition).map_err(|e| RenderError::Invalid(format!("{}: {e}", tr.meta.id)))?;
            let local = at - tr.start;
            let pass = Pass {
                params: f.pack(&tr.meta.params, &indexmap::IndexMap::new(), local),
                images: f.images_for(&tr.meta.params),
                fx: f,
                time: seconds(local),
                progress: seconds(local / (tr.end - tr.start)),
            };
            let a = decoded(ctx, &tr.outgoing, at, 2 * k)?;
            let b = decoded(ctx, &tr.incoming, at, 2 * k + 1)?;
            shown.push(Shown::Transition(a, b, pass));
        } else if let Some(seg) = track.segments.iter().find(|s| s.start <= at && at < s.end) {
            shown.push(Shown::Plain(decoded(ctx, seg, at, 2 * k)?));
        }
    }
    let items: Vec<Item<'_>> = shown
        .iter()
        .map(|s| match s {
            Shown::Plain(d) => Item::Picture(d.picture()),
            Shown::Transition(a, b, pass) => Item::Transition { outgoing: a.picture(), incoming: b.picture(), pass: pass.clone() },
        })
        .collect();
    Gpu::shared()?.render_on(ctx.canvas.width, ctx.canvas.height, &items, ctx.transparent)
}

/// [`crate::frame`] through the GPU compositor: one frame at `at` (snapped
/// down to its frame), `width` pixels wide, with clip effects and
/// transitions from `fx`.
pub fn frame(tl: &Timeline, at: Q, width: u32, format: ImageFormat, fx: &Registry) -> Result<Image> {
    // Single frames share the process-wide warm decoders (scrubbing).
    let mut pool = decoder::shared()?;
    frame_in(tl, at, width, format, fx, &mut pool)
}

/// [`frame`] with the caller's decoders: a background walk over many frames
/// in order keeps its own, so it never fights scrubbing over the shared ones.
pub fn frame_in(tl: &Timeline, at: Q, width: u32, format: ImageFormat, fx: &Registry, pool: &mut DecoderPool) -> Result<Image> {
    let canvas = project_canvas(tl).scaled_to_width(width);
    let r = canvas.rate.as_ratio();
    let at = (at * r).floor() / r;
    let end = timeline_end(tl);
    if at < Q::from_integer(0) || at >= end {
        return Err(RenderError::Invalid(format!("no frame there: the timeline has {} frames", (end * r).to_integer())));
    }
    let p = plan(tl, Span::new(at, at + Q::from_integer(1) / r))?;
    let sources = sources(tl);
    let rgba = render_rgba(&mut Ctx { sources: &sources, pool, canvas: &canvas, fx, transparent: false }, &p, at)?;
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

/// Bump when anything about how a frame is drawn changes, so cached frames
/// from an older engine are not reused.
pub const FRAME_KEY_VERSION: u32 = 1;

/// A hash of everything that decides the pixels of the frame at `at`, for
/// the frame cache: the canvas, and per video track the source file (by
/// content hash when the asset has one), the source frame shown, the
/// transform and effect values at that frame and any transition with its
/// progress. Edits that do not change a frame leave its key alone, so a
/// ripple that only shifts later frames in time keeps them cached. The key
/// does not say which renderer drew the frame: the GPU compositor and the
/// ffmpeg path agree exactly on plain frames, and only the GPU draws the rest.
pub fn frame_key(tl: &Timeline, at: Q, width: u32, format: ImageFormat, fx: &Registry) -> Result<String> {
    let r = project_canvas(tl).rate.as_ratio();
    let f = (at * r).floor().to_integer();
    let end = timeline_end(tl);
    if f < 0 || Q::from_integer(f) / r >= end {
        return Err(RenderError::Invalid(format!("no frame there: the timeline has {} frames", (end * r).to_integer())));
    }
    frame_keys(tl, f, f + 1, width, format, fx)?
        .pop()
        .ok_or_else(|| RenderError::Invalid(format!("no frame there: frame {f} is past the end of the timeline")))
}

/// [`frame_key`] for every frame in `[from, to)` (frame numbers at the
/// project rate, clipped to the timeline), from one plan of the range: cheap
/// enough to key a whole timeline, which is how render-ahead finds what an
/// edit changed.
pub fn frame_keys(tl: &Timeline, from: i64, to: i64, width: u32, format: ImageFormat, fx: &Registry) -> Result<Vec<String>> {
    let canvas = project_canvas(tl).scaled_to_width(width);
    let r = canvas.rate.as_ratio();
    let last = (timeline_end(tl) * r).ceil().to_integer();
    let (from, to) = (from.max(0), to.min(last));
    if to <= from {
        return Ok(Vec::new());
    }
    let p = plan(tl, Span::new(Q::from_integer(from) / r, Q::from_integer(to) / r))?;
    let files: HashMap<PathBuf, String> = tl
        .metadata
        .ve
        .assets
        .iter()
        .filter_map(|a| Some((local_path(&a.url)?, a.hash.clone()?)))
        .collect();
    // Files without a content hash are known by path, size and time; look each up once.
    let mut unhashed: HashMap<PathBuf, String> = HashMap::new();
    let mut file_id = |path: &Path| -> String {
        if let Some(h) = files.get(path) {
            return h.clone();
        }
        unhashed
            .entry(path.to_owned())
            .or_insert_with(|| {
                let m = std::fs::metadata(path).ok();
                let modified = m.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
                format!("{} {:?} {:?}", path.display(), m.map(|m| m.len()), modified)
            })
            .clone()
    };
    let head = format!("v{FRAME_KEY_VERSION} {}x{} {:?} {:?}", canvas.width, canvas.height, canvas.fit, format);
    let mut keys = Vec::with_capacity(usize::try_from(to - from).unwrap_or(0));
    for f in from..to {
        let at = Q::from_integer(f) / r;
        let mut h = blake3::Hasher::new();
        let mut put = |s: &str| {
            h.update(s.as_bytes());
            h.update(&[0]);
        };
        put(&head);
        let mut layer = |put: &mut dyn FnMut(&str), seg: &Segment| -> Result<()> {
            match &seg.text {
                Some(t) => put(&format!("text {}", serde_json::to_string(t.as_ref()).unwrap_or_default())),
                None => put(&format!("file {} frame {} rate {}", file_id(&seg.path), source_frame(seg, at), seg.media_rate)),
            }
            if let Some(t) = transform_at(seg, at) {
                put(&serde_json::to_string(&t).unwrap_or_default());
            }
            for pass in passes(seg, at, fx)? {
                put(&format!("fx {} {:x} t={} p={}", pass.fx.reference(), pass.fx.hash, pass.time, pass.progress));
                h_bytes(put, &pass.params);
            }
            Ok(())
        };
        for (k, track) in p.video.iter().enumerate() {
            put(&format!("track {k}"));
            if let Some(tr) = track.transitions.iter().find(|t| t.start <= at && at < t.end) {
                let fxr = fx.require(&tr.meta.transition, Kind::Transition).map_err(|e| RenderError::Invalid(format!("{}: {e}", tr.meta.id)))?;
                let local = at - tr.start;
                put(&format!("transition {} {:x} {} {}", fxr.reference(), fxr.hash, local, tr.end - tr.start));
                h_bytes(&mut put, &fxr.pack(&tr.meta.params, &indexmap::IndexMap::new(), local));
                layer(&mut put, &tr.outgoing)?;
                layer(&mut put, &tr.incoming)?;
            } else if let Some(seg) = track.segments.iter().find(|s| s.start <= at && at < s.end) {
                layer(&mut put, seg)?;
            }
        }
        keys.push(h.finalize().to_hex().to_string());
    }
    Ok(keys)
}

fn h_bytes(put: &mut dyn FnMut(&str), b: &[u8]) {
    put(&b.iter().map(|x| format!("{x:02x}")).collect::<String>());
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
    let f = s.format;
    if !f.has_video() {
        return crate::encode(tl, span, s, out, work, fx);
    }
    let encoder = match &s.encoder {
        Some(e) => e.clone(),
        None => f.video_encoder()?,
    };
    // No GPU: fail before an encoder is started.
    Gpu::shared()?;
    let sources = sources(tl);
    // An export reads with its own decoders: it walks the whole range in
    // order and must not fight a preview or another export over the shared
    // ones. They close when the export ends.
    let mut pool = DecoderPool::default();

    let mut g = Graph::new();
    let a = g.audio(&p, c, s.crossfade, fx)?;
    let pipe_index = g.input_count();
    g.chain(format!("[{pipe_index}:v]scale=out_color_matrix={OUTPUT_MATRIX}:out_range=tv,format={},setsar=1[gv]", f.pix_fmt()));
    let pipe = ["-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{}x{}", c.width, c.height), "-framerate", &c.rate.to_string(), "-i", "-"]
        .iter()
        .map(|a| (*a).to_owned())
        .collect();
    let mut args = encode_args(s, &encoder, "[gv]", &format!("[{a}]"), out);
    // Say what the pixels are, so players do not guess (before the output path).
    let tags = ["-colorspace", OUTPUT_MATRIX, "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv"];
    let last = args.len() - 1;
    args.splice(last..last, tags.iter().map(|t| (*t).to_owned()));

    let ctrl = crate::control::current();
    let mut ctx = Ctx { sources: &sources, pool: &mut pool, canvas: c, fx, transparent: f.alpha() };
    let mut produce = |send: &mut ffmpeg::Send| -> Result<()> {
        for k in 0..total {
            let at = span.start + Q::from_integer(k) / r;
            send(render_rgba(&mut ctx, &p, at)?)?;
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
