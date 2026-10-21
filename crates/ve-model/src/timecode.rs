//! Time strings that people and agents type, normalized to [`RationalTime`].
//!
//! Accepted forms (ARCHITECTURE.md, "Agent interface"), each with an optional
//! leading `-` for offsets:
//!
//! - SMPTE `HH:MM:SS:FF`. A `;` anywhere (`00:01:00;02`) means drop-frame,
//!   which exists only for 30000/1001 and 60000/1001.
//! - Frames `f2172`, at the given rate.
//! - Seconds `72.4s`, read as an exact decimal. The result can fall between
//!   frames; callers that need a frame boundary snap it themselves.
//!
//! Non-drop timecode at an NTSC rate counts frames at the nominal rate
//! (`00:00:01:00` at 29.97 is frame 30), as every editor does.

use thiserror::Error;

use crate::time::{Q, Rate, RationalTime};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TimecodeError {
    #[error("`{0}` is not a time; use HH:MM:SS:FF, HH:MM:SS;FF (drop-frame), f<frames> or <seconds>s")]
    Syntax(String),
    #[error("`{input}`: {field} is {value}, must be below {limit}")]
    FieldOutOfRange { input: String, field: &'static str, value: i64, limit: i64 },
    #[error("drop-frame timecode needs a 29.97 or 59.94 rate, not {0}")]
    DropFrameRate(Rate),
    #[error("`{0}` does not exist in drop-frame timecode (frames 0-1 or 0-3 are skipped at the start of most minutes)")]
    DroppedLabel(String),
    #[error("SMPTE timecode needs an integer or NTSC (n/1001) rate, not {0}")]
    UnsupportedRate(Rate),
    #[error("`{0}` is too large")]
    Overflow(String),
}

/// Parse any accepted time form at `rate`.
pub fn parse(input: &str, rate: Rate) -> Result<RationalTime, TimecodeError> {
    let s = input.trim();
    let (negative, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    let t = if let Some(frames) = body.strip_prefix('f') {
        RationalTime::from_frames(parse_int(frames, input)?, rate)
    } else if let Some(secs) = body.strip_suffix('s') {
        let seconds = parse_decimal(secs, input)?;
        let r = rate.as_ratio();
        let numer = seconds.numer().checked_mul(*r.numer()).ok_or_else(|| overflow(input))?;
        let denom = seconds.denom().checked_mul(*r.denom()).ok_or_else(|| overflow(input))?;
        RationalTime { value: Q::new(numer, denom), rate }
    } else if body.contains([':', ';']) {
        RationalTime::from_frames(parse_smpte(body, rate, input)?, rate)
    } else {
        return Err(TimecodeError::Syntax(input.to_owned()));
    };
    Ok(if negative { RationalTime { value: -t.value, rate: t.rate } } else { t })
}

/// SMPTE label for `t`, at `t`'s own rate. Sub-frame times round down to the
/// frame they fall in.
pub fn to_smpte(t: RationalTime, drop_frame: bool) -> Result<String, TimecodeError> {
    let nominal = nominal_fps(t.rate)?;
    let frames = t.floor_frames();
    let sign = if frames < 0 { "-" } else { "" };
    let mut n = frames.checked_abs().ok_or_else(|| TimecodeError::Overflow(format!("{frames} frames")))?;
    let sep = if drop_frame {
        let d = dropped_per_minute(t.rate)?;
        // Add back the labels drop-frame skips so `n` counts nominal frames.
        let per_10_min = 600 * nominal - 9 * d;
        let per_min = 60 * nominal - d;
        let (tens, rem) = (n / per_10_min, n % per_10_min);
        n += 9 * d * tens + if rem > d { d * ((rem - d) / per_min) } else { 0 };
        ';'
    } else {
        ':'
    };
    let ff = n % nominal;
    let total_secs = n / nominal;
    let (hh, mm, ss) = (total_secs / 3600, total_secs / 60 % 60, total_secs % 60);
    Ok(format!("{sign}{hh:02}:{mm:02}:{ss:02}{sep}{ff:02}"))
}

fn parse_smpte(body: &str, rate: Rate, input: &str) -> Result<i64, TimecodeError> {
    let drop_frame = body.contains(';');
    let parts: Vec<&str> = body.split([':', ';']).collect();
    let [hh, mm, ss, ff] = parts.as_slice() else {
        return Err(TimecodeError::Syntax(input.to_owned()));
    };
    let nominal = nominal_fps(rate)?;
    let (hh, mm, ss, ff) = (parse_int(hh, input)?, parse_int(mm, input)?, parse_int(ss, input)?, parse_int(ff, input)?);
    for (field, value, limit) in [("minutes", mm, 60), ("seconds", ss, 60), ("frames", ff, nominal)] {
        if value >= limit {
            return Err(TimecodeError::FieldOutOfRange { input: input.to_owned(), field, value, limit });
        }
    }
    let nominal_frames = hh
        .checked_mul(3600)
        .and_then(|s| s.checked_add(mm * 60 + ss))
        .and_then(|s| s.checked_mul(nominal))
        .and_then(|f| f.checked_add(ff))
        .ok_or_else(|| overflow(input))?;
    if !drop_frame {
        return Ok(nominal_frames);
    }
    let d = dropped_per_minute(rate)?;
    if mm % 10 != 0 && ss == 0 && ff < d {
        return Err(TimecodeError::DroppedLabel(input.to_owned()));
    }
    let total_minutes = hh * 60 + mm;
    Ok(nominal_frames - d * (total_minutes - total_minutes / 10))
}

/// Frames per second that timecode labels count in: 30 for both 30 and 29.97.
fn nominal_fps(rate: Rate) -> Result<i64, TimecodeError> {
    let q = rate.as_ratio();
    match (*q.numer(), *q.denom()) {
        (n, 1) => Ok(n),
        (n, 1001) if n % 1000 == 0 => Ok(n / 1000),
        _ => Err(TimecodeError::UnsupportedRate(rate)),
    }
}

fn dropped_per_minute(rate: Rate) -> Result<i64, TimecodeError> {
    let q = rate.as_ratio();
    match (*q.numer(), *q.denom()) {
        (30_000, 1001) => Ok(2),
        (60_000, 1001) => Ok(4),
        _ => Err(TimecodeError::DropFrameRate(rate)),
    }
}

fn parse_int(digits: &str, input: &str) -> Result<i64, TimecodeError> {
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(TimecodeError::Syntax(input.to_owned()));
    }
    digits.parse().map_err(|_| overflow(input))
}

/// Exact decimal: `72.4` is 724/10, never a float.
fn parse_decimal(text: &str, input: &str) -> Result<Q, TimecodeError> {
    let (whole, frac) = text.split_once('.').unwrap_or((text, ""));
    if (whole.is_empty() && frac.is_empty()) || text.ends_with('.') {
        return Err(TimecodeError::Syntax(input.to_owned()));
    }
    let whole = if whole.is_empty() { 0 } else { parse_int(whole, input)? };
    if frac.is_empty() {
        return Ok(Q::from_integer(whole));
    }
    let scale = 10i64.checked_pow(frac.len() as u32).ok_or_else(|| overflow(input))?;
    let numer = whole
        .checked_mul(scale)
        .and_then(|w| w.checked_add(parse_int(frac, input).ok()?))
        .ok_or_else(|| overflow(input))?;
    Ok(Q::new(numer, scale))
}

fn overflow(input: &str) -> TimecodeError {
    TimecodeError::Overflow(input.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn ntsc30() -> Rate {
        Rate::new(30_000, 1001)
    }

    fn ntsc60() -> Rate {
        Rate::new(60_000, 1001)
    }

    fn frames(s: &str, rate: Rate) -> i64 {
        let t = parse(s, rate).unwrap_or_else(|e| panic!("{s}: {e}"));
        assert_eq!(*t.value.denom(), 1, "{s} is not on a frame boundary");
        *t.value.numer()
    }

    #[test]
    fn non_drop_smpte() {
        assert_eq!(frames("00:01:12:12", Rate::integer(25)), 72 * 25 + 12);
        assert_eq!(frames("01:00:00:00", Rate::integer(24)), 86_400);
        // NDF at 29.97 counts nominal frames, so one "second" is 1.001 s.
        let t = parse("00:00:01:00", ntsc30()).unwrap();
        assert_eq!(t.value, Q::from_integer(30));
        assert_eq!(t.seconds(), Q::new(1001, 1000));
    }

    #[test]
    fn drop_frame_known_values() {
        assert_eq!(frames("00:00:59;29", ntsc30()), 1799);
        assert_eq!(frames("00:01:00;02", ntsc30()), 1800);
        assert_eq!(frames("00:10:00;00", ntsc30()), 17_982);
        assert_eq!(frames("01:00:00;00", ntsc30()), 107_892);
        assert_eq!(frames("00:01:00;04", ntsc60()), 3600);
        // Some tools write every separator as `;`.
        assert_eq!(frames("00;01;00;02", ntsc30()), 1800);
    }

    #[test]
    fn dropped_labels_are_rejected() {
        for s in ["00:01:00;00", "00:01:00;01", "00:59:00;01"] {
            assert_eq!(parse(s, ntsc30()), Err(TimecodeError::DroppedLabel(s.into())));
        }
        assert!(matches!(parse("00:01:00;03", ntsc60()), Err(TimecodeError::DroppedLabel(_))));
        // Every tenth minute keeps its first frames.
        assert_eq!(frames("00:20:00;00", ntsc30()), 2 * 17_982);
    }

    #[test]
    fn frames_and_seconds() {
        assert_eq!(frames("f2172", Rate::integer(30)), 2172);
        assert_eq!(frames("72.4s", Rate::integer(30)), 2172);
        assert_eq!(frames("72s", Rate::integer(25)), 1800);
        assert_eq!(frames(".5s", Rate::integer(30)), 15);
        assert_eq!(frames("-f12", Rate::integer(30)), -12);
        assert_eq!(frames(" f3 ", Rate::integer(30)), 3);
        // Seconds stay exact even between frames.
        let t = parse("72.4s", ntsc30()).unwrap();
        assert_eq!(t.seconds(), Q::new(724, 10));
        assert_eq!(t.floor_frames(), 2169);
    }

    #[test]
    fn bad_input_is_rejected_with_a_reason() {
        let r = Rate::integer(30);
        for s in ["", "abc", "f", "s", "f1.5", "1.s", "1:2:3", "1:2:3:4:5", "00:00:0a:00", "--f1", "f-1"] {
            assert!(matches!(parse(s, r), Err(TimecodeError::Syntax(_))), "{s:?}");
        }
        assert!(matches!(
            parse("00:61:00:00", r),
            Err(TimecodeError::FieldOutOfRange { field: "minutes", .. })
        ));
        assert!(matches!(
            parse("00:00:00:30", r),
            Err(TimecodeError::FieldOutOfRange { field: "frames", limit: 30, .. })
        ));
        assert_eq!(parse("00:00:00;00", Rate::integer(25)), Err(TimecodeError::DropFrameRate(Rate::integer(25))));
        assert!(matches!(parse("00:00:01:00", Rate::new(25, 2)), Err(TimecodeError::UnsupportedRate(_))));
        assert!(matches!(parse("99999999999999999999s", r), Err(TimecodeError::Overflow(_))));
        assert!(matches!(parse("9999999999999999:00:00:00", r), Err(TimecodeError::Overflow(_))));
        let msg = parse("00:61:00:00", r).unwrap_err().to_string();
        assert!(msg.contains("minutes is 61"), "{msg}");
    }

    #[test]
    fn formats_smpte() {
        let at = |f, r| RationalTime::from_frames(f, r);
        assert_eq!(to_smpte(at(1800, ntsc30()), true).unwrap(), "00:01:00;02");
        assert_eq!(to_smpte(at(17_982, ntsc30()), true).unwrap(), "00:10:00;00");
        assert_eq!(to_smpte(at(1799, ntsc30()), true).unwrap(), "00:00:59;29");
        assert_eq!(to_smpte(at(3600, ntsc60()), true).unwrap(), "00:01:00;04");
        assert_eq!(to_smpte(at(1812, Rate::integer(25)), false).unwrap(), "00:01:12:12");
        assert_eq!(to_smpte(at(-12, Rate::integer(30)), false).unwrap(), "-00:00:00:12");
        assert_eq!(to_smpte(at(90_000_000, Rate::integer(25)), false).unwrap(), "1000:00:00:00");
        assert!(to_smpte(at(0, Rate::integer(25)), true).is_err());
    }

    proptest! {
        #[test]
        fn smpte_round_trips(n in -50_000_000i64..50_000_000, which in 0usize..7) {
            let (rate, df) = [
                (ntsc30(), true), (ntsc60(), true), (ntsc30(), false),
                (Rate::new(24_000, 1001), false), (Rate::integer(24), false),
                (Rate::integer(25), false), (Rate::integer(60), false),
            ][which];
            let t = RationalTime::from_frames(n, rate);
            let s = to_smpte(t, df).unwrap();
            let back = parse(&s, rate).unwrap();
            prop_assert_eq!(back.value, t.value, "{}", s);
        }

        #[test]
        fn drop_frame_labels_are_consecutive(n in 0i64..20_000_000) {
            // The next frame's label is never a skipped one, and it parses back to n + 1.
            let s = to_smpte(RationalTime::from_frames(n + 1, ntsc30()), true).unwrap();
            prop_assert_eq!(parse(&s, ntsc30()).unwrap().value, Q::from_integer(n + 1));
        }
    }
}
