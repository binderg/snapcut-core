//! Typed `metadata.ve` blocks (ARCHITECTURE.md, "Extension namespace").
//!
//! [`Meta<T>`] splits an OTIO metadata object into our typed `ve` block and
//! every other key, which is kept untouched. Blocks are optional on read so
//! plain OTIO files from other tools still load; project open assigns any
//! missing IDs.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::NAMESPACE;
use crate::id::Id;
use crate::time::{Q, Rate, RationalTime};

/// OTIO metadata with our typed block under `ve` and all other keys as-is.
/// The `ve` key is omitted on write when the block is empty.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Meta<T> {
    pub ve: T,
    pub other: Map<String, Value>,
}

impl<T: Serialize + Default + PartialEq> Serialize for Meta<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{Error, SerializeMap};
        let ve = if self.ve == T::default() {
            None
        } else {
            Some(serde_json::to_value(&self.ve).map_err(S::Error::custom)?)
        };
        let mut m = s.serialize_map(Some(self.other.len() + usize::from(ve.is_some())))?;
        for (k, v) in &self.other {
            m.serialize_entry(k, v)?;
        }
        if let Some(ve) = ve {
            m.serialize_entry(NAMESPACE, &ve)?;
        }
        m.end()
    }
}

impl<'de, T: serde::de::DeserializeOwned + Default> Deserialize<'de> for Meta<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let mut other = Map::deserialize(d)?;
        let ve = match other.remove(NAMESPACE) {
            None | Some(Value::Null) => T::default(),
            Some(v) => T::deserialize(v).map_err(|e| D::Error::custom(format!("metadata.{NAMESPACE}: {e}")))?,
        };
        Ok(Self { ve, other })
    }
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

// --- Timeline ---

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TimelineVe {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectMeta>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<Asset>,
}

/// `metadata.ve.project`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectMeta {
    pub schema_version: semver::Version,
    /// Incremented by every committed command.
    pub revision: u64,
    pub output: OutputSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputSettings {
    pub width: u32,
    pub height: u32,
    pub rate: Rate,
    pub sample_rate: u32,
    pub color_space: String,
}

/// One row of `metadata.ve.assets`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Asset {
    pub id: Id,
    /// Media URI (`file://` in Phase 1).
    pub url: String,
    pub kind: AssetKind,
    /// blake3 hex digest of the file contents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Value>,
    /// Compact probe result, so edits and summaries need no ffprobe run and
    /// work while media is offline. The full ffprobe JSON is not stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaInfo>,
}

/// What an asset contains, from ffprobe, with exact times and rates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MediaInfo {
    /// Video stream duration if there is video, else audio. At the video
    /// rate (or the audio sample rate for audio-only files).
    pub duration: RationalTime,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video: Option<VideoInfo>,
    /// First audio stream.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioInfo>,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VideoInfo {
    pub codec: String,
    /// Stored (coded) size, before rotation.
    pub width: u32,
    pub height: u32,
    /// Nominal frame rate. For variable-frame-rate footage this is the
    /// capture rate, not the average.
    pub rate: Rate,
    /// Display rotation in degrees, 0/90/180/270 (phone portrait video is 90 or 270).
    #[serde(default, skip_serializing_if = "is_default")]
    pub rotation: u16,
    /// Frame timing varies (common on phones); renders must conform it.
    #[serde(default, skip_serializing_if = "is_default")]
    pub vfr: bool,
    /// How the pixels encode color, as far as the file says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorInfo>,
}

/// A video stream's color description (ffprobe's names). Fields the file
/// leaves unspecified are absent; players then usually assume Rec.709.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct ColorInfo {
    /// Pixel format, e.g. `yuv420p`, `yuv420p10le`.
    pub pixel_format: String,
    /// Bits per component: 8, 10, 12.
    pub bit_depth: u8,
    /// `4:2:0`, `4:2:2`, `4:4:4`, `rgb` or `gray`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chroma: Option<String>,
    /// `limited` (TV, 16-235) or `full` (PC, 0-255).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<String>,
    /// Color primaries: `bt709`, `bt2020`, `smpte432` (P3), ...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primaries: Option<String>,
    /// Transfer function: `bt709`, `smpte2084` (PQ), `arib-std-b67` (HLG), ...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer: Option<String>,
    /// YUV matrix: `bt709`, `bt2020nc`, `bt470bg`, ...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matrix: Option<String>,
    /// `sdr`, `hdr10` (PQ) or `hlg`, from the transfer function.
    pub dynamic_range: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AudioInfo {
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Source,
    Proxy,
    Analysis,
    Baked,
}

// --- Track ---

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TrackVe {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackMeta>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackMeta {
    pub id: Id,
}

// --- Clip ---

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ClipVe {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<ClipMeta>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<EffectInstance>,
}

/// `metadata.ve.clip`.
///
/// Speed follows OTIO's time-warp convention: the clip's `source_range`
/// starts at the media in-point and its duration is the time the clip takes
/// on the timeline. The media it plays is `duration * speed` long. `set_speed`
/// also writes an OTIO `LinearTimeWarp` effect so other tools see the speed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipMeta {
    pub id: Id,
    /// Playback speed as an exact fraction, written `[num, den]`.
    #[serde(default = "unit_speed")]
    pub speed: Q,
    #[serde(default)]
    pub volume_db: f64,
    /// Clips that share a link ID (a video clip and its audio) are edited
    /// together: split, trim, move, speed and delete apply to the whole group
    /// unless a call asks for one clip only (that is how J/L cuts are made).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<Id>,
}

impl ClipMeta {
    pub fn new(id: Id) -> Self {
        Self { id, speed: unit_speed(), volume_db: 0.0, link: None }
    }
}

fn unit_speed() -> Q {
    Q::from_integer(1)
}

/// One entry of `metadata.ve.effects`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectInstance {
    pub id: Id,
    /// Plugin effect id, e.g. `core.color/exposure@1`.
    pub effect: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub params: Map<String, Value>,
    /// Param name -> keyframes. Keyframe times are relative to the clip's
    /// start on the timeline.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub keyframes: IndexMap<String, Vec<Keyframe>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub t: RationalTime,
    pub v: Value,
    #[serde(default = "linear", skip_serializing_if = "is_linear")]
    pub ease: String,
}

fn linear() -> String {
    "linear".to_owned()
}

fn is_linear(s: &str) -> bool {
    s == "linear"
}

// --- Transition ---

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TransitionVe {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<TransitionMeta>,
}

/// `metadata.ve.transition`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransitionMeta {
    pub id: Id,
    /// Plugin transition id, e.g. `core.dissolve@1`.
    pub transition: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub params: Map<String, Value>,
}

// --- Marker ---

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MarkerVe {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<MarkerMeta>,
}

/// `metadata.ve.marker`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarkerMeta {
    pub id: Id,
    pub source: MarkerSource,
    #[serde(default, skip_serializing_if = "is_default")]
    pub payload: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerSource {
    User,
    Agent,
    Analyzer,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keeps_foreign_metadata_and_omits_empty_ve() {
        let j = json!({"resolve": {"color": 3}});
        let m: Meta<ClipVe> = serde_json::from_value(j.clone()).unwrap();
        assert_eq!(m.ve, ClipVe::default());
        assert_eq!(serde_json::to_value(&m).unwrap(), j);
    }

    #[test]
    fn clip_block_round_trips() {
        let j = json!({
            "other_tool": true,
            "ve": {"clip": {"id": "clip_7f3a", "speed": [1, 2], "volume_db": -3.0}}
        });
        let m: Meta<ClipVe> = serde_json::from_value(j.clone()).unwrap();
        let clip = m.ve.clip.as_ref().unwrap();
        assert_eq!(clip.id.as_str(), "clip_7f3a");
        assert_eq!(clip.speed, Q::new(1, 2));
        assert_eq!(serde_json::to_value(&m).unwrap(), j);
    }

    #[test]
    fn bad_ve_block_names_the_path() {
        let j = json!({"ve": {"clip": {"speed": [1, 1]}}});
        let err = serde_json::from_value::<Meta<ClipVe>>(j).unwrap_err().to_string();
        assert!(err.contains("metadata.ve"), "{err}");
    }
}
