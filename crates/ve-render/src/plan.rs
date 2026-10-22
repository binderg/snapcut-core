//! Resolver: which media plays where in a timeline range.
//!
//! The output is a list of segments per track, in exact timeline seconds,
//! each with the media position it starts at, its speed and gain. The graph
//! compiler turns that into frame and sample counts; nothing here rounds.

use std::path::PathBuf;

use ve_model::meta::{EffectInstance, TransitionMeta};
use ve_model::otio::{Clip, Composable, MediaReference, Timeline, Track, TrackKind};
use ve_model::time::{Q, Rate};
use ve_ops::lane::{Lane, clip_id, clip_speed, media_bounds};

use crate::RenderError;

/// A timeline range `[start, end)` in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: Q,
    pub end: Q,
}

impl Span {
    pub fn new(start: Q, end: Q) -> Self {
        Self { start, end }
    }

    pub fn duration(&self) -> Q {
        self.end - self.start
    }
}

/// What sits next to a segment edge on its track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    /// The render range cuts through the clip here.
    RangeEdge,
    /// Nothing (a gap or the timeline start/end).
    Open,
    /// Another clip starts or ends exactly here: a cut.
    Cut,
}

/// One clip's part of the render range.
#[derive(Debug, Clone)]
pub struct Segment {
    pub clip: String,
    pub path: PathBuf,
    /// Timeline seconds, clipped to the range.
    pub start: Q,
    pub end: Q,
    /// Media seconds at `start`.
    pub media_start: Q,
    pub speed: Q,
    pub volume_db: f64,
    /// Frame rate of the source video (conform target for VFR footage).
    pub media_rate: Rate,
    /// Media seconds the source has, if known.
    pub bounds: Option<(Q, Q)>,
    pub head: Edge,
    pub tail: Edge,
    /// The whole clip's timeline start and length (effect time and progress).
    pub clip_start: Q,
    pub clip_duration: Q,
    /// The clip's effect stack (`metadata.ve.effects`).
    pub effects: Vec<EffectInstance>,
}

impl Segment {
    /// Media seconds at `end`.
    pub fn media_end(&self) -> Q {
        self.media_start + (self.end - self.start) * self.speed
    }
}

/// A transition on a track, with both clips extended into it: the outgoing
/// clip plays past its out point, the incoming clip before its in point.
#[derive(Debug, Clone)]
pub struct TransitionPlan {
    pub meta: TransitionMeta,
    /// Timeline range it covers (not clipped to the render range).
    pub start: Q,
    pub end: Q,
    /// Segments covering the part of `[start, end)` inside the render range.
    pub outgoing: Segment,
    pub incoming: Segment,
}

#[derive(Debug, Clone)]
pub struct TrackPlan {
    pub name: String,
    pub segments: Vec<Segment>,
    pub transitions: Vec<TransitionPlan>,
}

/// Prefix of warnings about what only the GPU compositor renders.
pub const GPU_ONLY: &str = "only the GPU compositor renders";

#[derive(Debug, Clone)]
pub struct Plan {
    pub span: Span,
    /// Bottom to top.
    pub video: Vec<TrackPlan>,
    pub audio: Vec<TrackPlan>,
    pub warnings: Vec<String>,
}

/// End of the longest track, in seconds.
pub fn timeline_end(tl: &Timeline) -> Q {
    tl.tracks
        .children
        .iter()
        .filter(|c| matches!(c, Composable::Track(_)))
        .map(ve_ops::lane::item_seconds)
        .max()
        .unwrap_or(Q::from_integer(0))
}

/// Local path for a media URL. Phase 1 reads `file://` only.
pub fn local_path(url: &str) -> Option<PathBuf> {
    url::Url::parse(url).ok().filter(|u| u.scheme() == "file").and_then(|u| u.to_file_path().ok())
}

pub fn plan(tl: &Timeline, span: Span) -> Result<Plan, RenderError> {
    if span.end <= span.start {
        return Err(RenderError::Invalid("the render range is empty".into()));
    }
    let mut out = Plan { span, video: Vec::new(), audio: Vec::new(), warnings: Vec::new() };
    let mut transitions = 0;
    let mut effects = 0;
    for c in &tl.tracks.children {
        let Composable::Track(track) = c else {
            out.warnings.push("a top-level item that is not a track was skipped".into());
            continue;
        };
        if !track.enabled {
            continue;
        }
        let lane = Lane::from_track(track).map_err(|e| RenderError::Invalid(e.0))?;
        let mut segs: Vec<Segment> = Vec::new();
        let mut prev_clip_end: Option<Q> = None;
        for (i, p) in lane.items.iter().enumerate() {
            let (start, end) = (p.start, p.end());
            let clip = match &p.item {
                Composable::Clip(c) if c.enabled => c,
                Composable::Transition(_) => continue,
                Composable::Track(_) | Composable::Stack(_) if start < span.end && end > span.start => {
                    out.warnings.push(format!("a nested composition on track {} renders as a gap", track.name));
                    continue;
                }
                _ => continue,
            };
            let next_is_clip_at_end = lane.items[i + 1..]
                .iter()
                .find(|n| n.duration() > Q::from_integer(0))
                .is_some_and(|n| n.start == end && matches!(&n.item, Composable::Clip(c) if c.enabled));
            let head_is_cut = prev_clip_end == Some(start);
            prev_clip_end = Some(end);
            if end <= span.start || start >= span.end {
                continue;
            }
            let (s, e) = (start.max(span.start), end.min(span.end));
            let head = if s > start { Edge::RangeEdge } else if head_is_cut { Edge::Cut } else { Edge::Open };
            let tail = if e < end { Edge::RangeEdge } else if next_is_clip_at_end { Edge::Cut } else { Edge::Open };
            effects += clip.metadata.ve.effects.len();
            if let Some(seg) = segment(tl, track, clip, (start, end), (s, e), (head, tail), &mut out.warnings)? {
                segs.push(seg);
            }
        }
        // Transitions: both neighbours, extended into the transition's range.
        let mut trans = Vec::new();
        for (i, p) in lane.items.iter().enumerate() {
            let Composable::Transition(tr) = &p.item else { continue };
            let (Some(a), Some(b)) = (i.checked_sub(1).and_then(|j| lane.items.get(j)), lane.items.get(i + 1)) else { continue };
            let (Composable::Clip(ca), Composable::Clip(cb)) = (&a.item, &b.item) else { continue };
            let (w0, w1) = (p.start - tr.in_offset.seconds(), p.start + tr.out_offset.seconds());
            let (s, e) = (w0.max(span.start), w1.min(span.end));
            if track.kind != TrackKind::Video || s >= e || !ca.enabled || !cb.enabled {
                continue;
            }
            let Some(meta) = tr.metadata.ve.transition.clone() else {
                out.warnings.push(format!("a transition on {} has no engine data and renders as a cut", track.name));
                continue;
            };
            let edges = (Edge::RangeEdge, Edge::RangeEdge);
            let (Some(o), Some(n)) = (
                segment(tl, track, ca, (a.start, a.end()), (s, e), edges, &mut out.warnings)?,
                segment(tl, track, cb, (b.start, b.end()), (s, e), edges, &mut out.warnings)?,
            ) else {
                continue;
            };
            trans.push(TransitionPlan { meta, start: w0, end: w1, outgoing: o, incoming: n });
        }
        transitions += trans.len();
        let t = TrackPlan { name: track.name.clone(), segments: segs, transitions: trans };
        match track.kind {
            TrackKind::Video => out.video.push(t),
            TrackKind::Audio => out.audio.push(t),
        }
    }
    if transitions + effects > 0 {
        out.warnings.push(format!(
            "{transitions} transition(s) and {effects} effect(s) in range: {GPU_ONLY} them (frames with VE_RENDERER=gpu); this render shows plain cuts and unprocessed clips"
        ));
    }
    Ok(out)
}

/// The part `[s, e)` of `clip` (which spans `clip_range` on the timeline) as
/// a segment. `s` may lie before the clip's start or `e` after its end (a
/// transition plays into the clip's handles). `None` when the track kind
/// and the media do not match (with a warning).
fn segment(
    tl: &Timeline,
    track: &Track,
    clip: &Clip,
    (start, end): (Q, Q),
    (s, e): (Q, Q),
    (head, tail): (Edge, Edge),
    warnings: &mut Vec<String>,
) -> Result<Option<Segment>, RenderError> {
    let id = clip_id(clip).map_or_else(|| clip.name.clone(), ToString::to_string);
    let url = match clip.active_media() {
        Some(MediaReference::External(e)) => e.target_url.clone(),
        _ => return Err(RenderError::Media(format!("{id} has no media (missing reference)"))),
    };
    let path = local_path(&url).ok_or_else(|| RenderError::Media(format!("{id}: only file:// media can render, not {url}")))?;
    if !path.is_file() {
        return Err(RenderError::Media(format!("{id}: media file {} is missing", path.display())));
    }
    let asset = tl.metadata.ve.assets.iter().find(|a| a.url == url).and_then(|a| a.media.as_ref());
    if track.kind == TrackKind::Audio && asset.is_some_and(|m| m.audio.is_none()) {
        warnings.push(format!("{id} is on audio track {} but its media has no sound", track.name));
        return Ok(None);
    }
    if track.kind == TrackKind::Video && asset.is_some_and(|m| m.video.is_none()) {
        warnings.push(format!("{id} is on video track {} but its media has no picture", track.name));
        return Ok(None);
    }
    let src = clip.source_range.expect("lanes fill in source ranges");
    let speed = clip_speed(clip);
    Ok(Some(Segment {
        clip: id,
        path,
        start: s,
        end: e,
        media_start: src.start_time.seconds() + (s - start) * speed,
        speed,
        volume_db: clip.metadata.ve.clip.as_ref().map_or(0.0, |m| m.volume_db),
        media_rate: asset.and_then(|m| m.video.as_ref()).map_or(src.start_time.rate, |v| v.rate),
        bounds: media_bounds(clip),
        head,
        tail,
        clip_start: start,
        clip_duration: end - start,
        effects: clip.metadata.ve.effects.clone(),
    }))
}
