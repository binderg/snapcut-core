//! Audio effects that run in a plugin (`runtime: "process"`): VST3 and CLAP
//! hosts, DSP in any language (ARCHITECTURE.md, "Audio effects").
//!
//! ffmpeg cannot call out to a plugin, so a clip with such an effect is
//! rendered on its own first: its stream decoded with the channel mapping
//! and speed applied, from a little before the clip's start to a little
//! after its end (handles for crossfades), then passed through its effects
//! in order: runs of `ffmpeg` effects as one ffmpeg pass, each `process`
//! effect as a call to its plugin. The result is a float WAV in the project
//! cache, keyed by everything that decides its samples, and the render
//! graph reads it in place of the source. Effects after the last `process`
//! effect, gain, fades, crossfades and the mix stay in the graph.
//!
//! The plugin gets raw interleaved little-endian f32 files (`input`,
//! `output`), `frames`, `channels` (2), `sample_rate`, the effect's
//! `params` (defaults filled in) and `keyframes` (clip-relative times), and
//! `start_sample`: the clip-relative sample index of the first frame
//! (negative: the handle before the clip starts). It must write exactly
//! `frames` frames.

use std::borrow::Cow;
use std::hash::{Hash, Hasher};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use serde_json::{Map, Value, json};
use ve_model::meta::EffectInstance;
use ve_model::time::Q;

use crate::RenderError;
use crate::ffmpeg::{self, Inputs};
use crate::fx::{Fx, Kind, Registry};
use crate::graph::{atempo_chain, floor_micros, index, micros};
use crate::plan::{Plan, Segment, channel_filter};

type Result<T> = std::result::Result<T, RenderError>;

/// The engine side of plugin audio effects: the daemon's plugin host.
pub trait Processor: Send + Sync {
    /// It runs plugins for the project in `root`.
    fn serves(&self, root: &Path) -> bool;
    /// Where processed clips are kept (`<project cache>/audio_fx`).
    fn cache_dir(&self) -> Option<PathBuf>;
    /// `plugin@version` of the plugin that provides `method`, if any.
    fn version(&self, method: &str) -> Option<String>;
    /// Call `method` with `params`; the plugin writes `params.output`.
    fn process(&self, method: &str, params: Value) -> std::result::Result<Value, String>;
}

fn processors() -> &'static Mutex<Vec<Weak<dyn Processor>>> {
    static P: OnceLock<Mutex<Vec<Weak<dyn Processor>>>> = OnceLock::new();
    P.get_or_init(Default::default)
}

/// Make a processor available to renders (held weakly: dropping it removes it).
pub fn register(p: Weak<dyn Processor>) {
    let mut list = processors().lock().unwrap_or_else(|e| e.into_inner());
    list.retain(|w| w.strong_count() > 0);
    list.push(p);
}

/// The processor for a project, if one is registered.
pub fn processor_for(root: Option<&Path>) -> Option<Arc<dyn Processor>> {
    let root = root?;
    let list = processors().lock().unwrap_or_else(|e| e.into_inner());
    list.iter().filter_map(Weak::upgrade).find(|p| p.serves(root))
}

/// The effect runs in a plugin.
pub fn is_process(fx: &Fx) -> bool {
    fx.manifest.runtime == "process"
}

fn runs_in_plugin(fx: &Registry, e: &EffectInstance) -> bool {
    fx.get(&e.effect).is_some_and(|f| is_process(&f))
}

/// Room before and after a clip kept for crossfades and fades at its cuts.
const HANDLE: Q = Q::new_raw(1, 1);

/// `plan` with every audio segment that has a plugin effect replaced by its
/// processed file. Borrowed unchanged when there are none.
pub fn prepare<'a>(plan: &'a Plan, fx: &Registry, sample_rate: u32) -> Result<Cow<'a, Plan>> {
    let any = plan.audio.iter().flat_map(|t| &t.segments).any(|s| s.effects.iter().any(|e| runs_in_plugin(fx, e)));
    if !any {
        return Ok(Cow::Borrowed(plan));
    }
    let mut out = plan.clone();
    for t in &mut out.audio {
        for seg in &mut t.segments {
            if seg.effects.iter().any(|e| runs_in_plugin(fx, e)) {
                *seg = process_segment(seg, fx, sample_rate)?;
            }
        }
    }
    Ok(Cow::Owned(out))
}

/// One clip's audio through its effects up to the last plugin one.
fn process_segment(seg: &Segment, fx: &Registry, sample_rate: u32) -> Result<Segment> {
    let last = seg.effects.iter().rposition(|e| runs_in_plugin(fx, e)).expect("the segment has a plugin effect");
    let head: Vec<(Arc<Fx>, &EffectInstance)> = seg.effects[..=last]
        .iter()
        .map(|e| fx.require(&e.effect, Kind::Audio).map(|f| (f, e)).map_err(|err| RenderError::Invalid(format!("{} on {}: {err}", e.id, seg.clip))))
        .collect::<Result<_>>()?;
    let proc = processor_for(fx.root.as_deref()).ok_or_else(|| {
        let e = &seg.effects[last];
        RenderError::Invalid(format!(
            "{} on {} runs in a plugin ({}); plugin effects render through the engine daemon (`ve`), with the plugin installed",
            e.id, seg.clip, e.effect
        ))
    })?;
    let cache = proc.cache_dir().ok_or_else(|| RenderError::Invalid("plugin effects need an open project".into()))?;

    // Clip-relative timeline span to render, in samples: the whole clip plus handles.
    let sr = Q::from_integer(i64::from(sample_rate));
    let zero = Q::from_integer(0);
    let speed = seg.speed;
    let media_in = seg.media_start - (seg.start - seg.clip_start) * speed;
    let media_out = media_in + seg.clip_duration * speed;
    let (lo, hi) = seg.bounds.unwrap_or((zero, media_out + HANDLE * speed));
    let pre = HANDLE.min((media_in - lo).max(zero) / speed);
    let post = HANDLE.min((hi - media_out).max(zero) / speed);
    let pre_n = (pre * sr).floor().to_integer();
    let clip_n = (seg.clip_duration * sr).ceil().to_integer();
    let post_n = (post * sr).floor().to_integer();
    let n = pre_n + clip_n + post_n;
    if n <= 0 {
        return Err(RenderError::Invalid(format!("{} is too short to process", seg.clip)));
    }
    if n.saturating_mul(8) > i64::from(u32::MAX) - 64 {
        return Err(RenderError::Invalid(format!("{} is too long for plugin effects (over about 3 hours)", seg.clip)));
    }
    let media_from = media_in - Q::from_integer(pre_n) / sr * speed;

    // Everything that decides the samples.
    let mut key = blake3::Hasher::new();
    let meta = std::fs::metadata(&seg.path).ok();
    let stamp = meta.as_ref().map(|m| (m.len(), m.modified().ok().map(|t| t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0))));
    key.update(
        format!(
            "audio_fx v1 {} {stamp:?} a:{} {} {:?} {media_from} {n} {speed} {sample_rate} {pre_n}",
            seg.path.display(),
            seg.stream,
            seg.channels.label(),
            seg.source_channels
        )
        .as_bytes(),
    );
    for (f, e) in &head {
        let code = if is_process(f) { proc.version(&f.source).unwrap_or_else(|| "missing".into()) } else { format!("{:x}", f.hash) };
        key.update(format!("fx {} {code} {}", e.effect, serde_json::to_string(e).unwrap_or_default()).as_bytes());
    }
    let name = key.finalize().to_hex();
    let out_path = cache.join(format!("{}.wav", &name[..32]));
    let processed = Segment {
        path: out_path.clone(),
        stream: 0,
        channels: ve_model::meta::Channels::default(),
        source_channels: (2, true),
        speed: Q::from_integer(1),
        media_start: (seg.start - seg.clip_start) + Q::from_integer(pre_n) / sr,
        bounds: Some((zero, Q::from_integer(n) / sr)),
        effects: seg.effects[last + 1..].to_vec(),
        ..seg.clone()
    };
    if out_path.is_file() {
        return Ok(processed);
    }

    // Render it: decode, then each stage file to file.
    let work = cache.join("work");
    std::fs::create_dir_all(&work).map_err(|e| RenderError::Media(format!("{}: {e}", work.display())))?;
    let tmp = |k: usize| work.join(format!("{}-{}-{k}.f32", &name[..16], std::process::id()));
    let mut files: Vec<PathBuf> = Vec::new();
    let result = (|| -> Result<()> {
        let mut cur = tmp(0);
        files.push(cur.clone());
        decode(seg, media_from, n, sample_rate, &cur, &work)?;
        let local = Q::from_integer(-pre_n) / sr;
        let len = Q::from_integer(n) / sr;
        let mut i = 0;
        while i < head.len() {
            let next = tmp(files.len());
            files.push(next.clone());
            if is_process(&head[i].0) {
                let (f, e) = &head[i];
                run_plugin(proc.as_ref(), f, e, &seg.clip, (&cur, &next), (n, -pre_n, sample_rate))?;
                i += 1;
            } else {
                let j = head[i..].iter().position(|(f, _)| is_process(f)).map_or(head.len(), |k| i + k);
                let chains: Vec<String> = head[i..j]
                    .iter()
                    .enumerate()
                    .map(|(k, (f, e))| crate::afx::chain(f, &e.params, &e.keyframes, local, len, &format!("pfx{k}")))
                    .collect();
                filter_raw(&cur, &next, &chains.join(","), n, sample_rate, &work)?;
                i = j;
            }
            cur = next;
        }
        write_wav(&cur, &out_path, n, sample_rate)
    })();
    for f in files {
        let _ = std::fs::remove_file(f);
    }
    result.map(|()| processed)
}

/// The segment's stream from `media_from`, channel-mapped to stereo at the
/// project rate, sped to timeline time: exactly `n` frames of raw f32.
fn decode(seg: &Segment, media_from: Q, n: i64, sample_rate: u32, out: &Path, work: &Path) -> Result<()> {
    let sr = Q::from_integer(i64::from(sample_rate));
    let coarse = seg.path.extension().is_some_and(|e| ["mkv", "mka", "webm"].contains(&e.to_string_lossy().to_lowercase().as_str()));
    let seek = if coarse { Q::from_integer(0) } else { floor_micros(media_from.max(Q::from_integer(0))) };
    let a = index(media_from - seek, sr).max(0);
    let b = a + (Q::from_integer(n) * seg.speed).ceil().to_integer().max(1);
    let mut f = format!(
        "aresample={sample_rate}:async=1:first_pts=0,{},aformat=sample_fmts=flt:channel_layouts=stereo,atrim=start_sample={a}:end_sample={b},asetpts=PTS-STARTPTS",
        channel_filter(&seg.channels, seg.source_channels)
    );
    if seg.speed != Q::from_integer(1) {
        for t in atempo_chain(seg.speed) {
            f.push_str(&format!(",atempo={t}"));
        }
    }
    f.push_str(&format!(",apad=whole_len={n},atrim=end_sample={n}"));
    let mut args = Vec::new();
    if seek > Q::from_integer(0) {
        args.extend(["-ss".to_owned(), micros(seek)]);
    }
    args.extend(["-vn".into(), "-sn".into(), "-dn".into(), "-i".into(), seg.path.to_string_lossy().into_owned()]);
    args.extend(["-map".into(), format!("0:a:{}", seg.stream)]);
    let outs = raw_out(&f, sample_rate, out);
    ffmpeg::run_inputs(&Inputs::Args(args), &outs, work, false)?;
    check_len(out, n, &seg.clip)
}

fn raw_out(filter: &str, sample_rate: u32, out: &Path) -> Vec<String> {
    vec![
        "-af".into(),
        filter.into(),
        "-f".into(),
        "f32le".into(),
        "-ac".into(),
        "2".into(),
        "-ar".into(),
        sample_rate.to_string(),
        out.to_string_lossy().into_owned(),
    ]
}

/// Run `ffmpeg` audio effects over a raw file.
fn filter_raw(input: &Path, out: &Path, chain: &str, n: i64, sample_rate: u32, work: &Path) -> Result<()> {
    let args = vec![
        "-f".into(),
        "f32le".into(),
        "-ar".into(),
        sample_rate.to_string(),
        "-ac".into(),
        "2".into(),
        "-i".into(),
        input.to_string_lossy().into_owned(),
    ];
    let f = format!("{chain},apad=whole_len={n},atrim=end_sample={n}");
    ffmpeg::run_inputs(&Inputs::Args(args), &raw_out(&f, sample_rate, out), work, false)?;
    check_len(out, n, "an ffmpeg pass")
}

fn run_plugin(proc: &dyn Processor, f: &Fx, e: &EffectInstance, clip: &str, (input, output): (&Path, &Path), (n, start, sample_rate): (i64, i64, u32)) -> Result<()> {
    let mut params: Map<String, Value> = f.params.iter().map(|p| (p.name.clone(), p.default.clone())).collect();
    for (k, v) in &e.params {
        params.insert(k.clone(), v.clone());
    }
    let call = json!({
        "effect": e.effect,
        "clip": clip,
        "params": params,
        "keyframes": e.keyframes,
        "sample_rate": sample_rate,
        "channels": 2,
        "frames": n,
        "start_sample": start,
        "input": input,
        "output": output,
    });
    proc.process(&f.source, call).map_err(|m| RenderError::Invalid(format!("{} on {clip} ({}): {m}", e.id, e.effect)))?;
    check_len(output, n, &format!("plugin method {}", f.source))
}

fn check_len(path: &Path, n: i64, what: &str) -> Result<()> {
    let got = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let want = u64::try_from(n * 8).unwrap_or(0);
    if got != want {
        return Err(RenderError::Invalid(format!("{what} wrote {} audio frames, expected {n}", got / 8)));
    }
    Ok(())
}

/// Wrap raw stereo f32 in a WAV (IEEE float) header, written atomically.
fn write_wav(raw: &Path, out: &Path, n: i64, sample_rate: u32) -> Result<()> {
    let io = |e: std::io::Error| RenderError::Media(format!("{}: {e}", out.display()));
    let data = u32::try_from(n * 8).map_err(|_| RenderError::Invalid("processed audio is too long for a WAV file".into()))?;
    let part = out.with_extension(format!("wav.{}.part", std::process::id()));
    let mut w = std::io::BufWriter::new(std::fs::File::create(&part).map_err(io)?);
    let mut header = Vec::with_capacity(58);
    header.extend_from_slice(b"RIFF");
    header.extend_from_slice(&(50 + data).to_le_bytes());
    header.extend_from_slice(b"WAVEfmt ");
    header.extend_from_slice(&18u32.to_le_bytes());
    header.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    header.extend_from_slice(&2u16.to_le_bytes());
    header.extend_from_slice(&sample_rate.to_le_bytes());
    header.extend_from_slice(&(sample_rate * 8).to_le_bytes());
    header.extend_from_slice(&8u16.to_le_bytes());
    header.extend_from_slice(&32u16.to_le_bytes());
    header.extend_from_slice(&0u16.to_le_bytes());
    header.extend_from_slice(b"fact");
    header.extend_from_slice(&4u32.to_le_bytes());
    header.extend_from_slice(&u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes());
    header.extend_from_slice(b"data");
    header.extend_from_slice(&data.to_le_bytes());
    w.write_all(&header).map_err(io)?;
    let mut src = std::fs::File::open(raw).map_err(io)?;
    std::io::copy(&mut src, &mut w).map_err(io)?;
    w.into_inner().map_err(|e| io(e.into_error()))?.sync_all().map_err(io)?;
    std::fs::rename(&part, out).map_err(io)
}

/// A stable hash of the processors' view of a plan's plugin effects, for
/// cache keys that must change when a plugin is upgraded.
pub fn versions_key(plan: &Plan, fx: &Registry) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let proc = processor_for(fx.root.as_deref());
    for e in plan.audio.iter().flat_map(|t| &t.segments).flat_map(|s| &s.effects) {
        if let Some(f) = fx.get(&e.effect).filter(|f| is_process(f)) {
            proc.as_ref().and_then(|p| p.version(&f.source)).hash(&mut h);
        }
    }
    h.finish()
}
