//! Keyframe curves: how a keyframed value moves from one key to the next.
//!
//! A segment runs from key `a` to the next key `b`. It is shaped by one of:
//!
//! - `a.ease`: `linear` (default), `hold` (keep `a`'s value until `b`), a
//!   named ease (`ease`, `ease-in`, `ease-out`, `ease-in-out`) or
//!   `cubic-bezier(x1, y1, x2, y2)` as in CSS. These remap time only, so they
//!   work on every value type (numbers, vectors, colors).
//! - Value handles, for a graph editor: `a.out` and `b.in`, each
//!   `{"x": fraction of the segment's time, "y": offset from the key's value}`.
//!   The curve is the cubic bezier through `(0, va)`, `(out.x, va + out.y)`,
//!   `(1 - in.x, vb + in.y)`, `(1, vb)`. A missing handle sits on the straight
//!   line (`x = 1/3`), so one handle alone bends only its end. `y` is a number
//!   for number values and a list of the same length for vectors. Because `y`
//!   is an offset in value units, a segment between two equal values can
//!   still overshoot.
//!
//! A segment uses either an ease or handles, never both. Handle `x` values
//! are fractions of the segment, not times, so the curve keeps its shape when
//! keys move. Values before the first key hold the first value and after the
//! last hold the last. Curves may overshoot; renderers clamp to each
//! param's range.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::meta::Keyframe;
use crate::time::Q;


/// A graph-editor handle on one side of a key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Handle {
    /// Fraction of the segment's duration, 0 to 1.
    pub x: f64,
    /// Offset from the key's value: a number, or a list for vector values.
    pub y: Value,
}

/// A parsed `ease`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ease {
    Linear,
    Hold,
    /// CSS `cubic-bezier(x1, y1, x2, y2)`.
    Bezier(f64, f64, f64, f64),
}

/// The names `ease` accepts besides `cubic-bezier(...)`.
pub const EASE_NAMES: [&str; 6] = ["linear", "hold", "ease", "ease-in", "ease-out", "ease-in-out"];

impl Ease {
    pub fn parse(s: &str) -> Result<Ease, String> {
        let s = s.trim();
        Ok(match s {
            "linear" => Ease::Linear,
            "hold" => Ease::Hold,
            // The CSS keywords.
            "ease" => Ease::Bezier(0.25, 0.1, 0.25, 1.0),
            "ease-in" => Ease::Bezier(0.42, 0.0, 1.0, 1.0),
            "ease-out" => Ease::Bezier(0.0, 0.0, 0.58, 1.0),
            "ease-in-out" => Ease::Bezier(0.42, 0.0, 0.58, 1.0),
            _ => {
                let bad = || format!("ease `{s}`: use {}, or cubic-bezier(x1, y1, x2, y2) with x1 and x2 from 0 to 1", EASE_NAMES.join(", "));
                let inner = s.strip_prefix("cubic-bezier(").and_then(|r| r.strip_suffix(')')).ok_or_else(bad)?;
                let n: Vec<f64> = inner.split(',').map(|p| p.trim().parse::<f64>()).collect::<Result<_, _>>().map_err(|_| bad())?;
                let [x1, y1, x2, y2] = n[..] else { return Err(bad()) };
                if !n.iter().all(|v| v.is_finite()) || !(0.0..=1.0).contains(&x1) || !(0.0..=1.0).contains(&x2) {
                    return Err(bad());
                }
                Ease::Bezier(x1, y1, x2, y2)
            }
        })
    }

    /// Eased progress for linear progress `f` (0 to 1).
    pub fn apply(self, f: f64) -> f64 {
        match self {
            Ease::Linear => f,
            Ease::Hold => 0.0,
            Ease::Bezier(x1, y1, x2, y2) => {
                let s = solve_x(x1, x2, f);
                bezier(0.0, y1, y2, 1.0, s)
            }
        }
    }
}

/// Check one param's keys (sorted by time): every ease parses, handles are
/// in range and shaped like the value, and no segment mixes an ease with
/// handles.
pub fn check(keys: &[Keyframe]) -> Result<(), String> {
    for (i, k) in keys.iter().enumerate() {
        let ease = Ease::parse(&k.ease)?;
        for (side, h) in [("in", &k.r#in), ("out", &k.out)] {
            let Some(h) = h else { continue };
            if !(0.0..=1.0).contains(&h.x) {
                return Err(format!("{side} handle x {}: use a fraction of the segment from 0 to 1", h.x));
            }
            let fits = match (&k.v, &h.y) {
                (Value::Number(_), Value::Number(y)) => y.as_f64().is_some_and(f64::is_finite),
                (Value::Array(v), Value::Array(y)) => v.len() == y.len() && y.iter().all(|y| y.as_f64().is_some_and(f64::is_finite)),
                _ => false,
            };
            if !fits {
                return Err(format!("{side} handle y {}: use a number offset for a number value, or a list of {} numbers for a list value like {}", h.y, k.v.as_array().map_or(1, Vec::len), k.v));
            }
        }
        let next_in = keys.get(i + 1).is_some_and(|n| n.r#in.is_some());
        if ease != Ease::Linear && (k.out.is_some() || next_in) {
            return Err(format!("key at {}s has ease `{}` and handles on the same segment; use one or the other", k.t.seconds(), k.ease));
        }
    }
    Ok(())
}

/// Value of a keyframed param at `t` (seconds, same origin as the keys).
pub fn interpolate(keys: &[Keyframe], t: Q) -> Value {
    let mut sorted: Vec<&Keyframe> = keys.iter().collect();
    sorted.sort_by_key(|k| k.t);
    let Some(first) = sorted.first() else { return Value::Null };
    if t <= first.t.seconds() {
        return first.v.clone();
    }
    for w in sorted.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (ta, tb) = (a.t.seconds(), b.t.seconds());
        if t < tb {
            if tb <= ta {
                return a.v.clone();
            }
            let f = to_f64((t - ta) / (tb - ta));
            return segment(a, b, f);
        }
    }
    sorted.last().map_or(Value::Null, |k| k.v.clone())
}

/// The value `f` (0 to 1) of the way through the segment from `a` to `b`.
pub fn segment(a: &Keyframe, b: &Keyframe, f: f64) -> Value {
    let ease = Ease::parse(&a.ease).unwrap_or(Ease::Linear);
    if ease == Ease::Hold {
        return a.v.clone();
    }
    if a.out.is_none() && b.r#in.is_none() {
        return lerp(&a.v, &b.v, ease.apply(f));
    }
    // Value-space bezier: solve for the curve parameter once, then evaluate
    // each component with its own control values.
    let ox = a.out.as_ref().map_or(1.0 / 3.0, |h| h.x.clamp(0.0, 1.0));
    let ix = b.r#in.as_ref().map_or(1.0 / 3.0, |h| h.x.clamp(0.0, 1.0));
    let s = solve_x(ox, 1.0 - ix, f);
    let comp = |va: f64, vb: f64, oy: Option<f64>, iy: Option<f64>| {
        let oy = oy.unwrap_or((vb - va) / 3.0);
        let iy = iy.unwrap_or((va - vb) / 3.0);
        bezier(va, va + oy, vb + iy, vb, s)
    };
    let hy = |h: &Option<Handle>, i: Option<usize>| {
        h.as_ref().and_then(|h| match i {
            None => h.y.as_f64(),
            Some(i) => h.y.get(i).and_then(Value::as_f64),
        })
    };
    match (&a.v, &b.v) {
        (Value::Number(x), Value::Number(y)) => Value::from(comp(x.as_f64().unwrap_or(0.0), y.as_f64().unwrap_or(0.0), hy(&a.out, None), hy(&b.r#in, None))),
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => Value::Array(
            x.iter()
                .zip(y)
                .enumerate()
                .map(|(i, (x, y))| Value::from(comp(x.as_f64().unwrap_or(0.0), y.as_f64().unwrap_or(0.0), hy(&a.out, Some(i)), hy(&b.r#in, Some(i)))))
                .collect(),
        ),
        _ => lerp(&a.v, &b.v, f),
    }
}

fn lerp(a: &Value, b: &Value, f: f64) -> Value {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap_or(0.0), y.as_f64().unwrap_or(0.0));
            Value::from(x + (y - x) * f)
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => Value::Array(x.iter().zip(y).map(|(x, y)| lerp(x, y, f)).collect()),
        // Values that cannot blend (true/false, text) switch at the next key.
        _ => {
            if f < 1.0 {
                a.clone()
            } else {
                b.clone()
            }
        }
    }
}

fn bezier(p0: f64, p1: f64, p2: f64, p3: f64, s: f64) -> f64 {
    let u = 1.0 - s;
    u * u * u * p0 + 3.0 * u * u * s * p1 + 3.0 * u * s * s * p2 + s * s * s * p3
}

/// The curve parameter where the x of the bezier `(0, x1, x2, 1)` equals
/// `f`. With x1 and x2 in 0..1 that x is monotonic, so the answer is unique:
/// Newton steps, then bisection if they stall.
fn solve_x(x1: f64, x2: f64, f: f64) -> f64 {
    let f = f.clamp(0.0, 1.0);
    let x = |s: f64| bezier(0.0, x1, x2, 1.0, s);
    let dx = |s: f64| {
        let u = 1.0 - s;
        3.0 * u * u * x1 + 6.0 * u * s * (x2 - x1) + 3.0 * s * s * (1.0 - x2)
    };
    let mut s = f;
    for _ in 0..8 {
        let err = x(s) - f;
        if err.abs() < 1e-9 {
            return s;
        }
        let d = dx(s);
        if d.abs() < 1e-9 {
            break;
        }
        s = (s - err / d).clamp(0.0, 1.0);
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if x(mid) < f {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.0
}

fn to_f64(q: Q) -> f64 {
    *q.numer() as f64 / *q.denom() as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::{Rate, RationalTime};
    use serde_json::json;

    fn key(f: i64, v: Value, ease: &str) -> Keyframe {
        Keyframe { t: RationalTime::from_frames(f, Rate::integer(10)), v, ease: ease.into(), r#in: None, out: None }
    }

    fn at(keys: &[Keyframe], frame: i64) -> f64 {
        interpolate(keys, Q::new(frame, 10)).as_f64().unwrap()
    }

    #[test]
    fn named_eases_match_css() {
        // CSS ease-in-out is symmetric and passes through the middle.
        let e = Ease::parse("ease-in-out").unwrap();
        assert!((e.apply(0.5) - 0.5).abs() < 1e-6);
        assert!((e.apply(0.25) + e.apply(0.75) - 1.0).abs() < 1e-6);
        // ease-in starts slow, ease-out starts fast.
        assert!(Ease::parse("ease-in").unwrap().apply(0.25) < 0.25);
        assert!(Ease::parse("ease-out").unwrap().apply(0.25) > 0.25);
        // Ends are exact.
        for n in ["ease", "ease-in", "ease-out", "ease-in-out", "cubic-bezier(0.1, -0.5, 0.9, 1.5)"] {
            let e = Ease::parse(n).unwrap();
            assert!(e.apply(0.0).abs() < 1e-9 && (e.apply(1.0) - 1.0).abs() < 1e-9, "{n}");
        }
    }

    #[test]
    fn cubic_bezier_parses_and_can_overshoot() {
        let e = Ease::parse(" cubic-bezier(0.3, 1.6, 0.6, 1) ").unwrap();
        let peak = (1..100).map(|i| e.apply(i as f64 / 100.0)).fold(f64::MIN, f64::max);
        assert!(peak > 1.05, "overshoots: {peak}");
        assert!(Ease::parse("cubic-bezier(1.2, 0, 0.5, 1)").is_err(), "x1 must be 0..1");
        assert!(Ease::parse("cubic-bezier(0, 0, 1)").is_err());
        assert!(Ease::parse("bouncy").is_err());
    }

    #[test]
    fn eased_segment_keeps_its_ends() {
        let keys = [key(0, json!(0.0), "ease-in"), key(10, json!(10.0), "linear")];
        assert_eq!(at(&keys, 0), 0.0);
        assert!(at(&keys, 5) < 5.0, "ease-in is behind linear at the middle");
        assert_eq!(at(&keys, 10), 10.0);
        assert_eq!(at(&keys, 20), 10.0);
    }

    #[test]
    fn default_handles_are_linear() {
        let mut keys = [key(0, json!(2.0), "linear"), key(10, json!(12.0), "linear")];
        keys[0].out = Some(Handle { x: 1.0 / 3.0, y: json!(10.0 / 3.0) });
        for f in 0..=10 {
            assert!((at(&keys, f) - (2.0 + f as f64)).abs() < 1e-6, "frame {f}");
        }
    }

    #[test]
    fn handles_overshoot_between_equal_values() {
        let mut keys = [key(0, json!(1.0), "linear"), key(10, json!(1.0), "linear")];
        keys[0].out = Some(Handle { x: 0.3, y: json!(0.6) });
        keys[1].r#in = Some(Handle { x: 0.3, y: json!(0.6) });
        assert!(at(&keys, 5) > 1.3, "bulges up: {}", at(&keys, 5));
        assert_eq!(at(&keys, 10), 1.0);
    }

    #[test]
    fn vector_handles_bend_each_component() {
        let mut keys = [key(0, json!([0.0, 0.0]), "linear"), key(10, json!([1.0, 1.0]), "linear")];
        keys[0].out = Some(Handle { x: 0.5, y: json!([0.0, 1.0]) });
        let v = interpolate(&keys, Q::new(5, 10));
        let (x, y) = (v[0].as_f64().unwrap(), v[1].as_f64().unwrap());
        assert!(y > x, "second component leads: {x} {y}");
    }

    #[test]
    fn hold_and_switching_values() {
        let keys = [key(0, json!(3.0), "hold"), key(10, json!(9.0), "linear")];
        assert_eq!(at(&keys, 9), 3.0);
        assert_eq!(at(&keys, 10), 9.0);
        let flags = [key(0, json!(false), "ease-in"), key(10, json!(true), "linear")];
        assert_eq!(interpolate(&flags, Q::new(9, 10)), json!(false));
    }

    #[test]
    fn check_rejects_bad_shapes() {
        let mut keys = vec![key(0, json!(0.0), "ease-out"), key(10, json!(1.0), "linear")];
        assert!(check(&keys).is_ok());
        keys[1].r#in = Some(Handle { x: 0.2, y: json!(0.0) });
        assert!(check(&keys).unwrap_err().contains("one or the other"));
        keys[0].ease = "linear".into();
        assert!(check(&keys).is_ok());
        keys[1].r#in = Some(Handle { x: 1.5, y: json!(0.0) });
        assert!(check(&keys).is_err());
        keys[1].r#in = Some(Handle { x: 0.5, y: json!([0.0]) });
        assert!(check(&keys).unwrap_err().contains("number offset"));
        keys[1].r#in = None;
        keys[0].ease = "wobble".into();
        assert!(check(&keys).is_err());
    }
}
