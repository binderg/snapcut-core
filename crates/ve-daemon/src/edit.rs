//! `edit.*`, `marker.*` and `history.*` methods.
//!
//! Params carry times as the strings people type, parsed at the project rate
//! (media times at the asset's rate); the commands themselves live in
//! `ve-ops` as pure functions.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ve_history::{OpenGroup, StepSummary};
use ve_model::meta::{Channels, MarkerSource};
use ve_model::otio::TrackKind;
use ve_model::{Id, Timeline};
use ve_ops::EditReport;
use ve_ops::edit::{self, Delete, Insert, Move, RippleDelete, SetAudio, SetSpeed, SetVolume, Split, Trim};
use ve_ops::markers::{self, NewMarker};
use ve_ops::tracks::{self, TrackPlace, TrackReport};
use ve_ops::transform::{self, TransformChange, TransformReport};
use ve_protocol::{Actor, ActorKind, RpcError};

use crate::commit::{Committed, actor_label, commit, commit_doc, mutation_params, parse_ratio, parse_time};
use crate::project::OpenProject;

pub(crate) fn yes() -> bool {
    true
}

pub(crate) fn edit_err(e: ve_ops::EditError) -> RpcError {
    RpcError::invalid_params(e.0)
}

pub(crate) fn time(tl: &Timeline, field: &str, s: &str) -> Result<ve_model::time::Q, RpcError> {
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
        /// Audio streams to bring in (numbers from the asset's `audio_streams`). Default: every
        /// stream, each on its own audio track (the first on the audio target, the rest on the
        /// audio tracks after it, added if missing), all linked to the picture.
        #[serde(default)]
        pub streams: Option<Vec<u32>>,
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
            streams: a.streams.clone(),
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
    /// leaving a gap behind and overwriting what is at the destination, or
    /// with `reorder` trading places with what it passes.
    pub struct MoveParams {
        pub clip: Id,
        /// New timeline start of `clip`.
        pub to: String,
        /// Put `clip` (not its linked clips) on this track.
        #[serde(default)]
        pub track: Option<String>,
        #[serde(default = "yes")]
        pub linked: bool,
        /// Swap places: what lies between the old and new place slides over
        /// to fill the old one. Nothing else moves.
        #[serde(default)]
        pub reorder: bool,
    }
}

pub fn move_clip(p: &mut OpenProject, a: MoveParams) -> Result<Committed<EditReport>, RpcError> {
    commit(p, a.mutation(), "edit.move", |tl| {
        let op = Move { clip: a.clip.clone(), to: time(tl, "to", &a.to)?, track: a.track.clone(), linked: a.linked, push: false, reorder: a.reorder };
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
    /// `edit.set_volume`: gain for a clip's audio (a video clip's linked audio), and volume
    /// automation over time.
    pub struct SetVolumeParams {
        pub clip: Id,
        /// Gain in dB, -100 to +30; 0 is unchanged. Default: keep it.
        #[serde(default)]
        pub db: Option<f64>,
        /// Volume automation: dB added to the gain, keyframes timed from the clip's start
        /// (`[{"at": "0s", "value": 0}, {"at": "0.3s", "value": -12}]`), replacing what the clip
        /// had; `[]` removes it. Default: keep it.
        #[serde(default)]
        pub keyframes: Option<Vec<crate::fx::KeyframeArg>>,
        #[serde(default = "yes")]
        pub linked: bool,
    }
}

pub fn set_volume(p: &mut OpenProject, a: SetVolumeParams) -> Result<Committed<EditReport>, RpcError> {
    let keyframes = match &a.keyframes {
        Some(k) => {
            let map: std::collections::BTreeMap<String, Vec<crate::fx::KeyframeArg>> = [("volume".to_owned(), k.clone())].into_iter().collect();
            Some(crate::fx::keyframes(p, &map)?.shift_remove("volume").unwrap_or_default())
        }
        None => None,
    };
    commit(p, a.mutation(), "edit.set_volume", |tl| {
        edit::set_volume(tl, &SetVolume { clip: a.clip.clone(), db: a.db, keyframes: keyframes.clone(), linked: a.linked }).map_err(edit_err)
    })
}

mutation_params! {
    /// `edit.set_audio`: which audio stream an audio clip plays and how its channels reach the
    /// stereo mix. A video clip stands for its linked audio clip when it has exactly one.
    pub struct SetAudioParams {
        pub clip: Id,
        /// Audio stream number (from the asset's `audio_streams`). Default: unchanged.
        #[serde(default)]
        pub stream: Option<u32>,
        /// `auto` (standard downmix; first two channels of unknown layouts), `left` or `right`
        /// (that channel on both sides), `mono` (all channels averaged), or `[a, b]`: source
        /// channels (from 0) for left and right (`[a]` feeds both). Default: unchanged.
        #[serde(default)]
        pub channels: Option<Channels>,
    }
}

pub fn set_audio(p: &mut OpenProject, a: SetAudioParams) -> Result<Committed<EditReport>, RpcError> {
    commit(p, a.mutation(), "edit.set_audio", |tl| {
        edit::set_audio(tl, &SetAudio { clip: a.clip.clone(), stream: a.stream, channels: a.channels.clone() }).map_err(edit_err)
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

/// A field that may be missing (keep), `null` (reset to its default) or a number.
fn nullable<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

mutation_params! {
    /// `edit.set_transform`: position, size, rotation, opacity and crop of a
    /// video clip's picture. Fields left out keep their value; `null` resets one.
    pub struct SetTransformParams {
        /// A clip on a video track.
        pub clip: Id,
        /// Offset of the picture's center from the canvas center, in canvas widths (-10 to 10).
        #[serde(default, deserialize_with = "nullable")]
        pub x: Option<Option<f64>>,
        /// Offset in canvas heights; positive is down.
        #[serde(default, deserialize_with = "nullable")]
        pub y: Option<Option<f64>>,
        /// Size relative to the fitted picture (0 to 20; default 1).
        #[serde(default, deserialize_with = "nullable")]
        pub scale: Option<Option<f64>>,
        /// Degrees clockwise around the picture's center.
        #[serde(default, deserialize_with = "nullable")]
        pub rotation: Option<Option<f64>>,
        /// 0 (invisible) to 1 (opaque, default).
        #[serde(default, deserialize_with = "nullable")]
        pub opacity: Option<Option<f64>>,
        /// Fraction of the picture cut off this side (0 to 0.99).
        #[serde(default, deserialize_with = "nullable")]
        pub crop_left: Option<Option<f64>>,
        #[serde(default, deserialize_with = "nullable")]
        pub crop_right: Option<Option<f64>>,
        #[serde(default, deserialize_with = "nullable")]
        pub crop_top: Option<Option<f64>>,
        #[serde(default, deserialize_with = "nullable")]
        pub crop_bottom: Option<Option<f64>>,
        /// Field -> keyframes (times from the clip's start), replacing that field's keyframes; `[]` removes them.
        #[serde(default)]
        pub keyframes: Option<std::collections::BTreeMap<String, Vec<crate::fx::KeyframeArg>>>,
        /// How the picture mixes with the tracks below: `normal` (default), `multiply`, `screen`,
        /// `overlay`, `darken`, `lighten`, `color-dodge`, `color-burn`, `hard-light`, `soft-light`,
        /// `difference`, `exclusion`, `hue`, `saturation`, `color`, `luminosity`, `add`, `subtract`,
        /// `linear-burn`. `null` puts back `normal`.
        #[serde(default, deserialize_with = "nullable")]
        pub blend: Option<Option<String>>,
        /// Start from the default (fitted, centered, opaque, no keyframes) before applying the rest.
        #[serde(default)]
        pub reset: bool,
    }
}

pub fn set_transform(p: &mut OpenProject, a: SetTransformParams) -> Result<Committed<TransformReport>, RpcError> {
    let fields = [
        ("x", a.x),
        ("y", a.y),
        ("scale", a.scale),
        ("rotation", a.rotation),
        ("opacity", a.opacity),
        ("crop_left", a.crop_left),
        ("crop_right", a.crop_right),
        ("crop_top", a.crop_top),
        ("crop_bottom", a.crop_bottom),
    ];
    let change = TransformChange {
        reset: a.reset,
        values: fields.into_iter().filter_map(|(k, v)| v.map(|v| (k.to_owned(), v))).collect(),
        keyframes: a.keyframes.as_ref().map(|k| crate::fx::keyframes(p, k)).transpose()?.unwrap_or_default(),
        blend: a.blend.clone(),
    };
    commit(p, a.mutation(), "edit.set_transform", |tl| transform::set(tl, &a.clip, &change).map_err(edit_err))
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
    /// `track.set`: hide or show a video track, mute or unmute an audio track.
    pub struct TrackSetParams {
        /// Track ID or name.
        pub track: String,
        /// False hides a video track or mutes an audio track; renders skip it.
        pub enabled: bool,
    }
}

pub fn track_set(p: &mut OpenProject, a: TrackSetParams) -> Result<Committed<TrackReport>, RpcError> {
    commit(p, a.mutation(), "track.set", |tl| tracks::set_enabled(tl, &a.track, a.enabled).map_err(edit_err))
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
