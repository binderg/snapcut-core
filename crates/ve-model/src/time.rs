//! Exact rational time.
//!
//! OTIO stores `RationalTime.value` and `.rate` as JSON doubles, so NTSC rates
//! arrive as `29.97002997002997`. In memory everything is `Ratio<i64>`; on the
//! way in, rates snap to the exact NTSC fractions (`30000/1001`, ...) and on the
//! way out they are written as the nearest double, which snaps back losslessly.
//! Nothing in the engine ever does arithmetic on the doubles.

use std::cmp::Ordering;
use std::fmt;
use std::ops::{Add, Sub};

use num_rational::Ratio;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Exact rational number used for all time math.
pub type Q = Ratio<i64>;

/// Frames (or samples) per second, as an exact fraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rate(Q);

/// NTSC-family rates that OTIO writes as long doubles.
const NTSC: [(i64, i64); 6] = [
    (24_000, 1001),
    (30_000, 1001),
    (48_000, 1001),
    (60_000, 1001),
    (120_000, 1001),
    (240_000, 1001),
];

impl Rate {
    pub fn new(num: i64, den: i64) -> Self {
        assert!(num > 0 && den > 0, "rate must be positive");
        Self(Q::new(num, den))
    }

    pub const fn integer(fps: i64) -> Self {
        Self(Q::new_raw(fps, 1))
    }

    pub fn as_ratio(self) -> Q {
        self.0
    }

    pub fn to_f64(self) -> f64 {
        *self.0.numer() as f64 / *self.0.denom() as f64
    }

    /// Parse an OTIO double rate into an exact rate.
    pub fn from_f64(f: f64) -> Option<Self> {
        if !(f.is_finite() && f > 0.0) {
            return None;
        }
        for (n, d) in NTSC {
            if (f - n as f64 / d as f64).abs() < 1e-6 {
                return Some(Self::new(n, d));
            }
        }
        if f.fract() == 0.0 {
            return Some(Self::integer(f as i64));
        }
        Q::approximate_float(f).map(Self)
    }
}

impl fmt::Display for Rate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self.0.denom() == 1 {
            write!(f, "{}", self.0.numer())
        } else {
            write!(f, "{}/{}", self.0.numer(), self.0.denom())
        }
    }
}

/// A point in time: `value` units at `rate` units per second.
///
/// `value` is usually a whole frame count but is kept rational so sub-frame
/// audio edits and speed changes stay exact. Equality and ordering compare the
/// instant in seconds, so `30 @ 30fps == 25 @ 25fps`.
#[derive(Debug, Clone, Copy)]
pub struct RationalTime {
    pub value: Q,
    pub rate: Rate,
}

impl RationalTime {
    pub fn from_frames(frames: i64, rate: Rate) -> Self {
        Self { value: Q::from_integer(frames), rate }
    }

    pub fn zero(rate: Rate) -> Self {
        Self::from_frames(0, rate)
    }

    /// Exact time in seconds.
    pub fn seconds(self) -> Q {
        self.value / self.rate.as_ratio()
    }

    /// Same instant expressed at another rate (value may become fractional).
    pub fn rescaled_to(self, rate: Rate) -> Self {
        Self { value: self.seconds() * rate.as_ratio(), rate }
    }

    /// Whole frame index at this time's rate, rounding toward negative infinity.
    pub fn floor_frames(self) -> i64 {
        self.value.floor().to_integer()
    }
}

impl PartialEq for RationalTime {
    fn eq(&self, other: &Self) -> bool {
        self.seconds() == other.seconds()
    }
}
impl Eq for RationalTime {}
impl PartialOrd for RationalTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for RationalTime {
    fn cmp(&self, other: &Self) -> Ordering {
        self.seconds().cmp(&other.seconds())
    }
}

/// Result keeps the left-hand side's rate.
impl Add for RationalTime {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self { value: self.value + rhs.rescaled_to(self.rate).value, rate: self.rate }
    }
}
impl Sub for RationalTime {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self { value: self.value - rhs.rescaled_to(self.rate).value, rate: self.rate }
    }
}

/// OTIO wire form. Only used at the serde boundary.
#[derive(Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "RationalTime")]
struct RationalTimeWire {
    #[serde(rename = "OTIO_SCHEMA", default = "rt_schema")]
    schema: String,
    value: f64,
    rate: f64,
}
fn rt_schema() -> String {
    "RationalTime.1".into()
}

impl Serialize for RationalTime {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        RationalTimeWire {
            schema: rt_schema(),
            value: *self.value.numer() as f64 / *self.value.denom() as f64,
            rate: self.rate.to_f64(),
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for RationalTime {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let w = RationalTimeWire::deserialize(d)?;
        let rate = Rate::from_f64(w.rate).ok_or_else(|| D::Error::custom("invalid rate"))?;
        let value = if w.value.fract() == 0.0 && w.value.abs() < 9.0e15 {
            Q::from_integer(w.value as i64)
        } else {
            Q::approximate_float(w.value).ok_or_else(|| D::Error::custom("invalid value"))?
        };
        Ok(Self { value, rate })
    }
}

impl JsonSchema for RationalTime {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "RationalTime".into()
    }
    fn json_schema(g: &mut schemars::SchemaGenerator) -> schemars::Schema {
        RationalTimeWire::json_schema(g)
    }
}

/// OTIO `TimeRange.1`: a start and a duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(into = "TimeRangeWire", from = "TimeRangeWire")]
#[schemars(with = "TimeRangeWire")]
pub struct TimeRange {
    pub start_time: RationalTime,
    pub duration: RationalTime,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "TimeRange")]
struct TimeRangeWire {
    #[serde(rename = "OTIO_SCHEMA", default = "tr_schema")]
    schema: String,
    start_time: RationalTime,
    duration: RationalTime,
}
fn tr_schema() -> String {
    "TimeRange.1".into()
}
impl From<TimeRange> for TimeRangeWire {
    fn from(r: TimeRange) -> Self {
        Self { schema: tr_schema(), start_time: r.start_time, duration: r.duration }
    }
}
impl From<TimeRangeWire> for TimeRange {
    fn from(w: TimeRangeWire) -> Self {
        Self { start_time: w.start_time, duration: w.duration }
    }
}

impl TimeRange {
    pub fn new(start_time: RationalTime, duration: RationalTime) -> Self {
        Self { start_time, duration }
    }

    /// First instant after the range.
    pub fn end_exclusive(&self) -> RationalTime {
        self.start_time + self.duration
    }

    pub fn contains(&self, t: RationalTime) -> bool {
        t >= self.start_time && t < self.end_exclusive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn ntsc_rate_snaps_to_exact_fraction() {
        let r = Rate::from_f64(29.970_029_970_029_97).unwrap();
        assert_eq!(r, Rate::new(30_000, 1001));
        assert_eq!(Rate::from_f64(r.to_f64()).unwrap(), r);
    }

    #[test]
    fn equality_is_by_instant() {
        let a = RationalTime::from_frames(30, Rate::integer(30));
        let b = RationalTime::from_frames(25, Rate::integer(25));
        assert_eq!(a, b);
    }

    #[test]
    fn add_across_rates_keeps_lhs_rate() {
        let a = RationalTime::from_frames(30, Rate::integer(30));
        let b = RationalTime::from_frames(12, Rate::integer(24));
        let c = a + b;
        assert_eq!(c.rate, Rate::integer(30));
        assert_eq!(c.value, Q::from_integer(45));
    }

    #[test]
    fn otio_json_shape() {
        let t = RationalTime::from_frames(312, Rate::integer(30));
        let j = serde_json::to_value(t).unwrap();
        assert_eq!(
            j,
            serde_json::json!({"OTIO_SCHEMA": "RationalTime.1", "value": 312.0, "rate": 30.0})
        );
        let back: RationalTime = serde_json::from_value(j).unwrap();
        assert_eq!(back.value, t.value);
        assert_eq!(back.rate, t.rate);
    }

    #[test]
    fn time_range_json_shape() {
        let r = Rate::integer(30);
        let tr = TimeRange::new(RationalTime::from_frames(312, r), RationalTime::from_frames(450, r));
        let j = serde_json::to_value(tr).unwrap();
        assert_eq!(j["OTIO_SCHEMA"], "TimeRange.1");
        let back: TimeRange = serde_json::from_value(j).unwrap();
        assert_eq!(back, tr);
        assert!(back.contains(RationalTime::from_frames(761, r)));
        assert!(!back.contains(RationalTime::from_frames(762, r)));
    }

    proptest! {
        #[test]
        fn whole_frames_roundtrip_exactly(
            frames in -10_000_000i64..10_000_000,
            rate_idx in 0usize..9,
        ) {
            let rates = [
                Rate::integer(24), Rate::integer(25), Rate::integer(30), Rate::integer(50),
                Rate::integer(60), Rate::integer(48_000),
                Rate::new(24_000, 1001), Rate::new(30_000, 1001), Rate::new(60_000, 1001),
            ];
            let t = RationalTime::from_frames(frames, rates[rate_idx]);
            let back: RationalTime =
                serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
            prop_assert_eq!(back.value, t.value);
            prop_assert_eq!(back.rate, t.rate);
        }
    }
}
