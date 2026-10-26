//! `render.layers`: what is on screen and heard, as data. At one frame,
//! every video layer bottom to top with its placement, effect and transition
//! values and how much of it shows, and every audio clip playing with its
//! gain; over a range, the stretches where the same tracks show and are
//! heard. Nothing is decoded (`ve_render::layers`), so it answers at once.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ve_model::meta::Alpha;
use ve_ops::edit::tc;
use ve_protocol::RpcError;
use ve_render::layers::Visibility;

use crate::Snapshot;
use crate::commit::parse_time;
use crate::render::{render_error, span};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayersParams {
    /// One frame: every layer there in detail. Give this, or a range.
    #[serde(default)]
    pub at: Option<String>,
    /// Range start (instead of `at`); default the timeline start.
    #[serde(default)]
    pub from: Option<String>,
    /// Range end (exclusive); default the timeline end.
    #[serde(default)]
    pub to: Option<String>,
    /// `video` or `audio`: answer for that side only. Default both.
    #[serde(default)]
    pub only: Option<Only>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Only {
    Video,
    Audio,
}

/// `{at}` answers with the frame's layers; a range answers with `spans`.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum LayersResult {
    Frame(FrameLayers),
    Range(RangeLayers),
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CanvasSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct FrameLayers {
    /// Timecode of the frame.
    pub at: String,
    /// The project canvas; `rect`s are in its pixels.
    pub canvas: CanvasSize,
    /// Video tracks that show at least partly, bottom to top (absent with `only: audio`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<Vec<String>>,
    /// Video tracks turned off (never drawn).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub disabled: Vec<String>,
    /// Bottom to top. A track mid-transition has two layers (absent with `only: audio`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layers: Option<Vec<LayerInfo>>,
    /// Audio tracks with a clip that is heard (absent with `only: video`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audible: Option<Vec<String>>,
    /// Audio tracks turned off (never mixed).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub muted: Vec<String>,
    /// Audio clips playing, first audio track first (absent with `only: video`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<Vec<AudioInfo>>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct AudioInfo {
    pub track: String,
    /// Position among audio tracks, 0 first.
    pub track_index: usize,
    pub clip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    /// Link group: the clip's video partner (a layer with the same `link`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// Media time heard (`118.400s`, ms precision).
    pub source_time: String,
    /// The clip's gain in dB (`edit.set_volume`, plus what `edit.normalize` added). 0 is unity.
    pub gain_db: f64,
    /// Playback speed, when not 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    /// Audio effects, in order, with every param's value at this frame.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<EffectAt>,
    /// False when the gain is at the -100 dB floor (silenced).
    pub audible: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct PixelRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LayerInfo {
    pub track: String,
    /// Position among video tracks, 0 at the bottom.
    pub track_index: usize,
    pub clip: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    /// The words, for a text clip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Link group: the clip's audio partner (an `audio` entry with the same `link`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// Media time shown (`118.400s`, ms precision); absent for a still.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_time: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub still: bool,
    /// The drawn picture on the canvas, after crop, before rotation.
    pub rect: PixelRect,
    /// Degrees clockwise, around the uncropped rectangle's center.
    #[serde(skip_serializing_if = "is_zero")]
    pub rotation: f64,
    pub opacity: f64,
    /// The clip's transform at this frame (keyframes evaluated), if it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transform: Option<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<EffectAt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition: Option<TransitionAt>,
    /// The asset's transparency: `none`, `opaque`, `transparent` or `unmeasured`.
    pub alpha: Alpha,
    /// Hides what is under it wherever it is drawn: no see-through pixels,
    /// full opacity, normal blend, no effect, not mid-transition, square to the canvas.
    pub solid: bool,
    /// `full`, `partial` or `hidden`.
    pub visibility: String,
    /// Share of its on-canvas area that shows, 0-100.
    pub visible_percent: u8,
    /// Tracks above whose solid layers hide some of it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub covered_by: Vec<String>,
    /// Tracks above drawn over it that may let it show through.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub overlapped_by: Vec<String>,
    /// Why it is hidden when nothing covers it: `off the canvas`, `opacity 0`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct EffectAt {
    pub id: String,
    /// `id@major`, as stored on the clip.
    pub effect: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Every param's value at this frame (keyframes evaluated, defaults filled in).
    pub params: Map<String, Value>,
    /// Not installed: `params` holds only what the clip stores.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub missing: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TransitionAt {
    pub id: String,
    pub transition: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub params: Map<String, Value>,
    /// 0 at the start of the transition, 1 at the end.
    pub progress: f64,
    /// `outgoing` or `incoming`.
    pub role: String,
    /// The other clip in the transition.
    pub with: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub missing: bool,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RangeLayers {
    pub from: String,
    pub to: String,
    /// Stretches where the same tracks show and are heard, in order.
    /// Stretches with no clip on any track are left out.
    pub spans: Vec<VisibleSpan>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct VisibleSpan {
    pub from: String,
    pub to: String,
    /// Tracks that show, bottom to top (absent with `only: audio`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<Vec<String>>,
    /// Tracks with a clip there that is entirely hidden.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<String>,
    /// Audio tracks heard (absent with `only: video`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audible: Option<Vec<String>>,
}

pub fn layers(p: &Snapshot, a: LayersParams) -> Result<LayersResult, RpcError> {
    let tl = &p.timeline;
    let rate = tl.output_rate();
    let fx = ve_render::fx::Registry::for_project(Some(p.root()));
    let Some(at) = &a.at else {
        let sp = span(tl, a.from.as_ref(), a.to.as_ref())?;
        let mut spans = ve_render::layers::spans(tl, sp, &fx).map_err(render_error)?;
        // One side only: drop the other side, then join stretches that now read the same.
        if let Some(only) = a.only {
            let mut joined: Vec<ve_render::layers::VisibleSpan> = Vec::new();
            for mut s in spans {
                match only {
                    Only::Video => s.audible.clear(),
                    Only::Audio => {
                        s.visible.clear();
                        s.hidden.clear();
                    }
                }
                let empty = s.visible.is_empty() && s.hidden.is_empty() && s.audible.is_empty();
                match joined.last_mut() {
                    Some(l) if l.end == s.start && l.visible == s.visible && l.hidden == s.hidden && l.audible == s.audible => l.end = s.end,
                    _ if empty => {}
                    _ => joined.push(s),
                }
            }
            spans = joined;
        }
        let (video, audio) = (a.only != Some(Only::Audio), a.only != Some(Only::Video));
        return Ok(LayersResult::Range(RangeLayers {
            from: tc(sp.start, rate),
            to: tc(sp.end, rate),
            spans: spans
                .into_iter()
                .map(|s| VisibleSpan {
                    from: tc(s.start, rate),
                    to: tc(s.end, rate),
                    visible: video.then_some(s.visible),
                    hidden: s.hidden,
                    audible: audio.then_some(s.audible),
                })
                .collect(),
        }));
    };
    if a.from.is_some() || a.to.is_some() {
        return Err(RpcError::invalid_params("give `at` for one frame, or `from`/`to` for a range, not both"));
    }
    let f = ve_render::layers::at(tl, parse_time("at", at, rate)?, &fx).map_err(render_error)?;
    let px = |v: f32| (f64::from(v) * 100.0).round() / 100.0;
    let layers = f
        .layers
        .into_iter()
        .map(|l| LayerInfo {
            text: l.text,
            track: l.track,
            track_index: l.track_index,
            clip: l.clip,
            asset: l.asset,
            link: l.link,
            source_time: l.source_time.map(ve_ops::lane::seconds_text),
            still: l.still,
            rect: PixelRect { x: px(l.rect.x), y: px(l.rect.y), w: px(l.rect.w), h: px(l.rect.h) },
            rotation: l.rotation,
            opacity: l.opacity,
            transform: l.transform.map(|mut t| {
                t.keyframes.clear();
                serde_json::to_value(t).unwrap_or_default()
            }),
            effects: l
                .effects
                .into_iter()
                .map(|e| EffectAt { id: e.id, effect: e.effect, name: e.name, params: e.params, missing: e.missing })
                .collect(),
            transition: l.transition.map(|t| TransitionAt {
                id: t.id,
                transition: t.transition,
                name: t.name,
                params: t.params,
                progress: (t.progress * 1000.0).round() / 1000.0,
                role: t.role.into(),
                with: t.with,
                missing: t.missing,
            }),
            alpha: l.alpha,
            solid: l.solid,
            visibility: match l.visibility {
                Visibility::Full => "full",
                Visibility::Partial => "partial",
                Visibility::Hidden => "hidden",
            }
            .into(),
            visible_percent: l.visible_percent,
            covered_by: l.covered_by,
            overlapped_by: l.overlapped_by,
            note: l.note,
        })
        .collect();
    let (video, audio) = (a.only != Some(Only::Audio), a.only != Some(Only::Video));
    Ok(LayersResult::Frame(FrameLayers {
        at: tc(f.at, rate),
        canvas: CanvasSize { width: f.canvas.0, height: f.canvas.1 },
        visible: video.then_some(f.visible),
        disabled: if video { f.disabled } else { Vec::new() },
        layers: video.then_some(layers),
        audible: audio.then_some(f.audible),
        muted: if audio { f.muted } else { Vec::new() },
        audio: audio.then(|| {
            f
            .audio
            .into_iter()
            .map(|a| AudioInfo {
                track: a.track,
                track_index: a.track_index,
                clip: a.clip,
                asset: a.asset,
                link: a.link,
                source_time: ve_ops::lane::seconds_text(a.source_time),
                gain_db: a.gain_db,
                speed: (a.speed != ve_model::time::Q::from_integer(1)).then(|| *a.speed.numer() as f64 / *a.speed.denom() as f64),
                effects: a.effects.into_iter().map(|e| EffectAt { id: e.id, effect: e.effect, name: e.name, params: e.params, missing: e.missing }).collect(),
                audible: a.audible,
            })
            .collect()
        }),
    }))
}
