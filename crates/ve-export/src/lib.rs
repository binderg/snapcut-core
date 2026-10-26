//! Export: named presets over [`ve_render::encode`].
//!
//! Presets set the frame size, how sources fill it, and quality. The frame
//! rate and sample rate always come from the project. The encoder is picked
//! from what the local ffmpeg has (libx264, hardware, openh264, MPEG-4), so
//! the default build never needs a GPL codec.

use ve_model::Timeline;
use ve_render::{Canvas, EncodeSettings, Fit, Format, project_canvas};

#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    pub name: &'static str,
    pub description: &'static str,
    /// `None`: the project's output size.
    pub size: Option<(u32, u32)>,
    pub fit: Fit,
    pub crf: u8,
    pub x264_preset: &'static str,
    pub bits_per_pixel: f64,
    pub audio_kbps: u32,
}

pub const PRESETS: [Preset; 5] = [
    Preset {
        name: "master",
        description: "project size, high quality H.264 + AAC (default)",
        size: None,
        fit: Fit::Fit,
        crf: 18,
        x264_preset: "medium",
        bits_per_pixel: 0.15,
        audio_kbps: 256,
    },
    Preset {
        name: "youtube-1080p",
        description: "1920x1080 H.264 + AAC for upload",
        size: Some((1920, 1080)),
        fit: Fit::Fit,
        crf: 18,
        x264_preset: "medium",
        bits_per_pixel: 0.12,
        audio_kbps: 192,
    },
    Preset {
        name: "youtube-720p",
        description: "1280x720 H.264 + AAC",
        size: Some((1280, 720)),
        fit: Fit::Fit,
        crf: 20,
        x264_preset: "medium",
        bits_per_pixel: 0.12,
        audio_kbps: 160,
    },
    Preset {
        name: "vertical-1080x1920",
        description: "1080x1920 for Shorts/Reels/TikTok; landscape sources are center-cropped",
        size: Some((1080, 1920)),
        fit: Fit::Fill,
        crf: 19,
        x264_preset: "medium",
        bits_per_pixel: 0.12,
        audio_kbps: 192,
    },
    Preset {
        name: "draft-540p",
        description: "960x540, fast and small, for review",
        size: Some((960, 540)),
        fit: Fit::Fit,
        crf: 26,
        x264_preset: "veryfast",
        bits_per_pixel: 0.08,
        audio_kbps: 128,
    },
];

pub fn preset(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name == name)
}

/// Encode settings for a preset on this project.
pub fn settings(tl: &Timeline, p: &Preset) -> EncodeSettings {
    let base = project_canvas(tl);
    let (width, height) = p.size.unwrap_or((base.width, base.height));
    EncodeSettings {
        canvas: Canvas { width: width & !1, height: height & !1, fit: p.fit, ..base },
        format: Format::Mp4,
        encoder: None,
        crf: p.crf,
        x264_preset: p.x264_preset.to_owned(),
        bits_per_pixel: p.bits_per_pixel,
        audio_kbps: p.audio_kbps,
        crossfade: ve_render::default_crossfade(),
    }
}

/// Low-res review render for `render.range_preview`: quick to make and to send to a model.
pub fn preview_settings(tl: &Timeline, width: u32) -> EncodeSettings {
    EncodeSettings {
        canvas: project_canvas(tl).scaled_to_width(width),
        format: Format::Mp4,
        encoder: None,
        crf: 28,
        // Previews are played once, locally, right after they are made: encode speed matters, size does not.
        x264_preset: "ultrafast".to_owned(),
        bits_per_pixel: 0.07,
        audio_kbps: 96,
        crossfade: ve_render::default_crossfade(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_names_are_unique_and_found() {
        for p in &PRESETS {
            assert_eq!(preset(p.name), Some(p));
            assert_eq!(PRESETS.iter().filter(|q| q.name == p.name).count(), 1);
        }
        assert!(preset("nope").is_none());
    }
}
