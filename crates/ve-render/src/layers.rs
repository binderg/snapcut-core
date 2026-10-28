//! What is on screen and what is heard: the video layers at a frame,
//! bottom to top, with where each one lands, its effect and transition
//! values at that frame, and how much of it shows through the layers above;
//! and the audio clips playing then, with their gain.
//!
//! Geometry only, no decoding, so a frame costs microseconds and a whole
//! timeline can be scanned ([`spans`]). Coverage is worked out from
//! rectangles and is conservative: a layer only counts as hiding what is
//! under it when it is surely opaque there (no alpha, or a still measured
//! opaque; full opacity; normal blend; no effect; not mid-transition; not turned at an odd
//! angle). Anything else is reported as drawn over, never as hiding.

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{Map, Value};
use ve_model::Timeline;
use ve_model::meta::{Alpha, Transform};
use ve_model::otio::{Composable, TrackKind};
use ve_model::time::Q;

use crate::compose::{self, Source, layout, transform_at};
use crate::fx::{Kind, Registry, interpolate};
use crate::gpu::{Rect, Xf};
use crate::plan::{Plan, Segment, TrackPlan, plan, timeline_end};
use crate::{Canvas, RenderError, Span, local_path, project_canvas};
use ve_ops::lane::clip_id;

type Result<T> = std::result::Result<T, RenderError>;

/// The layers at one frame.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Snapped down to its frame.
    pub at: Q,
    pub canvas: (u32, u32),
    /// Bottom to top. A track mid-transition has two (outgoing, incoming).
    pub layers: Vec<Layer>,
    /// Video tracks with a picture that shows (at least partly), bottom to top.
    pub visible: Vec<String>,
    /// Video tracks turned off (`track.set {enabled: false}`): never drawn.
    pub disabled: Vec<String>,
    /// Audio clips playing, by track (first audio track first).
    pub audio: Vec<AudioLayer>,
    /// Audio tracks with a clip that is heard (gain above the -100 dB floor).
    pub audible: Vec<String>,
    /// Audio tracks turned off: never mixed.
    pub muted: Vec<String>,
}

/// An audio clip playing at a frame.
#[derive(Debug, Clone)]
pub struct AudioLayer {
    pub track: String,
    /// Position among audio tracks, 0 first.
    pub track_index: usize,
    pub clip: String,
    pub asset: Option<String>,
    /// The clip's link group: its video partner shares it.
    pub link: Option<String>,
    /// Media seconds heard.
    pub source_time: Q,
    /// The clip's gain (`edit.set_volume`, plus what `edit.normalize` added).
    pub gain_db: f64,
    pub speed: Q,
    /// Audio stream of the file the clip plays.
    pub stream: u32,
    /// Channel mapping to stereo (`auto`, `left`, `right`, `mono`, `[a, b]`).
    pub channels: ve_model::meta::Channels,
    /// Audio effects, in order, with their values at this frame.
    pub effects: Vec<EffectAt>,
    /// False when the gain is at the -100 dB floor (silenced).
    pub audible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Full,
    Partial,
    Hidden,
}

#[derive(Debug, Clone)]
pub struct Layer {
    pub track: String,
    /// Position among video tracks, 0 at the bottom.
    pub track_index: usize,
    pub clip: String,
    pub asset: Option<String>,
    /// The words of a text clip.
    pub text: Option<String>,
    /// The clip's link group: its audio partner shares it.
    pub link: Option<String>,
    /// Media seconds shown; `None` for a still.
    pub source_time: Option<Q>,
    pub still: bool,
    /// The drawn rectangle on the project canvas in pixels, after crop and
    /// before rotation (which turns it around the uncropped center).
    pub rect: Rect,
    /// Degrees clockwise.
    pub rotation: f64,
    pub opacity: f64,
    /// The clip's transform with keyframes evaluated; `None` when it has none.
    pub transform: Option<Transform>,
    pub effects: Vec<EffectAt>,
    pub transition: Option<TransitionAt>,
    pub alpha: Alpha,
    /// Hides what is under it wherever it is drawn.
    pub solid: bool,
    pub visibility: Visibility,
    /// Share of its on-canvas area that shows, 0-100 (1-99 when partial).
    pub visible_percent: u8,
    /// Tracks above that hide some of it.
    pub covered_by: Vec<String>,
    /// Tracks above drawn over it that may let it show through.
    pub overlapped_by: Vec<String>,
    /// Why it is hidden when nothing covers it (off the canvas, opacity 0).
    pub note: Option<String>,
    /// The canvas area it can touch: the drawn quad, turned, boxed.
    bbox: Bx,
}

/// An effect on a layer, with its values at this frame.
#[derive(Debug, Clone)]
pub struct EffectAt {
    pub id: String,
    /// `id@major`, as stored on the clip.
    pub effect: String,
    pub name: Option<String>,
    /// Every param: keyframes evaluated, else the clip's value, else the default.
    pub params: Map<String, Value>,
    /// The effect is not installed: `params` holds only what the clip stores.
    pub missing: bool,
}

#[derive(Debug, Clone)]
pub struct TransitionAt {
    pub id: String,
    pub transition: String,
    pub name: Option<String>,
    pub params: Map<String, Value>,
    /// 0 at the start of the transition, 1 at the end.
    pub progress: f64,
    /// `outgoing` or `incoming`.
    pub role: &'static str,
    /// The other clip in the transition.
    pub with: String,
    pub missing: bool,
}

/// What every frame needs: canvas, sources, assets by file.
struct Ctx<'a> {
    canvas: Canvas,
    sources: HashMap<PathBuf, Source>,
    assets: HashMap<PathBuf, (String, Alpha)>,
    /// Clip ID -> link group ID.
    links: HashMap<String, String>,
    fx: &'a Registry,
}

impl<'a> Ctx<'a> {
    fn new(tl: &Timeline, fx: &'a Registry) -> Self {
        let assets = tl
            .metadata
            .ve
            .assets
            .iter()
            .filter_map(|a| {
                let v = a.media.as_ref().and_then(|m| m.video.as_ref());
                // Assets probed before alpha was recorded: go by the pixel format.
                let alpha = v
                    .and_then(|v| v.alpha.or_else(|| v.color.as_ref().map(|c| Alpha::from_pixel_format(&c.pixel_format))))
                    .unwrap_or(Alpha::Unmeasured);
                Some((local_path(&a.url)?, (a.id.to_string(), alpha)))
            })
            .collect();
        let mut links = HashMap::new();
        for c in &tl.tracks.children {
            let Composable::Track(t) = c else { continue };
            for item in &t.children {
                if let Composable::Clip(c) = item
                    && let (Some(id), Some(link)) = (clip_id(c), c.metadata.ve.clip.as_ref().and_then(|m| m.link.as_ref()))
                {
                    links.insert(id.to_string(), link.to_string());
                }
            }
        }
        Self { canvas: project_canvas(tl), sources: compose::sources(tl), assets, links, fx }
    }
}

/// The layers at `at` (snapped down to its frame).
pub fn at(tl: &Timeline, at: Q, fx: &Registry) -> Result<Frame> {
    let r = tl.output_rate().as_ratio();
    let at = (at * r).floor() / r;
    let end = timeline_end(tl);
    if at < Q::from_integer(0) || at >= end {
        return Err(RenderError::Invalid(format!("no frame there: the timeline has {} frames", (end * r).to_integer())));
    }
    let p = plan(tl, Span::new(at, at + Q::from_integer(1) / r))?;
    let ctx = Ctx::new(tl, fx);
    let mut f = frame(&ctx, &p, at, true)?;
    f.disabled = off_tracks(tl, TrackKind::Video);
    f.muted = off_tracks(tl, TrackKind::Audio);
    Ok(f)
}

/// A stretch of the timeline where the same tracks show and are heard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleSpan {
    pub start: Q,
    pub end: Q,
    /// Video tracks that show, bottom to top.
    pub visible: Vec<String>,
    /// Video tracks with a clip there that is entirely hidden.
    pub hidden: Vec<String>,
    /// Audio tracks heard.
    pub audible: Vec<String>,
}

/// Which tracks show and are heard over `span`, as stretches where that
/// does not change. Every frame is checked (keyframed position or opacity
/// can change it at any frame); stretches with nothing on any track are
/// left out.
pub fn spans(tl: &Timeline, span: Span, fx: &Registry) -> Result<Vec<VisibleSpan>> {
    let r = tl.output_rate().as_ratio();
    let p = plan(tl, span)?;
    let ctx = Ctx::new(tl, fx);
    let (first, last) = ((span.start * r).ceil().to_integer(), (span.end * r).ceil().to_integer());
    let mut out: Vec<VisibleSpan> = Vec::new();
    for n in first..last {
        let at = Q::from_integer(n) / r;
        let f = frame(&ctx, &p, at, false)?;
        let mut hidden: Vec<String> = Vec::new();
        for l in f.layers.iter().filter(|l| l.visibility == Visibility::Hidden) {
            if !f.visible.contains(&l.track) && !hidden.contains(&l.track) {
                hidden.push(l.track.clone());
            }
        }
        let next = at + Q::from_integer(1) / r;
        match out.last_mut() {
            Some(s) if s.end == at && s.visible == f.visible && s.hidden == hidden && s.audible == f.audible => s.end = next,
            _ if f.layers.is_empty() && f.audio.is_empty() => {}
            _ => out.push(VisibleSpan { start: at, end: next, visible: f.visible, hidden, audible: f.audible }),
        }
    }
    Ok(out)
}

/// Tracks of `kind` turned off (`track.set {enabled: false}`).
fn off_tracks(tl: &Timeline, kind: TrackKind) -> Vec<String> {
    tl.tracks
        .children
        .iter()
        .filter_map(|c| match c {
            Composable::Track(t) if t.kind == kind && !t.enabled => Some(t.name.clone()),
            _ => None,
        })
        .collect()
}

/// The segment playing on a track at `at`. Segments are in time order.
fn playing(track: &TrackPlan, at: Q) -> Option<&Segment> {
    let i = track.segments.partition_point(|s| s.end <= at);
    track.segments.get(i).filter(|s| s.start <= at)
}

/// Layers at `at` from a plan that covers it. `detail` adds effect and
/// transition values (not needed to work out what shows).
fn frame(ctx: &Ctx<'_>, p: &Plan, at: Q, detail: bool) -> Result<Frame> {
    let mut layers = Vec::new();
    for (k, track) in p.video.iter().enumerate() {
        if let Some(tr) = track.transitions.iter().find(|t| t.start <= at && at < t.end) {
            let progress = f64::from(compose::seconds((at - tr.start) / (tr.end - tr.start)));
            for (seg, role, other) in [(&tr.outgoing, "outgoing", &tr.incoming), (&tr.incoming, "incoming", &tr.outgoing)] {
                let mut l = layer(ctx, &track.name, k, seg, at, detail)?;
                let f = ctx.fx.require(&tr.meta.transition, Kind::Transition).ok();
                l.transition = Some(TransitionAt {
                    id: tr.meta.id.to_string(),
                    transition: tr.meta.transition.clone(),
                    name: f.as_ref().map(|f| f.manifest.name.clone()),
                    params: if detail { values(f.as_deref(), &tr.meta.params, &indexmap::IndexMap::new(), Q::from_integer(0)) } else { Map::new() },
                    progress,
                    role,
                    with: other.clip.clone(),
                    missing: f.is_none(),
                });
                l.solid = false;
                layers.push(l);
            }
        } else if let Some(seg) = playing(track, at) {
            layers.push(layer(ctx, &track.name, k, seg, at, detail)?);
        }
    }
    let canvas = (ctx.canvas.width, ctx.canvas.height);
    cover(&mut layers, canvas);
    let mut visible: Vec<String> = Vec::new();
    for l in layers.iter().filter(|l| l.visibility != Visibility::Hidden) {
        if !visible.contains(&l.track) {
            visible.push(l.track.clone());
        }
    }
    let mut audio = Vec::new();
    for (k, track) in p.audio.iter().enumerate() {
        if let Some(seg) = playing(track, at) {
            audio.push(AudioLayer {
                track: track.name.clone(),
                track_index: k,
                clip: seg.clip.clone(),
                asset: ctx.assets.get(&seg.path).map(|(id, _)| id.clone()),
                link: ctx.links.get(&seg.clip).cloned(),
                source_time: seg.media_start + (at - seg.start) * seg.speed,
                gain_db: seg.volume_db
                    + if seg.volume_keys.is_empty() {
                        0.0
                    } else {
                        crate::fx::interpolate(&seg.volume_keys, at - seg.clip_start).as_f64().unwrap_or(0.0)
                    },
                speed: seg.speed,
                stream: seg.stream,
                channels: seg.channels.clone(),
                effects: if detail { effects_at(ctx, seg, at, Kind::Audio) } else { Vec::new() },
                audible: seg.volume_db > SILENT_DB || !seg.volume_keys.is_empty(),
            });
        }
    }
    let mut audible: Vec<String> = Vec::new();
    for a in audio.iter().filter(|a| a.audible) {
        if !audible.contains(&a.track) {
            audible.push(a.track.clone());
        }
    }
    Ok(Frame { at, canvas, layers, visible, disabled: Vec::new(), audio, audible, muted: Vec::new() })
}

/// `edit.set_volume`'s floor: a clip at it is silenced.
const SILENT_DB: f64 = -100.0;

fn layer(ctx: &Ctx<'_>, track: &str, track_index: usize, seg: &Segment, at: Q, detail: bool) -> Result<Layer> {
    let (rect, xf, asset, alpha) = match compose::text_picture(seg, at, &ctx.canvas)? {
        // Text is always see-through around its letters.
        Some((_, w, h)) => {
            let (rect, xf) = compose::text_layout(seg, at, (w, h), &ctx.canvas);
            (rect, xf, None, Alpha::Transparent)
        }
        None => {
            let src = *ctx
                .sources
                .get(&seg.path)
                .ok_or_else(|| RenderError::Media(format!("{}: {} has no probed video size", seg.clip, seg.path.display())))?;
            let (rect, xf, _) = layout(seg, at, src, &ctx.canvas);
            let (asset, alpha) = ctx.assets.get(&seg.path).cloned().map_or((None, Alpha::Unmeasured), |(id, a)| (Some(id), a));
            (rect, xf, asset, alpha)
        }
    };
    let effects = if detail { effects_at(ctx, seg, at, Kind::Effect) } else { Vec::new() };
    let tf = if detail { transform_at(seg, at) } else { None };
    let quarter = (f64::from(xf.rotation).to_degrees() / 90.0).round() * 90.0;
    let square = (f64::from(xf.rotation).to_degrees() - quarter).abs() < 1e-3;
    Ok(Layer {
        track: track.to_owned(),
        track_index,
        clip: seg.clip.clone(),
        asset,
        text: seg.text.as_ref().map(|t| t.text.clone()),
        link: ctx.links.get(&seg.clip).cloned(),
        source_time: (!seg.still).then(|| seg.media_start + (at - seg.start) * seg.speed),
        still: seg.still,
        rect: drawn(rect, xf),
        rotation: round3(f64::from(xf.rotation).to_degrees()),
        opacity: round3(f64::from(xf.opacity)),
        transform: tf,
        effects,
        transition: None,
        alpha,
        solid: alpha.is_solid() && xf.opacity >= 1.0 && xf.blend == 0 && seg.effects.is_empty() && square,
        visibility: Visibility::Full,
        visible_percent: 100,
        covered_by: Vec::new(),
        overlapped_by: Vec::new(),
        note: None,
        bbox: quad_box(rect, xf),
    })
}

/// A segment's effects with every param's value at `at`.
fn effects_at(ctx: &Ctx<'_>, seg: &Segment, at: Q, kind: Kind) -> Vec<EffectAt> {
    let local = at - seg.clip_start;
    seg.effects
        .iter()
        .map(|e| {
            let f = ctx.fx.require(&e.effect, kind).ok();
            EffectAt {
                id: e.id.to_string(),
                effect: e.effect.clone(),
                name: f.as_ref().map(|f| f.manifest.name.clone()),
                params: values(f.as_deref(), &e.params, &e.keyframes, local),
                missing: f.is_none(),
            }
        })
        .collect()
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// Param values at local time `t`: keyframes, else the stored value, else
/// the manifest default. Without the manifest, only what the clip stores.
fn values(f: Option<&crate::fx::Fx>, stored: &Map<String, Value>, keys: &indexmap::IndexMap<String, Vec<ve_model::meta::Keyframe>>, t: Q) -> Map<String, Value> {
    let at = |name: &str| keys.get(name).filter(|k| !k.is_empty()).map(|k| interpolate(k, t)).or_else(|| stored.get(name).cloned());
    match f {
        Some(f) => f.params.iter().map(|p| (p.name.clone(), at(&p.name).unwrap_or_else(|| p.default.clone()))).collect(),
        None => stored.keys().chain(keys.keys()).filter_map(|n| Some((n.clone(), at(n)?))).collect(),
    }
}

/// The quad actually drawn: the rectangle with the crop cut away.
fn drawn(r: Rect, xf: Xf) -> Rect {
    let [l, t, rr, b] = xf.crop;
    Rect { x: r.x + l * r.w, y: r.y + t * r.h, w: r.w * (1.0 - l - rr), h: r.h * (1.0 - t - b) }
}

/// An axis-aligned box, `[x0, x1) x [y0, y1)`, in f64 pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Bx {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl Bx {
    fn area(self) -> f64 {
        (self.x1 - self.x0).max(0.0) * (self.y1 - self.y0).max(0.0)
    }

    fn empty(self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }

    fn meet(self, o: Self) -> Self {
        Self { x0: self.x0.max(o.x0), y0: self.y0.max(o.y0), x1: self.x1.min(o.x1), y1: self.y1.min(o.y1) }
    }

    /// `self` minus `o`, as up to four boxes.
    fn minus(self, o: Self) -> Vec<Self> {
        let m = self.meet(o);
        if m.empty() {
            return vec![self];
        }
        [
            Self { y1: m.y0, ..self },
            Self { y0: m.y1, ..self },
            Self { x1: m.x0, y0: m.y0, y1: m.y1, ..self },
            Self { x0: m.x1, y0: m.y0, y1: m.y1, ..self },
        ]
        .into_iter()
        .filter(|b| !b.empty())
        .collect()
    }
}

/// The box a picture can touch: the cropped quad turned around the
/// uncropped rectangle's center (as the GPU draws it), then boxed. Exact at
/// multiples of 90 degrees, larger than the picture otherwise.
fn quad_box(r: Rect, xf: Xf) -> Bx {
    let d = drawn(r, xf);
    let (x0, y0, x1, y1) = (f64::from(d.x), f64::from(d.y), f64::from(d.x + d.w), f64::from(d.y + d.h));
    let (cx, cy) = (f64::from(r.x + r.w / 2.0), f64::from(r.y + r.h / 2.0));
    let (sin, cos) = f64::from(xf.rotation).sin_cos();
    let pts = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)].map(|(x, y)| {
        if xf.rotation == 0.0 {
            return (x, y);
        }
        let (dx, dy) = (x - cx, y - cy);
        (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos)
    });
    // Snap float noise so a quarter-turned layer still meets its neighbours exactly.
    let snap = |v: f64| (v * 1000.0).round() / 1000.0;
    Bx {
        x0: snap(pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min)),
        y0: snap(pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min)),
        x1: snap(pts.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max)),
        y1: snap(pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max)),
    }
}

/// Fill in visibility: how much of each layer the solid layers above hide.
fn cover(layers: &mut [Layer], (cw, ch): (u32, u32)) {
    let canvas = Bx { x0: 0.0, y0: 0.0, x1: f64::from(cw), y1: f64::from(ch) };
    let boxes: Vec<Bx> = layers.iter().map(|l| l.bbox.meet(canvas)).collect();
    for i in 0..layers.len() {
        let mine = boxes[i];
        if mine.empty() {
            layers[i].visibility = Visibility::Hidden;
            layers[i].visible_percent = 0;
            layers[i].note = Some("off the canvas".into());
            continue;
        }
        if layers[i].opacity <= 0.0 {
            layers[i].visibility = Visibility::Hidden;
            layers[i].visible_percent = 0;
            layers[i].note = Some("opacity 0".into());
            continue;
        }
        let mut left = vec![mine];
        let (mut covered, mut over) = (Vec::new(), Vec::new());
        for j in i + 1..layers.len() {
            let above = &layers[j];
            if above.track == layers[i].track || boxes[j].meet(mine).empty() || above.opacity <= 0.0 {
                continue;
            }
            if above.solid {
                let before: f64 = left.iter().map(|b| b.area()).sum();
                left = left.into_iter().flat_map(|b| b.minus(boxes[j])).collect();
                if left.iter().map(|b| b.area()).sum::<f64>() < before && !covered.contains(&above.track) {
                    covered.push(above.track.clone());
                }
            } else if !over.contains(&above.track) {
                over.push(above.track.clone());
            }
        }
        let shown: f64 = left.iter().map(|b| b.area()).sum();
        let l = &mut layers[i];
        let pct = shown / mine.area() * 100.0;
        (l.visibility, l.visible_percent) = if shown <= 0.0 {
            (Visibility::Hidden, 0)
        } else if shown >= mine.area() {
            (Visibility::Full, 100)
        } else {
            (Visibility::Partial, pct.round().clamp(1.0, 99.0) as u8)
        };
        l.covered_by = covered;
        l.overlapped_by = over;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bx(x0: f64, y0: f64, x1: f64, y1: f64) -> Bx {
        Bx { x0, y0, x1, y1 }
    }

    #[test]
    fn box_subtraction() {
        let a = bx(0.0, 0.0, 10.0, 10.0);
        assert_eq!(a.minus(bx(20.0, 20.0, 30.0, 30.0)), vec![a]);
        assert!(a.minus(bx(-1.0, -1.0, 11.0, 11.0)).is_empty());
        let hole: f64 = a.minus(bx(2.0, 2.0, 4.0, 4.0)).iter().map(|b| b.area()).sum();
        assert_eq!(hole, 96.0);
        let half: f64 = a.minus(bx(5.0, -5.0, 15.0, 15.0)).iter().map(|b| b.area()).sum();
        assert_eq!(half, 50.0);
    }
}
