//! `edit.set_transform`: position, size, rotation, opacity and crop of a
//! video clip's picture (`metadata.ve.clip.transform`).
//!
//! Only the document changes here. The compositor reads the transform at
//! each frame (with keyframes) in `ve-render::compose`.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::Serialize;
use ve_model::Id;
use ve_model::meta::{BLEND_MODES, ClipMeta, Keyframe, Transform};
use ve_model::otio::{Composable, Timeline, TrackKind};

use crate::EditError;
use crate::lane::clip_id;

type Result<T> = std::result::Result<T, EditError>;

/// A clip's transform as the command reports it: every field, defaults included.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct TransformReport {
    pub clip: Id,
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub rotation: f64,
    pub opacity: f64,
    pub crop_left: f64,
    pub crop_right: f64,
    pub crop_top: f64,
    pub crop_bottom: f64,
    /// Blend mode (`normal` unless set).
    pub blend: String,
    /// Fields with keyframes (their value above is the one without keyframes).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub keyframed: Vec<String>,
}

impl TransformReport {
    pub fn new(clip: Id, t: &Transform) -> Self {
        Self {
            clip,
            x: t.x,
            y: t.y,
            scale: t.scale,
            rotation: t.rotation,
            opacity: t.opacity,
            crop_left: t.crop_left,
            crop_right: t.crop_right,
            crop_top: t.crop_top,
            crop_bottom: t.crop_bottom,
            blend: t.blend.clone(),
            keyframed: t.keyframes.iter().filter(|(_, k)| !k.is_empty()).map(|(n, _)| n.clone()).collect(),
        }
    }
}

/// What to change. Fields not named keep their value.
#[derive(Debug, Clone, Default)]
pub struct TransformChange {
    /// Start from the default (fitted, centered, opaque, no keyframes).
    pub reset: bool,
    /// Field -> new value; `None` puts the field back to its default.
    pub values: Vec<(String, Option<f64>)>,
    /// Field -> keyframes, replacing that field's keyframes; an empty list removes them.
    pub keyframes: IndexMap<String, Vec<Keyframe>>,
    /// New blend mode; `Some(None)` puts it back to `normal`.
    pub blend: Option<Option<String>>,
}

/// Why a value is out of range, or `None` if it is fine.
pub fn check_value(field: &str, v: f64) -> Option<String> {
    if !v.is_finite() {
        return Some(format!("{field} must be a finite number"));
    }
    let Some((lo, hi, what)) = Transform::range(field) else {
        return Some(format!("no transform field `{field}`; fields are {}", Transform::FIELDS.join(", ")));
    };
    (!(lo..=hi).contains(&v)).then(|| format!("{field} {v} is out of range: {lo} to {hi} {what}"))
}

pub fn set(tl: &Timeline, clip: &Id, change: &TransformChange) -> Result<(Timeline, TransformReport)> {
    let mut next = tl.clone();
    let (kind, c) = next
        .tracks
        .children
        .iter_mut()
        .filter_map(|c| match c {
            Composable::Track(t) => Some(t),
            _ => None,
        })
        .flat_map(|t| {
            let kind = t.kind;
            t.children.iter_mut().filter_map(move |c| match c {
                Composable::Clip(c) => Some((kind, c)),
                _ => None,
            })
        })
        .find(|(_, c)| clip_id(c) == Some(clip))
        .ok_or_else(|| EditError::new(format!("no clip `{clip}`")))?;
    if kind != TrackKind::Video {
        return Err(EditError::new(format!("{clip} is an audio clip; transforms apply to video clips (pass the linked video clip)")));
    }
    let meta = c.metadata.ve.clip.get_or_insert_with(|| ClipMeta::new(clip.clone()));
    let mut t = if change.reset { Transform::default() } else { meta.transform.as_deref().cloned().unwrap_or_default() };
    for (field, v) in &change.values {
        let v = match v {
            Some(v) => *v,
            None => Transform::default_of(field).ok_or_else(|| EditError::new(format!("no transform field `{field}`")))?,
        };
        if let Some(e) = check_value(field, v) {
            return Err(EditError::new(e));
        }
        t.set(field, v);
    }
    if let Some(b) = &change.blend {
        let b = b.as_deref().unwrap_or("normal");
        if !BLEND_MODES.contains(&b) {
            return Err(EditError::new(format!("blend `{b}`: use one of {}", BLEND_MODES.join(", "))));
        }
        t.blend = b.to_owned();
    }
    for (field, keys) in &change.keyframes {
        if Transform::default_of(field).is_none() {
            return Err(EditError::new(format!("no transform field `{field}`; fields are {}", Transform::FIELDS.join(", "))));
        }
        if keys.is_empty() {
            t.keyframes.shift_remove(field);
            continue;
        }
        for k in keys {
            let v = k.v.as_f64().ok_or_else(|| EditError::new(format!("keyframe values for {field} must be numbers")))?;
            if let Some(e) = check_value(field, v) {
                return Err(EditError::new(format!("keyframe: {e}")));
            }
        }
        let mut keys = keys.clone();
        keys.sort_by_key(|k| k.t);
        t.keyframes.insert(field.clone(), keys);
    }
    for (a, b) in [("crop_left", "crop_right"), ("crop_top", "crop_bottom")] {
        let sum = t.get(a).unwrap_or(0.0) + t.get(b).unwrap_or(0.0);
        if sum >= 0.99 {
            return Err(EditError::new(format!("{a} + {b} is {sum}; together they must leave some of the picture (under 0.99)")));
        }
    }
    let report = TransformReport::new(clip.clone(), &t);
    meta.transform = (!t.is_identity()).then(|| Box::new(t));
    Ok((next, report))
}

/// A clip's transform as stored (default if it has none), for reports and queries.
pub fn get(tl: &Timeline, clip: &Id) -> Option<TransformReport> {
    tl.tracks
        .children
        .iter()
        .filter_map(|c| match c {
            Composable::Track(t) if t.kind == TrackKind::Video => Some(t),
            _ => None,
        })
        .flat_map(|t| t.children.iter())
        .filter_map(|c| match c {
            Composable::Clip(c) => Some(c),
            _ => None,
        })
        .find(|c| clip_id(c) == Some(clip))
        .map(|c| TransformReport::new(clip.clone(), &c.metadata.ve.clip.as_ref().and_then(|m| m.transform.as_deref().cloned()).unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{Insert, Split, insert, split};
    use ve_model::meta::{Asset, AssetKind, AudioInfo, MediaInfo, OutputSettings, VideoInfo};
    use ve_model::time::{Q, Rate, RationalTime};

    fn project() -> (Timeline, Id, Id) {
        let output = OutputSettings { width: 640, height: 360, rate: Rate::integer(25), sample_rate: 48_000, color_space: "bt709".into() };
        let mut tl = ve_model::doc::new_project("t", output);
        tl.metadata.ve.assets.push(Asset {
            id: Id::from_raw("asset_a"),
            url: "file:///a.mp4".into(),
            kind: AssetKind::Source,
            hash: None,
            provenance: None,
            rel: None,
            media: Some(MediaInfo {
                duration: RationalTime::from_frames(250, Rate::integer(25)),
                video: Some(VideoInfo { codec: "h264".into(), width: 640, height: 360, rate: Rate::integer(25), rotation: 0, vfr: false, still: false, color: None, alpha: None, see_through_percent: None }),
                audio: Some(AudioInfo { codec: "aac".into(), sample_rate: 48_000, channels: 2 }),
            audio_streams: Vec::new(),
                size_bytes: 0,
            }),
        });
        let p = Insert {
            asset: Id::from_raw("asset_a"),
            track: None,
            audio_track: None,
            at: None,
            source_in: None,
            source_out: None,
            duration: None,
            video: true,
            audio: true,
            streams: None,
            overwrite: false,
        };
        let (tl, r) = insert(&tl, &p).unwrap();
        let v = r.created.iter().find(|c| c.track == "V1").unwrap().id.clone();
        let a = r.created.iter().find(|c| c.track == "A1").unwrap().id.clone();
        (tl, v, a)
    }

    fn stored(tl: &Timeline, id: &Id) -> Option<Transform> {
        tl.tracks
            .children
            .iter()
            .filter_map(|c| match c {
                Composable::Track(t) => Some(t),
                _ => None,
            })
            .flat_map(|t| t.children.iter())
            .find_map(|c| match c {
                Composable::Clip(c) if clip_id(c) == Some(id) => c.metadata.ve.clip.as_ref().and_then(|m| m.transform.as_deref().cloned()),
                _ => None,
            })
    }

    fn values(v: &[(&str, Option<f64>)]) -> TransformChange {
        TransformChange { values: v.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect(), ..Default::default() }
    }

    #[test]
    fn sets_merges_and_resets() {
        let (tl, v, _) = project();
        let (tl, r) = set(&tl, &v, &values(&[("scale", Some(0.5)), ("x", Some(0.25))])).unwrap();
        assert_eq!((r.scale, r.x, r.opacity), (0.5, 0.25, 1.0));
        let (tl, r) = set(&tl, &v, &values(&[("opacity", Some(0.5))])).unwrap();
        assert_eq!((r.scale, r.opacity), (0.5, 0.5));
        // `None` resets one field; back at the default nothing is stored.
        let (tl, _) = set(&tl, &v, &values(&[("scale", None), ("x", None), ("opacity", None)])).unwrap();
        assert_eq!(stored(&tl, &v), None);
        let (tl, _) = set(&tl, &v, &values(&[("rotation", Some(90.0))])).unwrap();
        let (tl, r) = set(&tl, &v, &TransformChange { reset: true, ..Default::default() }).unwrap();
        assert_eq!(r.rotation, 0.0);
        assert_eq!(stored(&tl, &v), None);
    }

    #[test]
    fn refuses_bad_values_and_audio_clips() {
        let (tl, v, a) = project();
        assert!(set(&tl, &a, &values(&[("scale", Some(0.5))])).is_err());
        assert!(set(&tl, &v, &values(&[("opacity", Some(1.5))])).is_err());
        assert!(set(&tl, &v, &values(&[("zoom", Some(1.0))])).is_err());
        assert!(set(&tl, &v, &values(&[("x", Some(f64::NAN))])).is_err());
        assert!(set(&tl, &v, &values(&[("crop_left", Some(0.6)), ("crop_right", Some(0.5))])).is_err());
        let bad_key = TransformChange {
            keyframes: [("opacity".to_owned(), vec![Keyframe { t: RationalTime::from_frames(0, Rate::integer(25)), v: "x".into(), ease: "linear".into(), r#in: None, out: None }])]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        assert!(set(&tl, &v, &bad_key).is_err());
    }

    #[test]
    fn split_moves_keyframes_with_the_right_half() {
        let (tl, v, _) = project();
        let key = |f: i64, v: f64| Keyframe { t: RationalTime::from_frames(f, Rate::integer(25)), v: v.into(), ease: "linear".into(), r#in: None, out: None };
        let change = TransformChange { keyframes: [("x".to_owned(), vec![key(0, 0.0), key(200, 1.0)])].into_iter().collect(), ..Default::default() };
        let (tl, _) = set(&tl, &v, &change).unwrap();
        let (tl, r) = split(&tl, &Split { at: Q::from_integer(4), clips: vec![], tracks: vec![], linked: true }).unwrap();
        let right = r.created.iter().find(|c| c.track == "V1").unwrap().id.clone();
        let keys = &stored(&tl, &right).unwrap().keyframes["x"];
        // 100 frames in (4 s at 25 fps): the keys now count from the new clip's start.
        assert_eq!(keys[0].t.seconds(), Q::from_integer(-4));
        assert_eq!(keys[1].t.seconds(), Q::from_integer(4));
        assert_eq!(stored(&tl, &v).unwrap().keyframes["x"][1].t.seconds(), Q::from_integer(8));
    }
}
