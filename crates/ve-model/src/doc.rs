//! Whole-document helpers: creating a project and keeping its IDs complete.

use std::collections::HashSet;

use crate::SCHEMA_VERSION;
use crate::id::Id;
use crate::meta::{ClipMeta, MarkerMeta, MarkerSource, OutputSettings, ProjectMeta, TrackMeta, TransitionMeta};
use crate::otio::{Composable, Marker, Stack, Timeline, Track, TrackKind};
use crate::time::RationalTime;

/// A new, empty project: one video track `V1` and one audio track `A1`.
pub fn new_project(name: &str, output: OutputSettings) -> Timeline {
    let mut tl = Timeline::new(name);
    tl.global_start_time = Some(RationalTime::zero(output.rate));
    tl.metadata.ve.project = Some(ProjectMeta { schema_version: SCHEMA_VERSION, revision: 0, output });
    tl.tracks.children = vec![
        Composable::Track(Track::new("V1", TrackKind::Video)),
        Composable::Track(Track::new("A1", TrackKind::Audio)),
    ];
    tl.ensure_ids();
    tl
}

impl Timeline {
    pub fn project(&self) -> Option<&ProjectMeta> {
        self.metadata.ve.project.as_ref()
    }

    /// The project frame rate: the grid every edit snaps to. Files without a
    /// project block fall back to 24.
    pub fn output_rate(&self) -> crate::time::Rate {
        self.project().map_or(crate::time::Rate::integer(24), |p| p.output.rate)
    }

    /// Every stable ID in the document: tracks, clips, transitions, markers,
    /// clip link groups and assets. Used to keep new IDs unique.
    pub fn ids(&self) -> HashSet<Id> {
        let mut taken = HashSet::new();
        self.visit(&mut |item| {
            if let Some(id) = item.id() {
                taken.insert(id.clone());
            }
            if let ItemRef::Clip(c) = item {
                if let Some(link) = c.metadata.ve.clip.as_ref().and_then(|m| m.link.clone()) {
                    taken.insert(link);
                }
                taken.extend(c.metadata.ve.effects.iter().map(|e| e.id.clone()));
            }
        });
        taken.extend(self.metadata.ve.assets.iter().map(|a| a.id.clone()));
        taken
    }

    /// Give every track, clip, transition and marker that lacks a stable ID a
    /// fresh one, so files written by other tools become addressable. Returns
    /// how many IDs were added. Existing IDs are never changed.
    pub fn ensure_ids(&mut self) -> usize {
        let mut taken = HashSet::new();
        self.visit(&mut |item| {
            if let Some(id) = item.id() {
                taken.insert(id.clone());
            }
        });
        let mut added = 0;
        let mut fresh = |kind: &str| {
            added += 1;
            loop {
                let id = Id::new(kind);
                if taken.insert(id.clone()) {
                    return id;
                }
            }
        };
        visit_stack_mut(&mut self.tracks, &mut |item| match item {
            ItemMut::Track(t) if t.metadata.ve.track.is_none() => {
                t.metadata.ve.track = Some(TrackMeta { id: fresh("trk") });
            }
            ItemMut::Clip(c) if c.metadata.ve.clip.is_none() => {
                c.metadata.ve.clip = Some(ClipMeta::new(fresh("clip")));
            }
            ItemMut::Transition(t) if t.metadata.ve.transition.is_none() => {
                t.metadata.ve.transition = Some(TransitionMeta {
                    id: fresh("tr"),
                    transition: String::new(),
                    params: Default::default(),
                });
            }
            ItemMut::Marker(m) if m.metadata.ve.marker.is_none() => {
                m.metadata.ve.marker =
                    Some(MarkerMeta { id: fresh("mk"), source: MarkerSource::User, payload: Default::default() });
            }
            _ => {}
        });
        added
    }

    /// Point every clip media reference at `from` to `to`. Returns how many changed.
    pub fn retarget(&mut self, from: &str, to: &str) -> usize {
        let mut n = 0;
        visit_stack_mut(&mut self.tracks, &mut |item| {
            if let ItemMut::Clip(c) = item {
                for m in c.media_references.values_mut() {
                    if let crate::otio::MediaReference::External(e) = m
                        && e.target_url == from
                    {
                        e.target_url = to.to_owned();
                        n += 1;
                    }
                }
            }
        });
        n
    }

    /// IDs of the clips with any media reference pointing at `url`.
    pub fn clips_using(&self, url: &str) -> Vec<Id> {
        let mut out = Vec::new();
        self.visit(&mut |item| {
            if let ItemRef::Clip(c) = item {
                let uses = c.media_references.values().any(|m| matches!(m, crate::otio::MediaReference::External(e) if e.target_url == url));
                if uses {
                    out.extend(item.id().cloned());
                }
            }
        });
        out
    }

    /// Visit every item that can carry a stable ID, depth first.
    fn visit(&self, f: &mut impl FnMut(ItemRef<'_>)) {
        visit_stack(&self.tracks, f);
    }
}

/// Read-only view of an ID-carrying item.
enum ItemRef<'a> {
    Track(&'a Track),
    Clip(&'a crate::otio::Clip),
    Transition(&'a crate::otio::Transition),
    Marker(&'a Marker),
}

impl ItemRef<'_> {
    fn id(&self) -> Option<&Id> {
        match self {
            Self::Track(t) => t.metadata.ve.track.as_ref().map(|m| &m.id),
            Self::Clip(c) => c.metadata.ve.clip.as_ref().map(|m| &m.id),
            Self::Transition(t) => t.metadata.ve.transition.as_ref().map(|m| &m.id),
            Self::Marker(m) => m.metadata.ve.marker.as_ref().map(|m| &m.id),
        }
    }
}

enum ItemMut<'a> {
    Track(&'a mut Track),
    Clip(&'a mut crate::otio::Clip),
    Transition(&'a mut crate::otio::Transition),
    Marker(&'a mut Marker),
}

fn visit_stack<'a>(s: &'a Stack, f: &mut impl FnMut(ItemRef<'a>)) {
    s.markers.iter().for_each(|m| f(ItemRef::Marker(m)));
    s.children.iter().for_each(|c| visit_child(c, f));
}

fn visit_child<'a>(c: &'a Composable, f: &mut impl FnMut(ItemRef<'a>)) {
    match c {
        Composable::Track(t) => {
            f(ItemRef::Track(t));
            t.markers.iter().for_each(|m| f(ItemRef::Marker(m)));
            t.children.iter().for_each(|c| visit_child(c, f));
        }
        Composable::Clip(c) => {
            f(ItemRef::Clip(c));
            c.markers.iter().for_each(|m| f(ItemRef::Marker(m)));
        }
        Composable::Gap(g) => g.markers.iter().for_each(|m| f(ItemRef::Marker(m))),
        Composable::Transition(t) => f(ItemRef::Transition(t)),
        Composable::Stack(s) => visit_stack(s, f),
    }
}

fn visit_stack_mut(s: &mut Stack, f: &mut impl FnMut(ItemMut<'_>)) {
    s.markers.iter_mut().for_each(|m| f(ItemMut::Marker(m)));
    s.children.iter_mut().for_each(|c| visit_child_mut(c, f));
}

fn visit_child_mut(c: &mut Composable, f: &mut impl FnMut(ItemMut<'_>)) {
    match c {
        Composable::Track(t) => {
            f(ItemMut::Track(t));
            t.markers.iter_mut().for_each(|m| f(ItemMut::Marker(m)));
            t.children.iter_mut().for_each(|c| visit_child_mut(c, f));
        }
        Composable::Clip(c) => {
            f(ItemMut::Clip(c));
            c.markers.iter_mut().for_each(|m| f(ItemMut::Marker(m)));
        }
        Composable::Gap(g) => g.markers.iter_mut().for_each(|m| f(ItemMut::Marker(m))),
        Composable::Transition(t) => f(ItemMut::Transition(t)),
        Composable::Stack(s) => visit_stack_mut(s, f),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::otio::{Clip, MediaReference, MissingReference};
    use crate::time::{Rate, TimeRange};

    fn output() -> OutputSettings {
        OutputSettings {
            width: 1920,
            height: 1080,
            rate: Rate::integer(30),
            sample_rate: 48_000,
            color_space: "bt709".into(),
        }
    }

    #[test]
    fn new_project_has_ids_and_two_tracks() {
        let tl = new_project("demo", output());
        assert_eq!(tl.project().unwrap().revision, 0);
        let ids: Vec<String> = tl
            .tracks
            .children
            .iter()
            .map(|c| match c {
                Composable::Track(t) => t.metadata.ve.track.as_ref().unwrap().id.to_string(),
                _ => panic!("expected track"),
            })
            .collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.iter().all(|i| i.starts_with("trk_")));
    }

    #[test]
    fn ensure_ids_fills_gaps_and_keeps_existing() {
        let mut tl = new_project("demo", output());
        let r = Rate::integer(30);
        let range = TimeRange::new(RationalTime::zero(r), RationalTime::from_frames(30, r));
        let mut clip = Clip::new("c", MediaReference::Missing(MissingReference::default()), range);
        clip.markers.push(Marker::new("m", range));
        let Composable::Track(v1) = &mut tl.tracks.children[0] else { panic!() };
        v1.children.push(Composable::Clip(clip));
        let before = serde_json::to_value(&tl.tracks.children[1]).unwrap();

        assert_eq!(tl.ensure_ids(), 2);
        assert_eq!(tl.ensure_ids(), 0);
        // The untouched track kept its ID.
        assert_eq!(serde_json::to_value(&tl.tracks.children[1]).unwrap(), before);
    }
}
