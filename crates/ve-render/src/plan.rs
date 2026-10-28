//! Resolver: which media plays where in a timeline range.
//!
//! The output is a list of segments per track, in exact timeline seconds,
//! each with the media position it starts at, its speed and gain. The graph
//! compiler turns that into frame and sample counts; nothing here rounds.

use std::path::PathBuf;

use ve_model::meta::{ChannelMode, Channels, EffectInstance, Transform, TransitionMeta};
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
    /// Audio stream of the file (`a:N`). 0 for video segments.
    pub stream: u32,
    /// How the stream's channels reach stereo: `mono`, `left`, `[0, 2]`, `auto`
    /// (`metadata.ve.clip.audio`). The filter for it is [`channel_filter`].
    pub channels: Channels,
    /// The stream's channel count and whether its layout is a standard one.
    pub source_channels: (u32, bool),
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
    /// Placement on the canvas (`metadata.ve.clip.transform`), if not the default.
    pub transform: Option<Transform>,
    /// A still image: the same picture for the whole segment, possibly with
    /// transparency. `media_rate` is the project rate.
    pub still: bool,
    /// A text clip: drawn by [`crate::text`], not read from a file (`path` is empty).
    pub text: Option<std::sync::Arc<ve_model::text::TextClip>>,
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
    /// Transitions, effects and transforms in range: what only the GPU compositor renders.
    pub gpu_features: usize,
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
    let mut out = Plan { span, video: Vec::new(), audio: Vec::new(), warnings: Vec::new(), gpu_features: 0 };
    let mut transitions = 0;
    let mut effects = 0;
    let mut transforms = 0;
    let mut texts = 0;
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
            // Audio effects render in the ffmpeg audio graph; only video ones need the GPU.
            if track.kind == TrackKind::Video {
                effects += clip.metadata.ve.effects.len();
            }
            if let Some(seg) = segment(tl, track, clip, (start, end), (s, e), (head, tail), &mut out.warnings)? {
                // Only where the transform changes the picture: a fade at a
                // clip's ends must not send the rest of a long clip to the
                // (much slower) GPU path.
                let moves = seg.transform.as_ref().is_some_and(|t| changes_picture(t, seg.start - seg.clip_start, seg.end - seg.clip_start));
                transforms += usize::from(track.kind == TrackKind::Video && moves);
                texts += usize::from(seg.text.is_some());
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
    out.gpu_features = transitions + effects + transforms + texts;
    if texts > 0 {
        out.warnings.push(format!("{texts} text clip(s) in range: {GPU_ONLY} them; this render leaves them out"));
    }
    if transitions + effects > 0 {
        out.warnings.push(format!(
            "{transitions} transition(s) and {effects} effect(s) in range: {GPU_ONLY} them; this render shows plain cuts and unprocessed clips"
        ));
    }
    if transforms > 0 {
        out.warnings.push(format!(
            "{transforms} clip(s) with a transform (position, scale, rotation, opacity or crop) in range: {GPU_ONLY} them; this render shows them fitted and centered"
        ));
    }
    Ok(out)
}

/// Whether `t` draws the picture any differently from the default placement
/// at some time in `[from, to)` (seconds from the clip's start). A fade that
/// is fully opaque between its keys over the range does not: the ffmpeg path
/// draws those frames exactly as the GPU would.
///
/// Conservative with keyframes: a field counts as unchanged only when every
/// key that shapes the curve over the range (the one at or before `from`,
/// those inside, the one at or after `to`) holds the default value and none
/// of them has a graph-editor handle (a handle can bulge between equal keys).
pub(crate) fn changes_picture(t: &Transform, from: Q, to: Q) -> bool {
    let mut base = t.clone();
    base.keyframes.clear();
    for (field, keys) in &t.keyframes {
        if keys.is_empty() {
            continue;
        }
        let Some(default) = Transform::default_of(field) else { return true };
        // Keyframes override the field's own value.
        base.set(field, default);
        let mut keys: Vec<_> = keys.iter().collect();
        keys.sort_by_key(|k| k.t.seconds());
        let lo = keys.iter().rposition(|k| k.t.seconds() <= from).unwrap_or(0);
        let hi = keys.iter().position(|k| k.t.seconds() >= to).unwrap_or(keys.len() - 1).max(lo);
        let shaping = &keys[lo..=hi];
        let at_default = shaping.iter().all(|k| k.v.as_f64().is_some_and(|v| (v - default).abs() <= 1e-9));
        let no_handles = shaping.windows(2).all(|w| w[0].out.is_none() && w[1].r#in.is_none());
        if !(at_default && no_handles) {
            return true;
        }
    }
    !base.is_identity()
}

/// The ffmpeg filter that takes a stream with `n` channels (`known`: a
/// standard layout) to stereo the way `channels` says.
pub fn channel_filter(channels: &Channels, (n, known): (u32, bool)) -> String {
    let pan = |l: u32, r: u32| format!("pan=stereo|c0=c{l}|c1=c{r}");
    match channels {
        Channels::Mode(ChannelMode::Auto) if known || n == 0 => "aformat=channel_layouts=stereo".to_owned(),
        Channels::Mode(ChannelMode::Auto) => pan(0, u32::from(n > 1)),
        Channels::Mode(ChannelMode::Left) => pan(0, 0),
        Channels::Mode(ChannelMode::Right) => pan(1, 1),
        Channels::Mode(ChannelMode::Mono) => {
            let n = n.max(1);
            let sum: Vec<String> = (0..n).map(|i| format!("{:.6}*c{i}", 1.0 / f64::from(n))).collect();
            let sum = sum.join("+");
            format!("pan=stereo|c0={sum}|c1={sum}")
        }
        Channels::Pick(v) => pan(v[0], *v.get(1).unwrap_or(&v[0])),
    }
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
    if let Some(text) = clip.text() {
        // Text has no sound; on an audio track it plays nothing.
        if track.kind == TrackKind::Audio {
            return Ok(None);
        }
        let text = text.map_err(|e| RenderError::Invalid(format!("{id}: {e}")))?;
        let src = clip.source_range.expect("lanes fill in source ranges");
        return Ok(Some(Segment {
            clip: id,
            path: PathBuf::new(),
            start: s,
            end: e,
            media_start: src.start_time.seconds() + (s - start),
            speed: Q::from_integer(1),
            volume_db: 0.0,
            stream: 0,
            channels: Channels::default(),
            source_channels: (2, true),
            media_rate: tl.output_rate(),
            bounds: None,
            head,
            tail,
            clip_start: start,
            clip_duration: end - start,
            effects: clip.metadata.ve.effects.clone(),
            transform: clip.metadata.ve.clip.as_ref().and_then(|m| m.transform.as_deref().cloned()).filter(|t| !t.is_identity()),
            still: true,
            text: Some(std::sync::Arc::new(text)),
        }));
    }
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
    let pick = clip.metadata.ve.clip.as_ref().and_then(|m| m.audio.clone()).unwrap_or_default();
    let mut source_channels = (2, true);
    if track.kind == TrackKind::Audio
        && let Some(m) = asset
    {
        let Some(st) = m.audio_stream(pick.stream) else {
            warnings.push(format!("{id} plays audio stream {} but its media has {} stream(s); it is silent", pick.stream, m.audio_stream_count()));
            return Ok(None);
        };
        if let Err(e) = pick.channels.check(st.channels) {
            warnings.push(format!("{id}: {e}; it is silent"));
            return Ok(None);
        }
        if pick.channels.is_auto() && !st.layout_known() && st.channels > 2 {
            warnings.push(format!(
                "{id}: stream {} has {} channels with no standard layout; playing channels 0 and 1 (choose others with `ve audio {id} --channels`)",
                pick.stream, st.channels
            ));
        }
        source_channels = (st.channels, st.layout_known());
    }
    let src = clip.source_range.expect("lanes fill in source ranges");
    let speed = clip_speed(clip);
    let still = asset.and_then(|m| m.video.as_ref()).is_some_and(|v| v.still);
    Ok(Some(Segment {
        clip: id,
        path,
        start: s,
        end: e,
        media_start: src.start_time.seconds() + (s - start) * speed,
        speed,
        volume_db: clip.metadata.ve.clip.as_ref().map_or(0.0, |m| m.volume_db),
        stream: pick.stream,
        channels: pick.channels,
        source_channels,
        media_rate: if still { tl.output_rate() } else { asset.and_then(|m| m.video.as_ref()).map_or(src.start_time.rate, |v| v.rate) },
        bounds: media_bounds(clip),
        head,
        tail,
        clip_start: start,
        clip_duration: end - start,
        effects: clip.metadata.ve.effects.clone(),
        transform: clip.metadata.ve.clip.as_ref().and_then(|m| m.transform.as_deref().cloned()).filter(|t| !t.is_identity()),
        still,
        text: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn key(secs: i64, v: f64) -> serde_json::Value {
        json!({"t": {"OTIO_SCHEMA": "RationalTime.1", "rate": 1.0, "value": secs}, "v": v})
    }

    fn transform(v: serde_json::Value) -> Transform {
        serde_json::from_value(v).unwrap()
    }

    fn s(n: i64) -> Q {
        Q::from_integer(n)
    }

    /// A one-second fade in and out on a 100 s clip: only the ends differ.
    #[test]
    fn fades_change_only_the_ends() {
        let t = transform(json!({"keyframes": {"opacity": [key(0, 0.0), key(1, 1.0), key(99, 1.0), key(100, 0.0)]}}));
        assert!(changes_picture(&t, s(0), s(4)));
        assert!(!changes_picture(&t, s(1), s(5)));
        assert!(!changes_picture(&t, s(40), s(44)));
        assert!(!changes_picture(&t, s(95), s(99)));
        assert!(changes_picture(&t, s(96), s(100)));
    }

    #[test]
    fn static_values_and_handles_count() {
        assert!(changes_picture(&transform(json!({"x": 0.02})), s(10), s(14)));
        assert!(changes_picture(&transform(json!({"blend": "screen"})), s(10), s(14)));
        assert!(!changes_picture(&transform(json!({"opacity": 0.5, "keyframes": {"opacity": [key(0, 1.0)]}})), s(10), s(14)));
        // Equal keys with a graph handle may bulge in between.
        let mut a = key(0, 1.0);
        a["out"] = json!({"x": 0.3, "y": -0.5});
        let t = transform(json!({"keyframes": {"opacity": [a, key(20, 1.0)]}}));
        assert!(changes_picture(&t, s(5), s(9)));
        // Past the last key the value holds.
        let t = transform(json!({"keyframes": {"scale": [key(0, 2.0), key(5, 1.0)]}}));
        assert!(!changes_picture(&t, s(10), s(14)));
        assert!(changes_picture(&t, s(3), s(7)));
    }
}
