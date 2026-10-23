//! A track flattened to items with explicit start times.
//!
//! OTIO tracks place items implicitly, one after another. Edits are easier on
//! a list of `(start, item)` with holes allowed, so each command flattens the
//! tracks it touches into a [`Lane`], edits it, and rebuilds the OTIO children
//! (holes become gaps, adjacent plain gaps merge, trailing gaps go).
//!
//! All positions here are exact seconds on the timeline.

use std::collections::{HashMap, HashSet};

use ve_model::otio::{Clip, Composable, Gap, Marker, Track};
use ve_model::time::{Q, Rate, RationalTime, TimeRange};
use ve_model::Id;

use crate::EditError;

/// One item at an explicit timeline position.
#[derive(Debug, Clone)]
pub struct Placed {
    pub start: Q,
    pub item: Composable,
}

impl Placed {
    pub fn duration(&self) -> Q {
        item_seconds(&self.item)
    }

    pub fn end(&self) -> Q {
        self.start + self.duration()
    }

    pub fn clip(&self) -> Option<&Clip> {
        match &self.item {
            Composable::Clip(c) => Some(c),
            _ => None,
        }
    }

    pub fn clip_mut(&mut self) -> Option<&mut Clip> {
        match &mut self.item {
            Composable::Clip(c) => Some(c),
            _ => None,
        }
    }
}

/// Timeline duration of a track item, in seconds.
pub fn item_seconds(c: &Composable) -> Q {
    match c {
        Composable::Clip(c) => c.source_range.map_or(Q::from_integer(0), |r| r.duration.seconds()),
        Composable::Gap(g) => g.source_range.map_or(Q::from_integer(0), |r| r.duration.seconds()),
        Composable::Transition(_) => Q::from_integer(0),
        Composable::Track(t) => match t.source_range {
            Some(r) => r.duration.seconds(),
            None => t.children.iter().map(item_seconds).sum(),
        },
        Composable::Stack(s) => match s.source_range {
            Some(r) => r.duration.seconds(),
            None => s.children.iter().map(item_seconds).max().unwrap_or(Q::from_integer(0)),
        },
    }
}

/// Playback speed from `metadata.ve.clip` (1 when absent).
pub fn clip_speed(c: &Clip) -> Q {
    c.metadata.ve.clip.as_ref().map_or(Q::from_integer(1), |m| m.speed)
}

pub fn clip_id(c: &Clip) -> Option<&Id> {
    c.metadata.ve.clip.as_ref().map(|m| &m.id)
}

pub fn clip_link(c: &Clip) -> Option<&Id> {
    c.metadata.ve.clip.as_ref().and_then(|m| m.link.as_ref())
}

/// Media the clip may use, in media seconds: the active reference's
/// `available_range`, if it has one.
pub fn media_bounds(c: &Clip) -> Option<(Q, Q)> {
    let r = match c.active_media()? {
        ve_model::MediaReference::External(e) => e.available_range?,
        ve_model::MediaReference::Missing(m) => m.available_range?,
    };
    Some((r.start_time.seconds(), r.end_exclusive().seconds()))
}

/// `t` seconds as a time at `rate`.
pub fn at_rate(seconds: Q, rate: Rate) -> RationalTime {
    RationalTime { value: seconds * rate.as_ratio(), rate }
}

fn set_seconds(t: &mut RationalTime, seconds: Q) {
    t.value = seconds * t.rate.as_ratio();
}

/// Change a clip's timeline duration, keeping its in-point.
pub fn set_clip_duration(c: &mut Clip, seconds: Q) {
    if let Some(r) = c.source_range.as_mut() {
        set_seconds(&mut r.duration, seconds);
    }
}

/// Drop `delta` seconds of timeline time from the head of a clip (negative
/// `delta` extends it). The in-point moves by `delta * speed` of media.
pub fn trim_clip_head(c: &mut Clip, delta: Q) {
    let speed = clip_speed(c);
    if let Some(r) = c.source_range.as_mut() {
        let start = r.start_time.seconds() + delta * speed;
        let dur = r.duration.seconds() - delta;
        set_seconds(&mut r.start_time, start);
        set_seconds(&mut r.duration, dur);
    }
}

/// Check that a clip plays only media that exists.
pub fn check_media(c: &Clip) -> Result<(), EditError> {
    let (Some((lo, hi)), Some(r)) = (media_bounds(c), c.source_range) else { return Ok(()) };
    let start = r.start_time.seconds();
    let end = start + r.duration.seconds() * clip_speed(c);
    if start < lo || end > hi {
        let id = clip_id(c).map_or("clip".to_owned(), ToString::to_string);
        let over = if start < lo { lo - start } else { end - hi };
        return Err(EditError::new(format!(
            "{id} would need {} of media past the end of its source; the source has no more frames there",
            seconds_text(over)
        )));
    }
    Ok(())
}

/// `1.5s`, `0.033s`: for error messages only.
pub fn seconds_text(q: Q) -> String {
    let ms = (q * Q::from_integer(1000)).round().to_integer();
    format!("{}.{:03}s", ms / 1000, (ms % 1000).abs())
}

/// Plain gaps (no name, markers, metadata or effects) can be merged and dropped freely.
fn is_plain_gap(g: &Gap) -> bool {
    g.name.is_empty() && g.markers.is_empty() && g.metadata.is_empty() && g.effects.is_empty() && g.enabled && g.color.is_none()
}

/// Fresh, document-unique IDs.
pub struct Ids {
    taken: HashSet<Id>,
    relinks: HashMap<(Id, Q), Id>,
}

impl Ids {
    pub fn new(taken: HashSet<Id>) -> Self {
        Self { taken, relinks: HashMap::new() }
    }

    /// Link ID for the right halves of a linked group cut at `at`. Every
    /// track cut at the same point gets the same new ID, so the pieces stay
    /// linked to each other and not to the left halves.
    pub fn relink(&mut self, old: &Id, at: Q) -> Id {
        if let Some(id) = self.relinks.get(&(old.clone(), at)) {
            return id.clone();
        }
        let id = self.fresh("lnk");
        self.relinks.insert((old.clone(), at), id.clone());
        id
    }

    pub fn fresh(&mut self, kind: &str) -> Id {
        loop {
            let id = Id::new(kind);
            if self.taken.insert(id.clone()) {
                return id;
            }
        }
    }
}

/// A track's items with explicit starts, sorted, non-overlapping.
#[derive(Debug, Clone, Default)]
pub struct Lane {
    pub items: Vec<Placed>,
}

impl Lane {
    pub fn from_track(t: &Track) -> Result<Self, EditError> {
        let mut at = Q::from_integer(0);
        let mut items = Vec::with_capacity(t.children.len());
        for c in &t.children {
            let mut item = c.clone();
            if let Composable::Clip(clip) = &mut item
                && clip.source_range.is_none()
            {
                // OTIO falls back to the media's available range.
                let avail = match clip.active_media() {
                    Some(ve_model::MediaReference::External(e)) => e.available_range,
                    Some(ve_model::MediaReference::Missing(m)) => m.available_range,
                    None => None,
                };
                clip.source_range = Some(avail.ok_or_else(|| {
                    EditError::new(format!("clip {:?} on track {:?} has no source range", clip.name, t.name))
                })?);
            }
            let p = Placed { start: at, item };
            at = p.end();
            items.push(p);
        }
        Ok(Self { items })
    }

    /// End of the last item.
    pub fn end(&self) -> Q {
        self.items.last().map_or(Q::from_integer(0), Placed::end)
    }

    /// Index of the clip with this ID.
    pub fn find_clip(&self, id: &Id) -> Option<usize> {
        self.items.iter().position(|p| p.clip().and_then(clip_id) == Some(id))
    }

    /// Split the clip or gap that strictly contains `t`. Returns the index of
    /// the right part, which for a clip carries a fresh ID.
    pub fn split_at(&mut self, t: Q, ids: &mut Ids) -> Option<usize> {
        let i = self.items.iter().position(|p| p.start < t && t < p.end())?;
        let left = &self.items[i];
        let delta = t - left.start;
        let mut right = left.clone();
        let mut left = left.clone();
        match (&mut left.item, &mut right.item) {
            (Composable::Clip(l), Composable::Clip(r)) => {
                let media_split = l.source_range.map(|s| s.start_time.seconds()).unwrap_or_default() + delta * clip_speed(l);
                set_clip_duration(l, delta);
                trim_clip_head(r, delta);
                let (keep, give) = partition_markers(std::mem::take(&mut l.markers), media_split);
                l.markers = keep;
                r.markers = give;
                let transform_keys = r.metadata.ve.clip.as_mut().and_then(|m| m.transform.as_mut()).map(|t| &mut t.keyframes);
                for keys in transform_keys.into_iter().flat_map(|k| k.values_mut()) {
                    for k in keys.iter_mut() {
                        let s = k.t.seconds() - delta;
                        set_seconds(&mut k.t, s);
                    }
                }
                for fx in &mut r.metadata.ve.effects {
                    for keys in fx.keyframes.values_mut() {
                        for k in keys.iter_mut() {
                            let s = k.t.seconds() - delta;
                            set_seconds(&mut k.t, s);
                        }
                    }
                }
                if let Some(m) = r.metadata.ve.clip.as_mut() {
                    m.id = ids.fresh("clip");
                    if let Some(old) = &m.link {
                        m.link = Some(ids.relink(old, t));
                    }
                }
            }
            (Composable::Gap(l), Composable::Gap(r)) => {
                let src = l.source_range.map(|s| s.start_time.seconds()).unwrap_or_default();
                if let Some(s) = l.source_range.as_mut() {
                    set_seconds(&mut s.duration, delta);
                }
                if let Some(s) = r.source_range.as_mut() {
                    let start = s.start_time.seconds() + delta;
                    let dur = s.duration.seconds() - delta;
                    set_seconds(&mut s.start_time, start);
                    set_seconds(&mut s.duration, dur);
                }
                let (keep, give) = partition_markers(std::mem::take(&mut l.markers), src + delta);
                l.markers = keep;
                r.markers = give;
            }
            // Nested tracks and stacks cannot be cut yet; leave them whole.
            _ => return None,
        }
        right.start = t;
        self.items[i] = left;
        self.items.insert(i + 1, right);
        Some(i + 1)
    }

    /// True if a nested track or stack straddles `t` (it cannot be split).
    pub fn straddled_by_nested(&self, t: Q) -> bool {
        self.items.iter().any(|p| matches!(p.item, Composable::Track(_) | Composable::Stack(_)) && p.start < t && t < p.end())
    }

    /// Cut out `[a, b)`, leaving a hole. Returns what was removed.
    pub fn remove_range(&mut self, a: Q, b: Q, ids: &mut Ids) -> Result<Vec<Placed>, EditError> {
        if self.straddled_by_nested(a) || self.straddled_by_nested(b) {
            return Err(EditError::new("this edit would cut through a nested composition, which is not supported yet"));
        }
        self.split_at(a, ids);
        self.split_at(b, ids);
        let mut removed = Vec::new();
        self.items.retain(|p| {
            let zero = p.duration() == Q::from_integer(0);
            // Zero-length items (transitions) go only if strictly inside.
            let inside = if zero { a < p.start && p.start < b } else { p.start >= a && p.end() <= b };
            if inside {
                removed.push(p.clone());
            }
            !inside
        });
        Ok(removed)
    }

    /// Move every item that starts at or after `from` by `delta`.
    pub fn shift_from(&mut self, from: Q, delta: Q) {
        for p in &mut self.items {
            if p.start >= from {
                p.start += delta;
            }
        }
    }

    /// Put an item into free space.
    pub fn place(&mut self, p: Placed) {
        let i = self.items.partition_point(|q| q.start <= p.start);
        self.items.insert(i, p);
    }

    /// Back to OTIO children: holes become gaps at `rate`, adjacent plain
    /// gaps merge, empty items and trailing plain gaps go, and transitions
    /// that no longer sit between two clips are dropped.
    pub fn into_children(mut self, rate: Rate) -> Vec<Composable> {
        self.items.sort_by_key(|a| a.start);
        let zero = Q::from_integer(0);
        let mut out: Vec<Composable> = Vec::with_capacity(self.items.len());
        let mut cursor = zero;
        for p in self.items {
            let dur = p.duration();
            let is_transition = matches!(p.item, Composable::Transition(_));
            if dur <= zero && !is_transition {
                continue;
            }
            if p.start > cursor {
                push_gap(&mut out, p.start - cursor, rate);
            }
            cursor = cursor.max(p.start + dur);
            match p.item {
                Composable::Gap(g) if is_plain_gap(&g) => push_gap(&mut out, dur, rate),
                item => out.push(item),
            }
        }
        while matches!(out.last(), Some(Composable::Gap(g)) if is_plain_gap(g)) {
            out.pop();
        }
        // Transitions need a clip on both sides.
        let keep: Vec<bool> = (0..out.len())
            .map(|i| match out[i] {
                Composable::Transition(_) => {
                    i > 0
                        && matches!(out[i - 1], Composable::Clip(_))
                        && matches!(out.get(i + 1), Some(Composable::Clip(_)))
                }
                _ => true,
            })
            .collect();
        let mut k = keep.into_iter();
        out.retain(|_| k.next().unwrap_or(true));
        out
    }
}

/// Append a plain gap, merging into a plain gap already at the end.
fn push_gap(out: &mut Vec<Composable>, seconds: Q, rate: Rate) {
    if let Some(Composable::Gap(g)) = out.last_mut()
        && is_plain_gap(g)
    {
        let total = g.source_range.map_or(Q::from_integer(0), |r| r.duration.seconds()) + seconds;
        g.source_range = Some(TimeRange::new(RationalTime::zero(rate), at_rate(total, rate)));
        return;
    }
    out.push(Composable::Gap(Gap::new(at_rate(seconds, rate))));
}

/// Markers starting before `t` (in the item's source time) stay; the rest move.
fn partition_markers(markers: Vec<Marker>, t: Q) -> (Vec<Marker>, Vec<Marker>) {
    markers.into_iter().partition(|m| m.marked_range.start_time.seconds() < t)
}

/// Timeline-level markers after deleting `[a, b)`: later ones move left,
/// ones inside go, ones that overlap an edge shrink.
pub fn ripple_markers_delete(markers: &mut Vec<Marker>, a: Q, b: Q) {
    let len = b - a;
    markers.retain_mut(|m| {
        let s = m.marked_range.start_time.seconds();
        let e = s + m.marked_range.duration.seconds();
        if s >= b {
            set_seconds(&mut m.marked_range.start_time, s - len);
        } else if s >= a {
            if e <= b {
                return false;
            }
            set_seconds(&mut m.marked_range.start_time, a);
            set_seconds(&mut m.marked_range.duration, e - b);
        } else if e > a {
            let cut = e.min(b) - a;
            set_seconds(&mut m.marked_range.duration, e - s - cut);
        }
        true
    });
}

/// Timeline-level markers after inserting `len` seconds at `t`.
pub fn ripple_markers_insert(markers: &mut [Marker], t: Q, len: Q) {
    for m in markers {
        let s = m.marked_range.start_time.seconds();
        if s >= t {
            set_seconds(&mut m.marked_range.start_time, s + len);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ve_model::meta::ClipMeta;
    use ve_model::otio::{ExternalReference, MediaReference, TrackKind};

    fn q(n: i64) -> Q {
        Q::from_integer(n)
    }

    fn clip(id: &str, start: i64, frames: i64) -> Composable {
        let r = Rate::integer(10);
        let range = TimeRange::new(RationalTime::from_frames(start, r), RationalTime::from_frames(frames, r));
        let mut c = Clip::new(id, MediaReference::External(ExternalReference::new("file:///a.mov", None)), range);
        c.metadata.ve.clip = Some(ClipMeta::new(Id::new("clip")));
        Composable::Clip(c)
    }

    #[test]
    fn rebuild_fills_holes_and_merges_gaps() {
        let mut lane = Lane::default();
        lane.place(Placed { start: q(2), item: clip("b", 0, 10) });
        lane.place(Placed { start: q(0), item: clip("a", 0, 10) });
        lane.place(Placed { start: q(5), item: Composable::Gap(Gap::new(RationalTime::from_frames(10, Rate::integer(10)))) });
        let kids = lane.into_children(Rate::integer(10));
        // a(1s) gap(1s) b(1s) gap(2s)+gap(1s) merged, then dropped as trailing.
        assert_eq!(kids.len(), 3);
        assert!(matches!(kids[1], Composable::Gap(_)));
    }

    #[test]
    fn split_and_remove_keep_media_in_sync() {
        let mut t = Track::new("V1", TrackKind::Video);
        t.children = vec![clip("a", 100, 50)];
        let mut lane = Lane::from_track(&t).unwrap();
        let mut ids = Ids::new(HashSet::new());
        let removed = lane.remove_range(q(1), q(2), &mut ids).unwrap();
        assert_eq!(removed.len(), 1);
        lane.shift_from(q(2), q(-1));
        let kids = lane.into_children(Rate::integer(10));
        let starts: Vec<i64> = kids
            .iter()
            .map(|c| match c {
                Composable::Clip(c) => c.source_range.unwrap().start_time.value.to_integer(),
                _ => -1,
            })
            .collect();
        // [100..110) stays, [110..120) is gone, [120..150) follows directly.
        assert_eq!(starts, [100, 120]);
    }

    #[test]
    fn markers_ripple() {
        let r = Rate::integer(10);
        let mk = |s, d| Marker::new("m", TimeRange::new(RationalTime::from_frames(s, r), RationalTime::from_frames(d, r)));
        let mut ms = vec![mk(0, 5), mk(12, 3), mk(15, 10), mk(30, 0), mk(5, 20)];
        ripple_markers_delete(&mut ms, q(1), q(2));
        let got: Vec<(i64, i64)> =
            ms.iter().map(|m| (m.marked_range.start_time.value.to_integer(), m.marked_range.duration.value.to_integer())).collect();
        assert_eq!(got, [(0, 5), (10, 5), (20, 0), (5, 10)]);
    }
}
