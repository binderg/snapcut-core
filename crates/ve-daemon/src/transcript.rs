//! `transcript.*`: transcripts of source assets, and the words on the timeline.
//!
//! A transcript is user data that costs money or minutes to make, so it lives
//! in the project folder (`transcripts/<file hash>.json`), not the cache.
//! Keyed by the file's hash, it survives removing and re-importing the asset.
//! Making one is a plugin's job; plugins (or anyone) store it with
//! `transcript.set`. Times are media times of the asset's file; `transcript.get`
//! without an asset maps the words onto the timeline through the audio clips.

use std::collections::HashMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ve_model::meta::Asset;
use ve_model::time::Q;
use ve_model::transcript::{Seconds, Speaker, Transcript, Word};
use ve_model::{Composable, Id, Rate, Timeline, TrackKind};
use ve_ops::lane::{Lane, clip_id, clip_speed};
use ve_protocol::{Actor, ErrorCode, RpcError};

use crate::commit::{actor_label, parse_time};
use crate::project::OpenProject;

pub const FOLDER: &str = "transcripts";

/// Media time past the end of the file a word may reach (models round).
const END_SLACK: Q = Q::new_raw(1, 2);

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetParams {
    /// The asset the words belong to.
    pub asset: Id,
    /// Every word, with media times of the asset's file (`"12.345s"`).
    pub words: Vec<Word>,
    /// BCP 47 language tag (`en`, `he`), if known.
    #[serde(default)]
    pub language: Option<String>,
    /// Display names for speaker labels.
    #[serde(default)]
    pub speakers: Vec<Speaker>,
    /// Who made it: plugin, model, parameters. Free form; the actor is added.
    #[serde(default)]
    pub provenance: Option<serde_json::Value>,
    /// Who is storing it; a plugin's calls carry `{"kind": "plugin", "id": ...}`.
    #[serde(default)]
    pub actor: Option<Actor>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SetResult {
    pub asset: Id,
    pub words: usize,
    /// Media time the last word ends.
    pub end: Seconds,
    /// Replaced an earlier transcript of the same file.
    pub replaced: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetParams {
    /// One asset's transcript, in its own media time. Without it: the words
    /// that play on the timeline, in timeline time.
    #[serde(default)]
    pub asset: Option<Id>,
    /// Timeline start (with no `asset`). Default: the timeline start.
    #[serde(default)]
    pub from: Option<String>,
    /// Timeline end, exclusive. Default: the timeline end.
    #[serde(default)]
    pub to: Option<String>,
    /// `words` (default) or `text`: readable lines with times instead of a word list.
    #[serde(default)]
    pub format: Format,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    #[default]
    Words,
    Text,
}

/// `transcript.get` with an asset.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct AssetTranscript {
    pub asset: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub speakers: Vec<Speaker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<serde_json::Value>,
    /// With `format: words`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub words: Option<Vec<Word>>,
    /// With `format: text`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// One word as it plays on the timeline.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TimelineWord {
    /// Timeline frame the word starts in (timecode).
    pub start: String,
    /// First frame after the word (timecode). Cutting `start`..`end` removes
    /// the whole word.
    pub end: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// The audio clip it plays in.
    pub clip: Id,
    pub asset: Id,
    /// Media time in the asset's file.
    pub source_start: Seconds,
}

/// `transcript.get` without an asset.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct TimelineTranscript {
    pub revision: u64,
    /// With `format: words`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub words: Option<Vec<TimelineWord>>,
    /// With `format: text`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Assets on audio tracks in the range that have no transcript yet.
    pub missing: Vec<Id>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum GetResult {
    Asset(AssetTranscript),
    Timeline(TimelineTranscript),
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ListEntry {
    pub asset: Id,
    pub words: usize,
    pub end: Seconds,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ListResult {
    pub transcripts: Vec<ListEntry>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoveParams {
    pub asset: Id,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct RemoveResult {
    pub asset: Id,
    /// False when the asset had no transcript.
    pub removed: bool,
}

fn asset<'a>(tl: &'a Timeline, id: &Id) -> Result<&'a Asset, RpcError> {
    tl.metadata.ve.assets.iter().find(|a| &a.id == id).ok_or_else(|| RpcError::invalid_params(format!("no asset `{id}`; `ve call media.assets` lists them")))
}

fn path_for(project: &OpenProject, a: &Asset) -> Result<PathBuf, RpcError> {
    let hash = a.hash.as_deref().ok_or_else(|| RpcError::invalid_params(format!("{} has no content hash; re-import it", a.id)))?;
    Ok(project.root().join(FOLDER).join(format!("{}.json", crate::measure::cache_dir_name(hash))))
}

/// The stored transcript of an asset, if any.
fn load(project: &OpenProject, a: &Asset) -> Result<Option<Transcript>, RpcError> {
    let path = path_for(project, a)?;
    let Ok(bytes) = std::fs::read(&path) else { return Ok(None) };
    let mut t: Transcript = serde_json::from_slice(&bytes)
        .map_err(|e| RpcError::new(ErrorCode::Internal, format!("{} is not a valid transcript: {e}", path.display())))?;
    // The file is keyed by content; the asset id may have changed (re-import).
    t.asset = a.id.clone();
    Ok(Some(t))
}

pub fn set(project: &mut OpenProject, p: SetParams) -> Result<SetResult, RpcError> {
    let a = asset(&project.timeline, &p.asset)?.clone();
    let mut provenance = p.provenance;
    if let Some(actor) = &p.actor {
        let label = serde_json::Value::String(actor_label(Some(actor)));
        let p = provenance.get_or_insert_with(|| serde_json::json!({}));
        if let Some(o) = p.as_object_mut() {
            o.entry("actor").or_insert(label);
        }
    }
    let mut t = Transcript {
        schema: String::new(),
        asset: a.id.clone(),
        source_hash: a.hash.clone(),
        language: p.language,
        words: p.words,
        speakers: p.speakers,
        provenance,
    };
    t.normalize().map_err(RpcError::invalid_params)?;
    if let Some(media) = &a.media {
        let limit = media.duration.seconds() + END_SLACK;
        if let Some(w) = t.words.iter().find(|w| w.end.0 > limit) {
            return Err(RpcError::invalid_params(format!(
                "word `{}` ends at {}, past the end of {} ({})",
                w.text,
                w.end,
                a.id,
                Seconds::new(media.duration.seconds())
            )));
        }
    }
    let path = path_for(project, &a)?;
    let replaced = path.is_file();
    let mut text = serde_json::to_string_pretty(&t).map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string()))?;
    text.push('\n');
    crate::project::write_file_atomic(&path, text.as_bytes()).map_err(crate::rpc_error)?;
    Ok(SetResult { asset: a.id, words: t.words.len(), end: Seconds::new(t.end()), replaced })
}

pub fn get(project: &mut OpenProject, p: GetParams) -> Result<GetResult, RpcError> {
    if let Some(id) = &p.asset {
        if p.from.is_some() || p.to.is_some() {
            return Err(RpcError::invalid_params("from/to are timeline times; with `asset` the whole transcript comes back"));
        }
        let a = asset(&project.timeline, id)?.clone();
        let t = load(project, &a)?.ok_or_else(|| {
            RpcError::invalid_params(format!("{id} has no transcript yet; a speech-to-text plugin makes one (`ve plugin list`)"))
        })?;
        let text = (p.format == Format::Text).then(|| {
            let lines: Vec<Line> = t.words.iter().map(|w| Line::word(w.start.to_string(), w.start.0, w.end.0, &w.text, w.speaker.as_deref())).collect();
            render_lines(&lines)
        });
        let words = (p.format == Format::Words).then(|| t.words.clone());
        return Ok(GetResult::Asset(AssetTranscript { asset: t.asset, language: t.language, speakers: t.speakers, provenance: t.provenance, words, text }));
    }
    timeline(project, &p).map(GetResult::Timeline)
}

/// A word ready to be grouped into readable lines.
struct Line {
    label: String,
    start: Q,
    end: Q,
    text: String,
    speaker: Option<String>,
}

impl Line {
    fn word(label: String, start: Q, end: Q, text: &str, speaker: Option<&str>) -> Self {
        Self { label, start, end, text: text.to_owned(), speaker: speaker.map(String::from) }
    }
}

/// `[label] S0: words words words`, a new line on a speaker change, a pause
/// of a second or more, after a sentence once a line has 8 words, or at 20.
fn render_lines(words: &[Line]) -> String {
    let mut out = String::new();
    let mut count = 0;
    let mut prev: Option<&Line> = None;
    for w in words {
        let new_line = match prev {
            None => true,
            Some(p) => {
                p.speaker != w.speaker
                    || w.start - p.end >= Q::from_integer(1)
                    || count >= 20
                    || (count >= 8 && p.text.ends_with(['.', '?', '!']))
            }
        };
        if new_line {
            if prev.is_some() {
                out.push('\n');
            }
            out.push_str(&format!("[{}]", w.label));
            if let Some(s) = &w.speaker {
                out.push_str(&format!(" {s}:"));
            }
            count = 0;
        }
        out.push(' ');
        out.push_str(&w.text);
        count += 1;
        prev = Some(w);
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

fn timeline(project: &OpenProject, p: &GetParams) -> Result<TimelineTranscript, RpcError> {
    let tl = &project.timeline;
    let rate = tl.output_rate();
    let start = p.from.as_deref().map(|s| parse_time("from", s, rate)).transpose()?.unwrap_or(Q::from_integer(0));
    let end = p.to.as_deref().map(|s| parse_time("to", s, rate)).transpose()?;
    if end.is_some_and(|e| e <= start) {
        return Err(RpcError::invalid_params("`to` must be after `from`"));
    }
    let by_url: HashMap<&str, &Asset> = tl.metadata.ve.assets.iter().map(|a| (a.url.as_str(), a)).collect();
    let mut cache: HashMap<Id, Option<Transcript>> = HashMap::new();
    let mut words: Vec<(Q, TimelineWord, Q)> = Vec::new();
    let mut missing: Vec<Id> = Vec::new();
    for item in &tl.tracks.children {
        let Composable::Track(track) = item else { continue };
        if track.kind != TrackKind::Audio {
            continue;
        }
        let lane = Lane::from_track(track).map_err(|e| RpcError::new(ErrorCode::Internal, e.to_string()))?;
        for placed in &lane.items {
            let Some(clip) = placed.clip() else { continue };
            if placed.end() <= start || end.is_some_and(|e| placed.start >= e) {
                continue;
            }
            let Some(ve_model::MediaReference::External(ext)) = clip.active_media() else { continue };
            let Some(a) = by_url.get(ext.target_url.as_str()) else { continue };
            let Some(id) = clip_id(clip) else { continue };
            if !cache.contains_key(&a.id) {
                let t = load(project, a)?;
                cache.insert(a.id.clone(), t);
            }
            let Some(t) = cache.get(&a.id).and_then(Option::as_ref) else {
                if !missing.contains(&a.id) {
                    missing.push(a.id.clone());
                }
                continue;
            };
            let speed = clip_speed(clip);
            let src_in = clip.source_range.map_or(Q::from_integer(0), |r| r.start_time.seconds());
            let src_out = src_in + placed.duration() * speed;
            let to_timeline = |m: Q| placed.start + (m - src_in) / speed;
            for w in &t.words {
                let mid = (w.start.0 + w.end.0) / Q::from_integer(2);
                if mid < src_in || mid >= src_out {
                    continue;
                }
                let t0 = to_timeline(w.start.0.max(src_in));
                let t1 = to_timeline(w.end.0.min(src_out));
                if t1 <= start || end.is_some_and(|e| t0 >= e) {
                    continue;
                }
                let word = TimelineWord {
                    start: frame_tc(t0, rate, false),
                    end: frame_tc(t1, rate, true),
                    text: w.text.clone(),
                    speaker: w.speaker.clone(),
                    confidence: w.confidence,
                    clip: id.clone(),
                    asset: a.id.clone(),
                    source_start: w.start,
                };
                words.push((t0, word, t1));
            }
        }
    }
    words.sort_by_key(|w| w.0);
    let text = (p.format == Format::Text).then(|| {
        let lines: Vec<Line> = words.iter().map(|(t0, w, t1)| Line::word(w.start.clone(), *t0, *t1, &w.text, w.speaker.as_deref())).collect();
        render_lines(&lines)
    });
    let words = (p.format == Format::Words).then(|| words.into_iter().map(|(_, w, _)| w).collect());
    Ok(TimelineTranscript { revision: project.revision(), words, text, missing })
}

/// Timecode of the frame `t` falls in (or, with `ceil`, the first frame at or after it).
fn frame_tc(t: Q, rate: Rate, ceil: bool) -> String {
    let frames = t * rate.as_ratio();
    let f = if ceil { frames.ceil() } else { frames.floor() };
    ve_ops::edit::tc(f / rate.as_ratio(), rate)
}

pub fn list(project: &mut OpenProject) -> Result<ListResult, RpcError> {
    let assets = project.timeline.metadata.ve.assets.clone();
    let mut out = Vec::new();
    for a in &assets {
        if a.hash.is_none() {
            continue;
        }
        if let Some(t) = load(project, a)? {
            out.push(ListEntry { asset: a.id.clone(), words: t.words.len(), end: Seconds::new(t.end()), language: t.language, provenance: t.provenance });
        }
    }
    Ok(ListResult { transcripts: out })
}

pub fn remove(project: &mut OpenProject, p: RemoveParams) -> Result<RemoveResult, RpcError> {
    let a = asset(&project.timeline, &p.asset)?.clone();
    let path = path_for(project, &a)?;
    let removed = match std::fs::remove_file(&path) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(RpcError::new(ErrorCode::Internal, format!("{}: {e}", path.display()))),
    };
    Ok(RemoveResult { asset: a.id, removed })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_break_on_speaker_and_pause() {
        let w = |s: i64, t: &str, sp: &str| Line::word(format!("{s}s"), Q::from_integer(s), Q::from_integer(s) + Q::new(1, 2), t, Some(sp));
        let text = render_lines(&[w(0, "hi", "S0"), w(1, "there.", "S0"), w(2, "yes", "S1"), w(5, "later", "S1")]);
        assert_eq!(text, "[0s] S0: hi there.\n[2s] S1: yes\n[5s] S1: later\n");
    }

    #[test]
    fn frames_floor_and_ceil() {
        let r = Rate::integer(10);
        assert_eq!(frame_tc(Q::new(23, 100), r, false), "00:00:00:02");
        assert_eq!(frame_tc(Q::new(23, 100), r, true), "00:00:00:03");
        assert_eq!(frame_tc(Q::new(3, 10), r, true), "00:00:00:03");
    }
}
