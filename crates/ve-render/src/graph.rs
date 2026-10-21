//! Filtergraph compiler: a [`Plan`] becomes one FFmpeg `-filter_complex`.
//!
//! Exactness: every segment is cut by frame and sample counts, never by
//! float seconds. Video is conformed to the source's nominal rate first
//! (`fps=`, which also fixes VFR phone footage), cut with
//! `trim=start_frame/end_frame`, retimed, conformed to the output rate and
//! padded or cut to exactly the frames its timeline span covers. Audio is
//! resampled to the output rate, counted with `asetpts=N/SR/TB` and cut with
//! `atrim=start_sample/end_sample`. Positions round once from exact timeline
//! time, so lengths never drift.
//!
//! Decoding: segments that read the same file in increasing media order share
//! one input (`-ss` to the first, then `split`), so a 300-cut talking head
//! is one linear decode, not 300 seeks. A segment that goes backwards in the
//! media starts a new input.
//!
//! Audio cuts get a short crossfade by default: each side of a cut extends
//! into its media handle and they overlap, faded, on alternating lanes that
//! are summed (`amix normalize=0`). Clip edges next to a gap get a short
//! fade so nothing clicks. This is also what makes J/L cuts free: audio
//! clips are placed by their own timeline position.

use std::fmt::Write;
use std::path::PathBuf;

use ve_model::time::{Q, Rate};

use crate::plan::{Edge, Plan, Segment};

/// How a source fills the output frame when the aspect ratios differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Whole picture, letterboxed or pillarboxed.
    Fit,
    /// Cover the frame, cropping the overflow (vertical exports from landscape).
    Fill,
}

/// The output picture and sound.
#[derive(Debug, Clone, Copy)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub rate: Rate,
    pub sample_rate: u32,
    pub fit: Fit,
}

impl Canvas {
    /// Same aspect, scaled to `width` (even dimensions, as yuv420p needs).
    pub fn scaled_to_width(&self, width: u32) -> Self {
        let w = width.max(16) & !1;
        let h = ((u64::from(w) * u64::from(self.height) / u64::from(self.width.max(1))) as u32).max(16) & !1;
        Self { width: w, height: h, ..*self }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Video,
    Audio,
}

#[derive(Debug, Clone)]
struct Input {
    path: PathBuf,
    /// `-ss` in seconds, rounded down to the microsecond so the wanted frame
    /// is never dropped as "before the seek point".
    seek: Q,
    kind: Kind,
    threads: Option<u32>,
}

/// A filtergraph under construction.
#[derive(Debug, Default)]
pub struct Graph {
    inputs: Vec<Input>,
    chains: Vec<String>,
    n: usize,
}

/// Exact rational as an FFmpeg-friendly decimal with 6 places, rounded down.
fn micros(q: Q) -> String {
    let us = (q * Q::from_integer(1_000_000)).floor().to_integer().max(0);
    format!("{}.{:06}", us / 1_000_000, us % 1_000_000)
}

fn floor_micros(q: Q) -> Q {
    (q * Q::from_integer(1_000_000)).floor() / Q::from_integer(1_000_000)
}

fn rate_text(r: Rate) -> String {
    r.to_string()
}

/// Index of the frame (or sample) at `t` seconds, counting at `rate` per second.
fn index(t: Q, per_second: Q) -> i64 {
    (t * per_second).round().to_integer()
}

impl Graph {
    pub fn new() -> Self {
        Self::default()
    }

    fn label(&mut self, prefix: &str) -> String {
        self.n += 1;
        format!("{prefix}{}", self.n)
    }

    fn input(&mut self, path: PathBuf, seek: Q, kind: Kind) -> usize {
        self.inputs.push(Input { path, seek: floor_micros(seek.max(Q::from_integer(0))), kind, threads: None });
        self.inputs.len() - 1
    }

    /// Add a filter chain (`[in]filters[out]`).
    pub fn chain(&mut self, c: String) {
        self.chains.push(c);
    }

    /// The `-filter_complex` text.
    pub fn script(&self) -> String {
        self.chains.join(";\n")
    }

    /// `-ss ... -i ...` arguments for every input.
    pub fn input_args(&self) -> Vec<String> {
        let mut a = Vec::new();
        for i in &self.inputs {
            if i.seek > Q::from_integer(0) {
                a.push("-ss".into());
                a.push(micros(i.seek));
            }
            if let Some(t) = i.threads {
                a.push("-threads".into());
                a.push(t.to_string());
            }
            a.push(if i.kind == Kind::Video { "-an".into() } else { "-vn".into() });
            a.push("-sn".into());
            a.push("-dn".into());
            a.push("-i".into());
            a.push(i.path.to_string_lossy().into_owned());
        }
        a
    }

    pub fn input_count(&self) -> usize {
        self.inputs.len()
    }

    fn fit_filters(c: &Canvas, transparent: bool) -> String {
        let (w, h) = (c.width, c.height);
        let fmt = if transparent { "yuva420p" } else { "yuv420p" };
        match c.fit {
            Fit::Fit => {
                let pad = if transparent { "black@0" } else { "black" };
                format!(
                    "scale=w={w}:h={h}:force_original_aspect_ratio=decrease:force_divisible_by=2,format={fmt},pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color={pad},setsar=1"
                )
            }
            Fit::Fill => format!("scale=w={w}:h={h}:force_original_aspect_ratio=increase,crop={w}:{h},setsar=1,format={fmt}"),
        }
    }

    fn blank_video(&mut self, c: &Canvas, frames: i64, transparent: bool) -> String {
        let l = self.label("g");
        let (color, fmt) = if transparent { ("black@0", "yuva420p") } else { ("black", "yuv420p") };
        self.chain(format!(
            "color=c={color}:s={}x{}:r={},format={fmt},trim=end_frame={frames},setsar=1[{l}]",
            c.width,
            c.height,
            rate_text(c.rate)
        ));
        l
    }

    fn concat(&mut self, parts: Vec<String>, video: bool) -> String {
        if parts.len() == 1 {
            return parts.into_iter().next().expect("one part");
        }
        let l = self.label(if video { "vc" } else { "ac" });
        let ins: String = parts.iter().map(|p| format!("[{p}]")).collect();
        let (v, a) = if video { (1, 0) } else { (0, 1) };
        self.chain(format!("{ins}concat=n={}:v={v}:a={a}[{l}]", parts.len()));
        l
    }

    /// The composited video of `plan` on `canvas`. Returns the output label;
    /// the stream has exactly as many frames as the range covers.
    pub fn video(&mut self, plan: &Plan, c: &Canvas) -> String {
        let r = c.rate.as_ratio();
        let start = plan.span.start;
        let total = index(plan.span.end - start, r);
        let mut layers = Vec::new();
        for (ti, track) in plan.video.iter().enumerate() {
            if track.segments.is_empty() && ti > 0 {
                continue;
            }
            let transparent = !layers.is_empty();
            let mut placed: Vec<(i64, i64, &Segment)> = Vec::new();
            for s in &track.segments {
                let (f0, f1) = (index(s.start - start, r), index(s.end - start, r));
                if f1 > f0 {
                    placed.push((f0, f1, s));
                }
            }
            let mut seg_labels = self.video_segments(&placed, c, transparent);
            let mut parts = Vec::new();
            let mut cursor = 0;
            for (f0, f1, _) in &placed {
                if *f0 > cursor {
                    parts.push(self.blank_video(c, f0 - cursor, transparent));
                }
                parts.push(seg_labels.remove(0));
                cursor = *f1;
            }
            if cursor < total {
                parts.push(self.blank_video(c, total - cursor, transparent));
            }
            layers.push(self.concat(parts, true));
        }
        if layers.is_empty() {
            return self.blank_video(c, total, false);
        }
        let mut base = layers.remove(0);
        for top in layers {
            let l = self.label("ov");
            self.chain(format!("[{base}][{top}]overlay=eof_action=pass:shortest=0,format=yuv420p[{l}]"));
            base = l;
        }
        base
    }

    /// One chain per placed segment, sharing inputs along monotonic runs.
    fn video_segments(&mut self, placed: &[(i64, i64, &Segment)], c: &Canvas, transparent: bool) -> Vec<String> {
        // (first media frame, last media frame exclusive) per segment, at the source rate.
        let frames: Vec<(i64, i64)> = placed
            .iter()
            .map(|(_, _, s)| {
                let mr = s.media_rate.as_ratio();
                let a = index(s.media_start, mr);
                let b = index(s.media_end(), mr).max(a + 1);
                (a, b)
            })
            .collect();
        let mut out = Vec::with_capacity(placed.len());
        let mut i = 0;
        while i < placed.len() {
            // Extend the run while the same file keeps going forward.
            let mut j = i + 1;
            while j < placed.len()
                && placed[j].2.path == placed[i].2.path
                && placed[j].2.media_rate == placed[i].2.media_rate
                && frames[j].0 >= frames[j - 1].1
            {
                j += 1;
            }
            let first = placed[i].2;
            let mr = first.media_rate.as_ratio();
            let run_start = frames[i].0;
            let input = self.input(first.path.clone(), Q::from_integer(run_start) / mr, Kind::Video);
            // A frame or two (frame renders, contact sheet cells): a frame-threaded
            // decoder only adds memory.
            if frames[j - 1].1 - run_start <= 2 {
                self.inputs[input].threads = Some(1);
            }
            let n = j - i;
            let outs: Vec<String> = (0..n).map(|_| self.label("vs")).collect();
            // Geometry is fixed once per input, before the split: per-segment
            // scale/pad chains each keep their own full-size frame pools alive,
            // which for a 300-cut timeline is gigabytes.
            let mut head = format!("[{input}:v:0]fps={},{}", rate_text(first.media_rate), Self::fit_filters(c, transparent));
            if n > 1 {
                let _ = write!(head, ",split={n}");
            }
            for o in &outs {
                let _ = write!(head, "[{o}]");
            }
            self.chain(head);
            for (k, src) in outs.into_iter().enumerate() {
                let (f0, f1, s) = placed[i + k];
                let (a, b) = frames[i + k];
                let m = f1 - f0;
                let l = self.label("v");
                let simple = s.speed == Q::from_integer(1) && s.media_rate == c.rate && b - a == m;
                let ch = if simple {
                    // Same rate, normal speed: the trimmed frames are exactly the
                    // output frames. Skipping fps/tpad here also saves the frame
                    // each of them would hold for the rest of the render.
                    format!("[{src}]trim=start_frame={}:end_frame={},setpts=PTS-STARTPTS[{l}]", a - run_start, b - run_start)
                } else {
                    // settb first: retiming in the 1/fps timebase would truncate
                    // timestamps and merge frames.
                    let mut ch = format!("[{src}]trim=start_frame={}:end_frame={},setpts=PTS-STARTPTS,settb=AVTB", a - run_start, b - run_start);
                    if s.speed != Q::from_integer(1) {
                        let _ = write!(ch, ",setpts=PTS*{}/{}", s.speed.denom(), s.speed.numer());
                    }
                    let _ = write!(ch, ",fps={},tpad=stop_mode=clone:stop={m},trim=end_frame={m}[{l}]", rate_text(c.rate));
                    ch
                };
                self.chain(ch);
                out.push(l);
            }
            i = j;
        }
        out
    }

    /// The mixed audio of `plan`, stereo at the canvas sample rate, exactly
    /// as many samples as the range covers. `xfade` is the crossfade length
    /// at cuts (0 for hard cuts).
    pub fn audio(&mut self, plan: &Plan, c: &Canvas, xfade: Q) -> String {
        let sr = Q::from_integer(i64::from(c.sample_rate));
        let start = plan.span.start;
        let total = index(plan.span.end - start, sr);
        let half = index(xfade / 2, sr).max(0);
        let mut lanes: Vec<String> = Vec::new();
        for track in &plan.audio {
            let segs: Vec<&Segment> = track.segments.iter().collect();
            let placed: Vec<(i64, i64)> = segs.iter().map(|s| (index(s.start - start, sr), index(s.end - start, sr))).collect();
            // Crossfade half-length at each cut, bounded so neighbours never overlap past each other.
            let len = |k: usize| placed[k].1 - placed[k].0;
            let mut pieces: Vec<AudioPiece> = Vec::new();
            for (k, s) in segs.iter().enumerate() {
                let (s0, s1) = placed[k];
                if s1 <= s0 {
                    continue;
                }
                let cut_h = |other: Option<usize>| other.map_or(0, |o| half.min(len(k) / 2).min(len(o) / 2));
                let h_in = if s.head == Edge::Cut { cut_h(k.checked_sub(1)) } else { 0 };
                let h_out = if s.tail == Edge::Cut { cut_h(Some(k + 1).filter(|&o| o < segs.len())) } else { 0 };
                let speed_per_sample = s.speed / sr;
                let can_extend = |media: Q| s.bounds.is_none_or(|(lo, hi)| media >= lo && media <= hi);
                let e_in = if h_in > 0 && can_extend(s.media_start - Q::from_integer(h_in) * speed_per_sample) { h_in } else { 0 };
                let e_out = if h_out > 0 && can_extend(s.media_end() + Q::from_integer(h_out) * speed_per_sample) { h_out } else { 0 };
                let edge_fade = half.min(len(k) / 2);
                let fade_in = match s.head {
                    Edge::RangeEdge => 0,
                    Edge::Open => edge_fade,
                    Edge::Cut => h_in + e_in,
                };
                let fade_out = match s.tail {
                    Edge::RangeEdge => 0,
                    Edge::Open => edge_fade,
                    Edge::Cut => h_out + e_out,
                };
                let p0 = (s0 - e_in).max(0);
                let p1 = (s1 + e_out).min(total);
                let media_at_p0 = s.media_start - Q::from_integer(s0 - p0) * speed_per_sample;
                pieces.push(AudioPiece { seg: s, p0, p1, media_at_p0, fade_in, fade_out });
            }
            // Greedy lanes: a piece goes on the first lane that is free by its start.
            let mut lane_ends: Vec<i64> = Vec::new();
            let mut lane_of = Vec::with_capacity(pieces.len());
            for p in &pieces {
                let l = match lane_ends.iter().position(|&e| e <= p.p0) {
                    Some(l) => l,
                    None => {
                        lane_ends.push(0);
                        lane_ends.len() - 1
                    }
                };
                lane_ends[l] = p.p1;
                lane_of.push(l);
            }
            let labels = self.audio_pieces(&pieces, c);
            for lane in 0..lane_ends.len() {
                let mut parts = Vec::new();
                let mut cursor = 0;
                for (k, p) in pieces.iter().enumerate().filter(|(k, _)| lane_of[*k] == lane) {
                    if p.p0 > cursor {
                        parts.push(self.silence(c, p.p0 - cursor));
                    }
                    parts.push(labels[k].clone());
                    cursor = p.p1;
                }
                lanes.push(self.concat(parts, false));
            }
        }
        let mixed = match lanes.len() {
            0 => self.silence(c, total),
            1 => lanes.remove(0),
            n => {
                let l = self.label("mix");
                let ins: String = lanes.iter().map(|p| format!("[{p}]")).collect();
                self.chain(format!("{ins}amix=inputs={n}:normalize=0:duration=longest:dropout_transition=0[{l}]"));
                l
            }
        };
        let l = self.label("aout");
        self.chain(format!("[{mixed}]apad=whole_len={total},atrim=end_sample={total},asetpts=N/SR/TB[{l}]"));
        l
    }

    fn silence(&mut self, c: &Canvas, samples: i64) -> String {
        let l = self.label("s");
        self.chain(format!(
            "anullsrc=r={sr}:cl=stereo,atrim=end_sample={samples},aformat=sample_fmts=fltp:sample_rates={sr}:channel_layouts=stereo[{l}]",
            sr = c.sample_rate
        ));
        l
    }

    fn audio_pieces(&mut self, pieces: &[AudioPiece<'_>], c: &Canvas) -> Vec<String> {
        let sr = Q::from_integer(i64::from(c.sample_rate));
        let mut out = Vec::with_capacity(pieces.len());
        let media: Vec<(Q, Q)> = pieces.iter().map(|p| (p.media_at_p0, p.media_at_p0 + Q::from_integer(p.p1 - p.p0) / sr * p.seg.speed)).collect();
        let mut i = 0;
        while i < pieces.len() {
            let mut j = i + 1;
            while j < pieces.len() && pieces[j].seg.path == pieces[i].seg.path && media[j].0 >= media[j - 1].1 {
                j += 1;
            }
            // Matroska/WebM store timestamps in whole milliseconds, so a seek
            // into their audio lands up to half a millisecond off. Decode
            // those from the start and count samples instead.
            let coarse = pieces[i].seg.path.extension().is_some_and(|e| {
                let e = e.to_string_lossy().to_lowercase();
                ["mkv", "mka", "webm"].contains(&e.as_str())
            });
            let at = if coarse { Q::from_integer(0) } else { media[i].0 };
            let input = self.input(pieces[i].seg.path.clone(), at, Kind::Audio);
            let seek = self.inputs[input].seek;
            let n = j - i;
            let outs: Vec<String> = (0..n).map(|_| self.label("as")).collect();
            let mut head = format!(
                // first_pts=0 pads a stream that starts late, so counted samples match media time.
                "[{input}:a:0]aresample={sr}:async=1:first_pts=0,aformat=sample_fmts=fltp:channel_layouts=stereo,asetpts=N/SR/TB",
                sr = c.sample_rate
            );
            if n > 1 {
                let _ = write!(head, ",asplit={n}");
            }
            for o in &outs {
                let _ = write!(head, "[{o}]");
            }
            self.chain(head);
            for (k, src) in outs.into_iter().enumerate() {
                let p = &pieces[i + k];
                let (m0, m1) = media[i + k];
                let a = index(m0 - seek, sr).max(0);
                let b = index(m1 - seek, sr).max(a + 1);
                let n = p.p1 - p.p0;
                let l = self.label("a");
                let mut ch = format!("[{src}]atrim=start_sample={a}:end_sample={b},asetpts=PTS-STARTPTS");
                if p.seg.speed != Q::from_integer(1) {
                    for t in atempo_chain(p.seg.speed) {
                        let _ = write!(ch, ",atempo={t}");
                    }
                    ch.push_str(",asetpts=N/SR/TB");
                }
                if p.seg.volume_db != 0.0 {
                    let _ = write!(ch, ",volume={}dB", p.seg.volume_db);
                }
                let _ = write!(ch, ",apad=whole_len={n},atrim=end_sample={n}");
                if p.fade_in > 0 {
                    let _ = write!(ch, ",afade=t=in:ss=0:ns={}", p.fade_in.min(n));
                }
                if p.fade_out > 0 {
                    let fo = p.fade_out.min(n);
                    let _ = write!(ch, ",afade=t=out:ss={}:ns={fo}", n - fo);
                }
                let _ = write!(ch, ",aformat=sample_fmts=fltp:sample_rates={}:channel_layouts=stereo[{l}]", c.sample_rate);
                self.chain(ch);
                out.push(l);
            }
            i = j;
        }
        out
    }
}

struct AudioPiece<'a> {
    seg: &'a Segment,
    /// Output samples (relative to the range start) the piece covers, handles included.
    p0: i64,
    p1: i64,
    media_at_p0: Q,
    fade_in: i64,
    fade_out: i64,
}

/// `atempo` takes 0.5..100 per instance; chain instances for slower speeds.
fn atempo_chain(speed: Q) -> Vec<String> {
    let mut s = *speed.numer() as f64 / *speed.denom() as f64;
    let mut out = Vec::new();
    while s < 0.5 {
        out.push("0.5".to_owned());
        s /= 0.5;
    }
    while s > 100.0 {
        out.push("100".to_owned());
        s /= 100.0;
    }
    out.push(format!("{s:.9}"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn micros_round_down() {
        // Frame 7 at 30000/1001 is 0.2335666...s; the seek must not land after it.
        assert_eq!(micros(Q::new(7 * 1001, 30_000)), "0.233566");
        assert_eq!(micros(Q::from_integer(2)), "2.000000");
    }

    #[test]
    fn atempo_chains_slow_speeds() {
        assert_eq!(atempo_chain(Q::new(1, 8)), ["0.5", "0.5", "0.500000000"]);
        assert_eq!(atempo_chain(Q::new(3, 2)), ["1.500000000"]);
    }

    #[test]
    fn scaled_canvas_keeps_aspect_and_even_sizes() {
        let c = Canvas { width: 1920, height: 1080, rate: Rate::integer(30), sample_rate: 48_000, fit: Fit::Fit };
        let s = c.scaled_to_width(641);
        assert_eq!((s.width, s.height), (640, 360));
    }
}
