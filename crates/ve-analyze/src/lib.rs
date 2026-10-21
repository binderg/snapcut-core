//! Built-in analyzers, implemented with FFmpeg filters over the rendered
//! timeline: `silencedetect` (quiet stretches) and `scdet` (scene cuts).
//! They analyze what the timeline plays (the same graph `render` uses), so
//! the times they report are timeline times and can go straight into
//! `edit.ripple_delete`. Output becomes markers. Moves to `plugins/core`
//! once the process plugin host exists (dogfooding rule: built-ins use the
//! same API as third-party plugins).

use std::path::Path;

use ve_model::Timeline;
use ve_model::time::Q;
use ve_render::{RenderError, Span};

/// A quiet stretch, in timeline seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Silence {
    pub start: Q,
    pub end: Q,
}

/// A hard cut in the picture, in timeline seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneCut {
    pub at: Q,
    /// scdet's score, 0-100.
    pub score: f64,
}

/// A decimal an ffmpeg filter printed (`12.345`, `-0.0001`, `1e-05`) as exact seconds.
/// Plain decimals are read exactly; exponent forms (tiny values) go through
/// microseconds.
pub fn decimal(s: &str) -> Option<Q> {
    let s = s.trim().trim_end_matches(',');
    if s.contains(['e', 'E']) {
        let f: f64 = s.parse().ok()?;
        return f.is_finite().then(|| Q::new((f * 1e6).round() as i64, 1_000_000));
    }
    let (neg, body) = s.strip_prefix('-').map_or((false, s), |b| (true, b));
    let t = ve_model::timecode::parse(&format!("{body}s"), ve_model::Rate::integer(1)).ok()?.value;
    Some(if neg { -t } else { t })
}

/// `silence_start` / `silence_end` lines, offset by the span start. A
/// silence still open at the end of the log runs to the span end.
pub fn parse_silence(log: &str, span: Span) -> Vec<Silence> {
    let mut out = Vec::new();
    let mut open: Option<Q> = None;
    for line in log.lines() {
        if let Some(v) = value_after(line, "silence_start:") {
            open = Some(v);
        } else if let Some(v) = value_after(line, "silence_end:") {
            let start = open.take().unwrap_or(Q::from_integer(0));
            out.push(Silence { start: span.start + start.max(Q::from_integer(0)), end: (span.start + v).min(span.end) });
        }
    }
    if let Some(start) = open {
        out.push(Silence { start: span.start + start.max(Q::from_integer(0)), end: span.end });
    }
    out
}

/// `lavfi.scd.score: S, lavfi.scd.time: T` lines from scdet.
pub fn parse_scenes(log: &str, span: Span) -> Vec<SceneCut> {
    log.lines()
        .filter_map(|line| {
            let score: f64 = line.split("lavfi.scd.score:").nth(1)?.split(',').next()?.trim().parse().ok()?;
            let t = value_after(line, "lavfi.scd.time:")?;
            Some(SceneCut { at: span.start + t, score })
        })
        .collect()
}

fn value_after(line: &str, key: &str) -> Option<Q> {
    let rest = line.split(key).nth(1)?;
    decimal(rest.split_whitespace().next()?)
}

/// Exact seconds as a decimal string for a filter argument.
fn secs(q: Q) -> String {
    let us = (q * Q::from_integer(1_000_000)).round().to_integer();
    format!("{}.{:06}", us / 1_000_000, us % 1_000_000)
}

/// Quiet stretches of the timeline's mixed audio over `span`: below
/// `threshold_db` for at least `min_duration`.
pub fn silence(tl: &Timeline, span: Span, threshold_db: f64, min_duration: Q, work: &Path) -> Result<(Vec<Silence>, Vec<String>), RenderError> {
    let filter = format!("silencedetect=noise={threshold_db}dB:duration={}", secs(min_duration));
    let (log, warnings) = ve_render::audio_log(tl, span, &filter, work)?;
    Ok((parse_silence(&log, span), warnings))
}

/// Hard cuts in the timeline's picture over `span`. `threshold` is scdet's
/// (0-100, default 10; lower finds more).
pub fn scenes(tl: &Timeline, span: Span, threshold: f64, work: &Path) -> Result<(Vec<SceneCut>, Vec<String>), RenderError> {
    let filter = format!("scdet=threshold={threshold}");
    // 320 px is plenty to see a cut and keeps a 10-minute scan quick.
    let (log, warnings) = ve_render::video_log(tl, span, 320, &filter, work)?;
    Ok((parse_scenes(&log, span), warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_are_exact() {
        assert_eq!(decimal("1.25"), Some(Q::new(5, 4)));
        assert_eq!(decimal("-0.5"), Some(Q::new(-1, 2)));
        assert_eq!(decimal("1e-05"), Some(Q::new(10, 1_000_000)));
        assert_eq!(decimal("2"), Some(Q::from_integer(2)));
        assert_eq!(decimal("x"), None);
    }

    #[test]
    fn parses_silencedetect_log() {
        let log = "\
[silencedetect @ 0x55a2] silence_start: 0.999938
[silencedetect @ 0x55a2] silence_end: 2.00008 | silence_duration: 1.00015
size=N/A time=00:00:03.00
[silencedetect @ 0x55a2] silence_start: 2.5";
        let span = Span::new(Q::from_integer(10), Q::from_integer(13));
        let s = parse_silence(log, span);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].start, Q::from_integer(10) + Q::new(999_938, 1_000_000));
        assert_eq!(s[0].end, Q::from_integer(10) + Q::new(200_008, 100_000));
        assert_eq!(s[1], Silence { start: Q::new(25, 2), end: Q::from_integer(13) });
    }

    #[test]
    fn parses_scdet_log() {
        let log = "[scdet @ 0x5604] lavfi.scd.score: 17.615, lavfi.scd.time: 2\n[scdet @ 0x5604] lavfi.scd.score: 40.1, lavfi.scd.time: 3.5";
        let c = parse_scenes(log, Span::new(Q::from_integer(1), Q::from_integer(9)));
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].at, Q::from_integer(3));
        assert!((c[1].score - 40.1).abs() < 1e-9);
    }
}
