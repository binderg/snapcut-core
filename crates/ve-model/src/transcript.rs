//! Transcripts: what was said in a media file, word by word.
//!
//! The core owns the shape; getting a transcript (speech to text) is a
//! plugin's job. A transcript belongs to one source asset and its times are
//! **media times** of that file, so cuts, trims and speed changes never make
//! it stale. The engine maps words onto the timeline when asked.
//!
//! Word times are kept to the millisecond and written as exact decimal
//! seconds (`"12.345s"`), the same seconds form time parameters accept.

use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::Id;
use crate::time::Q;

/// File format tag, so a later change can be told apart.
pub const TRANSCRIPT_SCHEMA: &str = "Transcript.1";

/// A transcript of one asset's audio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Transcript {
    #[serde(default = "schema_tag")]
    pub schema: String,
    /// The asset this transcript belongs to.
    pub asset: Id,
    /// blake3 hash of the file that was transcribed, so a transcript of
    /// different content is never applied to an asset by mistake.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    /// BCP 47 language tag (`en`, `en-US`, `he`), if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Every word, in time order.
    pub words: Vec<Word>,
    /// Speakers named in `words`, with optional display names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub speakers: Vec<Speaker>,
    /// Who made it: plugin id and version, model, parameters, when. Free form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<serde_json::Value>,
}

fn schema_tag() -> String {
    TRANSCRIPT_SCHEMA.into()
}

/// One spoken word (or token, for languages without spaces).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Word {
    /// Media time the word starts, in seconds (`"1.230s"`).
    pub start: Seconds,
    /// Media time the word ends, exclusive.
    pub end: Seconds,
    /// The word as it should read, with punctuation if the model gives it.
    pub text: String,
    /// Speaker label (`"S0"`, `"S1"`), when the model separates speakers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    /// 0 to 1, when the model reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Speaker {
    /// The label words use.
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Transcript {
    /// Check words are well formed and sort them by start.
    pub fn normalize(&mut self) -> Result<(), String> {
        for (i, w) in self.words.iter().enumerate() {
            if w.start.0 < Q::from_integer(0) {
                return Err(format!("word {i} (`{}`) starts before 0", w.text));
            }
            if w.end.0 < w.start.0 {
                return Err(format!("word {i} (`{}`) ends before it starts", w.text));
            }
            if let Some(c) = w.confidence
                && !(0.0..=1.0).contains(&c)
            {
                return Err(format!("word {i} (`{}`): confidence {c} is not between 0 and 1", w.text));
            }
        }
        self.words.sort_by(|a, b| a.start.0.cmp(&b.start.0).then(a.end.0.cmp(&b.end.0)));
        self.schema = schema_tag();
        Ok(())
    }

    /// Media time of the end of the last word.
    pub fn end(&self) -> Q {
        self.words.iter().map(|w| w.end.0).max().unwrap_or(Q::from_integer(0))
    }
}

/// Exact seconds kept to the millisecond, written as `"12.345s"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Seconds(pub Q);

impl Seconds {
    /// Round to the nearest millisecond.
    pub fn new(q: Q) -> Self {
        let ms = (q * Q::from_integer(1000)).round();
        Self(ms / Q::from_integer(1000))
    }

    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim().strip_suffix('s')?.trim();
        let (neg, t) = match t.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, t),
        };
        let (whole, frac) = t.split_once('.').unwrap_or((t, ""));
        if whole.is_empty() && frac.is_empty() {
            return None;
        }
        if !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) || frac.len() > 15 {
            return None;
        }
        let w: i64 = if whole.is_empty() { 0 } else { whole.parse().ok()? };
        let f: i64 = if frac.is_empty() { 0 } else { frac.parse().ok()? };
        let den = 10i64.checked_pow(frac.len() as u32)?;
        let q = Q::from_integer(w) + Q::new(f, den);
        Some(Self::new(if neg { -q } else { q }))
    }
}

impl fmt::Display for Seconds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ms = (self.0 * Q::from_integer(1000)).round().to_integer();
        let sign = if ms < 0 { "-" } else { "" };
        let ms = ms.abs();
        write!(f, "{sign}{}.{:03}s", ms / 1000, ms % 1000)
    }
}

impl Serialize for Seconds {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Seconds {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let s = String::deserialize(d)?;
        Self::parse(&s).ok_or_else(|| D::Error::custom(format!("`{s}`: write seconds like \"12.345s\"")))
    }
}

impl JsonSchema for Seconds {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Seconds".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^-?[0-9]*(\\.[0-9]+)?s$",
            "description": "Exact decimal seconds, kept to the millisecond: \"12.345s\""
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_roundtrip() {
        for (text, q) in [("1.230s", Q::new(123, 100)), ("0.000s", Q::from_integer(0)), ("72.400s", Q::new(362, 5))] {
            let s = Seconds::parse(text).unwrap();
            assert_eq!(s.0, q);
            assert_eq!(s.to_string(), text);
        }
        assert_eq!(Seconds::parse("1.2345s").unwrap().to_string(), "1.235s");
        assert_eq!(Seconds::parse(".5s").unwrap().0, Q::new(1, 2));
        assert_eq!(Seconds::parse("-0.25s").unwrap().to_string(), "-0.250s");
        for bad in ["1.2", "s", "1.2.3s", "abc", "1e3s", ".s"] {
            assert!(Seconds::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn normalize_sorts_and_checks() {
        let w = |a: &str, b: &str, t: &str| Word {
            start: Seconds::parse(a).unwrap(),
            end: Seconds::parse(b).unwrap(),
            text: t.into(),
            speaker: None,
            confidence: None,
        };
        let mut t = Transcript {
            schema: String::new(),
            asset: Id::new("asset"),
            source_hash: None,
            language: None,
            words: vec![w("1s", "1.5s", "world"), w("0.2s", "0.8s", "hello")],
            speakers: vec![],
            provenance: None,
        };
        t.normalize().unwrap();
        assert_eq!(t.words[0].text, "hello");
        assert_eq!(t.schema, TRANSCRIPT_SCHEMA);
        assert_eq!(t.end(), Q::new(3, 2));
        t.words.push(w("2s", "1s", "back"));
        assert!(t.normalize().is_err());
    }
}
