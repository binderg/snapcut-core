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
        for (name, c) in [("color", &self.color), ("outline_color", &self.outline_color), ("shadow_color", &self.shadow_color), ("background", &self.background)] {
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

    #[test]
    fn round_trips_through_a_generator_reference() {
        let t = TextClip { text: "Hello".into(), style: TextStyle::caption() };
        let r = MediaReference::Generator(GeneratorReference::text(&t));
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["OTIO_SCHEMA"], "GeneratorReference.1");
        assert_eq!(v["generator_kind"], "ve.text");
        let back: MediaReference = serde_json::from_value(v).unwrap();
        assert_eq!(back, r);
    }
}
