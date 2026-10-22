//! Clip effects and transitions between clips, as document edits.
//!
//! These functions only change the document. Whether an effect id exists and
//! whether its params fit its schema is checked by the caller against the
//! effect registry (`ve-render::fx`), which needs to read effect files.
//!
//! - A clip's effects are `metadata.ve.effects`, applied in order. Keyframe
//!   times count from the clip's start on the timeline.
//! - A transition is an OTIO `Transition` between two touching clips on a
//!   video track. It reaches `in_offset` before the cut and `out_offset`
//!   after it, so the outgoing clip must have media past its out point and
//!   the incoming clip media before its in point (handles).

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Map, Value};
use ve_model::meta::{EffectInstance, Keyframe, TransitionMeta};
use ve_model::otio::{Clip, Composable, Timeline, Track, TrackKind, Transition};
use ve_model::time::{Q, RationalTime};
use ve_model::Id;

use crate::EditError;
use crate::edit::tc;
use crate::lane::{Ids, at_rate, clip_id, clip_speed, item_seconds, media_bounds, seconds_text};

type Result<T> = std::result::Result<T, EditError>;

/// One effect as the commands report it.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct EffectPlace {
    pub id: Id,
    /// Effect reference, e.g. `core.color/desaturate@1`.
    pub effect: String,
    #[serde(skip_serializing_if = "Map::is_empty")]
    pub params: Map<String, Value>,
    /// Params with keyframes.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub keyframed: Vec<String>,
}

/// What an effect command did.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct EffectReport {
    pub clip: Id,
    /// The effect added, changed or removed.
    pub effect: Id,
    /// The clip's effect stack afterwards, in order.
    pub effects: Vec<EffectPlace>,
}

fn place(e: &EffectInstance) -> EffectPlace {
    EffectPlace {
        id: e.id.clone(),
        effect: e.effect.clone(),
        params: e.params.clone(),
        keyframed: e.keyframes.iter().filter(|(_, k)| !k.is_empty()).map(|(n, _)| n.clone()).collect(),
    }
}

/// Every top-level clip of every track, mutably.
fn clips_mut(tl: &mut Timeline) -> impl Iterator<Item = (TrackKind, &mut Clip)> {
    tl.tracks.children.iter_mut().filter_map(|c| match c {
        Composable::Track(t) => Some(t),
        _ => None,
    }).flat_map(|t| {
        let kind = t.kind;
        t.children.iter_mut().filter_map(move |c| match c {
            Composable::Clip(c) => Some((kind, c)),
            _ => None,
        })
    })
}

fn clip_mut<'a>(tl: &'a mut Timeline, id: &Id) -> Result<(TrackKind, &'a mut Clip)> {
    clips_mut(tl).find(|(_, c)| clip_id(c) == Some(id)).ok_or_else(|| EditError::new(format!("no clip `{id}`")))
}

/// The clip holding effect `fx`.
fn owner_mut<'a>(tl: &'a mut Timeline, fx: &Id) -> Result<&'a mut Clip> {
    clips_mut(tl)
        .map(|(_, c)| c)
        .find(|c| c.metadata.ve.effects.iter().any(|e| &e.id == fx))
        .ok_or_else(|| EditError::new(format!("no effect `{fx}` on any clip")))
}

fn report(c: &Clip, fx: Id) -> EffectReport {
    EffectReport {
        clip: clip_id(c).cloned().unwrap_or_else(|| Id::from_raw("-")),
        effect: fx,
        effects: c.metadata.ve.effects.iter().map(place).collect(),
    }
}

/// Add effect `reference` to a video clip, at `index` in its stack (default: last).
pub fn add(tl: &Timeline, clip: &Id, reference: &str, params: Map<String, Value>, index: Option<usize>) -> Result<(Timeline, EffectReport)> {
    let mut next = tl.clone();
    let id = Ids::new(tl.ids()).fresh("fx");
    let (kind, c) = clip_mut(&mut next, clip)?;
    if kind != TrackKind::Video {
        return Err(EditError::new(format!("{clip} is an audio clip; effects apply to video clips (audio effects come later)")));
    }
    let n = c.metadata.ve.effects.len();
    let at = index.unwrap_or(n);
    if at > n {
        return Err(EditError::new(format!("index {at} is past the end of {clip}'s {n} effect(s)")));
    }
    c.metadata.ve.effects.insert(at, EffectInstance { id: id.clone(), effect: reference.to_owned(), params, keyframes: IndexMap::new() });
    let r = report(c, id);
    Ok((next, r))
}

/// Change an effect: merge `params` (a `null` value removes a param, so it
/// falls back to its default), replace the keyframes of each param named in
/// `keyframes` (an empty list removes them), and move it to `index`.
pub fn set(
    tl: &Timeline,
    fx: &Id,
    params: Option<&Map<String, Value>>,
    keyframes: Option<&IndexMap<String, Vec<Keyframe>>>,
    index: Option<usize>,
) -> Result<(Timeline, EffectReport)> {
    let mut next = tl.clone();
    let c = owner_mut(&mut next, fx)?;
    let i = c.metadata.ve.effects.iter().position(|e| &e.id == fx).expect("owner has it");
    {
        let e = &mut c.metadata.ve.effects[i];
        for (k, v) in params.into_iter().flatten() {
            if v.is_null() {
                e.params.remove(k);
            } else {
                e.params.insert(k.clone(), v.clone());
            }
        }
        for (k, keys) in keyframes.into_iter().flatten() {
            if keys.is_empty() {
                e.keyframes.shift_remove(k);
            } else {
                let mut keys = keys.clone();
                keys.sort_by_key(|k| k.t);
                e.keyframes.insert(k.clone(), keys);
            }
        }
    }
    if let Some(to) = index {
        let n = c.metadata.ve.effects.len();
        if to >= n {
            return Err(EditError::new(format!("index {to} is past the end; the clip has {n} effect(s) (0-{})", n - 1)));
        }
        let e = c.metadata.ve.effects.remove(i);
        c.metadata.ve.effects.insert(to, e);
    }
    let r = report(c, fx.clone());
    Ok((next, r))
}

/// The effect with this id, for the caller to check params against.
pub fn get<'a>(tl: &'a Timeline, fx: &Id) -> Option<&'a EffectInstance> {
    tl.tracks
        .children
        .iter()
        .filter_map(|c| match c {
            Composable::Track(t) => Some(t),
            _ => None,
        })
        .flat_map(|t| t.children.iter())
        .filter_map(|c| match c {
            Composable::Clip(c) => Some(c),
            _ => None,
        })
        .flat_map(|c| c.metadata.ve.effects.iter())
        .find(|e| &e.id == fx)
}

pub fn remove(tl: &Timeline, fx: &Id) -> Result<(Timeline, EffectReport)> {
    let mut next = tl.clone();
    let c = owner_mut(&mut next, fx)?;
    c.metadata.ve.effects.retain(|e| &e.id != fx);
    let r = report(c, fx.clone());
    Ok((next, r))
}

// --- transitions ---

/// Where a transition sits relative to its cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    /// Half before the cut, half after (default).
    #[default]
    Center,
    /// Starts at the cut: all of it over the incoming clip.
    Start,
    /// Ends at the cut: all of it over the outgoing clip.
    End,
}

/// What a transition command did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TransitionReport {
    pub id: Id,
    /// Transition reference, e.g. `core.transition/dissolve@1`.
    pub transition: String,
    pub track: String,
    /// Timeline range it covers, as timecode.
    pub start: String,
    pub end: String,
    /// The cut it sits on.
    pub cut: String,
    pub outgoing: Id,
    pub incoming: Id,
}

/// A new transition.
#[derive(Debug, Clone)]
pub struct NewTransition {
    /// The outgoing clip (the transition goes on its end), or
    pub clip: Option<Id>,
    /// the time of a cut, with an optional track (else the first video track with a cut there).
    pub at: Option<Q>,
    pub track: Option<String>,
    pub duration: Q,
    pub align: Align,
    pub transition: String,
    pub params: Map<String, Value>,
}

fn tracks(tl: &Timeline) -> impl Iterator<Item = (usize, &Track)> {
    tl.tracks.children.iter().enumerate().filter_map(|(i, c)| match c {
        Composable::Track(t) => Some((i, t)),
        _ => None,
    })
}

/// `(track index, child index of the outgoing clip)` for a cut.
fn find_cut(tl: &Timeline, p: &NewTransition) -> Result<(usize, usize)> {
    let touching = |t: &Track, i: usize| -> bool {
        matches!(t.children.get(i), Some(Composable::Clip(_))) && matches!(t.children.get(i + 1), Some(Composable::Clip(_)))
    };
    if let Some(clip) = &p.clip {
        for (ti, t) in tracks(tl) {
            if let Some(i) = t.children.iter().position(|c| matches!(c, Composable::Clip(c) if clip_id(c) == Some(clip))) {
                if t.kind != TrackKind::Video {
                    return Err(EditError::new(format!("{clip} is on audio track {}; audio cuts already get a short crossfade, transitions are for video", t.name)));
                }
                if matches!(t.children.get(i + 1), Some(Composable::Transition(_))) {
                    return Err(EditError::new(format!("the cut after {clip} already has a transition; change it with transition.set")));
                }
                if !touching(t, i) {
                    return Err(EditError::new(format!("{clip} is not followed directly by another clip on {}; a transition needs two touching clips", t.name)));
                }
                return Ok((ti, i));
            }
        }
        return Err(EditError::new(format!("no clip `{clip}`")));
    }
    let at = p.at.ok_or_else(|| EditError::new("give the outgoing clip, or the time of a cut"))?;
    let rate = tl.output_rate();
    for (ti, t) in tracks(tl) {
        if t.kind != TrackKind::Video || p.track.as_ref().is_some_and(|r| &t.name != r && t.metadata.ve.track.as_ref().is_none_or(|m| m.id.as_str() != r)) {
            continue;
        }
        let mut pos = Q::from_integer(0);
        for (i, c) in t.children.iter().enumerate() {
            pos += item_seconds(c);
            if pos == at && touching(t, i) {
                return Ok((ti, i));
            }
            if pos == at && matches!(t.children.get(i + 1), Some(Composable::Transition(_))) {
                return Err(EditError::new(format!("the cut at {} on {} already has a transition; change it with transition.set", tc(at, rate), t.name)));
            }
        }
    }
    let on = p.track.as_ref().map_or_else(|| "any video track".to_owned(), |t| format!("track {t}"));
    Err(EditError::new(format!("no cut between two clips at {} on {on}", tc(at, rate))))
}

/// `(in_offset, out_offset)` in seconds, snapped to frames.
fn offsets(tl: &Timeline, duration: Q, align: Align) -> Result<(Q, Q)> {
    let r = tl.output_rate().as_ratio();
    let d = (duration * r).round() / r;
    if d <= Q::from_integer(0) {
        return Err(EditError::new("a transition needs a duration of at least one frame"));
    }
    Ok(match align {
        Align::Center => {
            let half = (d / Q::from_integer(2) * r).floor() / r;
            (half, d - half)
        }
        Align::Start => (Q::from_integer(0), d),
        Align::End => (d, Q::from_integer(0)),
    })
}

fn clip_at(t: &Track, i: usize) -> &Clip {
    match &t.children[i] {
        Composable::Clip(c) => c,
        _ => unreachable!("checked to be a clip"),
    }
}

/// Check that both clips can play into the transition and build its report.
fn check(tl: &Timeline, ti: usize, i: usize, inn: Q, out: Q, tr: &TransitionMeta) -> Result<TransitionReport> {
    let Composable::Track(t) = &tl.tracks.children[ti] else { unreachable!() };
    // Children are [.., outgoing (i), transition (i + 1), incoming (i + 2), ..].
    let (a, b) = (clip_at(t, i), clip_at(t, i + 2));
    let (a_dur, b_dur) = (item_seconds(&t.children[i]), item_seconds(&t.children[i + 2]));
    let (aid, bid) = (clip_id(a).cloned().unwrap_or_else(|| Id::from_raw("-")), clip_id(b).cloned().unwrap_or_else(|| Id::from_raw("-")));
    if inn > a_dur {
        return Err(EditError::new(format!("the transition reaches {} before the cut but {aid} is only {} long", seconds_text(inn), seconds_text(a_dur))));
    }
    if out > b_dur {
        return Err(EditError::new(format!("the transition reaches {} after the cut but {bid} is only {} long", seconds_text(out), seconds_text(b_dur))));
    }
    // Handles: media past the outgoing clip's end, and before the incoming clip's start.
    if let (Some((_, hi)), Some(r)) = (media_bounds(a), a.source_range) {
        let end = r.start_time.seconds() + r.duration.seconds() * clip_speed(a);
        let need = out * clip_speed(a);
        if end + need > hi {
            return Err(EditError::new(format!(
                "{aid} needs {} of media after its out point for this transition but its source has {}; shorten the transition, align it to end at the cut (align: end), or trim {aid} shorter",
                seconds_text(need),
                seconds_text((hi - end).max(Q::from_integer(0)))
            )));
        }
    }
    if let (Some((lo, _)), Some(r)) = (media_bounds(b), b.source_range) {
        let need = inn * clip_speed(b);
        let start = r.start_time.seconds();
        if start - need < lo {
            return Err(EditError::new(format!(
                "{bid} needs {} of media before its in point for this transition but its source has {}; shorten the transition, align it to start at the cut (align: start), or trim {bid}'s start later",
                seconds_text(need),
                seconds_text((start - lo).max(Q::from_integer(0)))
            )));
        }
    }
    let cut: Q = t.children[..=i].iter().map(item_seconds).sum();
    let rate = tl.output_rate();
    Ok(TransitionReport {
        id: tr.id.clone(),
        transition: tr.transition.clone(),
        track: t.name.clone(),
        start: tc(cut - inn, rate),
        end: tc(cut + out, rate),
        cut: tc(cut, rate),
        outgoing: aid,
        incoming: bid,
    })
}

fn transition_type(reference: &str) -> String {
    if reference.starts_with("core.transition/dissolve") { "SMPTE_Dissolve".into() } else { "Custom_Transition".into() }
}

/// Put a transition on a cut between two touching clips on a video track.
pub fn add_transition(tl: &Timeline, p: &NewTransition) -> Result<(Timeline, TransitionReport)> {
    let (ti, i) = find_cut(tl, p)?;
    let (inn, out) = offsets(tl, p.duration, p.align)?;
    let rate = tl.output_rate();
    let meta = TransitionMeta { id: Ids::new(tl.ids()).fresh("tr"), transition: p.transition.clone(), params: p.params.clone() };
    let mut tr = Transition::new(at_rate(inn, rate), at_rate(out, rate));
    tr.transition_type = transition_type(&p.transition);
    tr.metadata.ve.transition = Some(meta.clone());
    let mut next = tl.clone();
    let Composable::Track(t) = &mut next.tracks.children[ti] else { unreachable!() };
    t.children.insert(i + 1, Composable::Transition(tr));
    let r = check(&next, ti, i, inn, out, &meta)?;
    Ok((next, r))
}

/// `(track index, child index)` of a transition.
fn find_transition(tl: &Timeline, id: &Id) -> Result<(usize, usize)> {
    for (ti, t) in tracks(tl) {
        if let Some(i) = t.children.iter().position(|c| matches!(c, Composable::Transition(x) if x.metadata.ve.transition.as_ref().is_some_and(|m| &m.id == id))) {
            return Ok((ti, i));
        }
    }
    Err(EditError::new(format!("no transition `{id}`")))
}

pub fn transition_meta<'a>(tl: &'a Timeline, id: &Id) -> Option<&'a TransitionMeta> {
    let (ti, i) = find_transition(tl, id).ok()?;
    let Composable::Track(t) = &tl.tracks.children[ti] else { return None };
    match &t.children[i] {
        Composable::Transition(x) => x.metadata.ve.transition.as_ref(),
        _ => None,
    }
}

/// Change a transition's length, alignment, kind or params (merged; `null` removes).
pub fn set_transition(
    tl: &Timeline,
    id: &Id,
    duration: Option<Q>,
    align: Option<Align>,
    transition: Option<&str>,
    params: Option<&Map<String, Value>>,
) -> Result<(Timeline, TransitionReport)> {
    let (ti, i) = find_transition(tl, id)?;
    let mut next = tl.clone();
    let Composable::Track(t) = &mut next.tracks.children[ti] else { unreachable!() };
    let Composable::Transition(tr) = &mut t.children[i] else { unreachable!() };
    let (cur_in, cur_out) = (tr.in_offset.seconds(), tr.out_offset.seconds());
    let cur_align = if cur_in == Q::from_integer(0) {
        Align::Start
    } else if cur_out == Q::from_integer(0) {
        Align::End
    } else {
        Align::Center
    };
    let (inn, out) = offsets(tl, duration.unwrap_or(cur_in + cur_out), align.unwrap_or(cur_align))?;
    let rate = tl.output_rate();
    tr.in_offset = at_rate(inn, rate);
    tr.out_offset = at_rate(out, rate);
    let meta = tr.metadata.ve.transition.as_mut().expect("found by its meta");
    if let Some(kind) = transition {
        meta.transition = kind.to_owned();
        tr.transition_type = transition_type(kind);
    }
    let meta = tr.metadata.ve.transition.as_mut().expect("found by its meta");
    for (k, v) in params.into_iter().flatten() {
        if v.is_null() {
            meta.params.remove(k);
        } else {
            meta.params.insert(k.clone(), v.clone());
        }
    }
    let meta = meta.clone();
    let r = check(&next, ti, i - 1, inn, out, &meta)?;
    Ok((next, r))
}

pub fn remove_transition(tl: &Timeline, id: &Id) -> Result<(Timeline, TransitionReport)> {
    let (ti, i) = find_transition(tl, id)?;
    let mut next = tl.clone();
    let Composable::Track(t) = &tl.tracks.children[ti] else { unreachable!() };
    let Composable::Transition(tr) = &t.children[i] else { unreachable!() };
    let meta = tr.metadata.ve.transition.clone().expect("found by its meta");
    let (inn, out) = (tr.in_offset.seconds(), tr.out_offset.seconds());
    // Report before removing (handles were fine when it was added; do not fail a removal on them).
    let r = check(tl, ti, i - 1, inn, out, &meta).unwrap_or_else(|_| TransitionReport {
        id: meta.id.clone(),
        transition: meta.transition.clone(),
        track: t.name.clone(),
        start: String::new(),
        end: String::new(),
        cut: String::new(),
        outgoing: Id::from_raw("-"),
        incoming: Id::from_raw("-"),
    });
    let Composable::Track(t) = &mut next.tracks.children[ti] else { unreachable!() };
    t.children.remove(i);
    Ok((next, r))
}

/// Keyframe at `t` seconds from the clip start.
pub fn keyframe(t: Q, v: Value, ease: Option<String>, rate: ve_model::time::Rate) -> Keyframe {
    Keyframe { t: RationalTime { value: t * rate.as_ratio(), rate }, v, ease: ease.unwrap_or_else(|| "linear".into()) }
}
