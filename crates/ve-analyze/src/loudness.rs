//! EBU R128 loudness from ffmpeg's `ebur128` filter log.
//!
//! The filter does the hard parts (K-weighting, gating, 4x oversampled true
//! peak); we read its per-100 ms lines and its summary.

use ve_model::time::Q;

use crate::decimal;

/// Below this the filter reports its floor (-70 LUFS gate, -120.7 for
/// digital silence); such readings mean "nothing to measure", not a level.
const FLOOR_LUFS: f64 = -69.9;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Loudness {
    /// Integrated loudness (whole range), LUFS. `None` if everything was below the gate.
    pub integrated: Option<f64>,
    /// Loudness range, LU.
    pub range: Option<f64>,
    /// Highest true peak over all channels, dBTP.
    pub true_peak: Option<f64>,
    /// Momentary loudness (400 ms window) every 100 ms: (seconds from the start, LUFS).
    pub momentary: Vec<(Q, f64)>,
}

/// Read `ebur128=peak=true:framelog=info` output.
pub fn parse_ebur128(log: &str) -> Loudness {
    let mut out = Loudness::default();
    let (frames, summary) = log.split_once("Summary:").unwrap_or((log, ""));
    for line in frames.lines() {
        let Some(t) = field(line, " t:").and_then(decimal) else { continue };
        if let Some(m) = field(line, " M:").and_then(number) {
            out.momentary.push((t, m));
        }
    }
    out.integrated = field(summary, " I:").and_then(number).filter(|&v| v > FLOOR_LUFS);
    out.range = field(summary, " LRA:").and_then(number);
    out.true_peak = field(summary, " Peak:").and_then(number).filter(|v| v.is_finite());
    out
}

/// The token after `key` (`" M:"` finds `M:-23.4` and `M: -23.4`).
fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let rest = &text[text.find(key)? + key.len()..];
    rest.split_whitespace().next()
}

fn number(s: &str) -> Option<f64> {
    match s {
        "-inf" => Some(f64::NEG_INFINITY),
        "inf" => Some(f64::INFINITY),
        _ => s.parse().ok(),
    }
}

/// Loudness per bucket of `step` seconds, averaging momentary readings in
/// the energy domain (the way loudness adds up). Buckets with only floor
/// readings are `None`.
pub fn bucket_lufs(momentary: &[(Q, f64)], step: Q, count: usize) -> Vec<Option<f64>> {
    let mut sum = vec![0.0f64; count];
    let mut n = vec![0usize; count];
    for &(t, lufs) in momentary {
        // The filter stamps each reading at the end of its 100 ms block.
        let i = ((t - Q::new(1, 20)) / step).floor().to_integer().max(0) as usize;
        // Floor readings are gated out, as R128's absolute gate does.
        if i < count && lufs > FLOOR_LUFS {
            sum[i] += 10f64.powf(lufs / 10.0);
            n[i] += 1;
        }
    }
    sum.iter().zip(&n).map(|(&s, &k)| (k > 0 && s > 0.0).then(|| round1(10.0 * (s / k as f64).log10()))).collect()
}

pub fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "\
[Parsed_ebur128_0 @ 0000] t: 0.1        TARGET:-23 LUFS    M:-120.7 S:-120.7     I: -70.0 LUFS       LRA:   0.0 LU  FTPK: -inf dBFS  TPK: -inf dBFS
[Parsed_ebur128_0 @ 0000] t: 0.4        TARGET:-23 LUFS    M: -20.0 S:-120.7     I: -20.0 LUFS       LRA:   0.0 LU  FTPK: -3.1 dBFS  TPK: -3.1 dBFS
[Parsed_ebur128_0 @ 0000] t: 1.1        TARGET:-23 LUFS    M: -30.0 S: -25.0     I: -21.0 LUFS       LRA:   2.0 LU  FTPK: -9.0 dBFS  TPK: -3.1 dBFS
[Parsed_ebur128_0 @ 0000] Summary:

  Integrated loudness:
    I:         -21.0 LUFS
    Threshold: -31.0 LUFS

  Loudness range:
    LRA:         2.0 LU
    Threshold: -41.0 LUFS
    LRA low:   -22.0 LUFS
    LRA high:  -20.0 LUFS

  True peak:
    Peak:       -3.1 dBFS";

    #[test]
    fn reads_summary_and_frames() {
        let l = parse_ebur128(LOG);
        assert_eq!(l.integrated, Some(-21.0));
        assert_eq!(l.range, Some(2.0));
        assert_eq!(l.true_peak, Some(-3.1));
        assert_eq!(l.momentary.len(), 3);
        assert_eq!(l.momentary[1], (Q::new(2, 5), -20.0));
    }

    #[test]
    fn silence_has_no_integrated_loudness() {
        let l = parse_ebur128("Summary:\n I: -70.0 LUFS\n LRA: 0.0 LU\n Peak: -inf dBFS");
        assert_eq!((l.integrated, l.true_peak), (None, None));
    }

    #[test]
    fn buckets_average_energy() {
        let l = parse_ebur128(LOG);
        let b = bucket_lufs(&l.momentary, Q::from_integer(1), 2);
        // Bucket 0: one floor reading (ignored) and -20; bucket 1: -30.
        assert_eq!(b, vec![Some(-20.0), Some(-30.0)]);
    }
}
