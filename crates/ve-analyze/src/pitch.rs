//! Fundamental frequency (pitch) with the YIN algorithm (de Cheveigné and
//! Kawahara, 2002), over mono audio decoded at [`SAMPLE_RATE`].
//!
//! Tuned for voices and most instruments: 60 to 800 Hz, a frame every 20 ms.
//! Plain signal processing, no model, so it belongs in core.

use ve_model::time::Q;

/// Decode rate for pitch. 8 kHz keeps 800 Hz well below Nyquist and makes
/// a 10-minute file a second or two of work.
pub const SAMPLE_RATE: u32 = 8_000;
const MIN_HZ: f64 = 60.0;
const MAX_HZ: f64 = 800.0;
/// Integration window: 30 ms.
const WINDOW: usize = 240;
/// Hop: 20 ms.
const HOP: usize = 160;
/// YIN's aperiodicity threshold; lower is stricter about calling a frame voiced.
const THRESHOLD: f64 = 0.15;
/// Frames quieter than this (dBFS RMS) are unvoiced without looking.
const GATE_DB: f64 = -50.0;

const TAU_MIN: usize = (SAMPLE_RATE as f64 / MAX_HZ) as usize;
const TAU_MAX: usize = (SAMPLE_RATE as f64 / MIN_HZ) as usize + 1;

/// Pitch per frame: `(seconds from the start, Hz)`, `None` where unvoiced.
#[derive(Default)]
pub struct PitchTracker {
    buf: Vec<f32>,
    /// Samples dropped from the front of `buf` so far.
    offset: u64,
    pub frames: Vec<(Q, Option<f64>)>,
}

impl PitchTracker {
    pub fn push(&mut self, mono: &[f32]) {
        self.buf.extend_from_slice(mono);
        let need = WINDOW + TAU_MAX;
        let mut start = 0;
        while start + need <= self.buf.len() {
            let f0 = yin(&self.buf[start..start + need]);
            // Stamp the frame at its window's centre.
            let centre = self.offset + start as u64 + (WINDOW / 2) as u64;
            self.frames.push((Q::new(centre as i64, i64::from(SAMPLE_RATE)), f0));
            start += HOP;
        }
        self.buf.drain(..start);
        self.offset += start as u64;
    }
}

/// YIN on one frame of `WINDOW + TAU_MAX` samples.
fn yin(x: &[f32]) -> Option<f64> {
    let energy: f64 = x[..WINDOW].iter().map(|&v| f64::from(v) * f64::from(v)).sum::<f64>() / WINDOW as f64;
    if energy <= 0.0 || 10.0 * energy.log10() < GATE_DB {
        return None;
    }
    // Difference function, then its cumulative mean normalized form.
    let mut d = [0.0f64; TAU_MAX + 1];
    for (tau, slot) in d.iter_mut().enumerate().skip(1) {
        let mut s = 0.0;
        for j in 0..WINDOW {
            let diff = f64::from(x[j]) - f64::from(x[j + tau]);
            s += diff * diff;
        }
        *slot = s;
    }
    let mut cmnd = [1.0f64; TAU_MAX + 1];
    let mut running = 0.0;
    for tau in 1..=TAU_MAX {
        running += d[tau];
        cmnd[tau] = if running > 0.0 { d[tau] * tau as f64 / running } else { 1.0 };
    }
    // First dip under the threshold, followed down to its local minimum.
    let mut tau = (TAU_MIN..TAU_MAX).find(|&t| cmnd[t] < THRESHOLD)?;
    while tau + 1 < TAU_MAX && cmnd[tau + 1] < cmnd[tau] {
        tau += 1;
    }
    // Parabolic interpolation for a sub-sample period.
    let (a, b, c) = (cmnd[tau - 1], cmnd[tau], cmnd[tau + 1]);
    let denom = a - 2.0 * b + c;
    let shift = if denom.abs() > 1e-12 { ((a - c) / (2.0 * denom)).clamp(-1.0, 1.0) } else { 0.0 };
    let hz = f64::from(SAMPLE_RATE) / (tau as f64 + shift);
    (MIN_HZ..=MAX_HZ).contains(&hz).then_some(hz)
}

#[derive(Debug, Clone, PartialEq)]
pub struct PitchSummary {
    pub median_hz: Option<f64>,
    /// 10th and 90th percentile: the usual range, ignoring octave slips.
    pub low_hz: Option<f64>,
    pub high_hz: Option<f64>,
    /// Share of frames with a pitch, 0 to 1.
    pub voiced: f64,
    /// Nearest note to the median, e.g. `A3`.
    pub note: Option<String>,
}

pub fn summarize(frames: &[(Q, Option<f64>)]) -> PitchSummary {
    let mut hz: Vec<f64> = frames.iter().filter_map(|f| f.1).collect();
    hz.sort_by(f64::total_cmp);
    let pick = |p: usize| (!hz.is_empty()).then(|| (hz[(hz.len() - 1) * p / 100] * 10.0).round() / 10.0);
    let median = pick(50);
    PitchSummary {
        median_hz: median,
        low_hz: pick(10),
        high_hz: pick(90),
        voiced: if frames.is_empty() { 0.0 } else { (hz.len() as f64 / frames.len() as f64 * 100.0).round() / 100.0 },
        note: median.map(note_name),
    }
}

/// Median pitch per bucket of `step` seconds (`None` where nothing was voiced).
pub fn bucket_hz(frames: &[(Q, Option<f64>)], step: Q, count: usize) -> Vec<Option<f64>> {
    let mut buckets: Vec<Vec<f64>> = vec![Vec::new(); count];
    for &(t, f) in frames {
        let i = (t / step).floor().to_integer().max(0) as usize;
        if let (Some(f), Some(b)) = (f, buckets.get_mut(i)) {
            b.push(f);
        }
    }
    buckets
        .into_iter()
        .map(|mut b| {
            b.sort_by(f64::total_cmp);
            (!b.is_empty()).then(|| (b[b.len() / 2] * 10.0).round() / 10.0)
        })
        .collect()
}

/// `440.0` -> `A4`, `261.6` -> `C4`.
pub fn note_name(hz: f64) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let midi = (69.0 + 12.0 * (hz / 440.0).log2()).round() as i64;
    format!("{}{}", NAMES[midi.rem_euclid(12) as usize], midi.div_euclid(12) - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f64, secs: f64) -> Vec<f32> {
        let n = (f64::from(SAMPLE_RATE) * secs) as usize;
        (0..n).map(|i| (0.3 * (std::f64::consts::TAU * hz * i as f64 / f64::from(SAMPLE_RATE)).sin()) as f32).collect()
    }

    #[test]
    fn finds_pure_tones() {
        for hz in [82.4, 110.0, 196.0, 220.0, 440.0, 659.3] {
            let mut t = PitchTracker::default();
            for chunk in tone(hz, 1.0).chunks(777) {
                t.push(chunk);
            }
            let s = summarize(&t.frames);
            let got = s.median_hz.unwrap();
            assert!((got - hz).abs() / hz < 0.01, "{hz} Hz read as {got}");
            assert!(s.voiced > 0.95, "{hz}: voiced {}", s.voiced);
        }
    }

    #[test]
    fn silence_and_noise_are_unvoiced() {
        let mut t = PitchTracker::default();
        t.push(&vec![0.0; 8000]);
        // Deterministic white-ish noise.
        let mut x: u32 = 12345;
        let noise: Vec<f32> = (0..8000)
            .map(|_| {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
                ((x >> 9) as f32 / (1u32 << 23) as f32 - 0.5) * 0.5
            })
            .collect();
        t.push(&noise);
        let s = summarize(&t.frames);
        assert!(s.voiced < 0.1, "voiced {}", s.voiced);
    }

    #[test]
    fn contour_follows_a_change() {
        let mut t = PitchTracker::default();
        let mut s = tone(150.0, 1.0);
        s.extend(tone(300.0, 1.0));
        t.push(&s);
        let b = bucket_hz(&t.frames, Q::from_integer(1), 2);
        assert!((b[0].unwrap() - 150.0).abs() < 2.0 && (b[1].unwrap() - 300.0).abs() < 3.0, "{b:?}");
    }

    #[test]
    fn note_names() {
        assert_eq!(note_name(440.0), "A4");
        assert_eq!(note_name(261.63), "C4");
        assert_eq!(note_name(110.0), "A2");
    }
}
