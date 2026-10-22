//! Sample-level statistics, computed in one streaming pass over decoded
//! float samples: per-channel RMS, peak, DC offset and clipping, the noise
//! floor, stereo balance and phase correlation, plus per-bucket series and
//! waveform peaks.

use ve_model::time::Q;

use crate::loudness::round1;

/// A sample at or above this magnitude counts as clipped (full scale, with
/// room for int16's 32767/32768).
const CLIP: f32 = 0.9999;
/// Window for the noise-floor estimate.
const FLOOR_WINDOW_SECS: f64 = 0.05;
/// Below this a window is digital silence (a gap), not room tone.
const DIGITAL_SILENCE_DB: f64 = -100.0;
/// Samples per waveform block; buckets are built from blocks once the total is known.
const BLOCK: usize = 64;

pub fn db(amplitude: f64) -> f64 {
    if amplitude <= 0.0 { f64::NEG_INFINITY } else { 20.0 * amplitude.log10() }
}

fn db_power(mean_square: f64) -> f64 {
    if mean_square <= 0.0 { f64::NEG_INFINITY } else { 10.0 * mean_square.log10() }
}

/// `None` for -inf, else rounded to 0.1 dB.
fn finite(v: f64) -> Option<f64> {
    v.is_finite().then(|| round1(v))
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChannelLevels {
    pub rms_db: Option<f64>,
    pub peak_db: Option<f64>,
    /// Mean sample value; well above 0.001 points at a faulty recorder or cable.
    pub dc_offset: f64,
    pub clipped_samples: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Levels {
    pub channels: Vec<ChannelLevels>,
    /// Level of the quietest stretches that are not digital silence (room
    /// tone, hiss): the 5th percentile of 50 ms windows. `None` if all silent.
    pub noise_floor_db: Option<f64>,
    /// Left minus right RMS in dB (stereo only). Large values mean one side
    /// is weak or dead, e.g. a lav mic on one channel.
    pub balance_db: Option<f64>,
    /// Left/right correlation, -1 to 1 (stereo only). Near 1: mono-like;
    /// near 0: wide or unrelated; negative: one side is polarity-flipped.
    pub correlation: Option<f64>,
    /// Per bucket: RMS and peak of all channels, dBFS.
    pub rms_series: Vec<Option<f64>>,
    pub peak_series: Vec<Option<f64>>,
    /// Frames (one sample per channel) seen.
    pub frames: u64,
}

/// Streaming accumulator for [`Levels`]. Feed interleaved samples with
/// [`LevelMeter::push`], then call [`LevelMeter::finish`].
pub struct LevelMeter {
    channels: usize,
    sum_sq: Vec<f64>,
    sum: Vec<f64>,
    peak: Vec<f32>,
    clipped: Vec<u64>,
    cross: f64,
    frames: u64,
    floor_len: u64,
    floor_acc: f64,
    floor_n: u64,
    windows: Vec<f64>,
    /// Series bucket boundaries: bucket = frame * step_den / (step_num * rate).
    series: Option<(i128, i128)>,
    bucket_sq: Vec<f64>,
    bucket_n: Vec<u64>,
    bucket_peak: Vec<f32>,
}

impl LevelMeter {
    /// `step`: series bucket length in seconds, or `None` for no series.
    pub fn new(channels: u32, sample_rate: u32, step: Option<Q>) -> Self {
        let channels = channels.max(1) as usize;
        let floor_len = ((f64::from(sample_rate) * FLOOR_WINDOW_SECS) as u64).max(1);
        Self {
            channels,
            sum_sq: vec![0.0; channels],
            sum: vec![0.0; channels],
            peak: vec![0.0; channels],
            clipped: vec![0; channels],
            cross: 0.0,
            frames: 0,
            floor_len,
            floor_acc: 0.0,
            floor_n: 0,
            windows: Vec::new(),
            series: step.map(|s| (i128::from(*s.numer()) * i128::from(sample_rate), i128::from(*s.denom()))),
            bucket_sq: Vec::new(),
            bucket_n: Vec::new(),
            bucket_peak: Vec::new(),
        }
    }

    pub fn push(&mut self, interleaved: &[f32]) {
        let ch = self.channels;
        for frame in interleaved.chunks_exact(ch) {
            let mut frame_sq = 0.0f64;
            let mut frame_peak = 0.0f32;
            for (c, &x) in frame.iter().enumerate() {
                let (xf, ax) = (f64::from(x), x.abs());
                self.sum_sq[c] += xf * xf;
                self.sum[c] += xf;
                self.peak[c] = self.peak[c].max(ax);
                if ax >= CLIP {
                    self.clipped[c] += 1;
                }
                frame_sq += xf * xf;
                frame_peak = frame_peak.max(ax);
            }
            if ch == 2 {
                self.cross += f64::from(frame[0]) * f64::from(frame[1]);
            }
            let mean_sq = frame_sq / ch as f64;
            self.floor_acc += mean_sq;
            self.floor_n += 1;
            if self.floor_n == self.floor_len {
                self.windows.push(self.floor_acc / self.floor_n as f64);
                self.floor_acc = 0.0;
                self.floor_n = 0;
            }
            if let Some((num, den)) = self.series {
                let i = (i128::from(self.frames) * den / num) as usize;
                if i >= self.bucket_sq.len() {
                    self.bucket_sq.resize(i + 1, 0.0);
                    self.bucket_n.resize(i + 1, 0);
                    self.bucket_peak.resize(i + 1, 0.0);
                }
                self.bucket_sq[i] += mean_sq;
                self.bucket_n[i] += 1;
                self.bucket_peak[i] = self.bucket_peak[i].max(frame_peak);
            }
            self.frames += 1;
        }
    }

    pub fn finish(mut self) -> Levels {
        let n = self.frames.max(1) as f64;
        let channels = (0..self.channels)
            .map(|c| ChannelLevels {
                rms_db: finite(db_power(self.sum_sq[c] / n)),
                peak_db: finite(db(f64::from(self.peak[c]))),
                dc_offset: (self.sum[c] / n * 1e6).round() / 1e6,
                clipped_samples: self.clipped[c],
            })
            .collect();
        let mut audible: Vec<f64> = self.windows.iter().map(|&w| db_power(w)).filter(|&d| d > DIGITAL_SILENCE_DB).collect();
        audible.sort_by(f64::total_cmp);
        let noise_floor_db = (!audible.is_empty()).then(|| round1(audible[audible.len() / 20]));
        let (balance_db, correlation) = if self.channels == 2 && self.frames > 0 {
            let (l, r) = (self.sum_sq[0], self.sum_sq[1]);
            let balance = finite(db_power(l / n) - db_power(r / n)).or(match (l > 0.0, r > 0.0) {
                (true, false) => Some(f64::INFINITY),
                (false, true) => Some(f64::NEG_INFINITY),
                _ => Some(0.0),
            });
            let corr = (l > 0.0 && r > 0.0).then(|| ((self.cross / (l * r).sqrt()) * 100.0).round() / 100.0);
            // Infinite balance (one side silent) is reported as ±120 so it stays valid JSON.
            (balance.map(|b| b.clamp(-120.0, 120.0)), corr)
        } else {
            (None, None)
        };
        let rms_series = self.bucket_sq.iter().zip(&self.bucket_n).map(|(&s, &k)| finite(db_power(s / k.max(1) as f64))).collect();
        let peak_series = self.bucket_peak.drain(..).map(|p| finite(db(f64::from(p)))).collect();
        Levels { channels, noise_floor_db, balance_db, correlation, rms_series, peak_series, frames: self.frames }
    }
}

/// Waveform peaks for drawing: per bucket, the lowest and highest sample on
/// any channel and the RMS over all channels (no downmix, so levels read as
/// recorded). Built from 64-frame blocks, so bucket edges are exact to 64
/// frames whatever the bucket count.
#[derive(Debug, Clone, PartialEq)]
pub struct Waveform {
    pub min: Vec<f32>,
    pub max: Vec<f32>,
    pub rms: Vec<f32>,
    /// Frames (one sample per channel) seen.
    pub samples: u64,
}

pub struct WaveformBuilder {
    channels: usize,
    blocks: Vec<(f32, f32, f64, u32)>,
    cur: (f32, f32, f64, u32),
    samples: u64,
}

impl WaveformBuilder {
    pub fn new(channels: u32) -> Self {
        Self { channels: channels.max(1) as usize, blocks: Vec::new(), cur: (0.0, 0.0, 0.0, 0), samples: 0 }
    }

    /// Interleaved samples, whole frames.
    pub fn push(&mut self, interleaved: &[f32]) {
        for frame in interleaved.chunks_exact(self.channels) {
            let lo = frame.iter().copied().fold(f32::INFINITY, f32::min);
            let hi = frame.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let sq = frame.iter().map(|&x| f64::from(x) * f64::from(x)).sum::<f64>() / frame.len() as f64;
            let c = &mut self.cur;
            if c.3 == 0 {
                *c = (lo, hi, 0.0, 0);
            }
            c.0 = c.0.min(lo);
            c.1 = c.1.max(hi);
            c.2 += sq;
            c.3 += 1;
            if c.3 as usize == BLOCK {
                self.blocks.push(*c);
                c.3 = 0;
            }
            self.samples += 1;
        }
    }

    pub fn finish(mut self, buckets: usize) -> Waveform {
        if self.cur.3 > 0 {
            self.blocks.push(self.cur);
        }
        let buckets = buckets.max(1);
        let total = self.blocks.len();
        let r4 = |v: f32| (v * 10_000.0).round() / 10_000.0;
        let (mut min, mut max, mut rms) = (Vec::with_capacity(buckets), Vec::with_capacity(buckets), Vec::with_capacity(buckets));
        for b in 0..buckets {
            let (a, z) = (b * total / buckets, ((b + 1) * total / buckets).max(b * total / buckets + 1).min(total));
            let blocks = self.blocks.get(a..z).unwrap_or(&[]);
            let lo = blocks.iter().map(|k| k.0).fold(0.0f32, f32::min);
            let hi = blocks.iter().map(|k| k.1).fold(0.0f32, f32::max);
            let (sq, n) = blocks.iter().fold((0.0f64, 0u64), |(s, n), k| (s + k.2, n + u64::from(k.3)));
            min.push(r4(lo));
            max.push(r4(hi));
            rms.push(r4(if n > 0 { (sq / n as f64).sqrt() as f32 } else { 0.0 }));
        }
        Waveform { min, max, rms, samples: self.samples }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, amp: f32, sr: u32, secs: f32) -> Vec<f32> {
        (0..(sr as f32 * secs) as usize).map(|i| amp * (std::f32::consts::TAU * freq * i as f32 / sr as f32).sin()).collect()
    }

    #[test]
    fn sine_levels() {
        // Stereo: a -6 dBFS sine left, the same flipped right.
        let s = sine(440.0, 0.5, 48_000, 1.0);
        let inter: Vec<f32> = s.iter().flat_map(|&x| [x, -x]).collect();
        let mut m = LevelMeter::new(2, 48_000, Some(Q::new(1, 2)));
        for chunk in inter.chunks(1000) {
            m.push(chunk);
        }
        let l = m.finish();
        assert_eq!(l.frames, 48_000);
        let c = &l.channels[0];
        assert_eq!(c.peak_db, Some(-6.0));
        // A sine's RMS is 3 dB under its peak.
        assert_eq!(c.rms_db, Some(-9.0));
        assert_eq!(c.clipped_samples, 0);
        assert_eq!(l.balance_db, Some(0.0));
        assert_eq!(l.correlation, Some(-1.0));
        assert_eq!(l.rms_series, vec![Some(-9.0), Some(-9.0)]);
    }

    #[test]
    fn dead_channel_and_clipping() {
        let inter: Vec<f32> = (0..4800).flat_map(|i| [if i % 2 == 0 { 1.0 } else { -1.0 }, 0.0]).collect();
        let l = {
            let mut m = LevelMeter::new(2, 48_000, None);
            m.push(&inter);
            m.finish()
        };
        assert_eq!(l.channels[0].clipped_samples, 4800);
        assert_eq!(l.channels[1].rms_db, None);
        assert_eq!(l.balance_db, Some(120.0));
        assert_eq!(l.correlation, None);
    }

    #[test]
    fn noise_floor_ignores_digital_silence() {
        // 1 s of -60 dBFS noise-ish tone, 1 s of exact zeros, 1 s of -10 dBFS tone.
        let mut s = sine(1000.0, 0.001, 48_000, 1.0);
        s.extend(vec![0.0; 48_000]);
        s.extend(sine(1000.0, 0.316, 48_000, 1.0));
        let mut m = LevelMeter::new(1, 48_000, None);
        m.push(&s);
        let floor = m.finish().noise_floor_db.unwrap();
        assert!((floor - -63.0).abs() < 0.5, "{floor}");
    }

    #[test]
    fn waveform_buckets() {
        let mut s = vec![0.5f32; 640];
        s.extend(vec![-0.25f32; 640]);
        let mut w = WaveformBuilder::new(1);
        w.push(&s);
        let w = w.finish(2);
        assert_eq!(w.samples, 1280);
        assert_eq!((w.min.clone(), w.max.clone()), (vec![0.0, -0.25], vec![0.5, 0.0]));
        assert_eq!(w.rms, vec![0.5, 0.25]);
    }
}
