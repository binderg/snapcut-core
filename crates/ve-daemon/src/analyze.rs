//! `analyze.*` methods: built-in analyzers whose results become timeline
//! markers (source `analyzer`). Re-running an analyzer over a range
//! replaces its earlier markers there instead of piling up duplicates.
//!
//! Silences snap inward to whole frames (a cut never eats a frame of sound
//! the detector heard) and `pad` keeps some air on both sides, so the marker
//! ranges can go straight into `edit.ripple_delete {"markers": "silence"}`.

use schemars::JsonSchema;
use serde::Serialize;
use serde_json::json;
use ve_model::meta::MarkerSource;
use ve_model::time::Q;
use ve_ops::edit::tc;
use ve_ops::markers::{self, NewMarker};
use ve_protocol::RpcError;

use crate::commit::{Committed, commit, mutation_params, parse_time};
use crate::project::OpenProject;
use crate::render::{render_error, span, work_dir};

fn yes() -> bool {
    true
}

fn default_threshold_db() -> String {
    "-35dB".into()
}

fn default_min() -> String {
    "0.5s".into()
}

fn default_pad() -> String {
    "0s".into()
}

fn default_scene_threshold() -> String {
    "10".into()
}

/// One finding.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Finding {
    /// `silence` or `scene`.
    pub kind: String,
    pub start: String,
    /// Exclusive end; equal to `start` for scene cuts.
    pub end: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct AnalyzeResult {
    pub ranges: Vec<Finding>,
    /// Markers written (empty with `markers: false` or on a dry run's report).
    pub markers: Vec<ve_model::Id>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

mutation_params! {
    /// `analyze.silence`: quiet stretches of the timeline's audio.
    pub struct SilenceParams {
        #[serde(default)]
        pub from: Option<String>,
        #[serde(default)]
        pub to: Option<String>,
        /// Level that counts as silence, e.g. `-35dB` (default). Raise it (-30dB) for noisy rooms.
        #[serde(default = "default_threshold_db")]
        pub threshold: String,
        /// Shortest silence to report (default `0.5s`).
        #[serde(default = "default_min")]
        pub min_duration: String,
        /// Shrink each silence by this much at both ends so cuts keep a breath (default `0s`; `0.1s` is natural).
        #[serde(default = "default_pad")]
        pub pad: String,
        /// Write `silence` markers (default true).
        #[serde(default = "yes")]
        pub markers: bool,
    }
}

fn parse_db(s: &str) -> Result<f64, RpcError> {
    let t = s.trim().trim_end_matches("dB").trim_end_matches("db").trim();
    t.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v < 0.0 && *v > -120.0)
        .ok_or_else(|| RpcError::invalid_params(format!("threshold `{s}`: use a negative level like -35dB")))
}

pub fn silence(p: &mut OpenProject, a: SilenceParams) -> Result<Committed<AnalyzeResult>, RpcError> {
    let work = work_dir(p);
    let threshold = parse_db(&a.threshold)?;
    commit(p, a.mutation(), "analyze.silence", |tl| {
        let rate = tl.output_rate();
        let r = rate.as_ratio();
        let sp = span(tl, a.from.as_ref(), a.to.as_ref())?;
        let min = parse_time("min_duration", &a.min_duration, rate)?;
        let pad = parse_time("pad", &a.pad, rate)?;
        if min <= Q::from_integer(0) || pad < Q::from_integer(0) {
            return Err(RpcError::invalid_params("min_duration must be positive and pad 0 or more"));
        }
        let (found, warnings) = ve_analyze::silence(tl, sp, threshold, min, &work).map_err(render_error)?;
        let mut ranges = Vec::new();
        let mut new = Vec::new();
        for s in found {
            // Inward to whole frames, after the pad.
            let start = ((s.start + pad) * r).ceil() / r;
            let end = ((s.end - pad) * r).floor() / r;
            if end <= start {
                continue;
            }
            ranges.push(Finding {
                kind: "silence".into(),
                start: tc(start, rate),
                end: tc(end, rate),
                note: format!("{}f", ((end - start) * r).to_integer()),
            });
            new.push(NewMarker {
                start,
                duration: end - start,
                name: "silence".into(),
                color: Some("purple".into()),
                comment: String::new(),
                source: MarkerSource::Analyzer,
                payload: json!({"analyzer": "silence", "threshold_db": threshold, "min_duration": a.min_duration, "pad": a.pad}),
            });
        }
        if !a.markers {
            return Ok((tl.clone(), AnalyzeResult { ranges, markers: vec![], warnings }));
        }
        let (next, ids) = markers::replace_analyzer(tl, "silence", sp.start, sp.end, new).map_err(|e| RpcError::invalid_params(e.0))?;
        Ok((next, AnalyzeResult { ranges, markers: ids, warnings }))
    })
}

mutation_params! {
    /// `analyze.scenes`: hard cuts in the timeline's picture.
    pub struct ScenesParams {
        #[serde(default)]
        pub from: Option<String>,
        #[serde(default)]
        pub to: Option<String>,
        /// scdet threshold, 0-100 (default 10; lower finds more cuts).
        #[serde(default = "default_scene_threshold")]
        pub threshold: String,
        /// Write `scene` markers (default true).
        #[serde(default = "yes")]
        pub markers: bool,
    }
}

pub fn scenes(p: &mut OpenProject, a: ScenesParams) -> Result<Committed<AnalyzeResult>, RpcError> {
    let work = work_dir(p);
    let threshold: f64 = a
        .threshold
        .trim()
        .parse()
        .ok()
        .filter(|v: &f64| (0.0..=100.0).contains(v))
        .ok_or_else(|| RpcError::invalid_params(format!("threshold `{}`: use a number from 0 to 100", a.threshold)))?;
    commit(p, a.mutation(), "analyze.scenes", |tl| {
        let rate = tl.output_rate();
        let r = rate.as_ratio();
        let sp = span(tl, a.from.as_ref(), a.to.as_ref())?;
        let (found, warnings) = ve_analyze::scenes(tl, sp, threshold, &work).map_err(render_error)?;
        let mut ranges = Vec::new();
        let mut new = Vec::new();
        for c in found {
            let at = (c.at * r).round() / r;
            ranges.push(Finding { kind: "scene".into(), start: tc(at, rate), end: tc(at, rate), note: format!("score {:.1}", c.score) });
            new.push(NewMarker {
                start: at,
                duration: Q::from_integer(0),
                name: "scene".into(),
                color: Some("cyan".into()),
                comment: String::new(),
                source: MarkerSource::Analyzer,
                payload: json!({"analyzer": "scenes", "score": (c.score * 1000.0).round() / 1000.0}),
            });
        }
        if !a.markers {
            return Ok((tl.clone(), AnalyzeResult { ranges, markers: vec![], warnings }));
        }
        let (next, ids) = markers::replace_analyzer(tl, "scenes", sp.start, sp.end, new).map_err(|e| RpcError::invalid_params(e.0))?;
        Ok((next, AnalyzeResult { ranges, markers: ids, warnings }))
    })
}
