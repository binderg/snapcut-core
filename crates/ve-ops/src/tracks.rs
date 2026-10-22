//! The `track.*` commands: add, remove, rename and reorder tracks.
//!
//! A project can have any number of video and audio tracks. Tracks keep
//! their stack order in the document; among video tracks a later one draws
//! on top of earlier ones, and all audio tracks mix. Positions in these
//! commands count among tracks of the same kind (0 is the first video track,
//! the bottom layer), so adding an audio track never shifts a video position.

use schemars::JsonSchema;
use serde::Serialize;
use ve_model::meta::TrackMeta;
use ve_model::otio::{Composable, Timeline, Track, TrackKind};
use ve_model::Id;

use crate::EditError;
use crate::lane::{Ids, clip_id};

type Result<T> = std::result::Result<T, EditError>;

/// One track as the `track.*` commands report it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TrackPlace {
    pub id: Id,
    pub name: String,
    /// `video` or `audio`.
    pub kind: String,
    /// Position among tracks of the same kind: 0 is the first (for video, the bottom layer).
    pub index: usize,
    /// Clips on the track.
    pub clips: usize,
}

/// What a track command did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct TrackReport {
    /// The track the command acted on, as it is afterwards (as it was, for a removal).
    pub track: TrackPlace,
    /// Every track afterwards, in document order.
    pub tracks: Vec<TrackPlace>,
    /// Clips deleted with a removed track.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<Id>,
}

fn kind_word(k: TrackKind) -> &'static str {
    match k {
        TrackKind::Video => "video",
        TrackKind::Audio => "audio",
    }
}

fn track_id(t: &Track) -> Option<&Id> {
    t.metadata.ve.track.as_ref().map(|m| &m.id)
}

fn tracks(tl: &Timeline) -> impl Iterator<Item = (usize, &Track)> {
    tl.tracks.children.iter().enumerate().filter_map(|(i, c)| match c {
        Composable::Track(t) => Some((i, t)),
        _ => None,
    })
}

fn track_at(tl: &Timeline, i: usize) -> &Track {
    match &tl.tracks.children[i] {
        Composable::Track(t) => t,
        _ => unreachable!("index comes from tracks()"),
    }
}

/// Clip IDs on a track (top level; nested compositions count as one item each).
fn clip_ids(t: &Track) -> Vec<Id> {
    t.children
        .iter()
        .filter_map(|c| match c {
            Composable::Clip(c) => clip_id(c).cloned(),
            _ => None,
        })
        .collect()
}

fn clip_count(t: &Track) -> usize {
    t.children.iter().filter(|c| !matches!(c, Composable::Gap(_) | Composable::Transition(_))).count()
}

/// Stack indices of the tracks of one kind, in order.
fn of_kind(tl: &Timeline, kind: TrackKind) -> Vec<usize> {
    tracks(tl).filter(|(_, t)| t.kind == kind).map(|(i, _)| i).collect()
}

/// Every track with its position among its kind.
pub fn list(tl: &Timeline) -> Vec<TrackPlace> {
    let (mut v, mut a) = (0, 0);
    tracks(tl)
        .map(|(_, t)| {
            let n = if t.kind == TrackKind::Video { &mut v } else { &mut a };
            let index = *n;
            *n += 1;
            TrackPlace {
                id: track_id(t).cloned().unwrap_or_else(|| Id::from_raw("-")),
                name: t.name.clone(),
                kind: kind_word(t.kind).to_owned(),
                index,
                clips: clip_count(t),
            }
        })
        .collect()
}

fn place_of(tl: &Timeline, id: &Id) -> TrackPlace {
    list(tl).into_iter().find(|p| &p.id == id).expect("track exists")
}

fn names(tl: &Timeline) -> String {
    tracks(tl)
        .map(|(_, t)| format!("{} ({})", t.name, track_id(t).map_or("-".to_owned(), Id::to_string)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Stack index of a track given by ID or name.
pub fn find(tl: &Timeline, r: &str) -> Result<usize> {
    let hits: Vec<usize> =
        tracks(tl).filter(|(_, t)| track_id(t).is_some_and(|id| id.as_str() == r) || t.name == r).map(|(i, _)| i).collect();
    match hits.as_slice() {
        [i] => Ok(*i),
        [] => Err(EditError::new(format!("no track `{r}`; tracks are {}", names(tl)))),
        _ => Err(EditError::new(format!("more than one track is named `{r}`; use its ID"))),
    }
}

/// A name must be non-empty and match no other track's name or ID.
fn check_name(tl: &Timeline, name: &str, except: Option<usize>) -> Result<()> {
    if name.trim().is_empty() {
        return Err(EditError::new("a track name cannot be empty"));
    }
    if name != name.trim() {
        return Err(EditError::new(format!("track name `{name}` has spaces at its ends")));
    }
    let clash = tracks(tl)
        .filter(|(i, _)| Some(*i) != except)
        .any(|(_, t)| t.name == name || track_id(t).is_some_and(|id| id.as_str() == name));
    if clash {
        return Err(EditError::new(format!("a track is already named `{name}`")));
    }
    Ok(())
}

/// `V3` after `V1`, `V2`: one past the highest number used by any track
/// named with the kind's letter.
fn next_name(tl: &Timeline, kind: TrackKind) -> String {
    let letter = if kind == TrackKind::Video { 'V' } else { 'A' };
    let n = tracks(tl)
        .filter_map(|(_, t)| t.name.strip_prefix(letter).and_then(|d| d.parse::<u32>().ok()))
        .max()
        .unwrap_or(0);
    let mut n = n + 1;
    loop {
        let name = format!("{letter}{n}");
        if check_name(tl, &name, None).is_ok() {
            return name;
        }
        n += 1;
    }
}

/// Where in the stack to put a track that should be `index`-th among its
/// kind, given the stack indices of the other tracks of that kind.
fn stack_slot(tl: &Timeline, kind: TrackKind, index: Option<usize>) -> Result<usize> {
    let same = of_kind(tl, kind);
    let index = index.unwrap_or(same.len());
    if index > same.len() {
        return Err(EditError::new(format!(
            "position {index} is past the end; there are {} {} tracks (positions 0-{})",
            same.len(),
            kind_word(kind),
            same.len()
        )));
    }
    Ok(match (same.get(index), same.last()) {
        (Some(&at), _) => at,
        (None, Some(&last)) => last + 1,
        // First track of its kind: video goes before everything, audio after.
        (None, None) if kind == TrackKind::Video => 0,
        (None, None) => tl.tracks.children.len(),
    })
}

fn report(tl: &Timeline, id: &Id) -> TrackReport {
    TrackReport { track: place_of(tl, id), tracks: list(tl), removed: Vec::new() }
}

/// `track.add`: a new empty track. `name` defaults to the next free `V<n>` /
/// `A<n>`; `index` (position among its kind) defaults to the end, which for
/// video is the top layer.
pub fn add(tl: &Timeline, kind: TrackKind, name: Option<&str>, index: Option<usize>) -> Result<(Timeline, TrackReport)> {
    let name = match name {
        Some(n) => {
            check_name(tl, n, None)?;
            n.to_owned()
        }
        None => next_name(tl, kind),
    };
    let slot = stack_slot(tl, kind, index)?;
    let id = Ids::new(tl.ids()).fresh("trk");
    let mut track = Track::new(name, kind);
    track.metadata.ve.track = Some(TrackMeta { id: id.clone() });
    let mut next = tl.clone();
    next.tracks.children.insert(slot, Composable::Track(track));
    let r = report(&next, &id);
    Ok((next, r))
}

/// `track.remove`. A track with clips is only removed with `force`, which
/// deletes its clips too (clips linked to them on other tracks stay).
pub fn remove(tl: &Timeline, r: &str, force: bool) -> Result<(Timeline, TrackReport)> {
    let i = find(tl, r)?;
    let t = track_at(tl, i);
    let n = clip_count(t);
    if n > 0 && !force {
        return Err(EditError::new(format!(
            "track {} has {n} clip(s); move or delete them first, or pass force to delete them with the track",
            t.name
        )));
    }
    let id = track_id(t).cloned().ok_or_else(|| EditError::new(format!("track {} has no ID", t.name)))?;
    let before = place_of(tl, &id);
    let removed = clip_ids(t);
    let mut next = tl.clone();
    next.tracks.children.remove(i);
    Ok((next.clone(), TrackReport { track: before, tracks: list(&next), removed }))
}

/// `track.rename`.
pub fn rename(tl: &Timeline, r: &str, name: &str) -> Result<(Timeline, TrackReport)> {
    let i = find(tl, r)?;
    let mut next = tl.clone();
    if track_at(tl, i).name == name {
        let id = track_id(track_at(tl, i)).cloned().unwrap_or_else(|| Id::from_raw("-"));
        return Ok((next, report(tl, &id)));
    }
    check_name(tl, name, Some(i))?;
    let Composable::Track(t) = &mut next.tracks.children[i] else { unreachable!() };
    t.name = name.to_owned();
    let id = track_id(t).cloned().unwrap_or_else(|| Id::from_raw("-"));
    Ok((next.clone(), report(&next, &id)))
}

/// `track.move`: put a track at `index` among tracks of its kind (0 is the
/// first; for video, the bottom layer).
pub fn reorder(tl: &Timeline, r: &str, index: usize) -> Result<(Timeline, TrackReport)> {
    let i = find(tl, r)?;
    let kind = track_at(tl, i).kind;
    let id = track_id(track_at(tl, i)).cloned().unwrap_or_else(|| Id::from_raw("-"));
    let mut next = tl.clone();
    let item = next.tracks.children.remove(i);
    let others = of_kind(&next, kind).len();
    if index > others {
        return Err(EditError::new(format!(
            "position {index} is past the end; {} {} tracks means positions 0-{others}",
            others + 1,
            kind_word(kind)
        )));
    }
    let slot = stack_slot(&next, kind, Some(index))?;
    next.tracks.children.insert(slot, item);
    Ok((next.clone(), report(&next, &id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ve_model::meta::OutputSettings;
    use ve_model::time::Rate;

    fn project() -> Timeline {
        let output = OutputSettings { width: 640, height: 360, rate: Rate::integer(25), sample_rate: 48_000, color_space: "bt709".into() };
        ve_model::doc::new_project("t", output)
    }

    fn order(tl: &Timeline) -> Vec<String> {
        tracks(tl).map(|(_, t)| t.name.clone()).collect()
    }

    #[test]
    fn add_names_and_places_tracks() {
        let tl = project();
        let (tl, r) = add(&tl, TrackKind::Video, None, None).unwrap();
        assert_eq!(r.track.name, "V2");
        assert_eq!(r.track.index, 1);
        let (tl, _) = add(&tl, TrackKind::Audio, None, None).unwrap();
        let (tl, _) = add(&tl, TrackKind::Video, Some("Titles"), None).unwrap();
        let (tl, r) = add(&tl, TrackKind::Video, None, Some(0)).unwrap();
        assert_eq!(r.track.name, "V3");
        assert_eq!(order(&tl), ["V3", "V1", "V2", "Titles", "A1", "A2"]);
        assert!(tl.ids().contains(&r.track.id));
    }

    #[test]
    fn first_track_of_a_kind() {
        let tl = project();
        let (tl, _) = remove(&tl, "V1", false).unwrap();
        let (tl, _) = add(&tl, TrackKind::Video, None, None).unwrap();
        assert_eq!(order(&tl), ["V1", "A1"]);
        let (tl, _) = remove(&tl, "A1", false).unwrap();
        let (tl, _) = add(&tl, TrackKind::Audio, None, None).unwrap();
        assert_eq!(order(&tl), ["V1", "A1"]);
    }

    #[test]
    fn names_must_be_unique() {
        let tl = project();
        assert!(add(&tl, TrackKind::Video, Some("A1"), None).is_err());
        assert!(add(&tl, TrackKind::Video, Some(" x"), None).is_err());
        assert!(add(&tl, TrackKind::Video, Some(""), None).is_err());
        assert!(rename(&tl, "V1", "A1").is_err());
        let (tl, r) = rename(&tl, "V1", "Camera").unwrap();
        assert_eq!(r.track.name, "Camera");
        assert!(find(&tl, "Camera").is_ok());
        assert!(add(&tl, TrackKind::Video, None, Some(5)).is_err());
    }

    #[test]
    fn reorder_within_kind() {
        let tl = project();
        let (tl, _) = add(&tl, TrackKind::Video, None, None).unwrap();
        let (tl, _) = add(&tl, TrackKind::Video, None, None).unwrap();
        let (tl, r) = reorder(&tl, "V3", 0).unwrap();
        assert_eq!(r.track.index, 0);
        assert_eq!(order(&tl), ["V3", "V1", "V2", "A1"]);
        let (tl, _) = reorder(&tl, "V3", 2).unwrap();
        assert_eq!(order(&tl), ["V1", "V2", "V3", "A1"]);
        assert!(reorder(&tl, "V3", 3).is_err());
    }
}
