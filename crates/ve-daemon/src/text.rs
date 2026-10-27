//! `text.*` methods: text clips (titles, labels) and captions made from the
//! transcript. The clips are ordinary clips on a video track
//! (`ve_model::text`), so every edit, transform, effect and transition works
//! on them; the edits themselves are `ve_ops::edit::{add_text, set_text}`.

use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Map, Value};
use ve_model::Id;
use ve_model::meta::Transform;
use ve_model::text::{Animation, TextClip, TextStyle, WordTime};
use ve_model::transcript::Seconds;
use ve_model::time::Q;
use ve_ops::EditReport;
use ve_ops::edit::{self, AddText, TEXT_SECONDS, TextChange, TextItem};
use ve_protocol::RpcError;

use crate::commit::{Committed, commit, mutation_params};
use crate::edit::{edit_err, time};
use crate::project::OpenProject;

/// A style from the fields a caller set over `base`, checked.
fn style(base: TextStyle, fields: Option<&Map<String, Value>>) -> Result<TextStyle, RpcError> {
    let mut v = serde_json::to_value(base).map_err(|e| RpcError::invalid_params(e.to_string()))?;
    if let (Some(obj), Some(fields)) = (v.as_object_mut(), fields) {
        for (k, val) in fields {
            obj.insert(k.clone(), val.clone());
        }
    }
    let s: TextStyle = serde_json::from_value(v).map_err(|e| RpcError::invalid_params(format!("style: {e}")))?;
    s.check().map_err(RpcError::invalid_params)?;
    Ok(s)
}

/// A transform that only moves the picture, or `None` at the center.
fn position(x: Option<f64>, y: Option<f64>) -> Result<Option<Transform>, RpcError> {
    let mut t = Transform::default();
    for (field, v) in [("x", x), ("y", y)] {
        if let Some(v) = v {
            if let Some(e) = ve_ops::transform::check_value(field, v) {
                return Err(RpcError::invalid_params(e));
            }
            t.set(field, v);
        }
    }
    Ok((!t.is_identity()).then_some(t))
}

mutation_params! {
    /// `text.add`: put a text clip on the timeline.
    pub struct AddParams {
        /// The words; `\n` starts a new line, and long lines wrap.
        pub text: String,
        /// Timeline start. Default the timeline start.
        #[serde(default)]
        pub at: Option<String>,
        /// Length. Default 3 s.
        #[serde(default)]
        pub duration: Option<String>,
        /// A video track by name or ID. Default: the topmost video track that is free
        /// there, else a new video track on top. Text never pushes other clips.
        #[serde(default)]
        pub track: Option<String>,
        /// On a named track, replace what is under the text instead of refusing.
        #[serde(default)]
        pub overwrite: bool,
        /// Look: `font`, `size`, `weight`, `italic`, `color`, `align`, `width`, `line_height`,
        /// `outline`, `outline_color`, `shadow`, `shadow_color`, `background`, `padding`.
        /// See the text docs for units and defaults.
        #[serde(default)]
        pub style: Option<Map<String, Value>>,
        /// Horizontal position of its center, canvas widths from the center (default 0).
        #[serde(default)]
        pub x: Option<f64>,
        /// Vertical position of its center, canvas heights from the center; positive is down
        /// (0.35 sits low, where captions go).
        #[serde(default)]
        pub y: Option<f64>,
        /// Entrance: `{"kind": "pop", "duration": "0.4s"}`. Kinds: fade, pop, zoom, slide-up,
        /// slide-down, slide-left, slide-right, typewriter, words.
        #[serde(default)]
        pub animate_in: Option<Animation>,
        /// Exit, the same kinds played backwards.
        #[serde(default)]
        pub animate_out: Option<Animation>,
    }
}

pub fn add(p: &mut OpenProject, a: AddParams) -> Result<Committed<EditReport>, RpcError> {
    let tl = &p.timeline;
    let start = a.at.as_deref().map(|s| time(tl, "at", s)).transpose()?.unwrap_or(Q::from_integer(0));
    let duration = a.duration.as_deref().map(|s| time(tl, "duration", s)).transpose()?.unwrap_or(Q::from_integer(TEXT_SECONDS));
    if duration <= Q::from_integer(0) {
        return Err(RpcError::invalid_params("duration must be more than zero"));
    }
    let text = TextClip {
        text: a.text.clone(),
        style: style(TextStyle::default(), a.style.as_ref())?,
        animate_in: a.animate_in.clone(),
        animate_out: a.animate_out.clone(),
        words: Vec::new(),
    };
    let item = TextItem { text, start, duration, transform: position(a.x, a.y)? };
    let req = AddText { items: vec![item], track: a.track.clone(), new_track_name: Some("Text".into()), overwrite: a.overwrite, clear_text: false };
    commit(p, a.mutation(), "text.add", |tl| edit::add_text(tl, &req).map_err(edit_err))
}

mutation_params! {
    /// `text.set`: change a text clip's words or look. Position, size on screen,
    /// rotation, opacity and blend are its transform (`edit.set_transform`).
    pub struct SetParams {
        pub clip: Id,
        #[serde(default)]
        pub text: Option<String>,
        /// Style fields to change; `null` puts one back to its default.
        #[serde(default)]
        pub style: Option<Map<String, Value>>,
        /// Start from the default look before applying `style`.
        #[serde(default)]
        pub reset_style: bool,
        /// New entrance (`{"kind": "fade"}`); `null` removes it.
        #[serde(default, deserialize_with = "nullable")]
        pub animate_in: Option<Option<Animation>>,
        /// New exit; `null` removes it.
        #[serde(default, deserialize_with = "nullable")]
        pub animate_out: Option<Option<Animation>>,
    }
}

fn nullable<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    <Option<T> as serde::Deserialize>::deserialize(d).map(Some)
}

pub fn set(p: &mut OpenProject, a: SetParams) -> Result<Committed<EditReport>, RpcError> {
    let change = TextChange {
        text: a.text.clone(),
        style: a.style.clone().unwrap_or_default(),
        reset_style: a.reset_style,
        animate_in: a.animate_in.clone(),
        animate_out: a.animate_out.clone(),
    };
    commit(p, a.mutation(), "text.set", |tl| edit::set_text(tl, &a.clip, &change).map_err(edit_err))
}

mutation_params! {
    /// `text.captions`: caption clips from the transcript, timed to the edited timeline.
    pub struct CaptionsParams {
        /// Timeline start. Default the timeline start.
        #[serde(default)]
        pub from: Option<String>,
        /// Timeline end, exclusive. Default the timeline end.
        #[serde(default)]
        pub to: Option<String>,
        /// A video track by name or ID. Its text clips in the range are replaced (make
        /// captions again after editing). Default: a new `Captions` track on top (or the
        /// topmost video track free over every caption).
        #[serde(default)]
        pub track: Option<String>,
        /// Look, over the caption default (bold, outlined, 5.5% of the height). Same fields as `text.add`.
        #[serde(default)]
        pub style: Option<Map<String, Value>>,
        /// Vertical position, canvas heights from the center (default 0.36, near the bottom).
        #[serde(default)]
        pub y: Option<f64>,
        /// Color of the word being spoken (`[1, 0.85, 0]`), the CapCut style. Same as `style.highlight`.
        #[serde(default)]
        pub highlight: Option<Vec<f64>>,
        /// Entrance for each caption (`{"kind": "pop", "duration": "0.15s"}`).
        #[serde(default)]
        pub animate_in: Option<Animation>,
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CaptionsResult {
    #[serde(flatten)]
    pub edit: EditReport,
    /// How many captions were made.
    pub captions: usize,
    /// Assets on audio tracks in the range with no transcript yet (no captions for them).
    pub missing: Vec<Id>,
}

pub fn captions(p: &mut OpenProject, a: CaptionsParams) -> Result<Committed<CaptionsResult>, RpcError> {
    let (cues, missing) = crate::transcript::timeline_cues(p, a.from.clone(), a.to.clone())?;
    if cues.is_empty() {
        let why = if missing.is_empty() { "no words play in the range" } else { "the audio in range has no transcript yet (a speech-to-text plugin makes one)" };
        return Err(RpcError::invalid_params(format!("no captions to make: {why}")));
    }
    let mut look = style(TextStyle::caption(), a.style.as_ref())?;
    if a.highlight.is_some() {
        look.highlight.clone_from(&a.highlight);
        look.check().map_err(RpcError::invalid_params)?;
    }
    let place = position(None, Some(a.y.unwrap_or(0.36)))?;
    let items: Vec<TextItem> = cues
        .iter()
        .map(|c| TextItem {
            text: TextClip {
                text: c.lines.join("\n"),
                style: look.clone(),
                animate_in: a.animate_in.clone(),
                animate_out: None,
                // Word times in the clip's own time: 0 is the caption's start.
                words: c.words.iter().map(|&(s, e)| WordTime { start: Seconds::new(s - c.start), end: Seconds::new(e - c.start) }).collect(),
            },
            start: c.start,
            duration: c.end - c.start,
            transform: place.clone(),
        })
        .collect();
    let count = items.len();
    let req = AddText { items, track: a.track.clone(), new_track_name: Some("Captions".into()), overwrite: false, clear_text: true };
    commit(p, a.mutation(), "text.captions", |tl| {
        let (next, edit) = edit::add_text(tl, &req).map_err(edit_err)?;
        Ok((next, CaptionsResult { edit, captions: count, missing: missing.clone() }))
    })
}
