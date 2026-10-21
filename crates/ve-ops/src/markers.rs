//! Timeline markers (`Stack.markers`, in timeline time): added by people,
//! agents and analyzers.

use serde_json::Value;
use ve_model::meta::{MarkerMeta, MarkerSource};
use ve_model::otio::{Marker, Timeline};
use ve_model::time::{Q, TimeRange};
use ve_model::Id;

use crate::EditError;
use crate::lane::{Ids, at_rate};

/// A marker to add, in timeline seconds.
#[derive(Debug, Clone)]
pub struct NewMarker {
    pub start: Q,
    pub duration: Q,
    pub name: String,
    pub color: Option<String>,
    pub comment: String,
    pub source: MarkerSource,
    pub payload: Value,
}

/// OTIO's marker colors.
pub const COLORS: [&str; 11] = ["PINK", "RED", "ORANGE", "YELLOW", "GREEN", "CYAN", "BLUE", "PURPLE", "MAGENTA", "BLACK", "WHITE"];

fn build(m: NewMarker, ids: &mut Ids, tl: &Timeline) -> Result<(Id, Marker), EditError> {
    let rate = tl.output_rate();
    let r = rate.as_ratio();
    let snap = |t: Q| (t * r).round() / r;
    let (start, duration) = (snap(m.start), snap(m.duration));
    if start < Q::from_integer(0) || duration < Q::from_integer(0) {
        return Err(EditError::new("markers need a start at or after 0 and a duration of 0 or more"));
    }
    let id = ids.fresh("mk");
    let mut marker = Marker::new(m.name, TimeRange::new(at_rate(start, rate), at_rate(duration, rate)));
    if let Some(c) = m.color {
        let up = c.to_uppercase();
        if !COLORS.contains(&up.as_str()) {
            return Err(EditError::new(format!("color `{c}`: use one of {}", COLORS.join(", ").to_lowercase())));
        }
        marker.color = up;
    }
    marker.comment = m.comment;
    marker.metadata.ve.marker = Some(MarkerMeta { id: id.clone(), source: m.source, payload: m.payload });
    Ok((id, marker))
}

/// Add timeline markers. Returns their IDs.
pub fn add(tl: &Timeline, new: Vec<NewMarker>) -> Result<(Timeline, Vec<Id>), EditError> {
    let mut next = tl.clone();
    let mut ids = Ids::new(tl.ids());
    let mut out = Vec::with_capacity(new.len());
    for m in new {
        let (id, marker) = build(m, &mut ids, tl)?;
        next.tracks.markers.push(marker);
        out.push(id);
    }
    sort(&mut next);
    Ok((next, out))
}

/// Remove timeline markers by ID and/or name. Returns the removed IDs.
pub fn remove(tl: &Timeline, ids: &[Id], name: Option<&str>) -> Result<(Timeline, Vec<Id>), EditError> {
    if ids.is_empty() && name.is_none() {
        return Err(EditError::new("give marker IDs or a marker name"));
    }
    for id in ids {
        if !tl.tracks.markers.iter().any(|m| marker_id(m) == Some(id)) {
            return Err(EditError::new(format!("no timeline marker `{id}`")));
        }
    }
    let mut next = tl.clone();
    let mut removed = Vec::new();
    next.tracks.markers.retain(|m| {
        let hit = marker_id(m).is_some_and(|i| ids.contains(i)) || name.is_some_and(|n| m.name == n);
        if hit && let Some(i) = marker_id(m) {
            removed.push(i.clone());
        }
        !hit
    });
    Ok((next, removed))
}

/// Replace one analyzer's markers inside `[from, to)` with a fresh set, so
/// re-running an analyzer never piles up duplicates.
pub fn replace_analyzer(tl: &Timeline, analyzer: &str, from: Q, to: Q, new: Vec<NewMarker>) -> Result<(Timeline, Vec<Id>), EditError> {
    let mut base = tl.clone();
    base.tracks.markers.retain(|m| {
        let mine = m.metadata.ve.marker.as_ref().is_some_and(|x| {
            x.source == MarkerSource::Analyzer && x.payload.get("analyzer").and_then(Value::as_str) == Some(analyzer)
        });
        let s = m.marked_range.start_time.seconds();
        !(mine && s >= from && s < to)
    });
    add(&base, new)
}

pub fn marker_id(m: &Marker) -> Option<&Id> {
    m.metadata.ve.marker.as_ref().map(|x| &x.id)
}

fn sort(tl: &mut Timeline) {
    tl.tracks.markers.sort_by_key(|a| a.marked_range.start_time);
}
