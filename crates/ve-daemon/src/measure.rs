//! `measure.*`: read-only measurements of audio, plus `edit.normalize`,
//! which acts on one.
//!
//! Every method measures either an asset's file (`asset`, the whole file as
//! it is on disk) or the timeline mix (`from`/`to`, what actually plays
//! after cuts, speed and volume). Times in results count from the start of
//! that source. `resolution` adds a series with one value per step, which is
//! what a UI draws and what an agent scans for "where is it too quiet".
//!
//! File measurements are cached in `<OS cache>/ve/measure/<file hash>/`,
//! keyed by method and settings, so a file is measured once however many
//! projects or calls ask. Timeline measurements are not cached.
//!
//! Loudness comes from ffmpeg's `ebur128` (EBU R128 with true peak); levels,
//! waveform and pitch are computed in `ve-analyze` from samples streamed out
//! of ffmpeg, so long files are never held in memory.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ve_analyze::levels::{LevelMeter, WaveformBuilder};
use ve_analyze::loudness::{self, Loudness, round1};
use ve_analyze::pitch::{self, PitchTracker};
use ve_model::time::Q;
use ve_model::{Id, Timeline};
use ve_ops::edit::{EditReport, tc};
use ve_protocol::{ErrorCode, RpcError};
use ve_render::{AudioSource, Span};

use crate::commit::{Committed, commit, mutation_params, parse_time};
use crate::project::{OpenProject, cache_base};
use crate::render::{render_error, span, work_dir};

/// Bump when any measurement's method or output changes, so stale cache entries are ignored.
const CACHE_VERSION: u32 = 1;
/// Most points a series or waveform may have.
const MAX_POINTS: usize = 100_000;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MeasureParams {
    /// Measure this asset's file (the whole file) instead of the timeline.
    #[serde(default)]
    pub asset: Option<Id>,
    /// Timeline range start. Default: timeline start. Not used with `asset`.
    #[serde(default)]
    pub from: Option<String>,
    /// Timeline range end, exclusive. Default: timeline end.
    #[serde(default)]
    pub to: Option<String>,
    /// Also return a series, one value per this much time (`1s`, `10f`, `0.5s`).
    #[serde(default)]
    pub resolution: Option<String>,
}

fn default_buckets() -> usize {
    1000
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaveformParams {
    #[serde(default)]
    pub asset: Option<Id>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    /// Number of points (default 1000, max 100000), e.g. the pixel width to draw.
    #[serde(default = "default_buckets")]
    pub buckets: usize,
}

/// What was measured.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceInfo {
    /// The asset, for file measurements; absent for the timeline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<Id>,
    /// Range measured: timeline times, or `00:00:00:00` to the file's length.
    pub from: String,
    pub to: String,
}

/// Values over time. Point `i` covers `[i * step, (i + 1) * step)` from the
/// source start; `null` where there was nothing to measure (silence).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Series {
    pub step: String,
    #[serde(flatten)]
    pub values: BTreeMap<String, Vec<Option<f64>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LoudnessResult {
    pub source: SourceInfo,
    /// Integrated loudness, LUFS (YouTube and podcasts aim near -14 to -16, EBU broadcast -23).
    /// Absent when everything is below -70 LUFS.
    pub integrated_lufs: Option<f64>,
    /// Loudness range, LU: how much loud and quiet parts differ. Speech is usually 3-8.
    pub range_lu: Option<f64>,
    /// Highest true peak (4x oversampled), dBTP. Above -1 risks distortion after encoding.
    pub true_peak_dbtp: Option<f64>,
    /// `lufs`: momentary loudness per step (energy average of 400 ms readings).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<Series>,
    /// Served from the cache.
    #[serde(default)]
    pub cached: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChannelLevels {
    /// RMS level, dBFS (absent for a silent channel).
    pub rms_db: Option<f64>,
    /// Sample peak, dBFS.
    pub peak_db: Option<f64>,
    /// Mean sample value; well above 0.001 points at a faulty recorder or cable.
    pub dc_offset: f64,
    /// Samples at full scale: clipping in the recording.
    pub clipped_samples: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LevelsResult {
    pub source: SourceInfo,
    pub sample_rate: u32,
    pub channels: Vec<ChannelLevels>,
    /// Level of the quietest stretches that are not digital silence (room tone, hiss), dBFS.
    pub noise_floor_db: Option<f64>,
    /// Stereo only: left minus right RMS, dB. Far from 0 means one side is weak or dead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance_db: Option<f64>,
    /// Stereo only: left/right correlation, -1 to 1. Negative: one side is polarity-flipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation: Option<f64>,
    /// `rms_db` and `peak_db` per step, over all channels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<Series>,
    #[serde(default)]
    pub cached: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WaveformResult {
    pub source: SourceInfo,
    pub sample_rate: u32,
    /// Frames (one sample per channel) measured.
    pub samples: u64,
    /// Per point: lowest and highest sample on any channel (-1 to 1) and RMS over all channels.
    pub min: Vec<f32>,
    pub max: Vec<f32>,
    pub rms: Vec<f32>,
    #[serde(default)]
    pub cached: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PitchResult {
    pub source: SourceInfo,
    /// Median fundamental frequency of voiced audio, Hz (60-800 Hz range).
    pub median_hz: Option<f64>,
    /// 10th and 90th percentile: the usual range.
    pub low_hz: Option<f64>,
    pub high_hz: Option<f64>,
    /// Nearest note to the median, e.g. `A3`.
    pub note: Option<String>,
    /// Share of 20 ms frames with a pitch, 0 to 1.
    pub voiced: f64,
    /// `hz`: median pitch per step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<Series>,
    #[serde(default)]
    pub cached: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

// --- sources ---

enum Target {
    /// Asset file, its content hash (the cache key) and length in seconds.
    File { asset: Id, path: PathBuf, hash: Option<String>, length: Q },
    Timeline(Span),
}

struct Resolved {
    target: Target,
    channels: u32,
    sample_rate: u32,
}

impl Resolved {
    fn source<'a>(&'a self, tl: &'a Timeline) -> AudioSource<'a> {
        match &self.target {
            Target::File { path, .. } => AudioSource::File { path },
            Target::Timeline(span) => AudioSource::Timeline { tl, span: *span },
        }
    }

    fn length(&self) -> Q {
        match &self.target {
            Target::File { length, .. } => *length,
            Target::Timeline(s) => s.duration(),
        }
    }

    fn info(&self, tl: &Timeline) -> SourceInfo {
        let rate = tl.output_rate();
        match &self.target {
            Target::File { asset, length, .. } => {
                SourceInfo { asset: Some(asset.clone()), from: tc(Q::from_integer(0), rate), to: tc(*length, rate) }
            }
            Target::Timeline(s) => SourceInfo { asset: None, from: tc(s.start, rate), to: tc(s.end, rate) },
        }
    }

    fn hash(&self) -> Option<&str> {
        match &self.target {
            Target::File { hash, .. } => hash.as_deref(),
            Target::Timeline(_) => None,
        }
    }
}

fn resolve(tl: &Timeline, asset: Option<&Id>, from: Option<&String>, to: Option<&String>) -> Result<Resolved, RpcError> {
    let Some(id) = asset else {
        let sample_rate = tl.project().map_or(48_000, |p| p.output.sample_rate);
        return Ok(Resolved { target: Target::Timeline(span(tl, from, to)?), channels: 2, sample_rate });
    };
    if from.is_some() || to.is_some() {
        return Err(RpcError::invalid_params("from/to are timeline times; an asset is measured whole"));
    }
    let a = tl
        .metadata
        .ve
        .assets
        .iter()
        .find(|a| &a.id == id)
        .ok_or_else(|| RpcError::invalid_params(format!("no asset `{id}`; `ve call media.assets` lists them")))?;
    let media = a.media.as_ref().ok_or_else(|| RpcError::invalid_params(format!("{id} has no probe data; re-import it")))?;
    let audio = media.audio.as_ref().ok_or_else(|| RpcError::invalid_params(format!("{id} has no audio")))?;
    let path = ve_render::local_path(&a.url)
        .ok_or_else(|| RpcError::new(ErrorCode::RenderFailure, format!("{id}: only file:// media can be measured ({})", a.url)))?;
    Ok(Resolved {
        target: Target::File { asset: id.clone(), path, hash: a.hash.clone(), length: media.duration.seconds() },
        channels: audio.channels.clamp(1, 8),
        sample_rate: audio.sample_rate,
    })
}

/// Parse `resolution` into a step and the number of points over `length`.
fn steps(tl: &Timeline, resolution: Option<&String>, length: Q) -> Result<Option<(Q, usize)>, RpcError> {
    let Some(r) = resolution else { return Ok(None) };
    let step = parse_time("resolution", r, tl.output_rate())?;
    if step <= Q::from_integer(0) {
        return Err(RpcError::invalid_params("resolution must be positive"));
    }
    let count = (length / step).ceil().to_integer().max(1) as usize;
    if count > MAX_POINTS {
        return Err(RpcError::invalid_params(format!("resolution {r} gives {count} points; the most is {MAX_POINTS}")));
    }
    Ok(Some((step, count)))
}

fn series(tl: &Timeline, step: Q, values: impl IntoIterator<Item = (&'static str, Vec<Option<f64>>)>) -> Series {
    Series { step: tc(step, tl.output_rate()), values: values.into_iter().map(|(k, v)| (k.to_owned(), v)).collect() }
}

// --- cache ---

/// Return the cached result for a file measurement, or compute and store it.
/// Timeline measurements (no hash) always compute.
fn cached<T: Serialize + DeserializeOwned>(
    hash: Option<&str>,
    method: &str,
    key: &Value,
    compute: impl FnOnce() -> Result<T, RpcError>,
) -> Result<(T, bool), RpcError> {
    let Some(path) = hash.and_then(|h| cache_path(h, method, key)) else {
        return Ok((compute()?, false));
    };
    if let Some(hit) = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()) {
        return Ok((hit, true));
    }
    let value = compute()?;
    if let Err(e) = write_cache(&path, &value) {
        tracing::warn!("could not cache {}: {e}", path.display());
    }
    Ok((value, false))
}

fn cache_path(hash: &str, method: &str, key: &Value) -> Option<PathBuf> {
    let base = cache_base().ok()?;
    let key = blake3::hash(format!("v{CACHE_VERSION} {key}").as_bytes()).to_hex();
    // Asset hashes are blake3 hex; keep the folder name short and path-safe.
    let dir: String = hash.chars().filter(char::is_ascii_alphanumeric).take(32).collect();
    Some(base.join("measure").join(dir).join(format!("{method}-{}.json", &key[..16])))
}

fn write_cache<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let dir = path.parent().expect("cache path has a folder");
    std::fs::create_dir_all(dir)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(value).map_err(std::io::Error::other)?)?;
    std::fs::rename(tmp, path)
}

// --- methods ---

fn measure_loudness(src: &AudioSource<'_>, work: &Path) -> Result<(Loudness, Vec<String>), RpcError> {
    let (log, warnings) = ve_render::audio_filter_log(src, "ebur128=peak=true:framelog=info", work).map_err(render_error)?;
    Ok((loudness::parse_ebur128(&log), warnings))
}

pub fn loudness(p: &mut OpenProject, a: MeasureParams) -> Result<LoudnessResult, RpcError> {
    let work = work_dir(p);
    let tl = &p.timeline;
    let r = resolve(tl, a.asset.as_ref(), a.from.as_ref(), a.to.as_ref())?;
    let steps = steps(tl, a.resolution.as_ref(), r.length())?;
    let key = json!({ "step": steps.map(|s| s.0.to_string()) });
    let (mut out, hit) = cached(r.hash(), "loudness", &key, || {
        let (l, warnings) = measure_loudness(&r.source(tl), &work)?;
        Ok(LoudnessResult {
            source: r.info(tl),
            integrated_lufs: l.integrated,
            range_lu: l.range,
            true_peak_dbtp: l.true_peak,
            series: steps.map(|(step, n)| series(tl, step, [("lufs", loudness::bucket_lufs(&l.momentary, step, n))])),
            cached: false,
            warnings,
        })
    })?;
    out.cached = hit;
    Ok(out)
}

pub fn levels(p: &mut OpenProject, a: MeasureParams) -> Result<LevelsResult, RpcError> {
    let work = work_dir(p);
    let tl = &p.timeline;
    let r = resolve(tl, a.asset.as_ref(), a.from.as_ref(), a.to.as_ref())?;
    let steps = steps(tl, a.resolution.as_ref(), r.length())?;
    let key = json!({ "step": steps.map(|s| s.0.to_string()) });
    let (mut out, hit) = cached(r.hash(), "levels", &key, || {
        let mut meter = LevelMeter::new(r.channels, r.sample_rate, steps.map(|s| s.0));
        let warnings =
            ve_render::decode_audio(&r.source(tl), r.sample_rate, r.channels, &work, &mut |s| meter.push(s)).map_err(render_error)?;
        let l = meter.finish();
        let series = steps.map(|(step, n)| {
            let fit = |mut v: Vec<Option<f64>>| {
                v.resize(n, None);
                v
            };
            series(tl, step, [("rms_db", fit(l.rms_series.clone())), ("peak_db", fit(l.peak_series.clone()))])
        });
        Ok(LevelsResult {
            source: r.info(tl),
            sample_rate: r.sample_rate,
            channels: l
                .channels
                .iter()
                .map(|c| ChannelLevels { rms_db: c.rms_db, peak_db: c.peak_db, dc_offset: c.dc_offset, clipped_samples: c.clipped_samples })
                .collect(),
            noise_floor_db: l.noise_floor_db,
            balance_db: l.balance_db,
            correlation: l.correlation,
            series,
            cached: false,
            warnings,
        })
    })?;
    out.cached = hit;
    Ok(out)
}

pub fn waveform(p: &mut OpenProject, a: WaveformParams) -> Result<WaveformResult, RpcError> {
    if a.buckets == 0 || a.buckets > MAX_POINTS {
        return Err(RpcError::invalid_params(format!("buckets must be 1 to {MAX_POINTS}")));
    }
    let work = work_dir(p);
    let tl = &p.timeline;
    let r = resolve(tl, a.asset.as_ref(), a.from.as_ref(), a.to.as_ref())?;
    let key = json!({ "buckets": a.buckets });
    let (mut out, hit) = cached(r.hash(), "waveform", &key, || {
        let mut w = WaveformBuilder::new(r.channels);
        let warnings =
            ve_render::decode_audio(&r.source(tl), r.sample_rate, r.channels, &work, &mut |s| w.push(s)).map_err(render_error)?;
        let w = w.finish(a.buckets);
        Ok(WaveformResult {
            source: r.info(tl),
            sample_rate: r.sample_rate,
            samples: w.samples,
            min: w.min,
            max: w.max,
            rms: w.rms,
            cached: false,
            warnings,
        })
    })?;
    out.cached = hit;
    Ok(out)
}

pub fn pitch(p: &mut OpenProject, a: MeasureParams) -> Result<PitchResult, RpcError> {
    let work = work_dir(p);
    let tl = &p.timeline;
    let r = resolve(tl, a.asset.as_ref(), a.from.as_ref(), a.to.as_ref())?;
    let steps = steps(tl, a.resolution.as_ref(), r.length())?;
    let key = json!({ "step": steps.map(|s| s.0.to_string()) });
    let (mut out, hit) = cached(r.hash(), "pitch", &key, || {
        let mut t = PitchTracker::default();
        let warnings = ve_render::decode_audio(&r.source(tl), pitch::SAMPLE_RATE, 1, &work, &mut |s| t.push(s)).map_err(render_error)?;
        let s = pitch::summarize(&t.frames);
        Ok(PitchResult {
            source: r.info(tl),
            median_hz: s.median_hz,
            low_hz: s.low_hz,
            high_hz: s.high_hz,
            note: s.note,
            voiced: s.voiced,
            series: steps.map(|(step, n)| series(tl, step, [("hz", pitch::bucket_hz(&t.frames, step, n))])),
            cached: false,
            warnings,
        })
    })?;
    out.cached = hit;
    Ok(out)
}

// --- edit.normalize ---

fn default_target() -> f64 {
    -14.0
}

fn default_ceiling() -> f64 {
    -1.0
}

mutation_params! {
    /// `edit.normalize`: measure the timeline mix and add one gain to its audio
    /// clips so the integrated loudness hits `target`.
    pub struct NormalizeParams {
        /// Target integrated loudness, LUFS: -14 (YouTube, default), -16 (podcasts), -23 (EBU broadcast).
        #[serde(default = "default_target")]
        pub target: f64,
        /// Highest true peak allowed after the gain, dBTP (default -1). The gain is
        /// limited to stay under it; louder targets then need a limiter.
        #[serde(default = "default_ceiling")]
        pub ceiling: f64,
        /// Measure only this range and change only clips that overlap it
        /// (whole clips, so audio outside the range changes too).
        #[serde(default)]
        pub from: Option<String>,
        #[serde(default)]
        pub to: Option<String>,
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct NormalizeResult {
    /// Integrated loudness before, LUFS.
    pub measured_lufs: f64,
    pub measured_true_peak_dbtp: Option<f64>,
    /// Gain added to every changed clip, dB.
    pub gain_db: f64,
    /// What the mix should measure after, assuming nothing overlapping clips outside the range.
    pub expected_lufs: f64,
    pub expected_true_peak_dbtp: Option<f64>,
    /// Audio clips whose volume changed.
    pub clips: Vec<Id>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(flatten)]
    pub report: EditReport,
}

pub fn normalize(p: &mut OpenProject, a: NormalizeParams) -> Result<Committed<NormalizeResult>, RpcError> {
    if !(-70.0..=0.0).contains(&a.target) {
        return Err(RpcError::invalid_params("target must be between -70 and 0 LUFS"));
    }
    if !(-20.0..=0.0).contains(&a.ceiling) {
        return Err(RpcError::invalid_params("ceiling must be between -20 and 0 dBTP"));
    }
    let work = work_dir(p);
    commit(p, a.mutation(), "edit.normalize", |tl| {
        let sp = span(tl, a.from.as_ref(), a.to.as_ref())?;
        let (l, mut warnings) = measure_loudness(&AudioSource::Timeline { tl, span: sp }, &work)?;
        let measured = l
            .integrated
            .ok_or_else(|| RpcError::invalid_params("nothing loud enough to measure there (all below -70 LUFS)"))?;
        let mut gain = a.target - measured;
        if let Some(tp) = l.true_peak
            && tp + gain > a.ceiling
        {
            let limited = a.ceiling - tp;
            warnings.push(format!(
                "gain limited to {limited:+.1} dB (not {gain:+.1}) so true peak stays at {} dBTP; reaching {} LUFS needs a limiter",
                a.ceiling, a.target
            ));
            gain = limited;
        }
        let gain = round1(gain);
        let mut result = NormalizeResult {
            measured_lufs: measured,
            measured_true_peak_dbtp: l.true_peak,
            gain_db: gain,
            expected_lufs: round1(measured + gain),
            expected_true_peak_dbtp: l.true_peak.map(|t| round1(t + gain)),
            clips: Vec::new(),
            warnings,
            report: ve_ops::edit::report_for(tl),
        };
        if gain == 0.0 {
            return Ok((tl.clone(), result));
        }
        let range = (a.from.is_some() || a.to.is_some()).then_some((sp.start, sp.end));
        let (next, report, clips) = ve_ops::edit::add_gain(tl, range, gain).map_err(|e| RpcError::invalid_params(e.0))?;
        result.report = report;
        result.clips = clips;
        Ok((next, result))
    })
}
