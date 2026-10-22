//! `edit.*`, `marker.*` and `history.*` methods.
//!
//! Params carry times as the strings people type, parsed at the project rate
//! (media times at the asset's rate); the commands themselves live in
//! `ve-ops` as pure functions.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ve_history::{OpenGroup, StepSummary};
use ve_model::meta::MarkerSource;
use ve_model::otio::TrackKind;
use ve_model::{Id, Timeline};
use ve_ops::EditReport;
use ve_ops::edit::{self, Delete, Insert, Move, RippleDelete, SetSpeed, SetVolume, Split, Trim};
use ve_ops::markers::{self, NewMarker};
use ve_ops::tracks::{self, TrackPlace, TrackReport};
use ve_protocol::{Actor, ActorKind, RpcError};

use crate::commit::{Committed, actor_label, commit, commit_doc, mutation_params, parse_ratio, parse_time};
use crate::project::OpenProject;

fn yes() -> bool {
    true
}

fn edit_err(e: ve_ops::EditError) -> RpcError {
    RpcError::invalid_params(e.0)
}

fn time(tl: &Timeline, field: &str, s: &str) -> Result<ve_model::time::Q, RpcError> {
    parse_time(field, s, tl.output_rate())
}

fn opt_time(tl: &Timeline, field: &str, s: Option<&String>) -> Result<Option<ve_model::time::Q>, RpcError> {
    s.map(|s| time(tl, field, s)).transpose()
}

mutation_params! {
    /// `edit.insert`: put an asset on the timeline. An asset with picture and
    /// sound becomes a linked video clip and audio clip.
    pub struct InsertParams {
        pub asset: Id,
        /// Track (ID or name) for the video, or for the audio of an audio-only
        /// asset. Defaults to the first track of the right kind.
        #[serde(default)]
        pub track: Option<String>,
        /// Track for the audio of an asset with both. Defaults to the first audio track.
        #[serde(default)]
        pub audio_track: Option<String>,
        /// Timeline position (`00:00:10:00`, `f300`, `10s`). Defaults to the end of the target tracks.
        #[serde(default)]
        pub at: Option<String>,
        /// Media in-point, at the media's frame rate. Defaults to the start of the file.
        #[serde(default)]
        pub source_in: Option<String>,
        /// Media out-point (exclusive). Defaults to the end of the file.
        #[serde(default)]
        pub source_out: Option<String>,
        /// Length to use instead of `source_out`.
        #[serde(default)]
        pub duration: Option<String>,
        /// Include the picture (default true).
        #[serde(default = "yes")]
        pub video: bool,
        /// Include the sound (default true).
        #[serde(default = "yes")]
        pub audio: bool,
        /// Overwrite what is at the position instead of pushing it later.
        #[serde(default)]
        pub overwrite: bool,
    }
}

pub fn insert(p: &mut OpenProject, a: InsertParams) -> Result<Committed<EditReport>, RpcError> {
    commit(p, a.mutation(), "edit.insert", |tl| {
        // Media times are typed at the media's own rate.
        let mrate = tl
            .metadata
            .ve
            .assets
            .iter()
            .find(|x| x.id == a.asset)
            .and_then(|x| x.media.as_ref())
            .map_or(tl.output_rate(), |m| m.video.as_ref().map_or(m.duration.rate, |v| v.rate));
        let media = |field: &str, s: Option<&String>| s.map(|s| parse_time(field, s, mrate)).transpose();
        let op = Insert {
            asset: a.asset.clone(),
            track: a.track.clone(),
            audio_track: a.audio_track.clone(),
            at: opt_time(tl, "at", a.at.as_ref())?,
            source_in: media("source_in", a.source_in.as_ref())?,
            source_out: media("source_out", a.source_out.as_ref())?,
            duration: media("duration", a.duration.as_ref())?,
            video: a.video,
            audio: a.audio,
            overwrite: a.overwrite,
        };
        edit::insert(tl, &op).map_err(edit_err)
    })
}

mutation_params! {
    /// `edit.split`: cut clips in two at a timeline time.
    pub struct SplitParams {
        pub at: String,
        /// Clips to cut (and their linked clips). Empty: every clip under `at`.
        #[serde(default)]
        pub clips: Vec<Id>,
        /// With no `clips`, only cut on these tracks.
        #[serde(default)]
        pub tracks: Vec<String>,
        /// Also cut linked clips (default true).
        #[serde(default = "yes")]
        pub linked: bool,
    }
}

pub fn split(p: &mut OpenProject, a: SplitParams) -> Result<Committed<EditReport>, RpcError> {
    commit(p, a.mutation(), "edit.split", |tl| {
        let op = Split { at: time(tl, "at", &a.at)?, clips: a.clips.clone(), tracks: a.tracks.clone(), linked: a.linked };
        edit::split(tl, &op).map_err(edit_err)
    })
}

mutation_params! {
    /// `edit.trim`: move a clip's in and/or out point. Ripple (default) shifts
    /// what follows; `roll` moves the edit point so the neighbour gives or
    /// takes the time. Roll one side of an A/V pair (`linked: false`) for a J/L cut.
    pub struct TrimParams {
        /// In-point change, signed: `+15f` starts later (shorter clip), `-15f` earlier.
        #[serde(default, rename = "in")]
        pub in_delta: Option<String>,
        /// Out-point change, signed: `-1s` ends a second earlier, `+10f` later.
        #[serde(default, rename = "out")]
        pub out_delta: Option<String>,
        pub clip: Id,
        #[serde(default)]
        pub roll: bool,
        #[serde(default = "yes")]
        pub linked: bool,
    }
}

pub fn trim(p: &mut OpenProject, a: TrimParams) -> Result<Committed<EditReport>, RpcError> {
    commit(p, a.mutation(), "edit.trim", |tl| {
        let op = Trim {
            clip: a.clip.clone(),
            in_delta: opt_time(tl, "in", a.in_delta.as_ref())?,
            out_delta: opt_time(tl, "out", a.out_delta.as_ref())?,
            roll: a.roll,
            linked: a.linked,
        };
        edit::trim(tl, &op).map_err(edit_err)
    })
}

mutation_params! {
    /// `edit.move`: move a clip (and its linked clips) to a new start,
    /// overwriting what is there and leaving a gap behind.
    pub struct MoveParams {
        pub clip: Id,
        /// New timeline start of `clip`.
        pub to: String,
        /// Put `clip` (not its linked clips) on this track.
        #[serde(default)]
        pub track: Option<String>,
        #[serde(default = "yes")]
        pub linked: bool,
    }
}

pub fn move_clip(p: &mut OpenProject, a: MoveParams) -> Result<Committed<EditReport>, RpcError> {
    commit(p, a.mutation(), "edit.move", |tl| {
        let op = Move { clip: a.clip.clone(), to: time(tl, "to", &a.to)?, track: a.track.clone(), linked: a.linked };
        edit::move_clip(tl, &op).map_err(edit_err)
    })
}

/// A timeline range `[from, to)`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RangeArg {
    pub from: String,
    pub to: String,
}

mutation_params! {
    /// `edit.ripple_delete`: remove time and close the gap. Give ranges, a
    /// marker name (e.g. `silence` after `analyze.silence`), or clips.
    pub struct RippleDeleteParams {
        #[serde(default)]
        pub ranges: Vec<RangeArg>,
        /// Delete the ranges of every timeline marker with this name.
        #[serde(default)]
        pub markers: Option<String>,
        /// Tracks the ranges apply to; all tracks if empty (keeps A/V in sync).
        #[serde(default)]
        pub tracks: Vec<String>,
        /// Delete these clips (and linked clips) and close the gaps on their tracks.
        #[serde(default)]
        pub clips: Vec<Id>,
        #[serde(default = "yes")]
        pub linked: bool,
    }
}

pub fn ripple_delete(p: &mut OpenProject, a: RippleDeleteParams) -> Result<Committed<EditReport>, RpcError> {
    commit(p, a.mutation(), "edit.ripple_delete", |tl| {
        let ranges = a
            .ranges
            .iter()
            .map(|r| Ok((time(tl, "from", &r.from)?, time(tl, "to", &r.to)?)))
            .collect::<Result<Vec<_>, RpcError>>()?;
        let op = RippleDelete { ranges, markers: a.markers.clone(), tracks: a.tracks.clone(), clips: a.clips.clone(), linked: a.linked };
        edit::ripple_delete(tl, &op).map_err(edit_err)
    })
}

mutation_params! {
    /// `edit.delete`: remove clips and leave gaps (nothing moves).
    pub struct DeleteParams {
        pub clips: Vec<Id>,
        #[serde(default = "yes")]
        pub linked: bool,
    }
}

pub fn delete(p: &mut OpenProject, a: DeleteParams) -> Result<Committed<EditReport>, RpcError> {
    commit(p, a.mutation(), "edit.delete", |tl| {
        edit::delete(tl, &Delete { clips: a.clips.clone(), linked: a.linked }).map_err(edit_err)
    })
}

mutation_params! {
    /// `edit.set_speed`: play a clip faster or slower. Also writes OTIO's
    /// `LinearTimeWarp` so other editors see the speed.
    pub struct SetSpeedParams {
        pub clip: Id,
        /// Exact speed: `3/2`, `1.5`, `0.5`.
        pub speed: String,
        #[serde(default = "yes")]
        pub linked: bool,
        /// Shift what follows (default). Without it a slower clip overwrites what follows.
        #[serde(default = "yes")]
        pub ripple: bool,
    }
}

pub fn set_speed(p: &mut OpenProject, a: SetSpeedParams) -> Result<Committed<EditReport>, RpcError> {
    commit(p, a.mutation(), "edit.set_speed", |tl| {
        let op = SetSpeed { clip: a.clip.clone(), speed: parse_ratio("speed", &a.speed)?, linked: a.linked, ripple: a.ripple };
        edit::set_speed(tl, &op).map_err(edit_err)
    })
}

mutation_params! {
    /// `edit.set_volume`: gain for a clip's audio (a video clip's linked audio).
    pub struct SetVolumeParams {
        pub clip: Id,
        /// Gain in dB, -100 to +30; 0 is unchanged.
        pub db: f64,
        #[serde(default = "yes")]
        pub linked: bool,
    }
}

pub fn set_volume(p: &mut OpenProject, a: SetVolumeParams) -> Result<Committed<EditReport>, RpcError> {
    commit(p, a.mutation(), "edit.set_volume", |tl| {
        edit::set_volume(tl, &SetVolume { clip: a.clip.clone(), db: a.db, linked: a.linked }).map_err(edit_err)
    })
}

mutation_params! {
    /// `edit.unlink`: break a clip's link with its partners (video and its audio).
    pub struct UnlinkParams {
        pub clip: Id,
    }
}

pub fn unlink(p: &mut OpenProject, a: UnlinkParams) -> Result<Committed<EditReport>, RpcError> {
    commit(p, a.mutation(), "edit.unlink", |tl| edit::unlink(tl, &a.clip).map_err(edit_err))
}

// --- markers ---

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct MarkerIds {
    pub markers: Vec<Id>,
}

mutation_params! {
    /// `marker.add`: a timeline marker.
    pub struct MarkerAddParams {
        pub at: String,
        #[serde(default)]
        pub duration: Option<String>,
        #[serde(default)]
        pub name: String,
        /// OTIO color: red, green, blue, yellow, ... (default red).
        #[serde(default)]
        pub color: Option<String>,
        #[serde(default)]
        pub comment: String,
    }
}

pub fn marker_add(p: &mut OpenProject, a: MarkerAddParams) -> Result<Committed<MarkerIds>, RpcError> {
    let source = match a.actor.as_ref().map(|x| x.kind) {
        Some(ActorKind::Agent) => MarkerSource::Agent,
        Some(ActorKind::Plugin) => MarkerSource::Analyzer,
        _ => MarkerSource::User,
    };
    commit(p, a.mutation(), "marker.add", |tl| {
        let m = NewMarker {
            start: time(tl, "at", &a.at)?,
            duration: opt_time(tl, "duration", a.duration.as_ref())?.unwrap_or_default(),
            name: a.name.clone(),
            color: a.color.clone(),
            comment: a.comment.clone(),
            source,
            payload: serde_json::Value::Null,
        };
        let (next, ids) = markers::add(tl, vec![m]).map_err(edit_err)?;
        Ok((next, MarkerIds { markers: ids }))
    })
}

mutation_params! {
    /// `marker.remove`: timeline markers by ID and/or by name (`silence` clears an analyzer's markers).
    pub struct MarkerRemoveParams {
        #[serde(default)]
        pub ids: Vec<Id>,
        #[serde(default)]
        pub name: Option<String>,
    }
}

pub fn marker_remove(p: &mut OpenProject, a: MarkerRemoveParams) -> Result<Committed<MarkerIds>, RpcError> {
    commit(p, a.mutation(), "marker.remove", |tl| {
        let (next, ids) = markers::remove(tl, &a.ids, a.name.as_deref()).map_err(edit_err)?;
        Ok((next, MarkerIds { markers: ids }))
    })
}

// --- history ---

mutation_params! {
    /// `history.undo` / `history.redo`: one step (a single edit or a whole group).
    pub struct UndoParams {}
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct UndoResult {
    /// The step that was undone or redone.
    pub step: StepSummary,
}

pub fn undo(p: &mut OpenProject, a: UndoParams, redo: bool) -> Result<Committed<UndoResult>, RpcError> {
    let current = p.revision();
    if a.base_rev != current {
        return Err(RpcError::stale_revision(current));
    }
    let step = if redo { p.history.redo_step() } else { p.history.undo_step() };
    let Some(step) = step.map(<[ve_history::Entry]>::to_vec) else {
        return Err(RpcError::invalid_params(if redo { "nothing to redo" } else { "nothing to undo" }));
    };
    let mut doc = p.timeline.clone();
    if redo {
        for e in &step {
            doc = ve_ops::apply_patch(&doc, &e.patch).map_err(edit_err)?;
        }
    } else {
        for e in step.iter().rev() {
            doc = ve_ops::apply_patch(&doc, &e.inverse).map_err(edit_err)?;
        }
    }
    let summary = p.history.summary(&step);
    let command = if redo { "history.redo" } else { "history.undo" };
    commit_doc(p, a.mutation(), command, doc, UndoResult { step: summary })
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryListParams {
    /// Most recent steps to list (default 20).
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct HistoryList {
    pub revision: u64,
    /// Steps `history.undo` would undo, most recent first.
    pub undo: Vec<StepSummary>,
    /// Steps `history.redo` would bring back, next first.
    pub redo: Vec<StepSummary>,
    /// Open groups by actor.
    pub open_groups: Vec<(String, OpenGroup)>,
}

pub fn list(p: &OpenProject, a: HistoryListParams) -> HistoryList {
    HistoryList {
        revision: p.revision(),
        undo: p.history.steps(a.limit.unwrap_or(20)),
        redo: p.history.redo_steps(),
        open_groups: p.history.open_groups().map(|(k, g)| (k.clone(), g.clone())).collect(),
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BeginGroupParams {
    /// What the group does, shown in `history.list` ("remove silences").
    pub label: String,
    #[serde(default)]
    pub actor: Option<Actor>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EndGroupParams {
    #[serde(default)]
    pub actor: Option<Actor>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct GroupEnded {
    pub group: OpenGroup,
    /// Edits the group holds; `history.undo` removes them all at once.
    pub edits: usize,
}

pub fn begin_group(p: &mut OpenProject, a: BeginGroupParams) -> Result<OpenGroup, RpcError> {
    let actor = actor_label(a.actor.as_ref());
    let id = loop {
        let id = Id::new("grp");
        if !p.timeline.ids().contains(&id) {
            break id;
        }
    };
    let g = p.history.begin_group(&actor, id, &a.label).map_err(|e| RpcError::invalid_params(e.0))?;
    p.save_history();
    Ok(g)
}

pub fn end_group(p: &mut OpenProject, a: EndGroupParams) -> Result<GroupEnded, RpcError> {
    let actor = actor_label(a.actor.as_ref());
    let (group, edits) = p.history.end_group(&actor).map_err(|e| RpcError::invalid_params(e.0))?;
    p.save_history();
    Ok(GroupEnded { group, edits })
}

// --- tracks ---

/// Track kind in params.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TrackKindParam {
    Video,
    Audio,
}

mutation_params! {
    /// `track.add`: a new empty track. Any number of video and audio tracks.
    pub struct TrackAddParams {
        pub kind: TrackKindParam,
        /// Defaults to the next free `V<n>` / `A<n>`. Must not match another track's name or ID.
        #[serde(default)]
        pub name: Option<String>,
        /// Position among tracks of the same kind: 0 is the first (for video, the bottom layer).
        /// Defaults to the end (for video, the top layer).
        #[serde(default)]
        pub index: Option<usize>,
    }
}

pub fn track_add(p: &mut OpenProject, a: TrackAddParams) -> Result<Committed<TrackReport>, RpcError> {
    let kind = match a.kind {
        TrackKindParam::Video => TrackKind::Video,
        TrackKindParam::Audio => TrackKind::Audio,
    };
    commit(p, a.mutation(), "track.add", |tl| tracks::add(tl, kind, a.name.as_deref(), a.index).map_err(edit_err))
}

mutation_params! {
    /// `track.remove`: delete a track.
    pub struct TrackRemoveParams {
        /// Track ID or name.
        pub track: String,
        /// Also delete the track's clips. Without it, a track with clips is not removed.
        #[serde(default)]
        pub force: bool,
    }
}

pub fn track_remove(p: &mut OpenProject, a: TrackRemoveParams) -> Result<Committed<TrackReport>, RpcError> {
    commit(p, a.mutation(), "track.remove", |tl| tracks::remove(tl, &a.track, a.force).map_err(edit_err))
}

mutation_params! {
    /// `track.rename`.
    pub struct TrackRenameParams {
        /// Track ID or current name.
        pub track: String,
        /// New name; must not match another track's name or ID.
        pub name: String,
    }
}

pub fn track_rename(p: &mut OpenProject, a: TrackRenameParams) -> Result<Committed<TrackReport>, RpcError> {
    commit(p, a.mutation(), "track.rename", |tl| tracks::rename(tl, &a.track, &a.name).map_err(edit_err))
}

mutation_params! {
    /// `track.move`: change a track's position among tracks of its kind.
    pub struct TrackMoveParams {
        /// Track ID or name.
        pub track: String,
        /// New position among tracks of the same kind: 0 is the first (for video, the bottom layer).
        pub index: usize,
    }
}

pub fn track_move(p: &mut OpenProject, a: TrackMoveParams) -> Result<Committed<TrackReport>, RpcError> {
    commit(p, a.mutation(), "track.move", |tl| tracks::reorder(tl, &a.track, a.index).map_err(edit_err))
}

/// `track.list` result.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TrackList {
    pub revision: u64,
    /// Every track in document order.
    pub tracks: Vec<TrackPlace>,
}

pub fn track_list(p: &OpenProject) -> TrackList {
    TrackList { revision: p.revision(), tracks: tracks::list(&p.timeline) }
}
