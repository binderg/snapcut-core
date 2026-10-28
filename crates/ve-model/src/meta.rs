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
    /// Where the file is relative to the project folder (`media/a.mp4`,
    /// `../footage/a.mp4`; `/` separators), when both are on one drive. If
    /// `url` is missing on open, the engine looks here, so a project folder
    /// moved together with its media keeps working.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rel: Option<String>,
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
    /// First audio stream (stream 0 of `audio_streams`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioInfo>,
    /// Every audio stream, in file order. A clip picks one with
    /// `metadata.ve.clip.audio.stream` (an index into this list). Empty in
    /// assets probed before streams were recorded; project open fills it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub audio_streams: Vec<AudioStream>,
    pub size_bytes: u64,
}

impl MediaInfo {
    /// Audio stream `i`, falling back to `audio` for stream 0 of an asset
    /// probed before streams were recorded.
    pub fn audio_stream(&self, i: u32) -> Option<AudioStream> {
        match self.audio_streams.get(i as usize) {
            Some(s) => Some(s.clone()),
            None if i == 0 && self.audio_streams.is_empty() => self.audio.as_ref().map(|a| AudioStream {
                index: 0,
                codec: a.codec.clone(),
                sample_rate: a.sample_rate,
                channels: a.channels,
                layout: None,
                language: None,
                title: None,
                default: true,
            }),
            None => None,
        }
    }

    /// How many audio streams the asset has.
    pub fn audio_stream_count(&self) -> u32 {
        if self.audio_streams.is_empty() { u32::from(self.audio.is_some()) } else { self.audio_streams.len() as u32 }
    }
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
    /// A still image (PNG, JPEG, ...): one picture, shown for as long as the
    /// clip lasts. `duration` is zero and `rate` means nothing.
    #[serde(default, skip_serializing_if = "is_default")]
    pub still: bool,
    /// How the pixels encode color, as far as the file says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorInfo>,
    /// Transparency. Absent on assets probed before it was recorded (read
    /// the pixel format then).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha: Option<Alpha>,
    /// Stills with `alpha: transparent`: the share of pixels that are not
    /// fully opaque, in percent, rounded up (so any at all is at least 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub see_through_percent: Option<u8>,
}

/// Whether a picture can show what is under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Alpha {
    /// No alpha channel: every pixel covers what is under it.
    None,
    /// An alpha channel, but every pixel was measured fully opaque.
    Opaque,
    /// Some pixels were measured partly or fully see-through.
    Transparent,
    /// An alpha channel that was not measured (video with alpha, e.g.
    /// ProRes 4444 or VP9 with alpha): assume it may be see-through.
    Unmeasured,
}

impl Alpha {
    /// What a pixel format alone says: `Unmeasured` for formats that can
    /// carry transparency (an alpha plane like `rgba`, `yuva420p`, `ya8`,
    /// `gbrap`, or a palette, which may mark entries clear), else `None`.
    pub fn from_pixel_format(pix: &str) -> Self {
        let alpha = pix == "pal8"
            || pix.starts_with("yuva")
            || pix.starts_with("gbrap")
            || pix.starts_with("ya")
            || ["rgba", "bgra", "argb", "abgr"].iter().any(|a| pix.contains(a));
        if alpha { Self::Unmeasured } else { Self::None }
    }

    /// True when the picture always hides what is under it.
    pub fn is_solid(self) -> bool {
        matches!(self, Self::None | Self::Opaque)
    }
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

/// One audio stream of a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AudioStream {
    /// Position among the file's audio streams (0 is the first), the number
    /// `metadata.ve.clip.audio.stream` uses. Not ffprobe's overall stream index.
    pub index: u32,
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u32,
    /// ffmpeg's layout name (`mono`, `stereo`, `5.1`, ...). Absent when the
    /// file does not say (discrete channels, as many recorders write them).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The file marks this stream as the one to play by default.
    #[serde(default)]
    pub default: bool,
}

impl AudioStream {
    /// The layout is one ffmpeg can downmix the standard way.
    pub fn layout_known(&self) -> bool {
        self.layout.as_deref().is_some_and(|l| !l.is_empty() && !l.contains("channels") && l != "unknown")
    }
}

/// `metadata.ve.clip.audio`: which stream of the file a clip plays, and how
/// its channels reach the stereo mix. Absent means stream 0, `auto`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct ClipAudio {
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub stream: u32,
    #[serde(default, skip_serializing_if = "Channels::is_auto")]
    pub channels: Channels,
}

impl ClipAudio {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

/// How a clip's source channels map to the stereo mix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Channels {
    /// `auto`, `left`, `right` or `mono`.
    Mode(ChannelMode),
    /// Source channels (0-based) for left and right; one channel feeds both.
    Pick(Vec<u32>),
}

impl Default for Channels {
    fn default() -> Self {
        Self::Mode(ChannelMode::Auto)
    }
}

impl Channels {
    pub fn is_auto(&self) -> bool {
        *self == Self::Mode(ChannelMode::Auto)
    }

    /// Short text for listings: `auto`, `left`, `[0, 2]`.
    pub fn label(&self) -> String {
        match self {
            Self::Mode(m) => m.as_str().to_owned(),
            Self::Pick(v) => format!("{v:?}"),
        }
    }

    /// Check against a stream with `channels` channels.
    pub fn check(&self, channels: u32) -> Result<(), String> {
        match self {
            Self::Pick(v) if v.is_empty() || v.len() > 2 => Err("channels needs one or two source channel numbers".into()),
            Self::Pick(v) => match v.iter().find(|&&c| c >= channels.max(1)) {
                Some(c) => Err(format!("channel {c} does not exist; the stream has {channels} (numbered from 0)")),
                None => Ok(()),
            },
            Self::Mode(ChannelMode::Right) if channels < 2 => Err("the stream is mono; it has no right channel".into()),
            Self::Mode(_) => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChannelMode {
    /// Known layouts downmix the standard way; unknown ones use their first two channels.
    Auto,
    /// The first channel on both sides.
    Left,
    /// The second channel on both sides.
    Right,
    /// All channels summed (averaged) onto both sides.
    Mono,
}

impl ChannelMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Left => "left",
            Self::Right => "right",
            Self::Mono => "mono",
        }
    }
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
    /// Volume automation: dB added to `volume_db` over time, keyframes timed
    /// from the clip's start (ducking writes these). Empty means none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volume_keyframes: Vec<Keyframe>,
    /// Clips that share a link ID (a video clip and its audio) are edited
    /// together: split, trim, move, speed and delete apply to the whole group
    /// unless a call asks for one clip only (that is how J/L cuts are made).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<Id>,
    /// Where a video clip's picture sits on the canvas. Absent means the
    /// default: fitted and centered, fully opaque.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Box<Transform>>,
    /// Which audio stream and channels the clip plays. Absent means stream 0, `auto`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<Box<ClipAudio>>,
}

impl ClipMeta {
    pub fn new(id: Id) -> Self {
        Self { id, speed: unit_speed(), volume_db: 0.0, volume_keyframes: Vec::new(), link: None, transform: None, audio: None }
    }
}

/// `metadata.ve.clip.transform`: position, size, rotation, opacity and crop
/// of a video clip's picture, applied after its effects, before it is
/// stacked on the tracks below.
///
/// The starting point is the picture fitted to the canvas and centered (the
/// project's fit mode). Spatial values are plain numbers (no exact-time
/// rule applies to space); keys left at their default are not written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    /// Horizontal offset of the picture's center from the canvas center, in
    /// canvas widths (0.25 moves it a quarter of the canvas right).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub x: f64,
    /// Vertical offset in canvas heights; positive is down.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub y: f64,
    /// Size relative to the fitted picture (0.5 is half size).
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub scale: f64,
    /// Degrees, clockwise, around the picture's center.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: f64,
    /// 0 (invisible) to 1 (opaque).
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub opacity: f64,
    /// Fraction of the picture cut away on each side (0 to 1). The rest stays
    /// where it was; it does not grow to fill the space.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub crop_left: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub crop_right: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub crop_top: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub crop_bottom: f64,
    /// How the picture mixes with what is below it: one of [`BLEND_MODES`].
    #[serde(default = "normal", skip_serializing_if = "is_normal")]
    pub blend: String,
    /// Field name -> keyframes (values are numbers). Times count from the
    /// clip's start on the timeline, as for effects.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub keyframes: IndexMap<String, Vec<Keyframe>>,
}

/// Blend modes a clip can use, in the order the compositor numbers them:
/// the W3C Compositing set, then add, subtract and linear burn.
pub const BLEND_MODES: [&str; 19] = [
    "normal",
    "multiply",
    "screen",
    "overlay",
    "darken",
    "lighten",
    "color-dodge",
    "color-burn",
    "hard-light",
    "soft-light",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
    "add",
    "subtract",
    "linear-burn",
];

fn normal() -> String {
    "normal".to_owned()
}

fn is_normal(s: &str) -> bool {
    s == "normal"
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            rotation: 0.0,
            opacity: 1.0,
            crop_left: 0.0,
            crop_right: 0.0,
            crop_top: 0.0,
            crop_bottom: 0.0,
            blend: normal(),
            keyframes: IndexMap::new(),
        }
    }
}

impl Transform {
    /// Every field a transform has, in the order they are reported.
    pub const FIELDS: [&'static str; 9] = ["x", "y", "scale", "rotation", "opacity", "crop_left", "crop_right", "crop_top", "crop_bottom"];

    pub fn get(&self, field: &str) -> Option<f64> {
        Some(match field {
            "x" => self.x,
            "y" => self.y,
            "scale" => self.scale,
            "rotation" => self.rotation,
            "opacity" => self.opacity,
            "crop_left" => self.crop_left,
            "crop_right" => self.crop_right,
            "crop_top" => self.crop_top,
            "crop_bottom" => self.crop_bottom,
            _ => return None,
        })
    }

    /// Set a field; false if there is no such field.
    pub fn set(&mut self, field: &str, v: f64) -> bool {
        let slot = match field {
            "x" => &mut self.x,
            "y" => &mut self.y,
            "scale" => &mut self.scale,
            "rotation" => &mut self.rotation,
            "opacity" => &mut self.opacity,
            "crop_left" => &mut self.crop_left,
            "crop_right" => &mut self.crop_right,
            "crop_top" => &mut self.crop_top,
            "crop_bottom" => &mut self.crop_bottom,
            _ => return false,
        };
        *slot = v;
        true
    }

    /// The default placement with no keyframes: renders exactly as no transform.
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }

    /// The default value of a field (what `null` resets it to).
    pub fn default_of(field: &str) -> Option<f64> {
        Self::default().get(field)
    }

    /// A field's allowed range and what its unit means.
    pub fn range(field: &str) -> Option<(f64, f64, &'static str)> {
        Some(match field {
            "x" | "y" => (-10.0, 10.0, "canvas sizes from the center"),
            "scale" => (0.0, 20.0, "times the fitted size"),
            "rotation" => (-3600.0, 3600.0, "degrees"),
            "opacity" => (0.0, 1.0, "(0 invisible, 1 opaque)"),
            "crop_left" | "crop_right" | "crop_top" | "crop_bottom" => (0.0, 0.99, "of the picture"),
            _ => return None,
        })
    }

    /// `v` within the field's range (keyframe curves may overshoot it).
    pub fn clamp(field: &str, v: f64) -> f64 {
        Self::range(field).map_or(v, |(lo, hi, _)| v.clamp(lo, hi))
    }
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

fn is_one(v: &f64) -> bool {
    *v == 1.0
}

fn one() -> f64 {
    1.0
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
    /// Shape of the segment to the next key (see [`crate::curve`]).
    #[serde(default = "linear", skip_serializing_if = "is_linear")]
    pub ease: String,
    /// Graph-editor handle on the incoming side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#in: Option<crate::curve::Handle>,
    /// Graph-editor handle on the outgoing side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out: Option<crate::curve::Handle>,
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
