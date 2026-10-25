//! The `edit.*` commands: insert, split, trim, move, ripple_delete,
//! set_speed, set_volume and delete (lift).
//!
//! Times in params are exact seconds on the timeline (or in the media, where
//! a field says so); the protocol layer parses the strings people type.

use std::collections::{BTreeMap, HashMap, HashSet};

use schemars::JsonSchema;
use serde::Serialize;
use serde_json::json;
use ve_model::meta::ClipMeta;
use ve_model::otio::{Clip, Composable, ExternalReference, MediaReference, Timeline, TrackKind};
use ve_model::time::{Q, Rate, RationalTime, TimeRange};
use ve_model::{Id, timecode};

use crate::EditError;
use crate::lane::{
    Ids, Lane, Placed, at_rate, check_media, clip_id, clip_link, clip_speed, ripple_markers_delete, ripple_markers_insert,
    seconds_text, set_clip_duration, trim_clip_head,
};

type Result<T> = std::result::Result<T, EditError>;

/// Where a clip sits after a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ClipPlace {
    pub id: Id,
    /// Track name (`V1`, `A1`).
    pub track: String,
    /// Timeline in and out (exclusive) as timecode at the project rate.
    pub start: String,
    pub end: String,
}

/// What a command did. Clips that only moved because of a ripple are not listed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub struct EditReport {
    /// New clips (inserted clips, right halves of splits).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub created: Vec<ClipPlace>,
    /// Clips whose range, speed or volume changed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modified: Vec<ClipPlace>,
    /// Clips that no longer exist.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<Id>,
    /// Timeline duration afterwards, as timecode.
    pub duration: String,
}

/// Format a timeline time (seconds) as timecode at `rate`, drop-frame where it applies.
pub fn tc(seconds: Q, rate: Rate) -> String {
    let t = at_rate(seconds, rate);
    timecode::to_smpte(t, rate.has_drop_frame()).unwrap_or_else(|_| format!("{}s", seconds_text(seconds)))
}

/// Working copy of a document with its tracks flattened.
struct Doc {
    tl: Timeline,
    lanes: Vec<Option<Lane>>,
    dirty: Vec<bool>,
    rate: Rate,
    ids: Ids,
    created: Vec<Id>,
    changed: Vec<Id>,
    before: HashSet<Id>,
}

impl Doc {
    fn new(tl: &Timeline) -> Result<Self> {
        let lanes = tl
            .tracks
            .children
            .iter()
            .map(|c| match c {
                Composable::Track(t) => Lane::from_track(t).map(Some),
                _ => Ok(None),
            })
            .collect::<Result<Vec<_>>>()?;
        let before: HashSet<Id> = lanes
            .iter()
            .flatten()
            .flat_map(|l| l.items.iter().filter_map(|p| p.clip().and_then(clip_id).cloned()))
            .collect();
        Ok(Self {
            dirty: vec![false; lanes.len()],
            lanes,
            rate: tl.output_rate(),
            ids: Ids::new(tl.ids()),
            tl: tl.clone(),
            created: Vec::new(),
            changed: Vec::new(),
            before,
        })
    }

    fn snap(&self, t: Q) -> Q {
        let r = self.rate.as_ratio();
        (t * r).round() / r
    }

    /// Snap down (used where rounding up could run past the media).
    fn snap_down(&self, t: Q) -> Q {
        let r = self.rate.as_ratio();
        (t * r).floor() / r
    }

    fn frame(&self) -> Q {
        Q::from_integer(1) / self.rate.as_ratio()
    }

    fn track(&self, i: usize) -> &ve_model::Track {
        match &self.tl.tracks.children[i] {
            Composable::Track(t) => t,
            _ => unreachable!("lanes exist only for tracks"),
        }
    }

    fn track_name(&self, i: usize) -> String {
        self.track(i).name.clone()
    }

    fn kind(&self, i: usize) -> TrackKind {
        self.track(i).kind
    }

    fn lane(&mut self, i: usize) -> &mut Lane {
        self.dirty[i] = true;
        self.lanes[i].as_mut().expect("lane index refers to a track")
    }

    fn lane_ref(&self, i: usize) -> &Lane {
        self.lanes[i].as_ref().expect("lane index refers to a track")
    }

    fn track_indices(&self) -> Vec<usize> {
        (0..self.lanes.len()).filter(|&i| self.lanes[i].is_some()).collect()
    }

    /// A track by stable ID or name.
    fn find_track(&self, r: &str) -> Result<usize> {
        let hits: Vec<usize> = self
            .track_indices()
            .into_iter()
            .filter(|&i| {
                let t = self.track(i);
                t.metadata.ve.track.as_ref().is_some_and(|m| m.id.as_str() == r) || t.name == r
            })
            .collect();
        match hits.as_slice() {
            [i] => Ok(*i),
            [] => Err(EditError::new(format!("no track `{r}`; tracks are {}", self.track_list()))),
            _ => Err(EditError::new(format!("more than one track is named `{r}`; use its ID"))),
        }
    }

    fn track_list(&self) -> String {
        self.track_indices()
            .into_iter()
            .map(|i| {
                let t = self.track(i);
                let id = t.metadata.ve.track.as_ref().map_or("-".to_owned(), |m| m.id.to_string());
                format!("{} ({id})", t.name)
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn tracks_or_all(&self, refs: &[String]) -> Result<Vec<usize>> {
        if refs.is_empty() { Ok(self.track_indices()) } else { refs.iter().map(|r| self.find_track(r)).collect() }
    }

    fn first_track(&self, kind: TrackKind) -> Option<usize> {
        self.track_indices().into_iter().find(|&i| self.kind(i) == kind)
    }

    fn locate(&self, id: &Id) -> Result<(usize, usize)> {
        for i in self.track_indices() {
            if let Some(j) = self.lane_ref(i).find_clip(id) {
                return Ok((i, j));
            }
        }
        Err(EditError::new(format!("no clip `{id}` in the timeline; `ve summary` lists clip IDs")))
    }

    fn placed(&self, id: &Id) -> Result<&Placed> {
        let (i, j) = self.locate(id)?;
        Ok(&self.lane_ref(i).items[j])
    }

    /// The clip and, when `linked`, every clip sharing its link ID, with their tracks.
    fn group(&self, id: &Id, linked: bool) -> Result<Vec<(usize, Id)>> {
        let (i, j) = self.locate(id)?;
        let link = self.lane_ref(i).items[j].clip().and_then(clip_link).cloned();
        let mut out = vec![(i, id.clone())];
        if let (true, Some(link)) = (linked, link) {
            for t in self.track_indices() {
                for p in &self.lane_ref(t).items {
                    if let Some(c) = p.clip()
                        && clip_link(c) == Some(&link)
                        && let Some(cid) = clip_id(c)
                        && cid != id
                    {
                        out.push((t, cid.clone()));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Timeline start of every clip, to see later what a ripple moved.
    fn starts(&self) -> HashMap<Id, Q> {
        let mut out = HashMap::new();
        for i in self.track_indices() {
            for p in &self.lane_ref(i).items {
                if let Some(id) = p.clip().and_then(clip_id) {
                    out.insert(id.clone(), p.start);
                }
            }
        }
        out
    }

    fn mark_changed(&mut self, id: &Id) {
        if !self.changed.contains(id) && !self.created.contains(id) {
            self.changed.push(id.clone());
        }
    }

    /// True when every track with anything after `t` is in `rippled`, so
    /// timeline markers should ripple with the content.
    fn all_tracks_rippled(&self, rippled: &HashSet<usize>, t: Q) -> bool {
        self.track_indices().into_iter().all(|i| rippled.contains(&i) || self.lane_ref(i).end() <= t)
    }

    fn timeline_end(&self) -> Q {
        self.lanes.iter().flatten().map(Lane::end).max().unwrap_or(Q::from_integer(0))
    }

    /// Rebuild touched tracks, check media bounds and build the report.
    fn finish(mut self) -> Result<(Timeline, EditReport)> {
        let zero = Q::from_integer(0);
        for i in self.track_indices() {
            if !self.dirty[i] {
                continue;
            }
            for p in &self.lane_ref(i).items {
                if p.clip().is_some() && p.duration() <= zero {
                    let id = p.clip().and_then(clip_id).map_or("a clip".to_owned(), ToString::to_string);
                    return Err(EditError::new(format!("{id} would have no duration left")));
                }
                if p.start < zero {
                    return Err(EditError::new("this edit would move content before the timeline start"));
                }
            }
            for w in self.lane_ref(i).items.windows(2) {
                if w[1].start < w[0].end() {
                    return Err(EditError::new(format!(
                        "this edit would overlap two items on track {} at {}",
                        self.track_name(i),
                        tc(w[1].start, self.rate)
                    )));
                }
            }
        }
        for id in self.created.iter().chain(&self.changed) {
            if let Ok((i, j)) = self.locate(id)
                && let Some(c) = self.lane_ref(i).items[j].clip()
            {
                check_media(c)?;
            }
        }
        let place = |d: &Doc, id: &Id| -> Option<ClipPlace> {
            let (i, j) = d.locate(id).ok()?;
            let p = &d.lane_ref(i).items[j];
            Some(ClipPlace { id: id.clone(), track: d.track_name(i), start: tc(p.start, d.rate), end: tc(p.end(), d.rate) })
        };
        let created = self.created.iter().filter_map(|id| place(&self, id)).collect();
        let modified = self.changed.iter().filter_map(|id| place(&self, id)).collect();
        let after: HashSet<Id> = self
            .lanes
            .iter()
            .flatten()
            .flat_map(|l| l.items.iter().filter_map(|p| p.clip().and_then(clip_id).cloned()))
            .collect();
        let mut removed: Vec<Id> = self.before.difference(&after).cloned().collect();
        removed.sort();
        let duration = tc(self.timeline_end(), self.rate);

        let rate = self.rate;
        for (i, lane) in self.lanes.into_iter().enumerate() {
            if let (Some(lane), true) = (lane, self.dirty[i])
                && let Composable::Track(t) = &mut self.tl.tracks.children[i]
            {
                t.children = lane.into_children(rate);
            }
        }
        Ok((self.tl, EditReport { created, modified, removed, duration }))
    }
}

fn need_positive(what: &str, t: Q) -> Result<()> {
    if t < Q::from_integer(0) {
        return Err(EditError::new(format!("{what} is before the timeline start")));
    }
    Ok(())
}

// --- insert ---

/// `edit.insert`: put an asset on the timeline.
#[derive(Debug, Clone)]
pub struct Insert {
    pub asset: Id,
    /// Track for the video (or, for audio-only assets, the audio). Defaults
    /// to the first track of the matching kind.
    pub track: Option<String>,
    /// Track for the audio of an asset that has both. Defaults to the first audio track.
    pub audio_track: Option<String>,
    /// Timeline position. Defaults to the end of the target tracks.
    pub at: Option<Q>,
    /// Media in-point, seconds from the start of the file. Defaults to 0.
    pub source_in: Option<Q>,
    /// Media out-point (exclusive). Defaults to `source_in + duration`, else the end of the file.
    pub source_out: Option<Q>,
    pub duration: Option<Q>,
    pub video: bool,
    pub audio: bool,
    /// Overwrite what is there instead of pushing later content right.
    pub overwrite: bool,
}

pub fn insert(tl: &Timeline, p: &Insert) -> Result<(Timeline, EditReport)> {
    let mut d = Doc::new(tl)?;
    let asset = tl
        .metadata
        .ve
        .assets
        .iter()
        .find(|a| a.id == p.asset)
        .ok_or_else(|| EditError::new(format!("no asset `{}`; `ve import` adds media and lists asset IDs", p.asset)))?
        .clone();
    let media = asset
        .media
        .clone()
        .ok_or_else(|| EditError::new(format!("asset {} has no media info; import it again", asset.id)))?;
    let media_end = media.duration.seconds();
    let mrate = media.duration.rate;

    let mut kinds = Vec::new();
    if media.video.is_some() && p.video {
        kinds.push(TrackKind::Video);
    }
    if media.audio.is_some() && p.audio {
        kinds.push(TrackKind::Audio);
    }
    if kinds.is_empty() {
        return Err(EditError::new(format!("asset {} has no {} to insert", asset.id, if p.video { "video" } else { "audio" })));
    }

    let zero = Q::from_integer(0);
    let src_in = p.source_in.unwrap_or(zero);
    let src_out = match (p.source_out, p.duration) {
        (Some(o), _) => o,
        (None, Some(dur)) => src_in + dur,
        (None, None) => media_end,
    };
    if src_in < zero || src_out > media_end || src_in >= src_out {
        return Err(EditError::new(format!(
            "media range {}..{} is outside {} (0..{})",
            seconds_text(src_in),
            seconds_text(src_out),
            asset.id,
            seconds_text(media_end)
        )));
    }
    let dur = d.snap_down(src_out - src_in);
    if dur <= zero {
        return Err(EditError::new("the media range is shorter than one frame"));
    }

    // Target track per kind.
    let primary = p.track.as_deref().map(|r| d.find_track(r)).transpose()?;
    let audio_ref = p.audio_track.as_deref().map(|r| d.find_track(r)).transpose()?;
    let mut targets = Vec::new();
    for kind in &kinds {
        let lane = match kind {
            TrackKind::Video => primary.filter(|&i| d.kind(i) == TrackKind::Video).or_else(|| d.first_track(TrackKind::Video)),
            TrackKind::Audio => audio_ref.or(primary.filter(|&i| d.kind(i) == TrackKind::Audio)).or_else(|| d.first_track(TrackKind::Audio)),
        }
        .ok_or_else(|| EditError::new(format!("the project has no {kind:?} track")))?;
        if d.kind(lane) != *kind {
            return Err(EditError::new(format!("track {} is not a {kind:?} track", d.track_name(lane))));
        }
        targets.push((*kind, lane));
    }
    if let Some(i) = primary
        && !targets.iter().any(|&(_, l)| l == i)
    {
        return Err(EditError::new(format!("asset {} has nothing for {:?} track {}", asset.id, d.kind(i), d.track_name(i))));
    }

    let at = match p.at {
        Some(t) => d.snap(t),
        None => targets.iter().map(|&(_, l)| d.lane_ref(l).end()).max().unwrap_or(zero),
    };
    need_positive("--at", at)?;

    let name = asset.url.rsplit('/').next().unwrap_or("clip").to_owned();
    let name = urlencoding_decode(&name);
    let link = (targets.len() > 1).then(|| d.ids.fresh("lnk"));
    let avail = TimeRange::new(RationalTime::zero(mrate), media.duration);
    let target_lanes: Vec<usize> = targets.iter().map(|&(_, l)| l).collect();
    let rippled = if p.overwrite {
        HashSet::new()
    } else {
        let room: Vec<(usize, Q, Q)> = target_lanes.iter().map(|&l| (l, at, dur)).collect();
        make_room(&mut d, &room, Room::Full)?
    };
    for lane in target_lanes {
        let id = d.ids.fresh("clip");
        let range = TimeRange::new(at_rate(src_in, mrate), at_rate(dur, mrate));
        let mut clip = Clip::new(name.clone(), MediaReference::External(ExternalReference::new(asset.url.clone(), Some(avail))), range);
        clip.metadata.ve.clip = Some(ClipMeta { link: link.clone(), ..ClipMeta::new(id.clone()) });
        let mut ids = std::mem::replace(&mut d.ids, Ids::new(HashSet::new()));
        let l = d.lane(lane);
        if p.overwrite {
            l.remove_range(at, at + dur, &mut ids)?;
        }
        l.place(Placed { start: at, item: Composable::Clip(clip) });
        d.ids = ids;
        d.created.push(id);
    }
    if !p.overwrite && d.all_tracks_rippled(&rippled, at) {
        ripple_markers_insert(&mut d.tl.tracks.markers, at, dur);
    }
    d.finish()
}

/// How much room [`make_room`] opens.
#[derive(Clone, Copy, PartialEq)]
enum Room {
    /// Push by the full length (insert).
    Full,
    /// Push only as far as needed for the item to fit (a move that pushes).
    Fit,
}

/// Make room for items of length `len` at `at` on each target lane without
/// pulling linked clips apart. A clip under a target point is cut there, and
/// so are its linked clips on other tracks. Everything on a target lane from
/// its point moves right by one shared amount; every other track that holds a
/// partner of a clip that moves is pushed by the same amount from that
/// partner's start (which keeps J/L offsets). Returns the lanes that moved.
fn make_room(d: &mut Doc, targets: &[(usize, Q, Q)], room: Room) -> Result<HashSet<usize>> {
    let zero = Q::from_integer(0);
    let mut ids = std::mem::replace(&mut d.ids, Ids::new(HashSet::new()));
    for &(lane, at, _) in targets {
        if d.lane_ref(lane).straddled_by_nested(at) {
            d.ids = ids;
            return Err(EditError::new("cannot insert inside a nested composition"));
        }
        let cut_link = d
            .lane_ref(lane)
            .items
            .iter()
            .find(|p| p.start < at && at < p.end())
            .and_then(|p| p.clip())
            .and_then(clip_link)
            .cloned();
        d.lane(lane).split_at(at, &mut ids);
        // Same `at`, so the right halves share one new link with the target's right half.
        if let Some(link) = cut_link {
            for other in d.track_indices() {
                let spans = d.lane_ref(other).items.iter().any(|p| {
                    p.start < at && at < p.end() && p.clip().and_then(clip_link) == Some(&link)
                });
                if spans && !d.lane_ref(other).straddled_by_nested(at) {
                    d.lane(other).split_at(at, &mut ids);
                }
            }
        }
    }
    d.ids = ids;

    let amount = match room {
        Room::Full => targets.iter().map(|t| t.2).max().unwrap_or(zero),
        Room::Fit => targets
            .iter()
            .map(|&(lane, at, len)| {
                let next = d
                    .lane_ref(lane)
                    .items
                    .iter()
                    .filter(|p| p.start >= at && p.duration() > zero && !matches!(p.item, Composable::Gap(_)))
                    .map(|p| p.start)
                    .min();
                next.map_or(zero, |n| (at + len - n).max(zero))
            })
            .max()
            .unwrap_or(zero),
    };
    if amount == zero {
        return Ok(HashSet::new());
    }

    // Where each lane starts moving. Target lanes move from their point; another lane
    // joins from the earliest partner of anything that moves, until nothing new joins.
    let mut from: HashMap<usize, Q> = HashMap::new();
    for &(lane, at, _) in targets {
        from.entry(lane).and_modify(|t| *t = (*t).min(at)).or_insert(at);
    }
    let pinned: HashSet<usize> = from.keys().copied().collect();
    loop {
        let moving: HashSet<Id> = from
            .iter()
            .flat_map(|(&l, &t)| d.lane_ref(l).items.iter().filter(move |p| p.start >= t).filter_map(|p| p.clip().and_then(clip_link).cloned()))
            .collect();
        let mut grew = false;
        for lane in d.track_indices() {
            if pinned.contains(&lane) {
                continue;
            }
            let first = d
                .lane_ref(lane)
                .items
                .iter()
                .filter(|p| p.clip().and_then(clip_link).is_some_and(|l| moving.contains(l)))
                .map(|p| p.start)
                .min();
            if let Some(s) = first
                && from.get(&lane).is_none_or(|&cur| s < cur)
            {
                from.insert(lane, s);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    for (&lane, &t) in &from {
        d.lane(lane).shift_from(t, amount);
    }
    Ok(from.into_keys().collect())
}

/// After a ripple moved content on the `pinned` lanes, keep links in sync.
/// Every other lane holding a partner of a clip that moved shifts by the same
/// amount from that partner's start (so J/L offsets keep), up to its next
/// partner that moved by a different amount. `old` holds clip starts from
/// before the edit. Refused when a lane has no room to move, or a link would
/// still be pulled apart. Returns the lanes that followed.
fn follow_links(d: &mut Doc, old: &HashMap<Id, Q>, pinned: &HashSet<usize>) -> Result<HashSet<usize>> {
    let zero = Q::from_integer(0);
    // (link, how far the clip moved, clip id) for every linked clip on a lane.
    let moves = |d: &Doc, lane: usize| -> Vec<(Id, Q, Id)> {
        d.lane_ref(lane)
            .items
            .iter()
            .filter_map(|p| {
                let c = p.clip()?;
                let id = clip_id(c)?;
                Some((clip_link(c)?.clone(), p.start - *old.get(id)?, id.clone()))
            })
            .collect()
    };
    let mut settled = pinned.clone();
    let mut followers: HashSet<usize> = HashSet::new();
    loop {
        let mut by_link: HashMap<Id, Q> = HashMap::new();
        for &l in &settled {
            for (link, dt, _) in moves(d, l) {
                by_link.entry(link).or_insert(dt);
            }
        }
        let mut grew = false;
        for l in d.track_indices() {
            if settled.contains(&l) {
                continue;
            }
            let mut anchors: Vec<(Q, Q)> = d
                .lane_ref(l)
                .items
                .iter()
                .filter_map(|p| Some((p.start, *by_link.get(p.clip().and_then(clip_link)?)?)))
                .collect();
            if anchors.iter().all(|a| a.1 == zero) {
                continue;
            }
            anchors.sort();
            let lane = d.lane(l);
            lane.drop_plain_gaps();
            for p in &mut lane.items {
                let k = anchors.partition_point(|a| a.0 <= p.start);
                if k > 0 {
                    p.start += anchors[k - 1].1;
                }
            }
            lane.items.sort_by(|a, b| a.start.cmp(&b.start).then(a.duration().cmp(&b.duration())));
            settled.insert(l);
            followers.insert(l);
            grew = true;
        }
        if !grew {
            break;
        }
    }

    let name = |p: &Placed| p.clip().and_then(clip_id).map_or("a clip".to_owned(), ToString::to_string);
    for &l in &followers {
        let lane = d.lane_ref(l);
        let clash = lane.items.windows(2).find(|w| w[1].start < w[0].end()).map(|w| (w[1].start, name(&w[0])));
        if let Some((at, id)) = clash {
            return Err(EditError::new(format!(
                "this ripple moves clips that are linked to clips on track {t}, but {t} has no room to move with them \
                 ({id} is in the way at {}); edit both sides together, or make room on {t} first",
                tc(at, d.rate),
                t = d.track_name(l)
            )));
        }
        if lane.items.iter().any(|p| p.start < zero) {
            return Err(EditError::new(format!(
                "this ripple would move linked clips on track {} before the timeline start",
                d.track_name(l)
            )));
        }
    }
    // A follower may still disagree with a partner elsewhere (two partners pulling one lane apart).
    let mut seen: HashMap<Id, (Q, bool)> = HashMap::new();
    for &l in &settled {
        let follower = followers.contains(&l);
        for (link, dt, id) in moves(d, l) {
            match seen.get(&link) {
                Some(&(other, f)) if other != dt && (f || follower) => {
                    return Err(EditError::new(format!(
                        "this ripple would pull {id} away from its linked clips; edit both sides together"
                    )));
                }
                Some(_) => {}
                None => {
                    seen.insert(link, (dt, follower));
                }
            }
        }
    }
    Ok(followers)
}

/// File names in URLs are percent-encoded (`IMG%204410.MOV`); show them plainly.
fn urlencoding_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_owned())
}

// --- split ---

#[derive(Debug, Clone, Default)]
pub struct Split {
    pub at: Q,
    /// Clips to cut. Empty: every clip under `at` on `tracks` (all tracks if empty).
    pub clips: Vec<Id>,
    pub tracks: Vec<String>,
    pub linked: bool,
}

pub fn split(tl: &Timeline, p: &Split) -> Result<(Timeline, EditReport)> {
    let mut d = Doc::new(tl)?;
    let t = d.snap(p.at);
    let mut targets: Vec<(usize, Id, bool)> = Vec::new(); // (lane, clip, named explicitly)
    if p.clips.is_empty() {
        for i in d.tracks_or_all(&p.tracks)? {
            for it in &d.lane_ref(i).items {
                if let Some(id) = it.clip().and_then(clip_id)
                    && it.start < t
                    && t < it.end()
                {
                    targets.push((i, id.clone(), false));
                }
            }
        }
        if targets.is_empty() {
            return Err(EditError::new(format!("no clip spans {}", tc(t, d.rate))));
        }
    } else {
        for id in &p.clips {
            for (lane, cid) in d.group(id, p.linked)? {
                if !targets.iter().any(|(_, c, _)| *c == cid) {
                    targets.push((lane, cid.clone(), cid == *id));
                }
            }
        }
    }
    for (lane, id, explicit) in targets {
        let (_, j) = d.locate(&id)?;
        let it = &d.lane_ref(lane).items[j];
        if !(it.start < t && t < it.end()) {
            if explicit {
                return Err(EditError::new(format!(
                    "{id} runs {}..{}, so {} is not inside it",
                    tc(it.start, d.rate),
                    tc(it.end(), d.rate),
                    tc(t, d.rate)
                )));
            }
            continue;
        }
        let mut ids = std::mem::replace(&mut d.ids, Ids::new(HashSet::new()));
        let right = d.lane(lane).split_at(t, &mut ids).expect("t is inside the clip");
        d.ids = ids;
        let right_id = d.lane_ref(lane).items[right].clip().and_then(clip_id).cloned().expect("clips have IDs");
        d.mark_changed(&id);
        d.created.push(right_id);
    }
    d.finish()
}

// --- trim ---

#[derive(Debug, Clone)]
pub struct Trim {
    pub clip: Id,
    /// Move the in-point: positive is later (shorter clip), negative earlier.
    pub in_delta: Option<Q>,
    /// Move the out-point: positive is later (longer clip), negative earlier.
    pub out_delta: Option<Q>,
    /// Roll the edit point instead of rippling: the neighbour gives or takes
    /// the same time, so nothing else moves. With `linked: false` on one side
    /// of an A/V pair this makes a J or L cut.
    pub roll: bool,
    pub linked: bool,
}

pub fn trim(tl: &Timeline, p: &Trim) -> Result<(Timeline, EditReport)> {
    let mut d = Doc::new(tl)?;
    let zero = Q::from_integer(0);
    let din = p.in_delta.map(|x| d.snap(x)).unwrap_or(zero);
    let dout = p.out_delta.map(|x| d.snap(x)).unwrap_or(zero);
    if din == zero && dout == zero {
        return Err(EditError::new("nothing to trim: give an in or out delta of at least one frame"));
    }
    let old = d.starts();
    let group = d.group(&p.clip, p.linked)?;
    let pinned: HashSet<usize> = group.iter().map(|g| g.0).collect();
    for (lane, id) in group {
        d.mark_changed(&id);
        if p.roll {
            if din != zero {
                roll_in(&mut d, lane, &id, din)?;
            }
            if dout != zero {
                roll_out(&mut d, lane, &id, dout)?;
            }
        } else {
            let (_, j) = d.locate(&id)?;
            let end = d.lane_ref(lane).items[j].end();
            let l = d.lane(lane);
            if din != zero {
                l.shift_from(end, -din);
                trim_clip_head(l.items[j].clip_mut().expect("located a clip"), din);
            }
            if dout != zero {
                let end = l.items[j].end();
                l.shift_from(end, dout);
                let c = l.items[j].clip_mut().expect("located a clip");
                let dur = c.source_range.map_or(zero, |r| r.duration.seconds()) + dout;
                set_clip_duration(c, dur);
            }
        }
    }
    if !p.roll {
        follow_links(&mut d, &old, &pinned)?;
    }
    d.finish()
}

/// Neighbour of a clip that shares an edge at `t`: the item ending (`before`) or starting there.
fn neighbour(lane: &Lane, t: Q, before: bool) -> Option<usize> {
    let zero = Q::from_integer(0);
    lane.items.iter().position(|p| p.duration() > zero && if before { p.end() == t } else { p.start == t })
}

fn roll_in(d: &mut Doc, lane: usize, id: &Id, delta: Q) -> Result<()> {
    let (_, j) = d.locate(id)?;
    let start = d.lane_ref(lane).items[j].start;
    let new_start = start + delta;
    need_positive("the new in-point", new_start)?;
    let prev = neighbour(d.lane_ref(lane), start, true);
    if prev.is_none() && delta < Q::from_integer(0) {
        // Rolling into a hole: the hole must be big enough.
        if d.lane_ref(lane).items.iter().any(|p| p.end() > new_start && p.end() <= start && p.start < start) {
            return Err(EditError::new(format!("{id} cannot roll into the item before it")));
        }
    }
    let l = d.lane(lane);
    if let Some(k) = prev {
        let pdur = l.items[k].duration() + delta;
        if pdur < Q::from_integer(0) || (pdur == Q::from_integer(0) && l.items[k].clip().is_some()) {
            return Err(EditError::new(format!("rolling {id} by {} would eat the whole item before it", seconds_text(delta))));
        }
        match &mut l.items[k].item {
            Composable::Clip(c) => set_clip_duration(c, pdur),
            Composable::Gap(g) => {
                if let Some(r) = g.source_range.as_mut() {
                    *r = TimeRange::new(r.start_time, at_rate(pdur, r.duration.rate));
                }
            }
            _ => {}
        }
    }
    move_zero_length(l, start, new_start);
    let it = &mut l.items[j];
    it.start = new_start;
    trim_clip_head(it.clip_mut().expect("located a clip"), delta);
    if let Some(k) = prev
        && let Some(cid) = d.lane_ref(lane).items[k].clip().and_then(clip_id).cloned()
    {
        d.mark_changed(&cid);
    }
    Ok(())
}

fn roll_out(d: &mut Doc, lane: usize, id: &Id, delta: Q) -> Result<()> {
    let (_, j) = d.locate(id)?;
    let end = d.lane_ref(lane).items[j].end();
    let new_end = end + delta;
    let next = neighbour(d.lane_ref(lane), end, false);
    if next.is_none() && delta > Q::from_integer(0) {
        // Rolling into a hole: stop at the next item.
        if let Some(after) = d.lane_ref(lane).items.iter().find(|p| p.start > end && p.duration() > Q::from_integer(0))
            && after.start < new_end
        {
            return Err(EditError::new(format!("{id} cannot roll into the item after it")));
        }
    }
    let l = d.lane(lane);
    if let Some(k) = next {
        let ndur = l.items[k].duration() - delta;
        if ndur < Q::from_integer(0) || (ndur == Q::from_integer(0) && l.items[k].clip().is_some()) {
            return Err(EditError::new(format!("rolling {id} by {} would eat the whole item after it", seconds_text(delta))));
        }
        let it = &mut l.items[k];
        it.start = new_end;
        match &mut it.item {
            Composable::Clip(c) => trim_clip_head(c, delta),
            Composable::Gap(g) => {
                if let Some(r) = g.source_range.as_mut() {
                    *r = TimeRange::new(r.start_time, at_rate(ndur, r.duration.rate));
                }
            }
            _ => {}
        }
    }
    move_zero_length(l, end, new_end);
    let c = l.items[j].clip_mut().expect("located a clip");
    let dur = c.source_range.map_or(Q::from_integer(0), |r| r.duration.seconds()) + delta;
    set_clip_duration(c, dur);
    if let Some(k) = next
        && let Some(cid) = d.lane_ref(lane).items[k].clip().and_then(clip_id).cloned()
    {
        d.mark_changed(&cid);
    }
    Ok(())
}

/// Transitions sit on edit points; keep them on the point when it rolls.
fn move_zero_length(l: &mut Lane, from: Q, to: Q) {
    for p in &mut l.items {
        if p.start == from && p.duration() == Q::from_integer(0) {
            p.start = to;
        }
    }
}

// --- move ---

#[derive(Debug, Clone)]
pub struct Move {
    pub clip: Id,
    /// New timeline start of `clip`; linked clips move by the same amount.
    pub to: Q,
    /// Move `clip` (not its linked clips) to this track.
    pub track: Option<String>,
    pub linked: bool,
    /// Push what is at the destination right (just far enough to fit, with
    /// its linked clips) instead of overwriting it.
    pub push: bool,
    /// Swap places instead: the clip ends up at `to` and what lay between
    /// its old and new place slides over to fill its old place. Nothing
    /// outside that span moves.
    pub reorder: bool,
}

/// Moves leave a gap behind and overwrite whatever is at the destination,
/// or with `push` move it right to make room.
pub fn move_clip(tl: &Timeline, p: &Move) -> Result<(Timeline, EditReport)> {
    let mut d = Doc::new(tl)?;
    let to = d.snap(p.to);
    if p.reorder {
        if p.push || p.track.is_some() {
            return Err(EditError::new("reorder works within the clip's own tracks; leave out push and track"));
        }
        reorder(&mut d, &p.clip, to, p.linked)?;
        return d.finish();
    }
    let delta = to - d.placed(&p.clip)?.start;
    let dest = p.track.as_deref().map(|r| d.find_track(r)).transpose()?;
    let mut moving = Vec::new();
    for (lane, id) in d.group(&p.clip, p.linked)? {
        let (_, j) = d.locate(&id)?;
        let mut it = d.lane(lane).items.remove(j);
        it.start += delta;
        need_positive("the new position", it.start)?;
        let target = if id == p.clip { dest.unwrap_or(lane) } else { lane };
        if d.kind(target) != d.kind(lane) {
            return Err(EditError::new(format!(
                "{id} is on a {:?} track and cannot go on {:?} track {}",
                d.kind(lane),
                d.kind(target),
                d.track_name(target)
            )));
        }
        moving.push((target, id, it));
    }
    if p.push {
        let room: Vec<(usize, Q, Q)> = moving.iter().map(|(t, _, it)| (*t, it.start, it.duration())).collect();
        make_room(&mut d, &room, Room::Fit)?;
    }
    for (target, id, it) in moving {
        let mut ids = std::mem::replace(&mut d.ids, Ids::new(HashSet::new()));
        let l = d.lane(target);
        l.remove_range(it.start, it.end(), &mut ids)?;
        l.place(it);
        d.ids = ids;
        d.mark_changed(&id);
    }
    d.finish()
}

/// Rotate the span between a clip's old and new place: the clip lands at
/// `to` and what it passed slides over by the clip's length. The same span
/// rotates on every track of the clip's link group and on every track that
/// holds a partner of something in the span, so links stay in sync; if a
/// clip crosses an edge of the span, or a link would be pulled apart, the
/// move is refused.
fn reorder(d: &mut Doc, clip: &Id, to: Q, linked: bool) -> Result<()> {
    let (lane0, j) = d.locate(clip)?;
    let o = d.lane_ref(lane0).items[j].start;
    let len = d.lane_ref(lane0).items[j].duration();
    if to == o {
        return Ok(());
    }
    need_positive("the new position", to)?;
    // [lo, mid) and [mid, hi) trade places.
    let (lo, mid, hi) = if to < o { (to, o, o + len) } else { (o, o + len, to + len) };
    let delta = |start: Q| -> Q {
        let first = start < mid;
        match (to < o, first) {
            (true, true) => len,     // what the clip passed moves right
            (true, false) => to - o, // the clip
            (false, true) => to - o, // the clip
            (false, false) => -len,  // what the clip passed moves left
        }
    };
    let inside = |p: &Placed| p.start >= lo && p.start < hi && !(p.duration() == Q::from_integer(0) && p.start == lo);

    let mut lanes: Vec<usize> = Vec::new();
    for (l, _) in d.group(clip, linked)? {
        if !lanes.contains(&l) {
            lanes.push(l);
        }
    }
    loop {
        let links: HashSet<Id> = lanes
            .iter()
            .flat_map(|&l| d.lane_ref(l).items.iter().filter(|p| inside(p)).filter_map(|p| p.clip().and_then(clip_link).cloned()))
            .collect();
        let more: Vec<usize> = d
            .track_indices()
            .into_iter()
            .filter(|l| !lanes.contains(l))
            .filter(|&l| d.lane_ref(l).items.iter().any(|p| p.clip().and_then(clip_link).is_some_and(|k| links.contains(k))))
            .collect();
        if more.is_empty() {
            break;
        }
        lanes.extend(more);
    }

    // Gaps across an edge of the span are cut; clips across one cannot move.
    let mut ids = std::mem::replace(&mut d.ids, Ids::new(HashSet::new()));
    for &l in &lanes {
        for b in [lo, mid, hi] {
            let straddler = d.lane_ref(l).items.iter().find(|p| p.start < b && b < p.end()).map(|p| p.clip().and_then(clip_id).cloned());
            match straddler {
                None => {}
                Some(None) => {
                    d.lane(l).split_at(b, &mut ids);
                }
                Some(Some(id)) => {
                    d.ids = ids;
                    return Err(EditError::new(format!(
                        "cannot swap: {id} on track {} runs across {}",
                        d.track_name(l),
                        tc(b, d.rate)
                    )));
                }
            }
        }
    }
    d.ids = ids;

    // Every member of a link must move by the same amount.
    let mut by_link: HashMap<Id, Q> = HashMap::new();
    for l in d.track_indices() {
        let rotating = lanes.contains(&l);
        for p in &d.lane_ref(l).items {
            let Some(link) = p.clip().and_then(clip_link) else { continue };
            let dt = if rotating && inside(p) { delta(p.start) } else { Q::from_integer(0) };
            if let Some(&other) = by_link.get(link)
                && other != dt
            {
                let id = p.clip().and_then(clip_id).map_or("a clip".to_owned(), ToString::to_string);
                return Err(EditError::new(format!("cannot swap: it would pull {id} away from its linked clips")));
            }
            by_link.insert(link.clone(), dt);
        }
    }

    for &l in &lanes {
        let mut moved = Vec::new();
        let lane = d.lane(l);
        // Transitions on the edges of the span would end up between the wrong clips.
        lane.items.retain(|p| !(p.duration() == Q::from_integer(0) && [lo, mid, hi].contains(&p.start)));
        for p in &mut lane.items {
            if inside(p) {
                p.start += delta(p.start);
                if let Some(id) = p.clip().and_then(clip_id) {
                    moved.push(id.clone());
                }
            }
        }
        lane.items.sort_by(|a, b| a.start.cmp(&b.start).then(a.duration().cmp(&b.duration())));
        for id in moved {
            d.mark_changed(&id);
        }
    }
    Ok(())
}

// --- ripple delete ---

#[derive(Debug, Clone, Default)]
pub struct RippleDelete {
    /// Timeline ranges `[from, to)` to remove.
    pub ranges: Vec<(Q, Q)>,
    /// Remove the ranges of timeline markers with this name (e.g. `silence`).
    pub markers: Option<String>,
    /// Tracks the ranges apply to; all tracks if empty.
    pub tracks: Vec<String>,
    /// Remove these clips (and their linked clips) and close the gap on their tracks.
    pub clips: Vec<Id>,
    pub linked: bool,
}

pub fn ripple_delete(tl: &Timeline, p: &RippleDelete) -> Result<(Timeline, EditReport)> {
    let mut d = Doc::new(tl)?;
    let mut global: Vec<(Q, Q)> = Vec::new();
    for &(a, b) in &p.ranges {
        let (a, b) = (d.snap(a), d.snap(b));
        need_positive("--from", a)?;
        if a >= b {
            return Err(EditError::new(format!("range {}..{} is empty or backwards", tc(a, d.rate), tc(b, d.rate))));
        }
        global.push((a, b));
    }
    if let Some(name) = &p.markers {
        let found: Vec<(Q, Q)> = d
            .tl
            .tracks
            .markers
            .iter()
            .filter(|m| &m.name == name && m.marked_range.duration.value > Q::from_integer(0))
            .map(|m| (d.snap(m.marked_range.start_time.seconds()), d.snap(m.marked_range.end_exclusive().seconds())))
            .filter(|(a, b)| a < b)
            .collect();
        if found.is_empty() {
            return Err(EditError::new(format!("no timeline markers named `{name}` with a duration")));
        }
        global.extend(found);
    }
    let lanes = d.tracks_or_all(&p.tracks)?;
    let mut per_lane: BTreeMap<usize, Vec<(Q, Q)>> = BTreeMap::new();
    if !global.is_empty() {
        for &l in &lanes {
            per_lane.entry(l).or_default().extend(&global);
        }
    }
    for id in &p.clips {
        for (lane, cid) in d.group(id, p.linked)? {
            let it = d.placed(&cid)?;
            per_lane.entry(lane).or_default().push((it.start, it.end()));
        }
    }
    if per_lane.is_empty() {
        return Err(EditError::new("nothing to delete: give ranges, a marker name or clips"));
    }

    let global = merge(global);
    let mut rippled: HashSet<usize> = per_lane.keys().copied().collect();
    let old = d.starts();
    // Marker rippling is decided against track ends from before the edit.
    let ends: HashMap<usize, Q> = d.track_indices().into_iter().map(|i| (i, d.lane_ref(i).end())).collect();
    for (lane, ranges) in per_lane {
        let mut ids = std::mem::replace(&mut d.ids, Ids::new(HashSet::new()));
        let l = d.lane(lane);
        for (a, b) in merge(ranges).into_iter().rev() {
            l.remove_range(a, b, &mut ids)?;
            l.shift_from(b, a - b);
        }
        d.ids = ids;
    }
    rippled.extend(follow_links(&mut d, &old, &rippled)?);
    let ripple_marks: Vec<(Q, Q)> =
        global.iter().copied().filter(|&(a, _)| ends.iter().all(|(i, &end)| rippled.contains(i) || end <= a)).collect();
    for (a, b) in ripple_marks.into_iter().rev() {
        ripple_markers_delete(&mut d.tl.tracks.markers, a, b);
    }
    d.finish()
}

/// Sort and merge overlapping or touching ranges.
fn merge(mut r: Vec<(Q, Q)>) -> Vec<(Q, Q)> {
    r.sort();
    let mut out: Vec<(Q, Q)> = Vec::with_capacity(r.len());
    for (a, b) in r {
        match out.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

// --- delete (lift) ---

#[derive(Debug, Clone, Default)]
pub struct Delete {
    pub clips: Vec<Id>,
    pub linked: bool,
}

/// Remove clips and leave gaps (nothing moves).
pub fn delete(tl: &Timeline, p: &Delete) -> Result<(Timeline, EditReport)> {
    let mut d = Doc::new(tl)?;
    if p.clips.is_empty() {
        return Err(EditError::new("no clips given"));
    }
    for id in &p.clips {
        for (lane, cid) in d.group(id, p.linked)? {
            if let Ok((_, j)) = d.locate(&cid) {
                d.lane(lane).items.remove(j);
            }
        }
    }
    d.finish()
}

// --- speed and volume ---

#[derive(Debug, Clone)]
pub struct SetSpeed {
    pub clip: Id,
    /// Exact speed, e.g. 3/2. Must be between 1/100 and 100.
    pub speed: Q,
    pub linked: bool,
    /// Shift later items so nothing is overwritten or left as a gap (default).
    /// Without it, a slower clip overwrites what follows and a faster one leaves a gap.
    pub ripple: bool,
}

pub fn set_speed(tl: &Timeline, p: &SetSpeed) -> Result<(Timeline, EditReport)> {
    let mut d = Doc::new(tl)?;
    let (lo, hi) = (Q::new(1, 100), Q::from_integer(100));
    if p.speed < lo || p.speed > hi {
        return Err(EditError::new("speed must be between 1/100 and 100 (reverse playback is not supported yet)"));
    }
    let old = d.starts();
    let group = d.group(&p.clip, p.linked)?;
    let pinned: HashSet<usize> = group.iter().map(|g| g.0).collect();
    for (lane, id) in group {
        let (_, j) = d.locate(&id)?;
        let it = &d.lane_ref(lane).items[j];
        let (start, old_dur) = (it.start, it.duration());
        let consumed = old_dur * clip_speed(it.clip().expect("located a clip"));
        // Round down so the clip never needs more media than it had.
        let new_dur = d.snap_down(consumed / p.speed);
        if new_dur < d.frame() {
            return Err(EditError::new(format!("at {} {id} would be shorter than one frame", speed_text(p.speed))));
        }
        let mut ids = std::mem::replace(&mut d.ids, Ids::new(HashSet::new()));
        let l = d.lane(lane);
        if p.ripple {
            l.shift_from(start + old_dur, new_dur - old_dur);
        } else if new_dur > old_dur {
            l.remove_range(start + old_dur, start + new_dur, &mut ids)?;
        }
        d.ids = ids;
        let (_, j) = d.locate(&id)?;
        let c = d.lane(lane).items[j].clip_mut().expect("located a clip");
        set_clip_duration(c, new_dur);
        apply_speed(c, p.speed, &id);
        d.mark_changed(&id);
    }
    if p.ripple {
        follow_links(&mut d, &old, &pinned)?;
    }
    d.finish()
}

fn speed_text(s: Q) -> String {
    if s.is_integer() { format!("{}x", s.numer()) } else { format!("{}/{}x", s.numer(), s.denom()) }
}

/// Store the speed in `metadata.ve.clip` and mirror it as OTIO's own
/// `LinearTimeWarp` effect so other tools see it.
fn apply_speed(c: &mut Clip, speed: Q, id: &Id) {
    c.metadata.ve.clip.get_or_insert_with(|| ClipMeta::new(id.clone())).speed = speed;
    let pos = c.effects.iter().position(|e| e["OTIO_SCHEMA"].as_str().is_some_and(|s| s.starts_with("LinearTimeWarp")));
    if speed == Q::from_integer(1) {
        if let Some(i) = pos {
            c.effects.remove(i);
        }
        return;
    }
    let scalar = *speed.numer() as f64 / *speed.denom() as f64;
    match pos {
        Some(i) => c.effects[i]["time_scalar"] = json!(scalar),
        None => c.effects.push(json!({
            "OTIO_SCHEMA": "LinearTimeWarp.1",
            "metadata": {},
            "name": "",
            "effect_name": "LinearTimeWarp",
            "enabled": true,
            "time_scalar": scalar
        })),
    }
}

#[derive(Debug, Clone)]
pub struct SetVolume {
    pub clip: Id,
    /// Gain in dB, -100 to +30. 0 is unity.
    pub db: f64,
    pub linked: bool,
}

/// Volume applies to the audio clips of the group (a video clip's linked audio).
pub fn set_volume(tl: &Timeline, p: &SetVolume) -> Result<(Timeline, EditReport)> {
    let mut d = Doc::new(tl)?;
    if !p.db.is_finite() || !(-100.0..=30.0).contains(&p.db) {
        return Err(EditError::new("volume must be between -100 and +30 dB"));
    }
    let mut any = false;
    for (lane, id) in d.group(&p.clip, p.linked)? {
        if d.kind(lane) != TrackKind::Audio {
            continue;
        }
        let (_, j) = d.locate(&id)?;
        let c = d.lane(lane).items[j].clip_mut().expect("located a clip");
        c.metadata.ve.clip.get_or_insert_with(|| ClipMeta::new(id.clone())).volume_db = p.db;
        d.mark_changed(&id);
        any = true;
    }
    if !any {
        return Err(EditError::new(format!("{} has no audio clip (on an audio track) to set the volume of", p.clip)));
    }
    d.finish()
}

/// Break a clip's link with its partners (a video clip and its audio), so
/// each is edited alone from now on. Positions and media do not change.
pub fn unlink(tl: &Timeline, clip: &Id) -> Result<(Timeline, EditReport)> {
    let mut d = Doc::new(tl)?;
    let group = d.group(clip, true)?;
    if group.len() < 2 {
        return Err(EditError::new(format!("{clip} is not linked to any other clip")));
    }
    for (_, id) in group {
        let (i, j) = d.locate(&id)?;
        let c = d.lane(i).items[j].clip_mut().expect("located a clip");
        if let Some(m) = c.metadata.ve.clip.as_mut() {
            m.link = None;
        }
        d.mark_changed(&id);
    }
    d.finish()
}

/// Add `db` to the volume of every audio clip overlapping `range` (timeline
/// seconds, `[from, to)`), or of every audio clip when `None`. Volumes stay
/// within -100..+30 dB and are rounded to 0.1 dB. Returns the clips touched.
pub fn add_gain(tl: &Timeline, range: Option<(Q, Q)>, db: f64) -> Result<(Timeline, EditReport, Vec<Id>)> {
    if !db.is_finite() || db.abs() > 130.0 {
        return Err(EditError::new("gain must be a finite number of dB"));
    }
    let mut d = Doc::new(tl)?;
    let mut touched = Vec::new();
    for i in d.track_indices() {
        if d.kind(i) != TrackKind::Audio {
            continue;
        }
        let ids: Vec<Id> = d
            .lane_ref(i)
            .items
            .iter()
            .filter(|p| range.is_none_or(|(a, b)| p.start < b && p.end() > a))
            .filter_map(|p| p.clip().and_then(clip_id).cloned())
            .collect();
        for id in ids {
            let Some(j) = d.lane_ref(i).find_clip(&id) else { continue };
            let c = d.lane(i).items[j].clip_mut().expect("found a clip");
            let m = c.metadata.ve.clip.get_or_insert_with(|| ClipMeta::new(id.clone()));
            m.volume_db = ((m.volume_db + db).clamp(-100.0, 30.0) * 10.0).round() / 10.0;
            d.mark_changed(&id);
            touched.push(id);
        }
    }
    if touched.is_empty() {
        return Err(EditError::new("no audio clips there to change the volume of"));
    }
    let (next, report) = d.finish()?;
    Ok((next, report, touched))
}

/// The report for a command that only touched markers: just the duration.
pub fn report_for(tl: &Timeline) -> EditReport {
    let rate = tl.output_rate();
    let end = tl
        .tracks
        .children
        .iter()
        .filter(|c| matches!(c, Composable::Track(_)))
        .map(crate::lane::item_seconds)
        .max()
        .unwrap_or(Q::from_integer(0));
    EditReport { duration: tc(end, rate), ..Default::default() }
}
