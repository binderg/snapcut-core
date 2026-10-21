//! OpenTimelineIO schema types, written to match what Python `opentimelineio`
//! 0.18 serializes (same field names, `null` for absent ranges, `Clip.2`
//! media reference map). `fixtures/otio/` holds files produced by the Python
//! library; tests prove these types read and write them unchanged.
//!
//! Only the schemas Phase 1 needs are modeled. Anything else (an unknown
//! child schema, an `ImageSequenceReference`) fails to parse with a clear
//! error instead of being dropped silently.
//!
//! Our own data lives in each object's `metadata.ve` block (see [`crate::meta`]).
//! Every other metadata key is kept as-is so other tools' data survives a save.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::meta::{ClipVe, MarkerVe, Meta, TimelineVe, TrackVe, TransitionVe};
use crate::time::{RationalTime, TimeRange};

/// Plain OTIO metadata for objects that have no `ve` block.
pub type Metadata = Map<String, Value>;

/// Key OTIO uses for a clip's main media reference.
pub const DEFAULT_MEDIA: &str = "DEFAULT_MEDIA";

/// A zero-sized `OTIO_SCHEMA` field that only accepts one exact schema string.
/// Serde's `#[serde(tag)]` on a struct writes the tag but never checks it on
/// read, which would let an old `Marker.1` load as a `Marker.2`.
macro_rules! schema_tag {
    ($name:ident, $schema:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name;

        impl $name {
            pub const NAME: &'static str = $schema;
        }

        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str($schema)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let got = String::deserialize(d)?;
                if got == $schema {
                    Ok($name)
                } else {
                    Err(serde::de::Error::invalid_value(
                        serde::de::Unexpected::Str(&got),
                        &$schema,
                    ))
                }
            }
        }
    };
}

schema_tag!(TimelineSchema, "Timeline.1");
schema_tag!(MarkerSchema, "Marker.2");

fn yes() -> bool {
    true
}

fn default_media_key() -> String {
    DEFAULT_MEDIA.to_owned()
}

fn default_marker_color() -> String {
    "RED".to_owned()
}

/// OTIO `Timeline.1`: the project document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    #[serde(rename = "OTIO_SCHEMA")]
    pub schema: TimelineSchema,
    #[serde(default)]
    pub metadata: Meta<TimelineVe>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub global_start_time: Option<RationalTime>,
    #[serde(with = "stack_tagged")]
    pub tracks: Stack,
}

impl Timeline {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema: TimelineSchema,
            metadata: Meta::default(),
            name: name.into(),
            global_start_time: None,
            tracks: Stack::new("tracks"),
        }
    }
}

/// Anything that can sit in a track or stack. Serialized with its
/// `OTIO_SCHEMA` tag; the inner structs carry no tag of their own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "OTIO_SCHEMA")]
pub enum Composable {
    #[serde(rename = "Clip.2")]
    Clip(Clip),
    #[serde(rename = "Gap.1")]
    Gap(Gap),
    #[serde(rename = "Transition.1")]
    Transition(Transition),
    #[serde(rename = "Track.1")]
    Track(Track),
    #[serde(rename = "Stack.1")]
    Stack(Stack),
}

/// OTIO `Stack.1`: children play at the same time (the timeline's tracks).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stack {
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub source_range: Option<TimeRange>,
    /// OTIO-level effects, passed through untouched. Ours live in `metadata.ve`.
    #[serde(default)]
    pub effects: Vec<Value>,
    #[serde(default)]
    pub markers: Vec<Marker>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub color: Option<Value>,
    #[serde(default)]
    pub children: Vec<Composable>,
}

impl Stack {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            metadata: Metadata::new(),
            name: name.into(),
            source_range: None,
            effects: Vec::new(),
            markers: Vec::new(),
            enabled: true,
            color: None,
            children: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrackKind {
    Video,
    Audio,
}

/// OTIO `Track.1`: children play one after another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    #[serde(default)]
    pub metadata: Meta<TrackVe>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub source_range: Option<TimeRange>,
    #[serde(default)]
    pub effects: Vec<Value>,
    #[serde(default)]
    pub markers: Vec<Marker>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub color: Option<Value>,
    #[serde(default)]
    pub children: Vec<Composable>,
    pub kind: TrackKind,
}

impl Track {
    pub fn new(name: impl Into<String>, kind: TrackKind) -> Self {
        Self {
            metadata: Meta::default(),
            name: name.into(),
            source_range: None,
            effects: Vec::new(),
            markers: Vec::new(),
            enabled: true,
            color: None,
            children: Vec::new(),
            kind,
        }
    }
}

/// OTIO `Clip.2`: a range of one media reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    #[serde(default)]
    pub metadata: Meta<ClipVe>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub source_range: Option<TimeRange>,
    #[serde(default)]
    pub effects: Vec<Value>,
    #[serde(default)]
    pub markers: Vec<Marker>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub color: Option<Value>,
    pub media_references: IndexMap<String, MediaReference>,
    #[serde(default = "default_media_key")]
    pub active_media_reference_key: String,
}

impl Clip {
    pub fn new(name: impl Into<String>, media: MediaReference, source_range: TimeRange) -> Self {
        Self {
            metadata: Meta::default(),
            name: name.into(),
            source_range: Some(source_range),
            effects: Vec::new(),
            markers: Vec::new(),
            enabled: true,
            color: None,
            media_references: IndexMap::from([(default_media_key(), media)]),
            active_media_reference_key: default_media_key(),
        }
    }

    /// The media reference OTIO considers active, if the key points at one.
    pub fn active_media(&self) -> Option<&MediaReference> {
        self.media_references.get(&self.active_media_reference_key)
    }
}

/// OTIO `Gap.1`: empty time in a track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gap {
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub source_range: Option<TimeRange>,
    #[serde(default)]
    pub effects: Vec<Value>,
    #[serde(default)]
    pub markers: Vec<Marker>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub color: Option<Value>,
}

impl Gap {
    pub fn new(duration: RationalTime) -> Self {
        Self {
            metadata: Metadata::new(),
            name: String::new(),
            source_range: Some(TimeRange::new(RationalTime::zero(duration.rate), duration)),
            effects: Vec::new(),
            markers: Vec::new(),
            enabled: true,
            color: None,
        }
    }
}

/// OTIO `Transition.1`: overlaps the neighbours by `in_offset` / `out_offset`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    #[serde(default)]
    pub metadata: Meta<TransitionVe>,
    #[serde(default)]
    pub name: String,
    pub in_offset: RationalTime,
    pub out_offset: RationalTime,
    #[serde(default)]
    pub transition_type: String,
}

impl Transition {
    pub fn new(in_offset: RationalTime, out_offset: RationalTime) -> Self {
        Self {
            metadata: Meta::default(),
            name: String::new(),
            in_offset,
            out_offset,
            transition_type: String::new(),
        }
    }
}

/// OTIO `Marker.2`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    #[serde(rename = "OTIO_SCHEMA")]
    pub schema: MarkerSchema,
    #[serde(default)]
    pub metadata: Meta<MarkerVe>,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_marker_color")]
    pub color: String,
    pub marked_range: TimeRange,
    #[serde(default)]
    pub comment: String,
}

impl Marker {
    pub fn new(name: impl Into<String>, marked_range: TimeRange) -> Self {
        Self {
            schema: MarkerSchema,
            metadata: Meta::default(),
            name: name.into(),
            color: default_marker_color(),
            marked_range,
            comment: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "OTIO_SCHEMA")]
pub enum MediaReference {
    #[serde(rename = "ExternalReference.1")]
    External(ExternalReference),
    #[serde(rename = "MissingReference.1")]
    Missing(MissingReference),
}

/// OTIO `ExternalReference.1`. `target_url` is a URI (`file://` in Phase 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExternalReference {
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub available_range: Option<TimeRange>,
    #[serde(default)]
    pub available_image_bounds: Option<Value>,
    pub target_url: String,
}

impl ExternalReference {
    pub fn new(target_url: impl Into<String>, available_range: Option<TimeRange>) -> Self {
        Self {
            metadata: Metadata::new(),
            name: String::new(),
            available_range,
            available_image_bounds: None,
            target_url: target_url.into(),
        }
    }
}

/// OTIO `MissingReference.1`: media that is offline or unknown.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MissingReference {
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub available_range: Option<TimeRange>,
    #[serde(default)]
    pub available_image_bounds: Option<Value>,
}

/// `Timeline.tracks` is a `Stack` that must carry its `Stack.1` tag, while
/// `Stack` itself is untagged so it can also sit inside [`Composable`].
mod stack_tagged {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::Stack;

    #[derive(Serialize)]
    #[serde(tag = "OTIO_SCHEMA")]
    enum Ref<'a> {
        #[serde(rename = "Stack.1")]
        Stack(&'a Stack),
    }

    #[derive(Deserialize)]
    #[serde(tag = "OTIO_SCHEMA")]
    enum Owned {
        #[serde(rename = "Stack.1")]
        Stack(Stack),
    }

    pub fn serialize<S: Serializer>(stack: &Stack, s: S) -> Result<S::Ok, S::Error> {
        Ref::Stack(stack).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Stack, D::Error> {
        let Owned::Stack(stack) = Owned::deserialize(d)?;
        Ok(stack)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_wrong_schema_version() {
        let j = json!({
            "OTIO_SCHEMA": "Marker.1",
            "marked_range": {
                "OTIO_SCHEMA": "TimeRange.1",
                "start_time": {"OTIO_SCHEMA": "RationalTime.1", "value": 0.0, "rate": 30.0},
                "duration": {"OTIO_SCHEMA": "RationalTime.1", "value": 0.0, "rate": 30.0}
            }
        });
        let err = serde_json::from_value::<Marker>(j).unwrap_err().to_string();
        assert!(err.contains("Marker.2"), "{err}");
    }

    #[test]
    fn rejects_unknown_child_schema() {
        let j = json!({"OTIO_SCHEMA": "Clip.1", "name": "old"});
        assert!(serde_json::from_value::<Composable>(j).is_err());
    }

    #[test]
    fn timeline_tracks_carry_stack_tag() {
        let j = serde_json::to_value(Timeline::new("t")).unwrap();
        assert_eq!(j["OTIO_SCHEMA"], "Timeline.1");
        assert_eq!(j["tracks"]["OTIO_SCHEMA"], "Stack.1");
        let back: Timeline = serde_json::from_value(j).unwrap();
        assert_eq!(back, Timeline::new("t"));
    }
}
