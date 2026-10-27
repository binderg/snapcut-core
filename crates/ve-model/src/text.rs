//! Text clips: words drawn by the engine, for titles, labels and captions.
//!
//! A text clip is an ordinary OTIO clip whose media is a `GeneratorReference`
//! with `generator_kind: "ve.text"` and the text and its style as
//! `parameters` (so other OTIO tools see a generated clip, not missing
//! media). It sits on a video track like any clip: trims, moves, splits,
//! transforms (position, scale, rotation, opacity, blend, keyframes),
//! effects and transitions all apply.
//!
//! Sizes are fractions of the canvas, so the same clip looks the same at any
//! render size. The picture is the text's own box (text plus padding,
//! outline and shadow), centered on the canvas; `transform.x`/`y` move it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::otio::{Clip, MediaReference};
use crate::time::Q;
use crate::transcript::Seconds;

/// `generator_kind` of text clips.
pub const TEXT_KIND: &str = "ve.text";

/// The text and how it looks: `GeneratorReference.parameters` of a text clip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextClip {
    /// The words. `\n` starts a new line; long lines wrap at `width`.
    pub text: String,
    #[serde(flatten)]
    pub style: TextStyle,
    /// How it appears at the clip's start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animate_in: Option<Animation>,
    /// How it leaves at the clip's end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animate_out: Option<Animation>,
    /// When each word of `text` (split at whitespace, in order) is spoken, in
    /// the clip's own time (0 is where the clip was made; trims and splits keep
    /// it). Captions fill this in; with `highlight` in the style the word
    /// being spoken is drawn in that color.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<WordTime>,
}

/// One word's time in a text clip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WordTime {
    pub start: Seconds,
    pub end: Seconds,
}

/// An entrance or exit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Animation {
    pub kind: AnimationKind,
    /// How long it takes. Default 0.4 s; at most half the clip.
    #[serde(default = "default_animation_duration")]
    pub duration: Seconds,
}

fn default_animation_duration() -> Seconds {
    Seconds(Q::new(2, 5))
}

/// What an entrance does (an exit plays it backwards).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum AnimationKind {
    /// Opacity from 0.
    Fade,
    /// Grows from half size with a little overshoot, fading in.
    Pop,
    /// Shrinks from 1.4x, fading in.
    Zoom,
    /// Rises into place from a little below, fading in.
    SlideUp,
    /// Drops into place from a little above.
    SlideDown,
    /// Moves in from the right, toward the left.
    SlideLeft,
    /// Moves in from the left, toward the right.
    SlideRight,
    /// Letters appear one after another.
    Typewriter,
    /// Words appear one after another.
    Words,
}

impl AnimationKind {
    pub const ALL: [AnimationKind; 9] = [
        AnimationKind::Fade,
        AnimationKind::Pop,
        AnimationKind::Zoom,
        AnimationKind::SlideUp,
        AnimationKind::SlideDown,
        AnimationKind::SlideLeft,
        AnimationKind::SlideRight,
        AnimationKind::Typewriter,
        AnimationKind::Words,
    ];
}

/// How a text clip looks at one moment, on top of its style and transform.
#[derive(Debug, Clone, PartialEq)]
pub struct TextFrame {
    /// Multiplies the clip's opacity.
    pub opacity: f64,
    /// Multiplies the clip's scale.
    pub scale: f64,
    /// Added to the clip's position, in canvas widths / heights.
    pub dx: f64,
    pub dy: f64,
    /// Draw only text before this byte (letters or words still to come stay
    /// invisible but keep their place).
    pub reveal: Option<usize>,
    /// Byte range of the word to draw in the highlight color.
    pub highlight: Option<(usize, usize)>,
}

impl TextFrame {
    pub const STILL: TextFrame = TextFrame { opacity: 1.0, scale: 1.0, dx: 0.0, dy: 0.0, reveal: None, highlight: None };

    /// What decides the glyph picture (not the placement), for cache keys.
    pub fn glyph_key(&self) -> String {
        format!("{:?} {:?}", self.reveal, self.highlight)
    }
}

/// Byte ranges of the words of `text` (split at whitespace).
pub fn word_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        match (c.is_whitespace(), start) {
            (true, Some(s)) => {
                out.push((s, i));
                start = None;
            }
            (false, None) => start = Some(i),
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, text.len()));
    }
    out
}

fn to_f64(q: Q) -> f64 {
    *q.numer() as f64 / *q.denom() as f64
}

/// The text clip `t` at `local` seconds into a clip `length` long, whose own
/// time (for word timings) is `own` there.
pub fn frame(t: &TextClip, local: Q, length: Q, own: Q) -> TextFrame {
    let mut f = TextFrame::STILL;
    let half = length / Q::from_integer(2);
    let zero = Q::from_integer(0);
    // Progress 0..1 into the entrance, and out of the exit (1 = fully shown).
    let progress = |a: &Animation, into: Q| -> f64 {
        let d = a.duration.0.min(half);
        if d <= zero { 1.0 } else { to_f64((into / d).clamp(zero, Q::from_integer(1))) }
    };
    let ease = |name: &str, x: f64| crate::curve::Ease::parse(name).map_or(x, |e| e.apply(x));
    for (anim, into) in [(t.animate_in.as_ref(), local), (t.animate_out.as_ref(), length - local)] {
        let Some(a) = anim else { continue };
        let p = progress(a, into);
        if p >= 1.0 {
            continue;
        }
        let e = ease("ease-out", p);
        let slide = 0.08 * (1.0 - e);
        match a.kind {
            AnimationKind::Fade => f.opacity *= e,
            AnimationKind::Pop => {
                f.scale *= 0.5 + 0.5 * ease("cubic-bezier(0.34, 1.56, 0.64, 1)", p);
                f.opacity *= (p * 3.0).min(1.0);
            }
            AnimationKind::Zoom => {
                f.scale *= 1.4 - 0.4 * e;
                f.opacity *= e;
            }
            AnimationKind::SlideUp => {
                f.dy += slide;
                f.opacity *= e;
            }
            AnimationKind::SlideDown => {
                f.dy -= slide;
                f.opacity *= e;
            }
            AnimationKind::SlideLeft => {
                f.dx += slide;
                f.opacity *= e;
            }
            AnimationKind::SlideRight => {
                f.dx -= slide;
                f.opacity *= e;
            }
            AnimationKind::Typewriter => {
                let chars: Vec<usize> = t.text.char_indices().map(|(i, _)| i).collect();
                let n = (p * chars.len() as f64).floor() as usize;
                let at = chars.get(n).copied().unwrap_or(t.text.len());
                f.reveal = Some(f.reveal.map_or(at, |r| r.min(at)));
            }
            AnimationKind::Words => {
                let words = word_ranges(&t.text);
                let n = (p * words.len() as f64).floor() as usize;
                let at = words.get(n).map_or(t.text.len(), |w| w.0);
                f.reveal = Some(f.reveal.map_or(at, |r| r.min(at)));
            }
        }
    }
    if t.style.highlight.is_some() && !t.words.is_empty() {
        let ranges = word_ranges(&t.text);
        // The word being spoken: the last one started, until the next starts.
        if let Some(i) = t.words.iter().rposition(|w| w.start.0 <= own)
            && let Some(r) = ranges.get(i)
        {
            f.highlight = Some(*r);
        }
    }
    f
}

/// How text looks. Every field has a default, so `{}` is plain white text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct TextStyle {
    /// Font family name (`"Arial"`, `"Inter"`), or `sans-serif`, `serif`,
    /// `monospace`. Installed fonts only; a missing family falls back to a sans-serif.
    pub font: String,
    /// Letter size: the font's em height as a fraction of the canvas height (0.07 is 7%).
    pub size: f64,
    /// 100 (thin) to 900 (black); 400 regular, 700 bold.
    pub weight: u16,
    pub italic: bool,
    /// sRGB `[r, g, b]` or `[r, g, b, a]`, 0 to 1.
    pub color: Vec<f64>,
    /// `left`, `center` or `right`, within the text box.
    pub align: Align,
    /// Longest line before wrapping, as a fraction of the canvas width.
    pub width: f64,
    /// Line spacing, as a multiple of `size`.
    pub line_height: f64,
    /// Outline thickness, as a fraction of `size` (0 for none).
    pub outline: f64,
    pub outline_color: Vec<f64>,
    /// Drop shadow offset (down and right), as a fraction of `size` (0 for none).
    pub shadow: f64,
    pub shadow_color: Vec<f64>,
    /// Box behind the text; alpha 0 (the default) draws none.
    pub background: Vec<f64>,
    /// Space between the text and the box edge, as a fraction of `size`.
    pub padding: f64,
    /// Color of the word being spoken (needs word timings, which captions have).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlight: Option<Vec<f64>>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: "sans-serif".into(),
            size: 0.07,
            weight: 400,
            italic: false,
            color: vec![1.0, 1.0, 1.0, 1.0],
            align: Align::Center,
            width: 0.8,
            line_height: 1.2,
            outline: 0.0,
            outline_color: vec![0.0, 0.0, 0.0, 1.0],
            shadow: 0.0,
            shadow_color: vec![0.0, 0.0, 0.0, 0.6],
            background: vec![0.0, 0.0, 0.0, 0.0],
            padding: 0.3,
            highlight: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Left,
    #[default]
    Center,
    Right,
}

impl TextStyle {
    /// The style for captions: bold, a little smaller, with an outline so it
    /// reads over any picture.
    pub fn caption() -> Self {
        Self { weight: 700, size: 0.055, outline: 0.12, width: 0.85, ..Self::default() }
    }

    /// Why this style cannot be drawn, if it cannot.
    pub fn check(&self) -> Result<(), String> {
        let range = |name: &str, v: f64, lo: f64, hi: f64| {
            if v.is_finite() && (lo..=hi).contains(&v) { Ok(()) } else { Err(format!("{name} is {v}; use {lo} to {hi}")) }
        };
        range("size", self.size, 0.005, 1.0)?;
        range("width", self.width, 0.05, 1.0)?;
        range("line_height", self.line_height, 0.5, 4.0)?;
        range("outline", self.outline, 0.0, 0.5)?;
        range("shadow", self.shadow, 0.0, 1.0)?;
        range("padding", self.padding, 0.0, 4.0)?;
        if !(100..=900).contains(&self.weight) {
            return Err(format!("weight is {}; use 100 to 900 (400 regular, 700 bold)", self.weight));
        }
        let highlight = self.highlight.clone().unwrap_or_else(|| vec![0.0, 0.0, 0.0]);
        for (name, c) in [("color", &self.color), ("outline_color", &self.outline_color), ("shadow_color", &self.shadow_color), ("background", &self.background), ("highlight", &highlight)] {
            if !(3..=4).contains(&c.len()) || c.iter().any(|v| !(0.0..=1.0).contains(v)) {
                return Err(format!("{name} must be [r, g, b] or [r, g, b, a] with values from 0 to 1"));
            }
        }
        if self.font.trim().is_empty() {
            return Err("font is empty; use a family name or sans-serif".into());
        }
        Ok(())
    }
}

/// A color as RGBA, alpha 1 when left out.
pub fn rgba(c: &[f64]) -> [f64; 4] {
    [c.first().copied().unwrap_or(0.0), c.get(1).copied().unwrap_or(0.0), c.get(2).copied().unwrap_or(0.0), c.get(3).copied().unwrap_or(1.0)]
}

/// OTIO `GeneratorReference.1`: media made by a generator from parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratorReference {
    #[serde(default)]
    pub metadata: crate::otio::Metadata,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub available_range: Option<crate::time::TimeRange>,
    #[serde(default)]
    pub available_image_bounds: Option<Value>,
    pub generator_kind: String,
    #[serde(default)]
    pub parameters: Value,
}

impl GeneratorReference {
    pub fn text(t: &TextClip) -> Self {
        Self {
            metadata: crate::otio::Metadata::new(),
            name: String::new(),
            available_range: None,
            available_image_bounds: None,
            generator_kind: TEXT_KIND.into(),
            parameters: serde_json::to_value(t).expect("text serializes"),
        }
    }
}

impl Clip {
    /// The text and style of a text clip; `None` for other clips. A text
    /// clip whose parameters do not parse is an error.
    pub fn text(&self) -> Option<Result<TextClip, String>> {
        match self.active_media()? {
            MediaReference::Generator(g) if g.generator_kind == TEXT_KIND => {
                Some(serde_json::from_value(g.parameters.clone()).map_err(|e| format!("bad text clip parameters: {e}")))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_fill_in_and_unknown_fields_are_refused() {
        let t: TextClip = serde_json::from_value(json!({"text": "Hi", "size": 0.1})).unwrap();
        assert_eq!(t.style.size, 0.1);
        assert_eq!(t.style.align, Align::Center);
        assert!(t.style.check().is_ok());
        assert!(serde_json::from_value::<TextClip>(json!({"text": "Hi", "colour": [1, 0, 0]})).is_err());
        let bad = TextStyle { color: vec![2.0, 0.0, 0.0], ..TextStyle::default() };
        assert!(bad.check().unwrap_err().contains("color"));
    }

    fn clip(text: &str) -> TextClip {
        TextClip { text: text.into(), style: TextStyle::default(), animate_in: None, animate_out: None, words: Vec::new() }
    }

    #[test]
    fn animations_shape_the_start_and_end() {
        let s = |x: i64| Q::new(x, 10);
        let anim = |kind| Some(Animation { kind, duration: Seconds(s(4)) });
        let t = TextClip { animate_in: anim(AnimationKind::Fade), animate_out: anim(AnimationKind::SlideDown), ..clip("Hello there") };
        let len = s(30);
        assert_eq!(frame(&t, s(0), len, s(0)).opacity, 0.0);
        let mid = frame(&t, s(2), len, s(2)).opacity;
        assert!(mid > 0.5 && mid < 1.0, "ease-out is past halfway at the middle: {mid}");
        assert_eq!(frame(&t, s(15), len, s(15)), TextFrame::STILL);
        let leaving = frame(&t, s(28), len, s(28));
        assert!(leaving.dy < 0.0 && leaving.opacity < 1.0, "slides up out as the exit plays backwards: {leaving:?}");

        let pop = TextClip { animate_in: anim(AnimationKind::Pop), ..clip("Hi") };
        let peak = (1..40).map(|i| frame(&pop, Q::new(i, 100), len, s(0)).scale).fold(0.0, f64::max);
        assert!(peak > 1.0, "pop overshoots: {peak}");

        let tw = TextClip { animate_in: anim(AnimationKind::Typewriter), ..clip("abcd") };
        assert_eq!(frame(&tw, s(0), len, s(0)).reveal, Some(0));
        assert_eq!(frame(&tw, s(2), len, s(0)).reveal, Some(2));
        assert_eq!(frame(&tw, s(4), len, s(0)).reveal, None);
        let words = TextClip { animate_in: anim(AnimationKind::Words), ..clip("one two three four") };
        assert_eq!(frame(&words, s(2), len, s(0)).reveal, Some(8), "two of four words");
        // A short clip gives each animation at most half of it.
        let short = TextClip { animate_in: anim(AnimationKind::Fade), ..clip("x") };
        assert_eq!(frame(&short, s(1), s(2), s(0)).opacity, 1.0);
    }

    #[test]
    fn highlight_follows_the_spoken_word() {
        let w = |a: i64, b: i64| WordTime { start: Seconds(Q::new(a, 10)), end: Seconds(Q::new(b, 10)) };
        let mut t = clip("So\ntoday we");
        t.words = vec![w(0, 3), w(4, 9), w(10, 12)];
        assert_eq!(frame(&t, Q::new(5, 10), Q::from_integer(2), Q::new(5, 10)).highlight, None, "needs a highlight color");
        t.style.highlight = Some(vec![1.0, 0.8, 0.0]);
        assert_eq!(frame(&t, Q::new(5, 10), Q::from_integer(2), Q::new(5, 10)).highlight, Some((3, 8)));
        assert_eq!(frame(&t, Q::new(11, 10), Q::from_integer(2), Q::new(11, 10)).highlight, Some((9, 11)));
        assert_eq!(word_ranges(" a  bc\nd"), [(1, 2), (4, 6), (7, 8)]);
    }

    #[test]
    fn round_trips_through_a_generator_reference() {
        let t = TextClip { text: "Hello".into(), style: TextStyle::caption(), animate_in: None, animate_out: None, words: Vec::new() };
        let r = MediaReference::Generator(GeneratorReference::text(&t));
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["OTIO_SCHEMA"], "GeneratorReference.1");
        assert_eq!(v["generator_kind"], "ve.text");
        let back: MediaReference = serde_json::from_value(v).unwrap();
        assert_eq!(back, r);
    }
}
